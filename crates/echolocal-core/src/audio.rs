//! In-memory recording buffer, voice activity detection and silence trimming.
//!
//! Audio arrives at 16 kHz mono. While recording, every 30 ms frame is
//! classified as speech or not and the flag is stored next to the samples.
//! Nothing is dropped during capture; on release, [`speech_range`] uses the
//! flags to trim leading and trailing silence in constant time, so VAD adds
//! no release-to-text latency.

use std::ops::Range;

/// Sample rate expected by Parakeet (and produced by the recorder).
pub const SAMPLE_RATE: u32 = 16_000;
/// 30 ms at 16 kHz: the frame size Silero VAD expects.
pub const VAD_FRAME_SAMPLES: usize = 480;

pub fn ms_to_samples(ms: u64) -> usize {
    (ms * SAMPLE_RATE as u64 / 1000) as usize
}

pub fn samples_to_ms(samples: usize) -> u64 {
    samples as u64 * 1000 / SAMPLE_RATE as u64
}

/// A frame-level speech classifier.
pub trait VoiceActivityDetector: Send {
    /// Classify one frame of exactly [`VAD_FRAME_SAMPLES`] samples.
    fn is_speech(&mut self, frame: &[f32]) -> anyhow::Result<bool>;
    /// Clear recurrent state before a new recording.
    fn reset(&mut self) {}
}

/// A dependency-free energy detector, used when Silero is unavailable.
///
/// It tracks the noise floor (fast to fall, slow to rise) and calls a frame
/// speech when it is clearly above both the floor and an absolute minimum.
#[derive(Debug, Clone)]
pub struct EnergyVad {
    noise_floor_db: f32,
}

impl EnergyVad {
    const MARGIN_DB: f32 = 12.0;
    const ABSOLUTE_MIN_DB: f32 = -55.0;
    const INITIAL_FLOOR_DB: f32 = -60.0;

    pub fn new() -> Self {
        Self {
            noise_floor_db: Self::INITIAL_FLOOR_DB,
        }
    }
}

impl Default for EnergyVad {
    fn default() -> Self {
        Self::new()
    }
}

fn rms_db(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return -120.0;
    }
    let mean_sq = frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32;
    10.0 * mean_sq.max(1e-12).log10()
}

impl VoiceActivityDetector for EnergyVad {
    fn is_speech(&mut self, frame: &[f32]) -> anyhow::Result<bool> {
        let db = rms_db(frame);
        if db < self.noise_floor_db {
            self.noise_floor_db = db.max(-90.0);
        } else {
            self.noise_floor_db += 0.05 * (db - self.noise_floor_db).min(1.0);
        }
        Ok(db > Self::ABSOLUTE_MIN_DB && db > self.noise_floor_db + Self::MARGIN_DB)
    }

    fn reset(&mut self) {
        self.noise_floor_db = Self::INITIAL_FLOOR_DB;
    }
}

/// A finished recording: 16 kHz mono samples plus per-frame speech flags.
#[derive(Debug, Clone, Default)]
pub struct RecordedAudio {
    pub samples: Vec<f32>,
    /// One flag per complete [`VAD_FRAME_SAMPLES`] frame, or `None` when VAD
    /// was disabled or failed (the whole recording is then kept).
    pub speech_frames: Option<Vec<bool>>,
}

impl RecordedAudio {
    pub fn duration_ms(&self) -> u64 {
        samples_to_ms(self.samples.len())
    }

    /// The samples to transcribe: trimmed to speech when flags exist.
    /// Returns an empty slice when VAD found no speech at all.
    pub fn speech(&self) -> &[f32] {
        match &self.speech_frames {
            None => &self.samples,
            Some(flags) => match speech_range(flags, self.samples.len(), &TrimConfig::default()) {
                Some(range) => &self.samples[range],
                None => &[],
            },
        }
    }
}

/// Accumulates samples during recording and classifies complete frames.
pub struct RecordingBuffer {
    samples: Vec<f32>,
    speech_frames: Vec<bool>,
    vad: Option<Box<dyn VoiceActivityDetector>>,
}

impl RecordingBuffer {
    pub fn new(mut vad: Option<Box<dyn VoiceActivityDetector>>) -> Self {
        if let Some(vad) = vad.as_mut() {
            vad.reset();
        }
        Self {
            // ~30 s preallocated; grows as needed.
            samples: Vec::with_capacity(SAMPLE_RATE as usize * 30),
            speech_frames: Vec::new(),
            vad,
        }
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn push(&mut self, samples: &[f32]) {
        self.samples.extend_from_slice(samples);
        let Some(vad) = self.vad.as_mut() else {
            return;
        };
        while (self.speech_frames.len() + 1) * VAD_FRAME_SAMPLES <= self.samples.len() {
            let start = self.speech_frames.len() * VAD_FRAME_SAMPLES;
            let frame = &self.samples[start..start + VAD_FRAME_SAMPLES];
            match vad.is_speech(frame) {
                Ok(speech) => self.speech_frames.push(speech),
                Err(e) => {
                    // Fail open: keep all audio rather than risk losing words.
                    log::warn!("VAD failed, keeping the whole recording: {e}");
                    self.vad = None;
                    return;
                }
            }
        }
    }

    pub fn finish(self) -> RecordedAudio {
        self.into_parts().0
    }

    /// Finish and hand back the detector so it can be reused (loading Silero
    /// takes longer than a recording should wait for).
    pub fn into_parts(self) -> (RecordedAudio, Option<Box<dyn VoiceActivityDetector>>) {
        let speech_frames = self.vad.is_some().then_some(self.speech_frames);
        let audio = RecordedAudio {
            samples: self.samples,
            speech_frames,
        };
        (audio, self.vad)
    }
}

#[derive(Debug, Clone)]
pub struct TrimConfig {
    /// Consecutive speech frames needed before a run counts as speech
    /// (filters out clicks and key noise).
    pub min_run_frames: usize,
    /// Audio kept before the first speech frame.
    pub pad_before_ms: u64,
    /// Audio kept after the last speech frame (word endings are quiet).
    pub pad_after_ms: u64,
}

impl Default for TrimConfig {
    fn default() -> Self {
        Self {
            min_run_frames: 2,
            pad_before_ms: 300,
            pad_after_ms: 450,
        }
    }
}

/// Sample range spanning all speech, with padding; `None` if there is none.
pub fn speech_range(
    flags: &[bool],
    total_samples: usize,
    cfg: &TrimConfig,
) -> Option<Range<usize>> {
    let min_run = cfg.min_run_frames.max(1);
    let mut first_frame: Option<usize> = None;
    let mut last_frame_end = 0usize;
    let mut run_start = 0usize;
    let mut run_len = 0usize;
    for (i, &speech) in flags.iter().enumerate() {
        if speech {
            if run_len == 0 {
                run_start = i;
            }
            run_len += 1;
            if run_len >= min_run {
                first_frame.get_or_insert(run_start);
                last_frame_end = i + 1;
            }
        } else {
            run_len = 0;
        }
    }
    let first_frame = first_frame?;
    let start = (first_frame * VAD_FRAME_SAMPLES).saturating_sub(ms_to_samples(cfg.pad_before_ms));
    // Samples after the last complete frame were never classified; the
    // padding may reach into them.
    let end =
        (last_frame_end * VAD_FRAME_SAMPLES + ms_to_samples(cfg.pad_after_ms)).min(total_samples);
    (start < end).then_some(start..end)
}

/// Pad very short utterances with trailing silence so the model always gets
/// at least `min_ms` of audio.
pub fn pad_to_min_duration(samples: &[f32], min_ms: u64) -> Vec<f32> {
    let min = ms_to_samples(min_ms);
    let mut out = samples.to_vec();
    if out.len() < min {
        out.resize(min, 0.0);
    }
    out
}

/// Downmix interleaved multi-channel audio to mono by averaging.
pub fn downmix_to_mono(interleaved: &[f32], channels: usize, out: &mut Vec<f32>) {
    if channels <= 1 {
        out.extend_from_slice(interleaved);
        return;
    }
    out.extend(
        interleaved
            .chunks_exact(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(ms: u64, amplitude: f32) -> Vec<f32> {
        (0..ms_to_samples(ms))
            .map(|i| {
                amplitude
                    * (i as f32 * 2.0 * std::f32::consts::PI * 220.0 / SAMPLE_RATE as f32).sin()
            })
            .collect()
    }

    fn silence(ms: u64) -> Vec<f32> {
        vec![0.0; ms_to_samples(ms)]
    }

    #[test]
    fn speech_range_none_without_speech() {
        assert_eq!(
            speech_range(&[false; 20], 20 * VAD_FRAME_SAMPLES, &TrimConfig::default()),
            None
        );
        assert_eq!(speech_range(&[], 0, &TrimConfig::default()), None);
    }

    #[test]
    fn single_frame_blips_are_ignored() {
        let mut flags = vec![false; 30];
        flags[5] = true;
        flags[20] = true;
        assert_eq!(
            speech_range(&flags, 30 * VAD_FRAME_SAMPLES, &TrimConfig::default()),
            None
        );
    }

    #[test]
    fn speech_range_pads_and_clamps() {
        let mut flags = vec![false; 100];
        for f in flags.iter_mut().take(60).skip(40) {
            *f = true;
        }
        let total = 100 * VAD_FRAME_SAMPLES;
        let cfg = TrimConfig::default();
        let r = speech_range(&flags, total, &cfg).unwrap();
        assert_eq!(r.start, 40 * VAD_FRAME_SAMPLES - ms_to_samples(300));
        assert_eq!(r.end, 60 * VAD_FRAME_SAMPLES + ms_to_samples(450));

        // Speech touching both edges clamps to the buffer.
        let flags = vec![true; 10];
        let r = speech_range(&flags, 10 * VAD_FRAME_SAMPLES + 100, &cfg).unwrap();
        assert_eq!(r, 0..10 * VAD_FRAME_SAMPLES + 100);
    }

    #[test]
    fn speech_range_spans_gaps_between_utterances() {
        let mut flags = vec![false; 200];
        for i in [10, 11, 12, 150, 151] {
            flags[i] = true;
        }
        let r = speech_range(&flags, 200 * VAD_FRAME_SAMPLES, &TrimConfig::default()).unwrap();
        assert!(r.start < 10 * VAD_FRAME_SAMPLES);
        assert!(r.end > 152 * VAD_FRAME_SAMPLES);
    }

    #[test]
    fn energy_vad_separates_tone_from_silence() {
        let mut vad = EnergyVad::new();
        let quiet = silence(30);
        let loud = tone(30, 0.3);
        for _ in 0..10 {
            assert!(!vad.is_speech(&quiet).unwrap());
        }
        assert!(vad.is_speech(&loud).unwrap());
    }

    #[test]
    fn buffer_trims_silence_around_speech() {
        let mut buf = RecordingBuffer::new(Some(Box::new(EnergyVad::new())));
        let mut all = silence(1000);
        all.extend(tone(600, 0.3));
        all.extend(silence(1500));
        // Push in odd-sized chunks, as a real device would deliver.
        for chunk in all.chunks(333) {
            buf.push(chunk);
        }
        let audio = buf.finish();
        assert_eq!(audio.samples.len(), all.len());
        let speech = audio.speech();
        let ms = samples_to_ms(speech.len());
        assert!((600..=600 + 300 + 450 + 60).contains(&ms), "kept {ms} ms");
    }

    #[test]
    fn silent_recording_yields_no_speech() {
        let mut buf = RecordingBuffer::new(Some(Box::new(EnergyVad::new())));
        buf.push(&silence(2000));
        assert!(buf.finish().speech().is_empty());
    }

    #[test]
    fn without_vad_everything_is_kept() {
        let mut buf = RecordingBuffer::new(None);
        buf.push(&silence(500));
        let audio = buf.finish();
        assert!(audio.speech_frames.is_none());
        assert_eq!(audio.speech().len(), ms_to_samples(500));
    }

    struct FailingVad;
    impl VoiceActivityDetector for FailingVad {
        fn is_speech(&mut self, _: &[f32]) -> anyhow::Result<bool> {
            anyhow::bail!("boom")
        }
    }

    #[test]
    fn failing_vad_fails_open() {
        let mut buf = RecordingBuffer::new(Some(Box::new(FailingVad)));
        buf.push(&silence(300));
        let audio = buf.finish();
        assert!(audio.speech_frames.is_none());
        assert_eq!(audio.speech().len(), ms_to_samples(300));
    }

    #[test]
    fn pads_short_audio() {
        assert_eq!(
            pad_to_min_duration(&[0.5; 10], 1000).len(),
            ms_to_samples(1000)
        );
        assert_eq!(
            pad_to_min_duration(&silence(1200), 1000).len(),
            ms_to_samples(1200)
        );
    }

    #[test]
    fn downmixes() {
        let mut out = Vec::new();
        downmix_to_mono(&[1.0, 0.0, 0.5, 0.5], 2, &mut out);
        assert_eq!(out, vec![0.5, 0.5]);
    }
}
