//! The small status pill shown near the bottom of the screen while dictating.
//!
//! The window is configured non-focusable, so showing it never takes focus
//! away from the app the user is dictating into.

use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};

const LABEL: &str = "overlay";
const BOTTOM_MARGIN: f64 = 96.0;

/// Incremented on every change so a delayed hide never hides a newer state.
static GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Listening,
    Transcribing,
    Polishing,
    Inserted,
    Notice,
    Error,
}

#[derive(Clone, Serialize)]
struct Payload {
    kind: Kind,
    message: String,
}

fn position(app: &AppHandle, window: &tauri::WebviewWindow) {
    let cursor = app.cursor_position().ok();
    let monitor = cursor
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let (Some(monitor), Ok(size)) = (monitor, window.outer_size()) else {
        return;
    };
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    let x = area.position.x as f64 + (area.size.width as f64 - size.width as f64) / 2.0;
    let y = area.position.y as f64 + area.size.height as f64
        - size.height as f64
        - BOTTOM_MARGIN * scale;
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

/// Show the overlay in the given state until changed or hidden.
pub fn show(app: &AppHandle, kind: Kind, message: impl Into<String>) -> u64 {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let payload = Payload {
        kind,
        message: message.into(),
    };
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.emit("overlay", payload);
        if !window.is_visible().unwrap_or(false) {
            position(app, &window);
            let _ = window.show();
        }
    }
    generation
}

/// Show a state briefly, then hide.
pub fn flash(app: &AppHandle, kind: Kind, message: impl Into<String>, duration: Duration) {
    let generation = show(app, kind, message);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(duration);
        if GENERATION.load(Ordering::SeqCst) == generation {
            hide(&app);
        }
    });
}

pub fn hide(app: &AppHandle) {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}
