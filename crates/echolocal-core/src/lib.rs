//! Platform-independent core of EchoLocal.
//!
//! Everything here is pure logic with no microphone, model, OS or UI access,
//! so it can be unit-tested on any machine.

pub mod ai;
pub mod audio;
pub mod catalog;
pub mod dictation;
pub mod insertion;
pub mod metrics;
pub mod resample;
pub mod segment;
pub mod settings;
pub mod shortcut;
pub mod text;

pub use catalog::{ModelId, ModelSpec};
pub use settings::{AiSettings, ModelUnload, PostProcessing, Settings};
