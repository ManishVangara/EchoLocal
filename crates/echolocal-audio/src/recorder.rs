//! Microphone capture.
//!
//! A dedicated thread owns the cpal stream (streams are not `Send` on every
//! platform). The audio callback only downmixes to mono and pushes into a
//! lock-free ring buffer; the capture thread drains it every few milliseconds,
//! resamples to 16 kHz and appends to a [`RecordingBuffer`] which classifies
//! speech frames as they arrive. Stopping therefore only has to flush the last
//! few milliseconds of audio.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use echolocal_core::audio::{RecordedAudio, RecordingBuffer, VoiceActivityDetector};
use echolocal_core::dictation::MAX_RECORDING_MS;
use echolocal_core::resample::StreamResampler;
use echolocal_core::segment::SegmentConfig;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const DRAIN_INTERVAL: Duration = Duration::from_millis(10);

/// Names of available input devices, default device first.
pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    let default = host.default_input_device().and_then(|d| d.name().ok());
    let mut names: Vec<String> = host
        .input_devices()
        .map(|devices| devices.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default();
    if let Some(default) = default {
        names.retain(|n| *n != default);
        names.insert(0, default);
    }
    names
}

enum Command {
    Stop,
    Cancel,
}

/// Called once, from the capture thread, when the recording hits
/// [`MAX_RECORDING_MS`]; capture stops appending audio at that point.
pub type LimitCallback = Box<dyn FnOnce() + Send>;

/// Receives finished pieces of a long recording (16 kHz, trimmed to speech),
/// in order, while recording continues. Called from the capture thread, so it
/// must return quickly (e.g. send to a channel).
pub type SegmentCallback = Box<dyn FnMut(Vec<f32>) + Send>;

/// Receives the not-yet-segmented speech so far (16 kHz, trimmed), about
/// every [`PREVIEW_INTERVAL`], for a live transcript. Called from the capture
/// thread: copy what you need and return quickly.
pub type PreviewCallback = Box<dyn FnMut(&[f32]) + Send>;

pub const PREVIEW_INTERVAL: Duration = Duration::from_millis(500);

/// Optional notifications from an active recording.
#[derive(Default)]
pub struct RecordingCallbacks {
    pub on_limit: Option<LimitCallback>,
    /// Setting this enables splitting long recordings at pauses (needs VAD).
    pub on_segment: Option<SegmentCallback>,
    pub on_preview: Option<PreviewCallback>,
}

pub struct RecordingOutput {
    pub audio: RecordedAudio,
    /// The detector, returned for reuse by the next recording.
    pub vad: Option<Box<dyn VoiceActivityDetector>>,
    /// Device sample rate before conversion to 16 kHz.
    pub device_sample_rate: u32,
}

/// An active recording. Dropping it without calling [`stop`](Self::stop)
/// cancels it.
pub struct Recording {
    commands: mpsc::Sender<Command>,
    thread: Option<JoinHandle<anyhow::Result<RecordingOutput>>>,
    started: Instant,
    first_audio: Arc<OnceLock<Instant>>,
}

impl Recording {
    /// Open the microphone and start buffering. Returns once the stream is
    /// running, so the caller can tell the user it is listening.
    pub fn start(
        device_name: Option<&str>,
        vad: Option<Box<dyn VoiceActivityDetector>>,
        callbacks: RecordingCallbacks,
    ) -> anyhow::Result<Recording> {
        let started = Instant::now();
        let (commands, command_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let first_audio = Arc::new(OnceLock::new());
        let device_name = device_name.map(str::to_string);
        let first_audio_thread = first_audio.clone();

        let thread = std::thread::Builder::new()
            .name("echolocal-capture".into())
            .spawn(move || {
                capture_thread(
                    device_name,
                    vad,
                    callbacks,
                    command_rx,
                    ready_tx,
                    first_audio_thread,
                )
            })?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Recording {
                commands,
                thread: Some(thread),
                started,
                first_audio,
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                let result = thread.join();
                Err(match result {
                    Ok(Err(e)) => e,
                    _ => anyhow::anyhow!("audio capture thread exited unexpectedly"),
                })
            }
        }
    }

    /// Time from `start` until the first audio arrived from the device.
    pub fn first_audio_latency(&self) -> Option<Duration> {
        self.first_audio
            .get()
            .map(|t| t.duration_since(self.started))
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Stop capture and return everything recorded.
    pub fn stop(mut self) -> anyhow::Result<RecordingOutput> {
        let _ = self.commands.send(Command::Stop);
        match self.thread.take().map(JoinHandle::join) {
            Some(Ok(result)) => result,
            _ => Err(anyhow::anyhow!("audio capture thread panicked")),
        }
    }

    /// Stop capture and discard the audio. Returns the detector for reuse.
    pub fn cancel(mut self) -> Option<Box<dyn VoiceActivityDetector>> {
        let _ = self.commands.send(Command::Cancel);
        match self.thread.take().map(JoinHandle::join) {
            Some(Ok(Ok(output))) => output.vad,
            _ => None,
        }
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = self.commands.send(Command::Cancel);
            let _ = thread.join();
        }
    }
}

fn find_device(host: &cpal::Host, name: Option<&str>) -> anyhow::Result<cpal::Device> {
    if let Some(name) = name {
        if let Some(device) = host
            .input_devices()?
            .find(|d| d.name().is_ok_and(|n| n == name))
        {
            return Ok(device);
        }
        log::warn!("Microphone '{name}' not found; using the system default");
    }
    host.default_input_device()
        .ok_or_else(|| anyhow::anyhow!("no microphone found"))
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut producer: rtrb::Producer<f32>,
    overflowed: Arc<AtomicU64>,
    failed: Arc<AtomicBool>,
) -> anyhow::Result<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels.max(1) as usize;
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _| {
            // Real-time context: no allocation, no locks.
            for frame in data.chunks_exact(channels) {
                let sum: f32 = frame
                    .iter()
                    .map(|&s| <f32 as FromSample<T>>::from_sample_(s))
                    .sum();
                if producer.push(sum / channels as f32).is_err() {
                    overflowed.fetch_add(1, Ordering::Relaxed);
                }
            }
        },
        move |err| {
            log::error!("Microphone stream error: {err}");
            failed.store(true, Ordering::Relaxed);
        },
        None,
    )?;
    Ok(stream)
}

fn capture_thread(
    device_name: Option<String>,
    vad: Option<Box<dyn VoiceActivityDetector>>,
    callbacks: RecordingCallbacks,
    commands: mpsc::Receiver<Command>,
    ready: mpsc::Sender<anyhow::Result<()>>,
    first_audio: Arc<OnceLock<Instant>>,
) -> anyhow::Result<RecordingOutput> {
    let setup = (|| {
        let host = cpal::default_host();
        let device = find_device(&host, device_name.as_deref())?;
        let supported = device.default_input_config()?;
        let config: cpal::StreamConfig = supported.config();
        // Two seconds of headroom between the callback and this thread.
        let (producer, consumer) = rtrb::RingBuffer::new(config.sample_rate.0 as usize * 2);
        let overflowed = Arc::new(AtomicU64::new(0));
        let failed = Arc::new(AtomicBool::new(false));
        let (o, f) = (overflowed.clone(), failed.clone());
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build_stream::<f32>(&device, &config, producer, o, f),
            SampleFormat::I16 => build_stream::<i16>(&device, &config, producer, o, f),
            SampleFormat::I32 => build_stream::<i32>(&device, &config, producer, o, f),
            SampleFormat::U16 => build_stream::<u16>(&device, &config, producer, o, f),
            SampleFormat::U8 => build_stream::<u8>(&device, &config, producer, o, f),
            SampleFormat::I8 => build_stream::<i8>(&device, &config, producer, o, f),
            SampleFormat::F64 => build_stream::<f64>(&device, &config, producer, o, f),
            other => Err(anyhow::anyhow!(
                "unsupported microphone sample format {other}"
            )),
        }?;
        stream.play()?;
        log::info!(
            "Microphone '{}' open at {} Hz, {} channel(s)",
            device.name().unwrap_or_default(),
            config.sample_rate.0,
            config.channels
        );
        anyhow::Ok((stream, consumer, config.sample_rate.0, overflowed, failed))
    })();

    let (stream, mut consumer, rate, overflowed, failed) = match setup {
        Ok(parts) => {
            let _ = ready.send(Ok(()));
            parts
        }
        Err(e) => {
            let _ = ready.send(Err(anyhow::anyhow!("could not open microphone: {e}")));
            return Err(anyhow::anyhow!("microphone setup failed"));
        }
    };

    let mut resampler = StreamResampler::new(rate)?;
    let RecordingCallbacks {
        mut on_limit,
        mut on_segment,
        mut on_preview,
    } = callbacks;
    let mut last_preview = Instant::now();
    let mut last_preview_len = 0usize;
    let mut buffer = RecordingBuffer::new(vad);
    if on_segment.is_some() {
        buffer = buffer.with_segmentation(SegmentConfig::default());
    }
    let limit_samples = echolocal_core::audio::ms_to_samples(MAX_RECORDING_MS);
    let mut raw = Vec::with_capacity(rate as usize / 10);
    let mut converted = Vec::with_capacity(1600);

    let mut drain = |consumer: &mut rtrb::Consumer<f32>,
                     buffer: &mut RecordingBuffer,
                     resampler: &mut StreamResampler|
     -> anyhow::Result<()> {
        raw.clear();
        while let Ok(sample) = consumer.pop() {
            raw.push(sample);
        }
        if raw.is_empty() {
            return Ok(());
        }
        first_audio.get_or_init(Instant::now);
        converted.clear();
        resampler.process(&raw, &mut converted)?;
        let room = limit_samples.saturating_sub(buffer.len());
        buffer.push(&converted[..converted.len().min(room)]);
        if let Some(callback) = on_segment.as_mut() {
            while let Some(segment) = buffer.next_segment() {
                last_preview_len = 0;
                callback(segment);
            }
        }
        if let Some(callback) = on_preview.as_mut() {
            if last_preview.elapsed() >= PREVIEW_INTERVAL {
                let pending = buffer.pending_speech();
                // Skip when nothing new was said since the last preview.
                if pending.len() != last_preview_len && pending.len() >= 4_800 {
                    last_preview = Instant::now();
                    last_preview_len = pending.len();
                    callback(pending);
                }
            }
        }
        if converted.len() >= room {
            if let Some(callback) = on_limit.take() {
                log::warn!("Recording reached the {} s limit", MAX_RECORDING_MS / 1000);
                callback();
            }
        }
        Ok(())
    };

    let keep = loop {
        match commands.recv_timeout(DRAIN_INTERVAL) {
            Ok(Command::Stop) => break true,
            Ok(Command::Cancel) | Err(mpsc::RecvTimeoutError::Disconnected) => break false,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        drain(&mut consumer, &mut buffer, &mut resampler)?;
    };

    // Close the microphone first so the recording indicator goes away
    // immediately, then flush what is left.
    drop(stream);
    drain(&mut consumer, &mut buffer, &mut resampler)?;

    let dropped = overflowed.load(Ordering::Relaxed);
    if dropped > 0 {
        log::warn!("Dropped {dropped} samples: capture thread fell behind");
    }
    if failed.load(Ordering::Relaxed) {
        log::warn!("The microphone reported an error during recording");
    }

    if keep && buffer.len() < limit_samples {
        let mut tail = Vec::new();
        resampler.finish(&mut tail)?;
        let room = limit_samples - buffer.len();
        buffer.push(&tail[..tail.len().min(room)]);
    }
    let (audio, vad) = buffer.into_parts();
    let audio = if keep {
        audio
    } else {
        RecordedAudio::default()
    };
    Ok(RecordingOutput {
        audio,
        vad,
        device_sample_rate: rate,
    })
}
