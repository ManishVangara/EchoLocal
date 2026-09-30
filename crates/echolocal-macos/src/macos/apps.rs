//! Running-application queries.

use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};
use std::time::{Duration, Instant};

pub fn bundle_id(pid: i32) -> Option<String> {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
        .and_then(|app| app.bundleIdentifier())
        .map(|id| id.to_string())
}

pub fn app_name(pid: i32) -> Option<String> {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
        .and_then(|app| app.localizedName())
        .map(|name| name.to_string())
}

pub fn frontmost_pid() -> Option<i32> {
    NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|app| app.processIdentifier())
}

/// Bring `pid` to the front if it isn't already, waiting up to `timeout`.
/// Returns whether it is frontmost afterwards.
pub fn activate(pid: i32, timeout: Duration) -> bool {
    if frontmost_pid() == Some(pid) {
        return true;
    }
    let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) else {
        return false;
    };
    #[allow(deprecated)]
    app.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if frontmost_pid() == Some(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}
