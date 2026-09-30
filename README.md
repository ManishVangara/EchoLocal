# EchoLocal

Local-first push-to-talk dictation for macOS on Apple Silicon.

**Hold a shortcut → speak → release → the text appears** in whatever app you
were typing in. Speech recognition runs on your Mac with NVIDIA Parakeet;
no account, no cloud audio, no subscription.

- **Two models, nothing else:** Parakeet TDT v2 (*For English*) and
  Parakeet TDT v3 (*For multilingual speech*).
- **Batch, not streaming:** audio is buffered in memory while you hold the key
  and transcribed in one pass on release. Parakeet is fast enough that this
  feels instant, and it is far simpler and more reliable than streaming.
- **Long dictations stay fast:** past ~8 seconds, finished parts are transcribed
  at natural pauses while you keep talking, so release only waits for the last bit.
- **Stays out of the way:** a menu-bar icon, a small "Listening" pill while
  you talk, and a settings window you rarely open.
- **Optional AI cleanup:** *Clean* or *Rewrite* the transcript with any
  OpenAI-compatible server (Ollama, LM Studio, a hosted API). Only the
  transcript text is sent, never audio. Off by default.

## Getting started (development)

Requirements: macOS 13+ on Apple Silicon, Xcode Command Line Tools, CMake,
[Rust](https://rustup.rs) (stable) and [Bun](https://bun.sh).

```bash
bun install
./scripts/fetch-resources.sh      # Silero VAD model (optional; falls back to an energy VAD)
bun run tauri dev                 # if CMake complains: CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev
```

On first launch the settings window opens:

1. **Download a speech model** (v2 ≈ 730 MB, v3 ≈ 740 MB; the exact size of
   the file you download is shown).
2. **Allow Accessibility access** — needed to type into other apps.
   macOS asks for **Microphone** access the first time you dictate.
3. Hold **⌥ Space** (changeable), speak, release. **Esc** cancels.

Build a signed-off-locally app bundle with `bun run tauri build`.

> During development macOS attributes permissions to the binary that asked, so
> after a rebuild you may need to toggle EchoLocal off and on again under
> *System Settings → Privacy & Security → Accessibility*.

## Benchmarking Parakeet on your Mac

The most important number is **release-to-text latency**. Every dictation logs
it (with inference time, real-time factor and insertion method) to the app log
(`~/Library/Logs/app.echolocal.desktop/`). To measure the models directly:

```bash
cargo run --release -p echolocal-engine --bin echolocal-bench -- download v2
cargo run --release -p echolocal-engine --bin echolocal-bench -- run v2 sample.wav --runs 10
```

It reports load time, first-run and median inference time, RTF, speed vs. real
time, compute device (Metal/CPU) and peak memory.

## How it works

```
Hold hotkey ─► capture focused app/field ─► open mic ─► buffer 16 kHz audio (VAD flags per 30 ms)
Release     ─► trim silence ─► Parakeet (resident in memory) ─► [optional AI cleanup] ─► insert text
```

Text insertion tries, in order: Accessibility (native text fields in apps
where it is reliable) → Unicode key events posted to the target process →
clipboard + layout-aware ⌘V (clipboard restored afterwards) → leave the text on
the clipboard and say so. Where the field can be read back, EchoLocal checks
that the text actually landed before moving to the next method.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full design.

## Repository layout

| Path | What |
| --- | --- |
| `crates/echolocal-core` | Pure logic: model catalog, settings, dictation state machine, VAD trimming, resampling, insertion policy, AI prompts, metrics. Fully unit-tested. |
| `crates/echolocal-engine` | `TranscriptionEngine` over transcribe.cpp, verified/resumable model downloads, `echolocal-bench`. |
| `crates/echolocal-audio` | Microphone capture (cpal) into an in-memory buffer; Silero VAD. |
| `crates/echolocal-macos` | Focus capture, Accessibility, key events, keyboard layouts, clipboard. |
| `src-tauri` | The Tauri 2 app: coordinator, hotkeys, tray, overlay, IPC commands. |
| `src` | React/TypeScript UI: settings window and overlay pill. |

## Tests

```bash
cargo test -p echolocal-core -p echolocal-engine -p echolocal-macos
bun run typecheck
```

## Roadmap

- [x] Phase 1–3: hotkey, in-memory recording, Parakeet v2/v3, end-to-end dictation
- [x] Phase 4: macOS text insertion with fallbacks (needs broad real-app testing:
      ChatGPT, Cursor, VS Code, Terminal, Mail, Slack, browsers)
- [x] Phase 5: minimal listening / transcribing / inserted feedback
- [x] Phase 6: Raw / Clean / Rewrite post-processing
- [ ] Phase 7: per-app profiles (e.g. Terminal → Raw, Mail → Clean)
- [x] Free model memory when idle (default 15 min), reloading while you speak
- [x] Background transcription of long dictations, cut at pauses
- [ ] Modifier-only shortcuts (e.g. hold Right ⌥), API key in Keychain,
      receipt-based clipboard restore

## License

MIT — see [LICENSE](LICENSE) and [NOTICE.md](NOTICE.md) for credits (the
architecture follows [Handy](https://github.com/cjpais/Handy); FluidVoice was
used only as a behavioral reference).
