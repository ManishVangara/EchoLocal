//! User settings, persisted as JSON in the app's config directory.
//!
//! Unknown or missing fields fall back to defaults so older settings files
//! keep loading after new fields are added.

use crate::catalog::ModelId;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// What happens to the transcript between speech recognition and insertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PostProcessing {
    /// Insert exactly what Parakeet heard.
    #[default]
    Off,
    /// Fix punctuation, casing and filler words; keep the wording.
    Clean,
    /// Rewrite according to the user's instruction.
    Rewrite,
}

/// When to free the speech model's memory (~0.7–1 GB) after the last use.
/// The model reloads while the next recording is in progress, so an unload
/// mostly costs nothing unless the next utterance is very short.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelUnload {
    Never,
    After5Minutes,
    #[default]
    After15Minutes,
    After1Hour,
}

impl ModelUnload {
    pub fn timeout(self) -> Option<std::time::Duration> {
        let minutes = match self {
            ModelUnload::Never => return None,
            ModelUnload::After5Minutes => 5,
            ModelUnload::After15Minutes => 15,
            ModelUnload::After1Hour => 60,
        };
        Some(std::time::Duration::from_secs(minutes * 60))
    }

    /// Whether a model idle for `idle` should be unloaded now.
    pub fn should_unload(self, idle: std::time::Duration) -> bool {
        self.timeout().is_some_and(|timeout| idle >= timeout)
    }
}

/// How much the floating overlay shows while dictating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlaySize {
    /// Just the waveform and status.
    Pill,
    /// Card with the live transcript, target app and controls.
    #[default]
    Card,
    /// A wider card with more transcript lines.
    Large,
}

impl OverlaySize {
    /// Window size in points, including a small margin for the shadow.
    pub fn window_size(self) -> (f64, f64) {
        match self {
            OverlaySize::Pill => (220.0, 64.0),
            OverlaySize::Card => (460.0, 156.0),
            OverlaySize::Large => (620.0, 204.0),
        }
    }

    /// Whether the overlay has clickable controls (and so accepts clicks).
    pub fn is_interactive(self) -> bool {
        self != OverlaySize::Pill
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayPosition {
    #[default]
    Bottom,
    /// Just below the menu bar (and the notch, on MacBooks that have one).
    Top,
}

/// Connection details for an OpenAI-compatible chat completions endpoint.
/// Works with local servers (Ollama, LM Studio, llama.cpp) and hosted APIs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiSettings {
    /// Base URL up to and including the version segment, e.g. `http://localhost:11434/v1`.
    pub base_url: String,
    /// Bearer token; empty for local servers that need none.
    pub api_key: String,
    pub model: String,
    /// Instruction used by [`PostProcessing::Rewrite`].
    pub rewrite_instruction: String,
}

pub const DEFAULT_REWRITE_INSTRUCTION: &str =
    "Rewrite this as a clear, concise message. Keep the original meaning and language.";

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434/v1".into(),
            api_key: String::new(),
            model: String::new(),
            rewrite_instruction: DEFAULT_REWRITE_INSTRUCTION.into(),
        }
    }
}

impl AiSettings {
    /// Whether enough is configured to attempt a request.
    pub fn is_configured(&self) -> bool {
        !self.base_url.trim().is_empty() && !self.model.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub model: ModelId,
    /// Push-to-talk shortcut (see [`crate::shortcut`]), e.g. `OptRight` to
    /// hold the right ⌥ key, or `Ctrl+Opt+Space`.
    pub shortcut: String,
    pub post_processing: PostProcessing,
    pub ai: AiSettings,
    pub launch_at_login: bool,
    /// Trim leading and trailing silence with voice activity detection.
    pub trim_silence: bool,
    /// Show what is being heard in the overlay while recording.
    pub live_preview: bool,
    pub overlay_size: OverlaySize,
    pub overlay_position: OverlayPosition,
    /// Distance from the bottom of the usable screen area (or the menu bar,
    /// for [`OverlayPosition::Top`]), in points.
    pub overlay_offset: u32,
    /// Free the model's memory after this much inactivity.
    pub unload_model: ModelUnload,
    /// Input device name; `None` uses the system default microphone.
    pub microphone: Option<String>,
}

pub const DEFAULT_OVERLAY_OFFSET: u32 = 48;

/// Hold the right ⌥ key. It types nothing on its own, so it doesn't collide
/// with other apps' shortcuts.
pub const DEFAULT_SHORTCUT: &str = "OptRight";

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: ModelId::default(),
            shortcut: DEFAULT_SHORTCUT.into(),
            post_processing: PostProcessing::Off,
            ai: AiSettings::default(),
            launch_at_login: false,
            trim_silence: true,
            live_preview: true,
            overlay_size: OverlaySize::default(),
            overlay_position: OverlayPosition::default(),
            overlay_offset: DEFAULT_OVERLAY_OFFSET,
            unload_model: ModelUnload::default(),
            microphone: None,
        }
    }
}

impl Settings {
    /// Load from `path`, returning defaults when the file is missing or invalid.
    /// An invalid file is left in place (not overwritten) until the next save.
    pub fn load(path: &Path) -> Settings {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                log::warn!("Ignoring unreadable settings at {}: {e}", path.display());
                Settings::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(e) => {
                log::warn!("Could not read settings at {}: {e}", path.display());
                Settings::default()
            }
        }
    }

    /// Atomically write to `path` (write to a sibling temp file, then rename).
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "echolocal-settings-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("settings.json")
    }

    #[test]
    fn missing_file_gives_defaults() {
        let path = temp_path("missing");
        assert_eq!(Settings::load(&path), Settings::default());
    }

    #[test]
    fn round_trips() {
        let path = temp_path("roundtrip");
        let mut s = Settings {
            model: ModelId::ParakeetTdtV3,
            post_processing: PostProcessing::Clean,
            ..Default::default()
        };
        s.ai.model = "llama3.2".into();
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
    }

    #[test]
    fn partial_file_fills_defaults() {
        let path = temp_path("partial");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"model":"parakeet-tdt-v3","ai":{"model":"x"}}"#).unwrap();
        let s = Settings::load(&path);
        assert_eq!(s.model, ModelId::ParakeetTdtV3);
        assert_eq!(s.shortcut, DEFAULT_SHORTCUT);
        assert_eq!(s.ai.model, "x");
        assert_eq!(s.ai.rewrite_instruction, DEFAULT_REWRITE_INSTRUCTION);
    }

    #[test]
    fn corrupt_file_gives_defaults() {
        let path = temp_path("corrupt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }

    #[test]
    fn overlay_defaults_and_sizes() {
        let s = Settings::default();
        assert_eq!(s.overlay_size, OverlaySize::Card);
        assert_eq!(s.overlay_position, OverlayPosition::Bottom);
        assert!(OverlaySize::Card.is_interactive());
        assert!(!OverlaySize::Pill.is_interactive());
        let (pw, _) = OverlaySize::Pill.window_size();
        let (cw, _) = OverlaySize::Card.window_size();
        let (lw, _) = OverlaySize::Large.window_size();
        assert!(pw < cw && cw < lw);
        let json = serde_json::to_string(&OverlaySize::Large).unwrap();
        assert_eq!(json, "\"large\"");
    }

    #[test]
    fn unload_policy() {
        use std::time::Duration;
        let min = |m: u64| Duration::from_secs(m * 60);
        assert!(!ModelUnload::Never.should_unload(min(10_000)));
        assert!(!ModelUnload::After15Minutes.should_unload(min(14)));
        assert!(ModelUnload::After15Minutes.should_unload(min(15)));
        assert!(ModelUnload::After5Minutes.should_unload(min(6)));
        assert!(!ModelUnload::After1Hour.should_unload(min(59)));
        assert_eq!(
            serde_json::to_string(&ModelUnload::After15Minutes).unwrap(),
            "\"after15_minutes\""
        );
    }

    #[test]
    fn ai_requires_url_and_model() {
        let mut ai = AiSettings::default();
        assert!(!ai.is_configured());
        ai.model = "llama3.2".into();
        assert!(ai.is_configured());
        ai.base_url = "  ".into();
        assert!(!ai.is_configured());
    }
}
