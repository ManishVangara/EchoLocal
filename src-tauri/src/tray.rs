//! The menu-bar icon and menu.

use echolocal_core::dictation::Phase;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Wry};

const IDLE_ICON: &[u8] = include_bytes!("../icons/tray.png");
const ACTIVE_ICON: &[u8] = include_bytes!("../icons/tray-recording.png");

pub struct Tray {
    status: MenuItem<Wry>,
    icon: TrayIcon<Wry>,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, "status", status_text(Phase::Idle), false, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
    let quit = MenuItem::with_id(app, "quit", "Quit EchoLocal", true, Some("CmdOrCtrl+Q"))?;
    let menu = Menu::with_items(
        app,
        &[
            &status,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    let icon = TrayIconBuilder::with_id("echolocal")
        .icon(Image::from_bytes(IDLE_ICON)?)
        .icon_as_template(true)
        .tooltip("EchoLocal")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "settings" => crate::show_settings(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    app.manage(Tray { status, icon });
    Ok(())
}

fn status_text(phase: Phase) -> &'static str {
    match phase {
        Phase::Idle => "Ready",
        Phase::Preparing | Phase::Recording => "Listening…",
        Phase::Transcribing => "Transcribing…",
        Phase::PostProcessing => "Polishing…",
        Phase::Inserting => "Inserting…",
    }
}

pub fn set_phase(app: &AppHandle, phase: Phase) {
    let Some(tray) = app.try_state::<Tray>() else {
        return;
    };
    let _ = tray.status.set_text(status_text(phase));
    let bytes = if phase.is_idle() {
        IDLE_ICON
    } else {
        ACTIVE_ICON
    };
    if let Ok(image) = Image::from_bytes(bytes) {
        let _ = tray.icon.set_icon(Some(image));
        let _ = tray.icon.set_icon_as_template(true);
    }
}
