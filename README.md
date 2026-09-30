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
- **See it as you speak:** a small floating pill shows that EchoLocal is
  listening, with a live transcript of what it hears so far.
- **Any shortcut:** hold a single key like Right ⌥ or fn, or use a combination
  such as ⌃⌥Space.
- **Stays out of the way:** lives in the menu bar; a settings window you rarely
  open.
- **Optional AI cleanup:** *Clean* or *Rewrite* the transcript with any
  OpenAI-compatible server (Ollama, LM Studio, a hosted API). Only the
  transcript text is sent, never audio. Off by default.

## Install (use it every day)

Requirements: macOS 13+ on Apple Silicon, Xcode Command Line Tools, CMake,
[Rust](https://rustup.rs) and [Bun](https://bun.sh).

```bash
git clone https://github.com/ManishVangara/EchoLocal.git && cd EchoLocal
./scripts/install-mac.sh
```

This builds `EchoLocal.app`, installs it into `/Applications` and launches it.
A short setup walks you through microphone and Accessibility permission, the
speech model download (v2 ≈ 730 MB, v3 ≈ 740 MB) and your shortcut. Then hold
**Right ⌥** (changeable), speak, and release. **Esc** cancels.

See [docs/DISTRIBUTION.md](docs/DISTRIBUTION.md) for DMG releases, signing and
notarization.

## Development

```bash
bun install
./scripts/fetch-resources.sh      # Silero VAD model (optional; falls back to an energy VAD)
bun run tauri dev                 # with CMake 4: export CMAKE_POLICY_VERSION_MINIMUM=3.5
```

> During development macOS attributes permissions to the terminal that
> launched the app, so you may need to allow Terminal (or iTerm) under
> Privacy & Security → Accessibility.

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
- [x] Live transcript while speaking, modifier-only shortcuts (hold Right ⌥)
- [x] Standalone app install, release workflow (see docs/DISTRIBUTION.md)
- [ ] Settings on the overlay itself, API key in Keychain, receipt-based
      clipboard restore, auto-updates

## License

MIT — see [LICENSE](LICENSE) and [NOTICE.md](NOTICE.md) for credits (the
architecture follows [Handy](https://github.com/cjpais/Handy); FluidVoice was
used only as a behavioral reference).
