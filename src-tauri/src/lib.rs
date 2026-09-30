//! EchoLocal: hold a shortcut, speak, release — the text appears where you
//! were typing. Speech recognition runs locally with NVIDIA Parakeet.
//!
//! Hearing (microphone, VAD, Parakeet), writing (optional AI cleanup) and
//! controlling the computer (focus, Accessibility, keyboard, clipboard) are
//! kept in separate crates; this crate wires them into a menu-bar app.

mod ai;
mod background;
mod commands;
mod dictation;
mod hotkey;
mod overlay;
mod preview;
mod state;
mod tray;

use state::AppState;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

pub fn show_settings(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.show();
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // A menu-bar utility: no Dock icon, no app switcher entry.
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);

    let handle = app.handle().clone();
    let settings_path = app.path().app_config_dir()?.join("settings.json");
    let models_dir = app.path().app_data_dir()?.join("models");
    let silero = app
        .path()
        .resolve(
            "resources/models/silero_vad_v4.onnx",
            BaseDirectory::Resource,
        )
        .ok();
    log::info!(
        "Settings: {}; models: {}",
        settings_path.display(),
        models_dir.display()
    );

    echolocal_engine::engine::init_backend();
    app.manage(AppState::new(settings_path, models_dir, silero));
    let settings = handle.state::<AppState>().settings();
    app.manage(dictation::Dictation::start(&handle));
    // Starts listening once Accessibility is granted (retries until then).
    app.manage(hotkey::Hotkeys::start(&handle, settings.shortcut.clone()));
    overlay::create(&handle)?;
    tray::create(&handle)?;
    echolocal_macos::refresh_keyboard_layout();
    let state = handle.state::<AppState>();

    // Keep the selected model resident: repeat dictations skip loading.
    state::load_selected_model(&handle);
    state::start_idle_unloader(&handle);

    // Warm the VAD in the background so the first recording doesn't pay for it.
    let vad_handle = handle.clone();
    std::thread::spawn(move || {
        let state = vad_handle.state::<AppState>();
        let vad = echolocal_audio::create_vad(state.silero_model.as_deref());
        state::lock(&state.vad).get_or_insert(vad);
    });

    // First run (or missing permissions): open settings so the user can
    // download a model and grant access.
    if !state.store.is_downloaded(settings.model)
        || !echolocal_macos::accessibility_trusted()
        || echolocal_macos::microphone_permission() != echolocal_macos::Permission::Granted
    {
        show_settings(&handle);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn tauri_nspanel_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_nspanel::init()
}

#[cfg(not(target_os = "macos"))]
fn tauri_nspanel_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("nspanel-noop").build()
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_settings(app)
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_nspanel_plugin())
        .setup(setup)
        .on_window_event(|window, event| {
            // Closing settings hides it; EchoLocal keeps running in the menu bar.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "settings" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::list_microphones,
            commands::save_settings,
            commands::download_model,
            commands::cancel_download,
            commands::delete_model,
            commands::request_accessibility,
            commands::open_accessibility_settings,
            commands::open_microphone_settings,
            commands::request_microphone,
            commands::start_shortcut_capture,
            commands::stop_shortcut_capture,
            commands::overlay_set_mode,
            commands::overlay_cancel,
            commands::overlay_open_settings,
            commands::test_ai,
        ])
        .run(tauri::generate_context!())
        .expect("error while running EchoLocal");
}
