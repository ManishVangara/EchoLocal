//! Controlling the computer: capturing the dictation target and delivering
//! text into it.
//!
//! On macOS this uses Accessibility, synthesized keyboard events and the
//! clipboard (see [`echolocal_core::insertion`] for the fallback policy). On
//! other platforms every call fails gracefully, so the app still builds.

use echolocal_core::insertion::{InsertionMethod, Landed, TargetInfo};
use std::time::Duration;

#[cfg(target_os = "macos")]
mod macos;
mod utf16;

/// Where the text should go: captured when the hotkey goes down, before any
/// EchoLocal UI appears.
#[derive(Debug, Clone)]
pub struct FocusTarget {
    pub pid: i32,
    pub app_name: Option<String>,
    pub info: TargetInfo,
    #[cfg(target_os = "macos")]
    native: macos::NativeTarget,
}

#[derive(Debug, Clone)]
pub struct InsertOptions {
    /// Insert a space when the caret follows a word or punctuation.
    pub smart_spacing: bool,
    /// How long after ⌘V before the previous clipboard is put back.
    pub clipboard_restore_delay: Duration,
}

impl Default for InsertOptions {
    fn default() -> Self {
        Self {
            smart_spacing: true,
            clipboard_restore_delay: Duration::from_millis(350),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InsertionReport {
    pub method: InsertionMethod,
    pub landed: Landed,
}

/// Whether EchoLocal may use Accessibility and post keyboard events.
pub fn accessibility_trusted() -> bool {
    #[cfg(target_os = "macos")]
    return macos::accessibility_trusted();
    #[cfg(not(target_os = "macos"))]
    false
}

/// Show the system Accessibility prompt if permission is missing.
pub fn request_accessibility() -> bool {
    #[cfg(target_os = "macos")]
    return macos::request_accessibility();
    #[cfg(not(target_os = "macos"))]
    false
}

/// Open System Settings at Privacy & Security → Accessibility.
pub fn open_accessibility_settings() {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn();
}

/// Open System Settings at Privacy & Security → Microphone.
pub fn open_microphone_settings() {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
        .spawn();
}

/// Capture the focused application and text element. Fast (a few AX calls)
/// and callable from any thread.
pub fn capture_focus_target() -> FocusTarget {
    #[cfg(target_os = "macos")]
    return macos::capture_focus_target();
    #[cfg(not(target_os = "macos"))]
    FocusTarget {
        pid: 0,
        app_name: None,
        info: TargetInfo::default(),
    }
}

/// Deliver `text` to `target`, trying each method of the insertion plan.
/// Blocks for up to about a second; call off the main thread.
pub fn insert_text(
    target: &FocusTarget,
    text: &str,
    options: &InsertOptions,
) -> anyhow::Result<InsertionReport> {
    #[cfg(target_os = "macos")]
    return macos::insert_text(target, text, options);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (target, text, options);
        anyhow::bail!("text insertion is only implemented on macOS")
    }
}

/// Put `text` on the clipboard (no restore).
pub fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    return macos::copy_to_clipboard(text);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = text;
        anyhow::bail!("clipboard access is only implemented on macOS")
    }
}

/// Refresh the cached keyboard layout information. Main thread only.
pub fn refresh_keyboard_layout() {
    #[cfg(target_os = "macos")]
    macos::keyboard::refresh();
}
