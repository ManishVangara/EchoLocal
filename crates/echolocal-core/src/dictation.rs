//! The dictation lifecycle as a pure state machine.
//!
//! ```text
//! Idle → Preparing → Recording → Transcribing → (PostProcessing) → Inserting → Idle
//! ```
//!
//! The runtime (in the app crate) owns side effects; this module only decides
//! what a hotkey press, release or cancel means in the current phase, so the
//! rules are testable without a microphone or a model.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Idle,
    /// Hotkey is down; the target app is captured and the microphone is opening.
    Preparing,
    /// Microphone is open and audio is being buffered in memory.
    Recording,
    /// Buffered audio is being run through Parakeet.
    Transcribing,
    /// Optional AI cleanup or rewrite of the transcript.
    PostProcessing,
    /// Text is being delivered to the target application.
    Inserting,
}

impl Phase {
    pub fn is_idle(self) -> bool {
        self == Phase::Idle
    }

    /// Whether audio capture is (or is about to be) active.
    pub fn is_capturing(self) -> bool {
        matches!(self, Phase::Preparing | Phase::Recording)
    }
}

/// An input to the state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    HotkeyPressed,
    HotkeyReleased,
    Cancel,
}

/// What the runtime should do in response to a [`Trigger`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    StartRecording,
    StopAndTranscribe,
    /// Discard the recording; nothing is transcribed or inserted.
    DiscardRecording,
    /// Abort transcription or post-processing; nothing is inserted.
    AbortProcessing,
    Ignore,
}

pub fn decide(phase: Phase, trigger: Trigger) -> Decision {
    use Decision::*;
    use Phase::*;
    use Trigger::*;
    match (phase, trigger) {
        (Idle, HotkeyPressed) => StartRecording,
        // Key auto-repeat, or a press while the previous dictation is still
        // finishing: never start a second concurrent session.
        (_, HotkeyPressed) => Ignore,

        (Preparing | Recording, HotkeyReleased) => StopAndTranscribe,
        (_, HotkeyReleased) => Ignore,

        (Preparing | Recording, Cancel) => DiscardRecording,
        (Transcribing | PostProcessing, Cancel) => AbortProcessing,
        // Insertion is short and not safely interruptible (half-typed text).
        (Inserting | Idle, Cancel) => Ignore,
    }
}

/// Recordings shorter than this are treated as accidental taps.
pub const MIN_RECORDING_MS: u64 = 200;

/// Recordings are stopped automatically after this long.
pub const MAX_RECORDING_MS: u64 = 5 * 60 * 1000;

#[cfg(test)]
mod tests {
    use super::*;
    use Decision::*;
    use Phase::*;
    use Trigger::*;

    #[test]
    fn happy_path() {
        assert_eq!(decide(Idle, HotkeyPressed), StartRecording);
        assert_eq!(decide(Recording, HotkeyReleased), StopAndTranscribe);
    }

    #[test]
    fn release_during_preparation_still_transcribes() {
        assert_eq!(decide(Preparing, HotkeyReleased), StopAndTranscribe);
    }

    #[test]
    fn presses_outside_idle_are_ignored() {
        for phase in [
            Preparing,
            Recording,
            Transcribing,
            PostProcessing,
            Inserting,
        ] {
            assert_eq!(decide(phase, HotkeyPressed), Ignore, "{phase:?}");
        }
    }

    #[test]
    fn stray_releases_are_ignored() {
        for phase in [Idle, Transcribing, PostProcessing, Inserting] {
            assert_eq!(decide(phase, HotkeyReleased), Ignore, "{phase:?}");
        }
    }

    #[test]
    fn cancellation_rules() {
        assert_eq!(decide(Idle, Cancel), Ignore);
        assert_eq!(decide(Preparing, Cancel), DiscardRecording);
        assert_eq!(decide(Recording, Cancel), DiscardRecording);
        assert_eq!(decide(Transcribing, Cancel), AbortProcessing);
        assert_eq!(decide(PostProcessing, Cancel), AbortProcessing);
        assert_eq!(decide(Inserting, Cancel), Ignore);
    }

    #[test]
    fn serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&PostProcessing).unwrap(),
            "\"post_processing\""
        );
    }
}
