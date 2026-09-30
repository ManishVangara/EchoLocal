# EchoLocal architecture

EchoLocal keeps three problems separate:

| Problem | Handled by | Code |
| --- | --- | --- |
| **Hearing** | microphone, VAD, Parakeet | `echolocal-audio`, `echolocal-engine` |
| **Writing** (optional) | AI cleanup / rewrite of the transcript | `echolocal-core::ai`, `src-tauri/src/ai.rs` |
| **Controlling the computer** | focus, Accessibility, key events, clipboard | `echolocal-macos` |

`echolocal-core` holds every decision that can be made without hardware so it
is unit-tested; the other crates are thin adapters around OS and native APIs.

## Dictation lifecycle

```
Idle ─press─► Preparing ─mic open─► Recording ─release─► Transcribing ─► (PostProcessing) ─► Inserting ─► Idle
                  │                    │                       │                 │
                  └──── Esc: discard ──┘                       └─ Esc: abort ────┘
```

- `echolocal_core::dictation::decide(phase, trigger)` is the whole rulebook:
  presses outside `Idle` (key auto-repeat, pressing while still inserting) are
  ignored; releases stop only an active recording; cancel discards a recording
  or aborts inference/post-processing but never interrupts insertion.
- The hotkey handler evaluates `decide` **at the moment of the event** and
  claims `Idle → Preparing` atomically, then hands work to a single worker
  thread (`src-tauri/src/dictation.rs`) that executes steps in order.
  Cancelling inference sets a shared flag that is bridged to transcribe.cpp's
  abort callback.
- Recordings shorter than 200 ms are treated as accidental taps; recordings
  stop automatically at 5 minutes.

## Hearing

1. **Capture** (`echolocal-audio/src/recorder.rs`): a dedicated thread owns
   the cpal stream. The real-time callback only downmixes to mono and pushes
   into a lock-free ring buffer (no allocation, no locks).
2. Every 10 ms the capture thread drains the ring, **resamples to 16 kHz**
   incrementally (`echolocal-core::resample`, rubato FFT) and appends to a
   `RecordingBuffer`.
3. The buffer classifies each 30 ms frame with the **VAD** (Silero v4 through
   ONNX Runtime, or a built-in energy detector) and stores one flag per frame.
   Nothing is discarded during capture.
4. On release, the mic closes first, the tail is flushed, and
   `speech_range()` trims leading/trailing silence from the stored flags in
   O(frames) — VAD adds no release-to-text latency. A run must be ≥ 2 speech
   frames to count (ignores clicks); 300 ms of pre-roll and 450 ms of tail are
   kept. If VAD fails at any point it fails open (keeps all audio). If VAD
   finds no speech at all, inference is skipped.
5. **Inference** (`echolocal-engine/src/engine.rs`): `TranscriptionEngine` is
   the only interface the app sees. The transcribe.cpp implementation loads the
   GGUF once (Metal on Apple Silicon via `Backend::Auto`, CPU fallback) and
   keeps the session resident; switching models drops the old one first so two
   ~700 MB models never share RAM.

### Models

`echolocal-core::catalog` pins exactly two artifacts (Hugging Face repo,
revision, filename, **exact byte size**, SHA-256). The UI shows the size of the
actual file downloaded. Downloads stream to `<file>.part` while hashing,
resume with HTTP `Range`, and are renamed into place only after the size and
hash match. Both models currently use the Q8_0 quantization; changing that is a
catalog edit.

## Controlling the computer (macOS)

**Before any UI appears**, the worker captures the target: the focused
Accessibility element, its PID and bundle id, its role, whether its selected
text is settable, and whether it is a secure (password) field. The overlay
window is non-focusable, so showing it does not move focus.

After transcription, `insert_text`:

1. waits (≤ 600 ms) for physical modifier keys to be released, so a still-held
   hotkey modifier can't turn "a" into "å";
2. re-activates the target app if the user switched away;
3. snapshots the focused field (value, UTF-16 length, selection) when readable
   and adds a leading space if the caret follows a word;
4. walks the plan from `echolocal_core::insertion::insertion_plan`:
   - **Accessibility** — set `AXSelectedText`. Only for native text roles in
     apps not on the "AX unreliable" list (terminals, browsers, Electron and
     web-view apps), where AX writes can succeed without the app noticing.
   - **Key events** — Unicode `CGEvent`s (≤ 20 UTF-16 units each, never
     splitting surrogate pairs) posted to the target PID. Layout-independent;
     works in terminals and Electron apps.
   - **Clipboard paste** — save every pasteboard item/representation, set the
     transcript (marked transient for clipboard managers), send ⌘ + the key
     code that types "v" *in the current layout* (resolved with
     `UCKeyTranslate`, ⌘-aware for "Dvorak – QWERTY ⌘"), then restore the
     original clipboard unless something else changed it meanwhile.
   - **Clipboard only** — leave the text on the clipboard and tell the user.
5. After each method it re-reads the field (polling briefly, since apps process
   events asynchronously). It moves to the next method **only** when the same
   element is focused and nothing observable changed — the one case where a
   retry cannot duplicate text. If the field can't be read, the attempt is
   trusted.

Secure fields only ever get key events (never AX writes, clipboard or
read-back). Without Accessibility permission, macOS silently drops synthesized
input, so EchoLocal copies the text to the clipboard and points the user to the
permission instead.

## Writing (optional AI)

`Off` inserts the raw transcript. `Clean` and `Rewrite` send **only the
transcript text** to an OpenAI-compatible `/chat/completions` endpoint
configured by the user (default: local Ollama). Prompts wrap the transcript in
`<transcript>` tags and instruct the model to treat it as data. Replies are
sanitized (reasoning blocks, code fences, wrapping quotes) and rejected if
empty or implausibly long; any failure inserts the raw transcript and says so.
Cancel works during the request.

## Metrics

Each dictation logs `DictationMetrics`: mic start latency, recorded vs. trimmed
audio, inference time and RTF, post-processing time, insertion time and method,
and **release-to-text** latency. `echolocal-bench` measures model load,
first-run and median inference, RTF and peak RSS for a WAV file.

## Deliberately not in v1

Streaming transcription, live transcript overlays, other STT engines, cloud
speech, accounts/sync, transcript history, Windows/Linux, per-app profiles
(designed for: the target's bundle id is already captured per dictation).
