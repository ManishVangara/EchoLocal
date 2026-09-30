//! Benchmark Parakeet on this machine.
//!
//! ```text
//! echolocal-bench download <v2|v3> [--dir DIR]
//! echolocal-bench run <v2|v3> <audio.wav> [--dir DIR] [--runs N]
//! ```
//!
//! Reports model load time, per-run inference latency, real-time factor and
//! peak memory — the numbers behind EchoLocal's release-to-text latency.

use echolocal_core::{catalog::format_size, ModelId};
use echolocal_engine::{engine, models::ModelStore, wav};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

fn usage() -> ! {
    eprintln!(
        "usage:\n  echolocal-bench download <v2|v3> [--dir DIR]\n  echolocal-bench run <v2|v3> <audio.wav> [--dir DIR] [--runs N]"
    );
    std::process::exit(2)
}

fn parse_model(arg: Option<&String>) -> ModelId {
    match arg.map(String::as_str) {
        Some("v2") => ModelId::ParakeetTdtV2,
        Some("v3") => ModelId::ParakeetTdtV3,
        _ => usage(),
    }
}

fn default_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cache/echolocal-bench")
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = flag(&args, "--dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_dir);
    let store = ModelStore::new(&dir);

    match args.first().map(String::as_str) {
        Some("download") => {
            let id = parse_model(args.get(1));
            let spec = id.spec();
            println!(
                "Downloading {} ({}) to {}",
                spec.display_name,
                format_size(spec.size_bytes),
                dir.display()
            );
            let mut last_pct = u64::MAX;
            let path = store.download(id, &AtomicBool::new(false), |have, total| {
                let pct = have * 100 / total.max(1);
                if pct != last_pct {
                    last_pct = pct;
                    eprint!("\r{pct:3}%");
                }
            })?;
            eprintln!();
            println!("Verified {}", path.display());
        }
        Some("run") => {
            let id = parse_model(args.get(1));
            let wav_path = args.get(2).unwrap_or_else(|| usage());
            let runs: usize = flag(&args, "--runs")
                .and_then(|r| r.parse().ok())
                .unwrap_or(5);
            anyhow::ensure!(
                store.is_downloaded(id),
                "model not found in {}; run `echolocal-bench download {}` first",
                dir.display(),
                args[1]
            );

            let pcm = wav::read_16k_mono(std::path::Path::new(wav_path))?;
            let audio_ms = echolocal_core::audio::samples_to_ms(pcm.len());
            engine::init_backend();

            let rss_before = peak_rss_mb();
            let load_start = Instant::now();
            let mut engine = engine::load_parakeet(id, &store.path(id))?;
            let load_ms = load_start.elapsed().as_millis();
            println!("model       {}", id.spec().display_name);
            println!("device      {}", engine.device());
            println!("audio       {:.2} s", audio_ms as f64 / 1000.0);
            println!("load        {load_ms} ms");

            let cancel = engine::CancelFlag::default();
            let mut times = Vec::with_capacity(runs);
            let mut text = String::new();
            // One warm-up run: the first inference pays one-time allocations.
            let warm = engine.transcribe(&pcm, &cancel)?;
            println!("first run   {} ms", warm.inference_ms);
            for _ in 0..runs {
                let t = engine.transcribe(&pcm, &cancel)?;
                times.push(t.inference_ms);
                text = t.text;
            }
            times.sort_unstable();
            let median = times[times.len() / 2];
            println!(
                "median      {median} ms over {runs} runs (min {}, max {})",
                times[0],
                times[times.len() - 1]
            );
            println!("RTF         {:.4}", median as f64 / audio_ms.max(1) as f64);
            println!(
                "speed       {:.0}x real time",
                audio_ms as f64 / median.max(1) as f64
            );
            if let (Some(before), Some(after)) = (rss_before, peak_rss_mb()) {
                println!(
                    "peak RSS    {after:.0} MB (model ≈ {:.0} MB)",
                    after - before
                );
            }
            println!("transcript  {text}");
        }
        _ => usage(),
    }
    Ok(())
}

/// Peak resident set size of this process in MB.
fn peak_rss_mb() -> Option<f64> {
    #[cfg(unix)]
    {
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
            return None;
        }
        let max = usage.ru_maxrss as f64;
        // macOS reports bytes, Linux kilobytes.
        let bytes = if cfg!(target_os = "macos") {
            max
        } else {
            max * 1024.0
        };
        Some(bytes / 1_048_576.0)
    }
    #[cfg(not(unix))]
    {
        None
    }
}
