//! Streaming conversion from the microphone's native rate to 16 kHz.
//!
//! Resampling happens incrementally while recording, so releasing the hotkey
//! only has to flush one final chunk.

use crate::audio::SAMPLE_RATE;
use rubato::{FftFixedIn, Resampler};

const CHUNK: usize = 1024;

pub struct StreamResampler {
    inner: Option<FftFixedIn<f32>>,
    pending: Vec<f32>,
    /// Leading output samples still to discard (filter delay).
    skip: usize,
    input_total: u64,
    output_total: u64,
    input_rate: u32,
}

impl StreamResampler {
    pub fn new(input_rate: u32) -> anyhow::Result<Self> {
        anyhow::ensure!(input_rate > 0, "invalid input sample rate");
        let inner = if input_rate == SAMPLE_RATE {
            None
        } else {
            Some(FftFixedIn::<f32>::new(
                input_rate as usize,
                SAMPLE_RATE as usize,
                CHUNK,
                1,
                1,
            )?)
        };
        let skip = inner.as_ref().map_or(0, |r| r.output_delay());
        Ok(Self {
            inner,
            pending: Vec::with_capacity(CHUNK * 2),
            skip,
            input_total: 0,
            output_total: 0,
            input_rate,
        })
    }

    /// Output samples the input seen so far should produce in total.
    fn expected_output(&self) -> u64 {
        self.input_total * SAMPLE_RATE as u64 / self.input_rate as u64
    }

    fn emit(&mut self, samples: &[f32], out: &mut Vec<f32>) {
        let skip = self.skip.min(samples.len());
        self.skip -= skip;
        let samples = &samples[skip..];
        let room = self.expected_output().saturating_sub(self.output_total) as usize;
        let take = samples.len().min(room);
        out.extend_from_slice(&samples[..take]);
        self.output_total += take as u64;
    }

    /// Feed mono samples at the input rate; appends 16 kHz samples to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) -> anyhow::Result<()> {
        self.input_total += input.len() as u64;
        let Some(resampler) = self.inner.as_mut() else {
            out.extend_from_slice(input);
            self.output_total += input.len() as u64;
            return Ok(());
        };
        self.pending.extend_from_slice(input);
        let mut produced = Vec::new();
        let mut offset = 0;
        while self.pending.len() - offset >= CHUNK {
            let chunk = &self.pending[offset..offset + CHUNK];
            let result = resampler.process(&[chunk], None)?;
            produced.extend_from_slice(&result[0]);
            offset += CHUNK;
        }
        self.pending.drain(..offset);
        self.emit(&produced, out);
        Ok(())
    }

    /// Flush buffered input (and the filter tail) at the end of a recording.
    pub fn finish(&mut self, out: &mut Vec<f32>) -> anyhow::Result<()> {
        let target = self.expected_output() + self.skip as u64;
        let Some(resampler) = self.inner.as_mut() else {
            return Ok(());
        };
        let mut produced = Vec::new();
        // Zero-padding pushes the remaining real samples (and the filter delay)
        // through; `emit` caps the output at the exact expected length.
        let mut rounds = 0;
        while (self.output_total + produced.len() as u64) < target && rounds < 8 {
            let mut chunk = std::mem::take(&mut self.pending);
            chunk.resize(CHUNK, 0.0);
            let result = resampler.process(&[&chunk[..]], None)?;
            produced.extend_from_slice(&result[0]);
            rounds += 1;
        }
        self.pending.clear();
        self.emit(&produced, out);
        Ok(())
    }
}

/// One-shot conversion of a whole buffer to 16 kHz.
pub fn resample_to_16k(samples: &[f32], input_rate: u32) -> anyhow::Result<Vec<f32>> {
    let mut resampler = StreamResampler::new(input_rate)?;
    let mut out =
        Vec::with_capacity(samples.len() * SAMPLE_RATE as usize / input_rate as usize + 1);
    resampler.process(samples, &mut out)?;
    resampler.finish(&mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, freq: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (i as f32 * 2.0 * std::f32::consts::PI * freq / rate as f32).sin() * 0.5)
            .collect()
    }

    #[test]
    fn passthrough_at_16k() {
        let input = sine(16_000, 440.0, 5000);
        assert_eq!(resample_to_16k(&input, 16_000).unwrap(), input);
    }

    #[test]
    fn output_length_matches_rate_ratio() {
        for rate in [44_100u32, 48_000, 22_050, 8_000] {
            for n in [0usize, 1, 1023, 1024, 4800, 48_000 + 17] {
                let out = resample_to_16k(&sine(rate, 300.0, n), rate).unwrap();
                assert_eq!(
                    out.len() as u64,
                    n as u64 * 16_000 / rate as u64,
                    "rate {rate} n {n}"
                );
            }
        }
    }

    #[test]
    fn chunked_equals_one_shot() {
        let input = sine(48_000, 300.0, 48_000);
        let one_shot = resample_to_16k(&input, 48_000).unwrap();
        let mut r = StreamResampler::new(48_000).unwrap();
        let mut out = Vec::new();
        for chunk in input.chunks(441) {
            r.process(chunk, &mut out).unwrap();
        }
        r.finish(&mut out).unwrap();
        assert_eq!(out.len(), one_shot.len());
        for (a, b) in out.iter().zip(&one_shot) {
            assert!((a - b).abs() < 1e-4);
        }
    }

    #[test]
    fn preserves_a_tone_without_delay() {
        // A 300 Hz tone at 48 kHz should come out as the same tone at 16 kHz,
        // aligned with the input (filter delay removed).
        let input = sine(48_000, 300.0, 48_000);
        let out = resample_to_16k(&input, 48_000).unwrap();
        let expected = sine(16_000, 300.0, out.len());
        let middle = 2000..out.len() - 2000;
        let max_err = middle
            .map(|i| (out[i] - expected[i]).abs())
            .fold(0.0f32, f32::max);
        assert!(max_err < 0.02, "max error {max_err}");
    }
}
