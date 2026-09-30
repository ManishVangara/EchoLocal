//! Speech recognition for EchoLocal: Parakeet TDT via transcribe.cpp, model
//! storage and downloads.

pub mod engine;
pub mod models;
pub mod wav;

pub use engine::{load_parakeet, CancelFlag, EngineError, Transcription, TranscriptionEngine};
pub use models::{DownloadError, ModelStore};
