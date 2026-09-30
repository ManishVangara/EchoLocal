//! The floating overlay shown while dictating: live waveform, transcript,
//! the app the text will go into, and quick controls.
//!
//! On macOS the window is an `NSPanel` (via `tauri-nspanel`, as in Handy and
//! FluidVoice): non-activating so it never takes focus from the app being
//! dictated into, at status-bar level, on every Space and over full-screen
//! apps. Like FluidVoice it is never ordered out: when "hidden" it is made
//! transparent, click-through and parked off-screen, so showing it again is
//! a position + alpha change with no window-server round trip. All window
//! operations run on the main thread, as AppKit requires.

use crate::state::AppState;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use echolocal_core::{OverlayPosition, OverlaySize, PostProcessing};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl};

pub const LABEL: &str = "overlay";
const PARKED: f64 = -30_000.0;

/// Incremented on every change so a delayed hide never hides a newer state.
static GENERATION: AtomicU64 = AtomicU64::new(0);
/// Size currently applied to the window, to resize only when it changes.
static APPLIED_SIZE: Mutex<Option<OverlaySize>> = Mutex::new(None);
/// Icons as data URLs, by bundle id (looked up once per app).
static ICONS: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);

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

/// What the overlay needs to render its chrome for one dictation.
#[derive(Clone, Serialize)]
pub struct Context {
    pub size: OverlaySize,
    pub mode: PostProcessing,
    /// Whether an AI server is configured (otherwise only Raw is offered).
    pub ai_configured: bool,
    pub app_name: Option<String>,
    /// `data:image/png;base64,…`
    pub app_icon: Option<String>,
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

fn settings(app: &AppHandle) -> echolocal_core::Settings {
    app.state::<AppState>().settings()
}

/// Create the overlay window, parked and invisible. Call once during setup.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let size = settings(app).overlay_size;
    let (width, height) = size.window_size();
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::{CollectionBehavior, PanelBuilder, PanelLevel, StyleMask};
        let panel = PanelBuilder::<_, OverlayPanel>::new(app, LABEL)
            .url(WebviewUrl::App("overlay.html".into()))
            .title("EchoLocal")
            .size(tauri::Size::Logical(LogicalSize::new(width, height)))
            .position(tauri::Position::Logical(LogicalPosition::new(
                PARKED, PARKED,
            )))
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
        panel.set_alpha_value(0.0);
        panel.order_front_regardless();
    }
    #[cfg(not(target_os = "macos"))]
    {
        tauri::WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("overlay.html".into()))
            .inner_size(width, height)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .visible(false)
            .build()?;
    }
    *crate::state::lock(&APPLIED_SIZE) = Some(size);
    Ok(())
}

/// Where the overlay goes on the screen the mouse is on. Main thread only.
fn place(app: &AppHandle, window: &tauri::WebviewWindow, size: OverlaySize) {
    let s = settings(app);
    let (width, height) = size.window_size();
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
    let offset = s.overlay_offset.min(400) as f64;
    let x = ax + (aw - width) / 2.0;
    let y = match s.overlay_position {
        OverlayPosition::Bottom => ay + ah - height - offset,
        // The work area starts below the menu bar (and notch).
        OverlayPosition::Top => ay + offset.min(ah - height),
    };
    let _ = window.set_position(LogicalPosition::new(x, y));
}

fn show_on_main(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    let size = settings(app).overlay_size;
    {
        let mut applied = crate::state::lock(&APPLIED_SIZE);
        if *applied != Some(size) {
            let (w, h) = size.window_size();
            let _ = window.set_size(LogicalSize::new(w, h));
            *applied = Some(size);
        }
    }
    place(app, &window, size);
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::ManagerExt;
        if let Ok(panel) = app.get_webview_panel(LABEL) {
            // Card sizes have buttons; the pill lets clicks through.
            panel.set_ignores_mouse_events(!size.is_interactive());
            panel.set_alpha_value(1.0);
            panel.order_front_regardless();
            return;
        }
    }
    let _ = window.show();
}

fn hide_on_main(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::ManagerExt;
        if let Ok(panel) = app.get_webview_panel(LABEL) {
            panel.set_alpha_value(0.0);
            panel.set_ignores_mouse_events(true);
            if let Some(window) = app.get_webview_window(LABEL) {
                let _ = window.set_position(LogicalPosition::new(PARKED, PARKED));
            }
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

/// Update the live transcript (empty clears it).
pub fn set_text(app: &AppHandle, text: &str) {
    let _ = app.emit_to(LABEL, "overlay-text", text);
}

/// Update the microphone level meter (0.0–1.0).
pub fn set_level(app: &AppHandle, level: f32) {
    let _ = app.emit_to(LABEL, "overlay-level", level);
}

/// Send the chrome for a new dictation. The target app's icon is looked up
/// on a background thread and sent when ready, so it never delays recording.
pub fn set_context(app: &AppHandle, target: &echolocal_macos::FocusTarget) {
    let s = settings(app);
    let (pid, app_name) = (target.pid, target.app_name.clone());
    let key = target
        .info
        .bundle_id
        .clone()
        .unwrap_or_else(|| pid.to_string());
    let cached = crate::state::lock(&ICONS)
        .as_ref()
        .and_then(|icons| icons.get(&key).cloned());
    let context = Context {
        size: s.overlay_size,
        mode: s.post_processing,
        ai_configured: s.ai.is_configured(),
        app_name,
        app_icon: cached.clone().flatten(),
    };
    let _ = app.emit_to(LABEL, "overlay-context", context.clone());
    if cached.is_some() || pid <= 0 {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let icon = echolocal_macos::app_icon_png(pid, 64)
            .map(|png| format!("data:image/png;base64,{}", BASE64.encode(png)));
        crate::state::lock(&ICONS)
            .get_or_insert_with(HashMap::new)
            .insert(key, icon.clone());
        if icon.is_some() {
            let _ = app.emit_to(
                LABEL,
                "overlay-context",
                Context {
                    app_icon: icon,
                    ..context
                },
            );
        }
    });
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
    set_level(app, 0.0);
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || hide_on_main(&handle));
}
