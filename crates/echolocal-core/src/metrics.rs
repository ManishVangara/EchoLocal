//! Per-dictation latency measurements.
//!
//! The number that matters is release-to-text: from hotkey release until the
//! text is in the target app. Each stage is recorded so regressions can be
//! traced to recording, inference, post-processing or insertion.

use crate::insertion::InsertionMethod;
use serde::Serialize;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default, Serialize)]
pub struct DictationMetrics {
    /// Hotkey press until the microphone delivered its first samples.
    pub mic_start_ms: Option<u64>,
    /// Length of the recording.
    pub audio_ms: u64,
    /// Length of audio passed to the model after silence trimming.
    pub speech_ms: u64,
    /// Time spent waiting for the model to (re)load after an idle unload.
    pub model_wait_ms: u64,
    pub inference_ms: u64,
    pub post_processing_ms: Option<u64>,
    pub insertion_ms: u64,
    pub release_to_text_ms: u64,
    pub insertion_method: Option<InsertionMethod>,
}

impl DictationMetrics {
    /// Inference time divided by audio duration (lower is faster).
    pub fn real_time_factor(&self) -> Option<f64> {
        (self.speech_ms > 0).then(|| self.inference_ms as f64 / self.speech_ms as f64)
    }

    pub fn summary(&self) -> String {
        format!(
            "release→text {} ms (model wait {} ms, inference {} ms for {} ms speech, RTF {}, post {} ms, insert {} ms via {:?}); recorded {} ms; mic start {} ms",
            self.release_to_text_ms,
            self.model_wait_ms,
            self.inference_ms,
            self.speech_ms,
            self.real_time_factor().map_or("n/a".into(), |r| format!("{r:.3}")),
            self.post_processing_ms.unwrap_or(0),
            self.insertion_ms,
            self.insertion_method,
            self.audio_ms,
            self.mic_start_ms.map_or("n/a".into(), |ms| ms.to_string()),
        )
    }
}

pub fn ms(d: Duration) -> u64 {
    d.as_millis() as u64
}

/// Milliseconds elapsed since `start`.
pub fn since(start: Instant) -> u64 {
    ms(start.elapsed())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtf() {
        let m = DictationMetrics {
            speech_ms: 2000,
            inference_ms: 100,
            ..Default::default()
        };
        assert_eq!(m.real_time_factor(), Some(0.05));
        assert_eq!(DictationMetrics::default().real_time_factor(), None);
        assert!(m.summary().contains("RTF 0.050"));
    }
}
