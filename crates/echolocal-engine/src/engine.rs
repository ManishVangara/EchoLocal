//! The speech-to-text engine abstraction and its transcribe.cpp implementation.
//!
//! The rest of the app talks to [`TranscriptionEngine`] only; transcribe.cpp,
//! GGUF and backend selection stay behind this module.

use echolocal_core::ModelId;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("transcription was cancelled")]
    Cancelled,
    #[error("{0}")]
    Failed(String),
}

/// Result of transcribing one buffer.
#[derive(Debug, Clone, Default)]
pub struct Transcription {
    pub text: String,
    /// Language detected by the model, when it reports one.
    pub language: Option<String>,
    pub inference_ms: u64,
}

/// A shareable flag that aborts an in-flight transcription.
#[derive(Debug, Clone, Default)]
pub struct CancelFlag(Arc<AtomicBool>);

impl CancelFlag {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn reset(&self) {
        self.0.store(false, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

pub trait TranscriptionEngine: Send {
    fn model_id(&self) -> ModelId;
    /// Compute device the model is running on, for logs (e.g. "Metal").
    fn device(&self) -> String;
    /// Transcribe 16 kHz mono f32 PCM in [-1, 1].
    fn transcribe(
        &mut self,
        pcm: &[f32],
        cancel: &CancelFlag,
    ) -> Result<Transcription, EngineError>;
}

/// Load a Parakeet model. The returned engine keeps the model resident.
pub fn load_parakeet(model: ModelId, path: &Path) -> anyhow::Result<Box<dyn TranscriptionEngine>> {
    imp::load(model, path)
}

/// One-time backend setup; call before the first load.
pub fn init_backend() {
    imp::init();
}

#[cfg(feature = "native")]
mod imp {
    use super::*;
    use std::time::Instant;
    use transcribe_cpp::{CancelToken, Error, Model, ModelOptions, RunOptions, Session};

    pub fn init() {
        transcribe_cpp::init_logging();
        if let Err(e) = transcribe_cpp::init_backends_default() {
            log::warn!("transcribe.cpp backend initialization failed: {e}");
        }
    }

    pub struct ParakeetEngine {
        model_id: ModelId,
        session: Session,
        token: CancelToken,
        options: RunOptions,
        max_audio_samples: usize,
        device: String,
    }

    pub fn load(model_id: ModelId, path: &Path) -> anyhow::Result<Box<dyn TranscriptionEngine>> {
        let started = Instant::now();
        // Auto picks Metal on Apple Silicon and falls back to CPU.
        let model = Model::load_with(path, &ModelOptions::default())
            .map_err(|e| anyhow::anyhow!("could not load {}: {e}", model_id.spec().display_name))?;
        let caps = model.capabilities();
        anyhow::ensure!(
            caps.native_sample_rate == 0
                || caps.native_sample_rate == echolocal_core::audio::SAMPLE_RATE as i32,
            "unexpected model sample rate {}",
            caps.native_sample_rate
        );
        let device = model.backend();
        let mut session = model.session()?;
        let token = CancelToken::new();
        session.set_cancel_token(&token);

        // Pass the language hint only if the model advertises it; otherwise
        // let it detect (v3) or use its only language (v2).
        let language = model_id
            .spec()
            .language_hint
            .filter(|lang| caps.languages.iter().any(|l| l == lang))
            .map(str::to_string);
        let options = RunOptions {
            language,
            ..RunOptions::default()
        };
        let max_audio_samples = if caps.max_audio_ms > 0 {
            echolocal_core::audio::ms_to_samples(caps.max_audio_ms as u64)
        } else {
            usize::MAX
        };
        log::info!(
            "Loaded {} on {} in {} ms (languages: {}, streaming: {})",
            model_id.spec().display_name,
            device,
            started.elapsed().as_millis(),
            caps.languages.len(),
            caps.supports_streaming
        );
        Ok(Box::new(ParakeetEngine {
            model_id,
            session,
            token,
            options,
            max_audio_samples,
            device,
        }))
    }

    impl ParakeetEngine {
        fn run(&mut self, pcm: &[f32]) -> Result<transcribe_cpp::Transcript, EngineError> {
            self.session.run(pcm, &self.options).map_err(|e| match e {
                Error::Aborted { .. } => EngineError::Cancelled,
                other => EngineError::Failed(other.to_string()),
            })
        }
    }

    impl TranscriptionEngine for ParakeetEngine {
        fn model_id(&self) -> ModelId {
            self.model_id
        }

        fn device(&self) -> String {
            self.device.clone()
        }

        fn transcribe(
            &mut self,
            pcm: &[f32],
            cancel: &CancelFlag,
        ) -> Result<Transcription, EngineError> {
            if cancel.is_cancelled() {
                return Err(EngineError::Cancelled);
            }
            self.token.reset();
            // Bridge the app's flag to the native abort callback for this run.
            let watcher_flag = cancel.clone();
            let token = self.token.clone();
            let done = Arc::new(AtomicBool::new(false));
            let watcher_done = done.clone();
            let watcher = std::thread::spawn(move || {
                while !watcher_done.load(Ordering::Relaxed) {
                    if watcher_flag.is_cancelled() {
                        token.cancel();
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            });

            let started = Instant::now();
            let result = (|| {
                let mut texts = Vec::new();
                let mut language = None;
                for chunk in pcm.chunks(self.max_audio_samples.max(1)) {
                    let transcript = self.run(chunk)?;
                    language = language.or(transcript.language.clone());
                    texts.push(transcript.text);
                }
                Ok(Transcription {
                    text: texts.join(" "),
                    language,
                    inference_ms: started.elapsed().as_millis() as u64,
                })
            })();

            done.store(true, Ordering::Relaxed);
            let _ = watcher.join();
            if cancel.is_cancelled() {
                return Err(EngineError::Cancelled);
            }
            result
        }
    }
}

#[cfg(not(feature = "native"))]
mod imp {
    use super::*;

    pub fn init() {}

    pub fn load(_: ModelId, _: &Path) -> anyhow::Result<Box<dyn TranscriptionEngine>> {
        anyhow::bail!("this build of EchoLocal was compiled without the speech engine")
    }
}
