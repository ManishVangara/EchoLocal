//! Audio input for EchoLocal.

mod recorder;
#[cfg(feature = "silero")]
mod silero;

pub use recorder::{list_input_devices, LimitCallback, Recording, RecordingOutput};

use echolocal_core::audio::{EnergyVad, VoiceActivityDetector};
use std::path::Path;

/// The best available voice activity detector: Silero when its model loads,
/// otherwise the built-in energy detector.
pub fn create_vad(silero_model: Option<&Path>) -> Box<dyn VoiceActivityDetector> {
    #[cfg(feature = "silero")]
    if let Some(path) = silero_model.filter(|p| p.exists()) {
        match silero::SileroVad::load(path) {
            Ok(vad) => {
                log::info!("Using Silero VAD");
                return Box::new(vad);
            }
            Err(e) => log::warn!("{e}; falling back to the energy detector"),
        }
    }
    #[cfg(not(feature = "silero"))]
    let _ = silero_model;
    log::info!("Using the energy-based VAD");
    Box::new(EnergyVad::new())
}
