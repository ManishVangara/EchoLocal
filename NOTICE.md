# Third-party notices

## Handy (MIT)

EchoLocal's architecture follows [Handy](https://github.com/cjpais/Handy)
(Copyright (c) 2025 CJ Pais, MIT License): Tauri 2 + Rust, buffered recording
with batch Parakeet inference through transcribe.cpp, Silero VAD with
pre-roll/hangover padding, and a transcription coordinator. EchoLocal's code is
a fresh, smaller implementation; values such as the Silero threshold (0.3),
VAD padding durations and the Parakeet GGUF artifact catalog entries
(repository, revision, file sizes and SHA-256 hashes) were taken from Handy.

## FluidVoice (GPL-3.0) — behavioral reference only

[FluidVoice](https://github.com/altic-dev/FluidVoice) was studied as a
reference for *behavior* (focus capture before showing UI, PID-targeted
Unicode key events, layout-aware ⌘V, clipboard preservation, verifying that
text landed). No FluidVoice source code was copied; the implementation in
`crates/echolocal-macos` was written independently.

## Models

- NVIDIA Parakeet TDT 0.6B v2 / v3 — CC-BY-4.0. GGUF conversions published by
  the `handy-computer` organization on Hugging Face.
- Silero VAD v4 — MIT.

## Libraries

transcribe.cpp / transcribe-cpp (MIT), Tauri (MIT/Apache-2.0), cpal
(Apache-2.0), rubato (MIT), vad-rs (MIT), ONNX Runtime (MIT), React (MIT).
