//! IPC commands used by the settings window.

use crate::state::{self, AppState, EngineStatus, ModelView};
use echolocal_core::dictation::Phase;
use echolocal_core::{AiSettings, ModelId, PostProcessing, Settings};
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
    shortcut_error: Option<String>,
}

/// The last shortcut registration error, shown in settings until resolved.
#[derive(Default)]
pub struct ShortcutError(pub std::sync::Mutex<Option<String>>);

fn snapshot(app: &AppHandle) -> Snapshot {
    let state = app.state::<AppState>();
    Snapshot {
        settings: state.settings(),
        models: state::model_views(app),
        engine: state.engine_status(),
        phase: state.phase(),
        accessibility: echolocal_macos::accessibility_trusted(),
        shortcut_error: state::lock(&app.state::<ShortcutError>().0).clone(),
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
        crate::hotkey::register_dictation(&app, &settings.shortcut)?;
        *state::lock(&app.state::<ShortcutError>().0) = None;
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
