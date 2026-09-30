//! Transcribing long dictations while the user is still speaking.
//!
//! The recorder splits long recordings at pauses (see
//! [`echolocal_core::segment`]) and hands finished pieces to a
//! [`BackgroundTranscriber`], which runs Parakeet on them one by one on its
//! own thread. On release only the remaining tail has to be transcribed, so
//! release-to-text stays short however long the user talked.

use crate::state::{self, AppState};
use echolocal_audio::SegmentCallback;
use echolocal_core::audio::{pad_to_min_duration, samples_to_ms};
use echolocal_core::metrics::since;
use echolocal_core::text::normalize_transcript;
use echolocal_engine::{EngineError, Transcription};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;
use tauri::{AppHandle, Manager};

/// Parakeet gets at least this much audio (short clips are padded).
const MIN_MODEL_AUDIO_MS: u64 = 1000;

#[derive(Debug)]
pub enum TranscribeError {
    Cancelled,
    /// A short message suitable for the overlay; details are logged.
    Failed(&'static str),
}

/// Transcribe one buffer with the selected model, loading it first if needed
/// (e.g. after an idle unload). Returns the transcription and how long was
/// spent waiting for the model.
pub fn transcribe_pcm(
    app: &AppHandle,
    speech: &[f32],
) -> Result<(Transcription, u64), TranscribeError> {
    let state = app.state::<AppState>();
    let model = state.settings().model;
    let pcm = pad_to_min_duration(speech, MIN_MODEL_AUDIO_MS);
    let wait_started = Instant::now();
    let mut engine = state.engine();
    if engine.as_ref().map(|e| e.model_id()) != Some(model) {
        drop(engine);
        if let Err(e) = state::load_selected_model_now(app) {
            log::error!("{e:#}");
            return Err(TranscribeError::Failed("Speech model couldn't load"));
        }
        engine = state.engine();
    }
    let Some(engine) = engine.as_mut() else {
        return Err(TranscribeError::Failed("Speech model not ready"));
    };
    let waited = since(wait_started);
    let result = engine.transcribe(&pcm, &state.cancel);
    state.touch_model();
    match result {
        Ok(t) => Ok((t, waited)),
        Err(EngineError::Cancelled) => Err(TranscribeError::Cancelled),
        Err(EngineError::Failed(e)) => {
            log::error!("Transcription failed: {e}");
            Err(TranscribeError::Failed("Transcription failed"))
        }
    }
}

/// Transcripts of the pieces finished during recording, in order.
#[derive(Debug, Default)]
pub struct Pieces {
    pub texts: Vec<String>,
    pub speech_ms: u64,
    pub inference_ms: u64,
}

/// Transcripts of finished pieces, shared with the live preview.
pub type SharedTexts = Arc<Mutex<Vec<String>>>;

pub struct BackgroundTranscriber {
    thread: JoinHandle<Result<Pieces, TranscribeError>>,
    abandoned: Arc<AtomicBool>,
}

impl BackgroundTranscriber {
    /// Start the worker; pass the returned callback to the recorder.
    /// Finished piece texts are also appended to `shared` as they complete.
    pub fn start(app: &AppHandle, shared: SharedTexts) -> (Self, SegmentCallback) {
        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        let abandoned = Arc::new(AtomicBool::new(false));
        let flag = abandoned.clone();
        let app = app.clone();
        let thread = std::thread::Builder::new()
            .name("echolocal-background-asr".into())
            .spawn(move || {
                let mut pieces = Pieces::default();
                // Ends when the recorder drops the callback (recording stopped).
                for segment in rx {
                    if flag.load(Ordering::Relaxed) {
                        break;
                    }
                    let (transcription, _) = transcribe_pcm(&app, &segment)?;
                    log::info!(
                        "Background piece {}: {} ms of speech in {} ms",
                        pieces.texts.len() + 1,
                        samples_to_ms(segment.len()),
                        transcription.inference_ms
                    );
                    pieces.speech_ms += samples_to_ms(segment.len());
                    pieces.inference_ms += transcription.inference_ms;
                    let text = normalize_transcript(&transcription.text);
                    state::lock(&shared).push(text.clone());
                    pieces.texts.push(text);
                }
                Ok(pieces)
            })
            .expect("failed to start background transcription");
        let callback: SegmentCallback = Box::new(move |segment| {
            let _ = tx.send(segment);
        });
        (Self { thread, abandoned }, callback)
    }

    /// Wait for all pieces to be transcribed. Call after the recording has
    /// stopped (so no more pieces can arrive).
    pub fn finish(self) -> Result<Pieces, TranscribeError> {
        self.thread
            .join()
            .unwrap_or(Err(TranscribeError::Failed("Transcription failed")))
    }

    /// Stop after the current piece and discard everything. Doesn't wait.
    pub fn abandon(self) {
        self.abandoned.store(true, Ordering::Relaxed);
    }
}
