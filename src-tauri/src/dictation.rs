//! The dictation runtime: turns hotkey events into
//! record → transcribe → (post-process) → insert.
//!
//! Hotkey events are validated against the current phase at the moment they
//! happen (see [`echolocal_core::dictation::decide`]) and then handed to one
//! worker thread, which performs every step in order. Cancelling transcription
//! or post-processing goes through a shared flag instead, since the worker is
//! busy at that point.

use crate::background::{self, BackgroundTranscriber, SharedTexts, TranscribeError};
use crate::hotkey::Hotkeys;
use crate::overlay::{self, Kind};
use crate::preview::LivePreview;
use crate::state::{lock, notify_changed, AppState, EngineStatus};
use crate::{ai, tray};
use echolocal_audio::{Recording, RecordingCallbacks};
use echolocal_core::audio::samples_to_ms;
use echolocal_core::dictation::{decide, Decision, Phase, Trigger, MIN_RECORDING_MS};
use echolocal_core::insertion::InsertionMethod;
use echolocal_core::metrics::{since, DictationMetrics};
use echolocal_core::text::{join_transcripts, normalize_transcript};
use echolocal_core::PostProcessing;
use echolocal_macos::{FocusTarget, InsertOptions};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

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
    /// Transcribes pieces of a long dictation while recording (needs VAD).
    background: Option<BackgroundTranscriber>,
    preview: Option<LivePreview>,
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
        self.app.state::<Hotkeys>().set_cancel_enabled(false);
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
        let mut callbacks = RecordingCallbacks {
            on_limit: Some(Box::new(move || {
                let dictation = app.state::<Dictation>();
                dictation.trigger(&app, Trigger::HotkeyReleased);
            })),
            on_segment: None,
            on_preview: None,
            on_level: None,
        };
        let level_app = self.app.clone();
        callbacks.on_level = Some(Box::new(move |level| overlay::set_level(&level_app, level)));
        let pieces: SharedTexts = Default::default();
        // Pieces are cut at pauses, so this needs the VAD.
        let background = vad.is_some().then(|| {
            let (transcriber, on_segment) = BackgroundTranscriber::start(&self.app, pieces.clone());
            callbacks.on_segment = Some(on_segment);
            transcriber
        });
        let preview = settings.live_preview.then(|| {
            let (preview, on_preview) = LivePreview::start(&self.app, pieces);
            callbacks.on_preview = Some(on_preview);
            preview
        });
        match Recording::start(settings.microphone.as_deref(), vad, callbacks) {
            Ok(recording) => {
                self.active = Some(Active {
                    recording,
                    background,
                    preview,
                    target,
                    pressed_at,
                });
                self.set_phase(Phase::Recording);
                self.app.state::<Hotkeys>().set_cancel_enabled(true);
                if let Some(active) = &self.active {
                    overlay::set_context(&self.app, &active.target);
                }
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
            if let Some(preview) = active.preview {
                preview.stop();
            }
            if let Some(vad) = active.recording.cancel() {
                *lock(&self.state().vad) = Some(vad);
            }
            if let Some(background) = active.background {
                // Abort a piece mid-transcription; the flag is reset when the
                // next dictation starts.
                self.state().cancel.cancel();
                background.abandon();
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
        // From here Esc aborts processing (Escape stays registered until
        // `finish`), including background pieces still being transcribed.
        self.set_phase(Phase::Transcribing);
        // Free the engine for the real transcription right away.
        if let Some(preview) = active.preview {
            preview.stop();
        }
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
            if let Some(background) = active.background {
                background.abandon();
            }
            overlay::hide(&self.app);
            self.finish();
            return;
        }
        let tail = audio.speech();
        metrics.speech_ms = samples_to_ms(tail.len());

        match self.process(
            &active.target,
            active.background,
            tail,
            released_at,
            &mut metrics,
        ) {
            Ok(Some((method, words))) => {
                metrics.insertion_method = Some(method);
                log::info!("Dictation: {}", metrics.summary());
                let state = self.state();
                let mut stats = lock(&state.stats);
                stats.dictations += 1;
                stats.words += words as u64;
                stats.last_release_to_text_ms = Some(metrics.release_to_text_ms);
                stats.last_audio_ms = Some(metrics.audio_ms);
                drop(stats);
                notify_changed(&self.app);
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
        background: Option<BackgroundTranscriber>,
        tail: &[f32],
        released_at: Instant,
        metrics: &mut DictationMetrics,
    ) -> Result<Option<(InsertionMethod, usize)>, String> {
        let state = self.state();
        let settings = state.settings();

        overlay::show(&self.app, Kind::Transcribing, "Transcribing…");

        // Pieces split off while recording; usually already done by now.
        let pieces = match background {
            Some(background) => {
                let started = Instant::now();
                let result = background.finish();
                metrics.background_wait_ms = since(started);
                match result {
                    Ok(pieces) => pieces,
                    Err(TranscribeError::Cancelled) => {
                        overlay::hide(&self.app);
                        return Ok(None);
                    }
                    Err(TranscribeError::Failed(message)) => return Err(message.into()),
                }
            }
            None => background::Pieces::default(),
        };
        metrics.background_pieces = pieces.texts.len() as u32;
        metrics.background_speech_ms = pieces.speech_ms;

        // The rest of the recording.
        let tail_text = if tail.is_empty() {
            String::new()
        } else {
            match background::transcribe_pcm(&self.app, tail) {
                Ok((transcription, waited)) => {
                    metrics.model_wait_ms = waited;
                    metrics.inference_ms = transcription.inference_ms;
                    log::debug!("Tail transcript language: {:?}", transcription.language);
                    normalize_transcript(&transcription.text)
                }
                Err(TranscribeError::Cancelled) => {
                    overlay::hide(&self.app);
                    return Ok(None);
                }
                Err(TranscribeError::Failed(message)) => return Err(message.into()),
            }
        };

        let mut text = join_transcripts(
            pieces
                .texts
                .iter()
                .map(String::as_str)
                .chain(std::iter::once(tail_text.as_str())),
        );
        if text.is_empty() {
            overlay::flash(&self.app, Kind::Notice, "No speech detected", FLASH);
            return Ok(None);
        }
        overlay::set_text(&self.app, &text);

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
                Ok(Some((report.method, text.split_whitespace().count())))
            }
            Err(e) => {
                log::error!("Insertion failed: {e:#}");
                let _ = echolocal_macos::copy_to_clipboard(&text);
                Err("Couldn't insert — text copied to clipboard".into())
            }
        }
    }
}
