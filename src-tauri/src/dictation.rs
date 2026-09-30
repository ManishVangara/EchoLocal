//! The dictation runtime: turns hotkey events into
//! record → transcribe → (post-process) → insert.
//!
//! Hotkey events are validated against the current phase at the moment they
//! happen (see [`echolocal_core::dictation::decide`]) and then handed to one
//! worker thread, which performs every step in order. Cancelling transcription
//! or post-processing goes through a shared flag instead, since the worker is
//! busy at that point.

use crate::overlay::{self, Kind};
use crate::state::{lock, notify_changed, AppState, EngineStatus};
use crate::{ai, hotkey, tray};
use echolocal_audio::Recording;
use echolocal_core::audio::{pad_to_min_duration, samples_to_ms};
use echolocal_core::dictation::{decide, Decision, Phase, Trigger, MIN_RECORDING_MS};
use echolocal_core::insertion::InsertionMethod;
use echolocal_core::metrics::{since, DictationMetrics};
use echolocal_core::text::normalize_transcript;
use echolocal_core::PostProcessing;
use echolocal_engine::EngineError;
use echolocal_macos::{FocusTarget, InsertOptions};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// Parakeet gets at least this much audio (short clips are padded).
const MIN_MODEL_AUDIO_MS: u64 = 1000;
const FLASH: Duration = Duration::from_millis(900);
const FLASH_LONG: Duration = Duration::from_millis(2600);

enum Command {
    Start { pressed_at: Instant },
    Stop { released_at: Instant },
    Discard,
}

pub struct Dictation {
    commands: Sender<Command>,
}

impl Dictation {
    pub fn start(app: &AppHandle) -> Self {
        let (commands, rx) = mpsc::channel();
        let app = app.clone();
        std::thread::Builder::new()
            .name("echolocal-dictation".into())
            .spawn(move || Worker { app, active: None }.run(rx))
            .expect("failed to start dictation thread");
        Self { commands }
    }

    /// Handle a trigger. Called from the hotkey handler; never blocks.
    pub fn trigger(&self, app: &AppHandle, trigger: Trigger) {
        let state = app.state::<AppState>();
        let now = Instant::now();
        let phase = state.phase();
        match decide(phase, trigger) {
            Decision::StartRecording => {
                // Claim the session synchronously so key auto-repeat can't
                // start a second one before the worker picks this up.
                if state.transition(Phase::Idle, Phase::Preparing) {
                    let _ = self.commands.send(Command::Start { pressed_at: now });
                }
            }
            Decision::StopAndTranscribe => {
                let _ = self.commands.send(Command::Stop { released_at: now });
            }
            Decision::DiscardRecording => {
                let _ = self.commands.send(Command::Discard);
            }
            Decision::AbortProcessing => {
                log::info!("Cancelling during {phase:?}");
                state.cancel.cancel();
            }
            Decision::Ignore => {}
        }
    }
}

struct Active {
    recording: Recording,
    target: FocusTarget,
    pressed_at: Instant,
}

struct Worker {
    app: AppHandle,
    active: Option<Active>,
}

impl Worker {
    fn state(&self) -> tauri::State<'_, AppState> {
        self.app.state::<AppState>()
    }

    fn set_phase(&self, phase: Phase) {
        self.state().set_phase(phase);
        tray::set_phase(&self.app, phase);
        notify_changed(&self.app);
    }

    fn run(mut self, rx: Receiver<Command>) {
        while let Ok(command) = rx.recv() {
            match command {
                Command::Start { pressed_at } => self.start(pressed_at),
                Command::Stop { released_at } => self.stop(released_at),
                Command::Discard => self.discard(),
            }
        }
    }

    fn finish(&mut self) {
        hotkey::set_cancel_enabled(&self.app, false);
        self.set_phase(Phase::Idle);
    }

    fn start(&mut self, pressed_at: Instant) {
        let state = self.state();
        state.cancel.reset();
        let settings = state.settings();

        // Capture where the text should go before any UI appears.
        let target = echolocal_macos::capture_focus_target();
        let _ = self
            .app
            .run_on_main_thread(echolocal_macos::refresh_keyboard_layout);

        if !state.store.is_downloaded(settings.model) {
            overlay::flash(
                &self.app,
                Kind::Error,
                "Download a speech model first",
                FLASH_LONG,
            );
            crate::show_settings(&self.app);
            self.finish();
            return;
        }
        // If the model was unloaded while idle, reload it now so loading
        // overlaps with the user speaking; transcription waits for it.
        if !matches!(state.engine_status(), EngineStatus::Ready { model } if model == settings.model)
            && !matches!(state.engine_status(), EngineStatus::Loading { .. })
        {
            crate::state::load_selected_model(&self.app);
        }

        let vad = if settings.trim_silence {
            lock(&state.vad)
                .take()
                .or_else(|| Some(echolocal_audio::create_vad(state.silero_model.as_deref())))
        } else {
            None
        };
        let app = self.app.clone();
        let on_limit = Box::new(move || {
            let dictation = app.state::<Dictation>();
            dictation.trigger(&app, Trigger::HotkeyReleased);
        });
        match Recording::start(settings.microphone.as_deref(), vad, Some(on_limit)) {
            Ok(recording) => {
                self.active = Some(Active {
                    recording,
                    target,
                    pressed_at,
                });
                self.set_phase(Phase::Recording);
                hotkey::set_cancel_enabled(&self.app, true);
                overlay::show(&self.app, Kind::Listening, "Listening");
                log::info!(
                    "Recording (target: {}, mic open after {} ms)",
                    self.active
                        .as_ref()
                        .unwrap()
                        .target
                        .app_name
                        .as_deref()
                        .unwrap_or("unknown"),
                    since(pressed_at)
                );
            }
            Err(e) => {
                log::error!("{e:#}");
                overlay::flash(&self.app, Kind::Error, "Microphone unavailable", FLASH_LONG);
                self.finish();
            }
        }
    }

    fn discard(&mut self) {
        if let Some(active) = self.active.take() {
            if let Some(vad) = active.recording.cancel() {
                *lock(&self.state().vad) = Some(vad);
            }
            log::info!("Recording discarded");
        }
        overlay::hide(&self.app);
        self.finish();
    }

    fn stop(&mut self, released_at: Instant) {
        let Some(active) = self.active.take() else {
            // Start failed; the phase was already reset.
            return;
        };
        hotkey::set_cancel_enabled(&self.app, false);
        let mut metrics = DictationMetrics {
            mic_start_ms: active
                .recording
                .first_audio_latency()
                .map(|d| d.as_millis() as u64),
            ..Default::default()
        };
        let held_ms = since(active.pressed_at);
        let output = match active.recording.stop() {
            Ok(output) => output,
            Err(e) => {
                log::error!("Recording failed: {e:#}");
                overlay::flash(&self.app, Kind::Error, "Recording failed", FLASH_LONG);
                self.finish();
                return;
            }
        };
        if let Some(vad) = output.vad {
            *lock(&self.state().vad) = Some(vad);
        }
        let audio = output.audio;
        metrics.audio_ms = audio.duration_ms();
        if held_ms < MIN_RECORDING_MS || audio.samples.is_empty() {
            log::info!("Ignoring {held_ms} ms tap");
            overlay::hide(&self.app);
            self.finish();
            return;
        }
        let speech = audio.speech();
        metrics.speech_ms = samples_to_ms(speech.len());
        if speech.is_empty() {
            overlay::flash(&self.app, Kind::Notice, "No speech detected", FLASH);
            self.finish();
            return;
        }

        match self.process(&active.target, speech, released_at, &mut metrics) {
            Ok(Some(method)) => {
                metrics.insertion_method = Some(method);
                log::info!("Dictation: {}", metrics.summary());
            }
            Ok(None) => {}
            Err(message) => {
                overlay::flash(&self.app, Kind::Error, message, FLASH_LONG);
            }
        }
        self.finish();
    }

    /// Transcribe, optionally post-process, and insert. `Ok(None)` means the
    /// dictation ended quietly (cancelled or nothing heard).
    fn process(
        &mut self,
        target: &FocusTarget,
        speech: &[f32],
        released_at: Instant,
        metrics: &mut DictationMetrics,
    ) -> Result<Option<InsertionMethod>, String> {
        let state = self.state();
        let settings = state.settings();

        // Transcribe.
        self.set_phase(Phase::Transcribing);
        overlay::show(&self.app, Kind::Transcribing, "Transcribing…");
        let pcm = pad_to_min_duration(speech, MIN_MODEL_AUDIO_MS);
        let wait_started = Instant::now();
        let transcription = {
            let mut engine = state.engine();
            if engine.as_ref().map(|e| e.model_id()) != Some(settings.model) {
                // Not loaded yet (first run, or loading failed): load now.
                drop(engine);
                if let Err(e) = crate::state::load_selected_model_now(&self.app) {
                    log::error!("{e:#}");
                    return Err("Speech model couldn't load".into());
                }
                engine = state.engine();
            }
            let Some(engine) = engine.as_mut() else {
                return Err("Speech model not ready".into());
            };
            metrics.model_wait_ms = since(wait_started);
            let result = engine.transcribe(&pcm, &state.cancel);
            state.touch_model();
            result
        };
        let transcription = match transcription {
            Ok(t) => t,
            Err(EngineError::Cancelled) => {
                overlay::hide(&self.app);
                return Ok(None);
            }
            Err(EngineError::Failed(e)) => {
                log::error!("Transcription failed: {e}");
                return Err("Transcription failed".into());
            }
        };
        metrics.inference_ms = transcription.inference_ms;
        let mut text = normalize_transcript(&transcription.text);
        log::debug!("Transcript ({:?}): {text}", transcription.language);
        if text.is_empty() {
            overlay::flash(&self.app, Kind::Notice, "No speech detected", FLASH);
            return Ok(None);
        }

        // Optional cleanup / rewrite. Failures fall back to the raw transcript.
        let mut ai_failed = false;
        if settings.post_processing != PostProcessing::Off && settings.ai.is_configured() {
            self.set_phase(Phase::PostProcessing);
            overlay::show(&self.app, Kind::Polishing, "Polishing…");
            let started = Instant::now();
            match ai::post_process(settings.post_processing, &text, &settings.ai, &state.cancel) {
                Ok(polished) => text = polished,
                Err(ai::AiError::Cancelled) => {
                    overlay::hide(&self.app);
                    return Ok(None);
                }
                Err(ai::AiError::Failed(e)) => {
                    log::warn!("Post-processing failed, inserting the raw transcript: {e}");
                    ai_failed = true;
                }
            }
            metrics.post_processing_ms = Some(since(started));
        }
        if state.cancel.is_cancelled() {
            overlay::hide(&self.app);
            return Ok(None);
        }

        // Without Accessibility permission macOS silently drops synthesized
        // input, so hand the text over via the clipboard instead.
        if !echolocal_macos::accessibility_trusted() {
            let _ = echolocal_macos::copy_to_clipboard(&text);
            crate::show_settings(&self.app);
            return Err("Allow Accessibility access — text copied".into());
        }

        // Insert.
        self.set_phase(Phase::Inserting);
        let started = Instant::now();
        let result = echolocal_macos::insert_text(target, &text, &InsertOptions::default());
        metrics.insertion_ms = since(started);
        metrics.release_to_text_ms = since(released_at);
        match result {
            Ok(report) => {
                let (kind, message, duration) = match report.method {
                    InsertionMethod::ClipboardOnly => {
                        (Kind::Notice, "Copied — press ⌘V to paste", FLASH_LONG)
                    }
                    _ if ai_failed => (Kind::Notice, "Inserted without AI cleanup", FLASH_LONG),
                    _ => (Kind::Inserted, "Inserted", Duration::from_millis(500)),
                };
                overlay::flash(&self.app, kind, message, duration);
                Ok(Some(report.method))
            }
            Err(e) => {
                log::error!("Insertion failed: {e:#}");
                let _ = echolocal_macos::copy_to_clipboard(&text);
                Err("Couldn't insert — text copied to clipboard".into())
            }
        }
    }
}
