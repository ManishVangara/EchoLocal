//! The status pill near the bottom of the screen while dictating, with the
//! live transcript while recording.
//!
//! On macOS the window is an `NSPanel` (via `tauri-nspanel`, as in Handy):
//! non-activating so it never takes focus from the app being dictated into,
//! at status-bar level so it floats above other windows, and allowed on every
//! Space and over full-screen apps. It's shown with `orderFrontRegardless`,
//! which works while EchoLocal (a menu-bar app) is in the background. All
//! window operations run on the main thread, as AppKit requires.

use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl};

pub const LABEL: &str = "overlay";
/// Transparent window; the pill is drawn inside it, anchored to the bottom.
const WIDTH: f64 = 560.0;
const HEIGHT: f64 = 150.0;
const BOTTOM_MARGIN: f64 = 72.0;

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

#[cfg(target_os = "macos")]
tauri_nspanel::tauri_panel! {
    panel!(OverlayPanel {
        config: {
            can_become_key_window: false,
            is_floating_panel: true
        }
    })
}

/// Create the (hidden) overlay window. Call once during setup.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::{CollectionBehavior, PanelBuilder, PanelLevel, StyleMask};
        let panel = PanelBuilder::<_, OverlayPanel>::new(app, LABEL)
            .url(WebviewUrl::App("overlay.html".into()))
            .title("EchoLocal")
            .size(tauri::Size::Logical(LogicalSize::new(WIDTH, HEIGHT)))
            .level(PanelLevel::Status)
            .has_shadow(false)
            .transparent(true)
            .no_activate(true)
            .ignores_mouse_events(true)
            .hides_on_deactivate(false)
            .style_mask(StyleMask::empty().borderless().nonactivating_panel())
            .with_window(|w| {
                w.decorations(false)
                    .transparent(true)
                    .focusable(false)
                    .skip_taskbar(true)
            })
            .collection_behavior(
                CollectionBehavior::new()
                    .can_join_all_spaces()
                    .full_screen_auxiliary()
                    .stationary(),
            )
            .build()?;
        panel.hide();
    }
    #[cfg(not(target_os = "macos"))]
    {
        tauri::WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("overlay.html".into()))
            .inner_size(WIDTH, HEIGHT)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .visible(false)
            .build()?;
    }
    Ok(())
}

/// Bottom-centre of the screen the mouse is on. Main thread only.
fn place(app: &AppHandle, window: &tauri::WebviewWindow) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let (ax, ay) = (
        area.position.x as f64 / scale,
        area.position.y as f64 / scale,
    );
    let (aw, ah) = (
        area.size.width as f64 / scale,
        area.size.height as f64 / scale,
    );
    let x = ax + (aw - WIDTH) / 2.0;
    let y = ay + ah - HEIGHT - BOTTOM_MARGIN;
    let _ = window.set_position(LogicalPosition::new(x, y));
}

fn show_on_main(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::ManagerExt;
        if let Ok(panel) = app.get_webview_panel(LABEL) {
            if !panel.is_visible() {
                place(app, &window);
                panel.show();
            }
            return;
        }
    }
    if !window.is_visible().unwrap_or(false) {
        place(app, &window);
        let _ = window.show();
    }
}

fn hide_on_main(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::ManagerExt;
        if let Ok(panel) = app.get_webview_panel(LABEL) {
            panel.hide();
            return;
        }
    }
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

/// Show the overlay in the given state until changed or hidden.
pub fn show(app: &AppHandle, kind: Kind, message: impl Into<String>) -> u64 {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = app.emit_to(
        LABEL,
        "overlay",
        Payload {
            kind,
            message: message.into(),
        },
    );
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || show_on_main(&handle));
    generation
}

/// Update the live transcript shown under the status (empty clears it).
pub fn set_text(app: &AppHandle, text: &str) {
    let _ = app.emit_to(LABEL, "overlay-text", text);
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
    set_text(app, "");
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || hide_on_main(&handle));
}
