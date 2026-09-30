//! IPC commands used by the settings window.

use crate::hotkey::{self, HotkeyStatus, Hotkeys};
use crate::state::{self, AppState, EngineStatus, ModelView, SessionStats};
use echolocal_core::dictation::Phase;
use echolocal_core::shortcut::shortcut_label;
use echolocal_core::{AiSettings, ModelId, PostProcessing, Settings};
use echolocal_macos::Permission;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;

#[derive(Serialize)]
pub struct Snapshot {
    settings: Settings,
    models: Vec<ModelView>,
    engine: EngineStatus,
    phase: Phase,
    accessibility: bool,
    microphone: Permission,
    shortcut_label: String,
    hotkey: HotkeyStatus,
    stats: SessionStats,
    version: String,
}

fn snapshot(app: &AppHandle) -> Snapshot {
    let state = app.state::<AppState>();
    let settings = state.settings();
    let stats = state::lock(&state.stats).clone();
    Snapshot {
        shortcut_label: shortcut_label(&settings.shortcut),
        settings,
        models: state::model_views(app),
        engine: state.engine_status(),
        phase: state.phase(),
        accessibility: echolocal_macos::accessibility_trusted(),
        microphone: echolocal_macos::microphone_permission(),
        hotkey: app.state::<Hotkeys>().status(),
        stats,
        version: app.package_info().version.to_string(),
    }
}

#[tauri::command]
pub fn get_state(app: AppHandle) -> Snapshot {
    snapshot(&app)
}

#[tauri::command]
pub fn list_microphones() -> Vec<String> {
    echolocal_audio::list_input_devices()
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<Snapshot, String> {
    let previous = state.settings();
    let mut settings = settings;
    settings.shortcut = settings.shortcut.trim().to_string();

    if settings.shortcut != previous.shortcut {
        app.state::<Hotkeys>().set_shortcut(&settings.shortcut)?;
    }
    if settings.launch_at_login != previous.launch_at_login {
        let autolaunch = app.autolaunch();
        let result = if settings.launch_at_login {
            autolaunch.enable()
        } else {
            autolaunch.disable()
        };
        result.map_err(|e| format!("Couldn't change launch at login: {e}"))?;
    }
    state
        .save_settings(settings.clone())
        .map_err(|e| e.to_string())?;
    if settings.model != previous.model {
        state::load_selected_model(&app);
    }
    state::notify_changed(&app);
    Ok(snapshot(&app))
}

#[tauri::command]
pub fn download_model(app: AppHandle, id: ModelId) {
    state::start_download(&app, id);
}

#[tauri::command]
pub fn cancel_download(app: AppHandle, id: ModelId) {
    state::cancel_download(&app, id);
}

#[tauri::command]
pub fn delete_model(app: AppHandle, id: ModelId) -> Result<(), String> {
    state::delete_model(&app, id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn request_accessibility() -> bool {
    echolocal_macos::request_accessibility()
}

#[tauri::command]
pub fn open_accessibility_settings() {
    echolocal_macos::open_accessibility_settings();
}

/// Show the microphone permission prompt; the window refreshes when answered.
#[tauri::command]
pub fn request_microphone(app: AppHandle) {
    echolocal_macos::request_microphone(move |_| state::notify_changed(&app));
}

/// Begin recording a new push-to-talk shortcut (see [`hotkey::CAPTURE_EVENT`]).
#[tauri::command]
pub fn start_shortcut_capture(app: AppHandle) -> Result<(), String> {
    hotkey::start_capture(&app)
}

#[tauri::command]
pub fn stop_shortcut_capture(app: AppHandle) {
    hotkey::stop_capture(&app);
}

/// Switch Raw / Clean / Rewrite from the overlay. Applies to the dictation
/// in progress (post-processing reads the setting on release).
#[tauri::command]
pub fn overlay_set_mode(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: PostProcessing,
) -> Result<(), String> {
    let mut settings = state.settings();
    settings.post_processing = mode;
    state.save_settings(settings).map_err(|e| e.to_string())?;
    state::notify_changed(&app);
    Ok(())
}

/// The overlay's ✕ button: same as pressing Esc.
#[tauri::command]
pub fn overlay_cancel(app: AppHandle) {
    let dictation = app.state::<crate::dictation::Dictation>();
    dictation.trigger(&app, echolocal_core::dictation::Trigger::Cancel);
}

#[tauri::command]
pub fn overlay_open_settings(app: AppHandle) {
    crate::show_settings(&app);
}

#[tauri::command]
pub fn open_microphone_settings() {
    echolocal_macos::open_microphone_settings();
}

/// Try the AI settings on a sample sentence; returns the model's output.
#[tauri::command]
pub async fn test_ai(ai: AiSettings, mode: PostProcessing) -> Result<String, String> {
    if !ai.is_configured() {
        return Err("Enter a server URL and model name first".into());
    }
    let mode = if mode == PostProcessing::Off {
        PostProcessing::Clean
    } else {
        mode
    };
    tauri::async_runtime::spawn_blocking(move || {
        let sample =
            "um so i think we should uh meet on thursday instead of friday if that works for you";
        crate::ai::post_process(mode, sample, &ai, &echolocal_engine::CancelFlag::default())
            .map_err(|e| match e {
                crate::ai::AiError::Cancelled => "cancelled".to_string(),
                crate::ai::AiError::Failed(e) => e,
            })
    })
    .await
    .map_err(|e| e.to_string())?
}
