//! Silero VAD (v4 ONNX model) through ONNX Runtime.

use echolocal_core::audio::{VoiceActivityDetector, SAMPLE_RATE, VAD_FRAME_SAMPLES};
use std::path::Path;

/// Speech probability above which a frame counts as speech. Deliberately
/// permissive: trimming a quiet word is worse than keeping some silence.
const THRESHOLD: f32 = 0.3;

pub struct SileroVad {
    engine: vad_rs::Vad,
}

impl SileroVad {
    pub fn load(model_path: &Path) -> anyhow::Result<Self> {
        let engine = vad_rs::Vad::new(model_path, SAMPLE_RATE as usize)
            .map_err(|e| anyhow::anyhow!("could not load Silero VAD: {e}"))?;
        Ok(Self { engine })
    }
}

impl VoiceActivityDetector for SileroVad {
    fn is_speech(&mut self, frame: &[f32]) -> anyhow::Result<bool> {
        anyhow::ensure!(
            frame.len() == VAD_FRAME_SAMPLES,
            "bad VAD frame length {}",
            frame.len()
        );
        let result = self
            .engine
            .compute(frame)
            .map_err(|e| anyhow::anyhow!("Silero VAD error: {e}"))?;
        Ok(result.prob > THRESHOLD)
    }

    fn reset(&mut self) {
        self.engine.reset();
    }
}
