//! WAV loading for benchmarks and tests.

use echolocal_core::{audio::downmix_to_mono, resample::resample_to_16k};
use std::path::Path;

/// Read a WAV file as 16 kHz mono f32 samples.
pub fn read_16k_mono(path: &Path) -> anyhow::Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / scale))
                .collect::<Result<_, _>>()?
        }
    };
    let mut mono = Vec::with_capacity(interleaved.len() / spec.channels.max(1) as usize);
    downmix_to_mono(&interleaved, spec.channels as usize, &mut mono);
    resample_to_16k(&mono, spec.sample_rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_stereo_44k_as_16k_mono() {
        let path = std::env::temp_dir().join(format!("echolocal-wav-{}.wav", std::process::id()));
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..44_100 {
            writer.write_sample(i16::MAX / 2).unwrap();
            writer.write_sample(0i16).unwrap();
        }
        writer.finalize().unwrap();

        let pcm = read_16k_mono(&path).unwrap();
        assert_eq!(pcm.len(), 16_000);
        assert!((pcm[8000] - 0.25).abs() < 0.01, "{}", pcm[8000]);
        let _ = std::fs::remove_file(path);
    }
}
