//! Shared application state and the model service.

use echolocal_core::audio::VoiceActivityDetector;
use echolocal_core::catalog::format_size;
use echolocal_core::dictation::Phase;
use echolocal_core::{ModelId, Settings};
use echolocal_engine::{CancelFlag, DownloadError, ModelStore, TranscriptionEngine};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager};

/// Emitted whenever something the settings window shows has changed; the
/// window then re-fetches [`AppSnapshot`].
pub const STATE_CHANGED_EVENT: &str = "state-changed";
pub const DOWNLOAD_PROGRESS_EVENT: &str = "download-progress";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EngineStatus {
    NoModel,
    Loading { model: ModelId },
    Ready { model: ModelId },
    Failed { model: ModelId, error: String },
}

pub struct AppState {
    pub settings_path: PathBuf,
    settings: Mutex<Settings>,
    pub store: ModelStore,
    engine: Mutex<Option<Box<dyn TranscriptionEngine>>>,
    engine_status: Mutex<EngineStatus>,
    pub cancel: CancelFlag,
    phase: Mutex<Phase>,
    downloads: Mutex<HashMap<ModelId, Arc<AtomicBool>>>,
    download_progress: Mutex<HashMap<ModelId, (u64, u64)>>,
    /// Reused across recordings (Silero takes a moment to load).
    pub vad: Mutex<Option<Box<dyn VoiceActivityDetector>>>,
    pub silero_model: Option<PathBuf>,
}

/// Lock a mutex, recovering from poisoning (a panicked holder must not take
/// dictation down with it).
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    pub fn new(settings_path: PathBuf, models_dir: PathBuf, silero_model: Option<PathBuf>) -> Self {
        let settings = Settings::load(&settings_path);
        Self {
            settings_path,
            settings: Mutex::new(settings),
            store: ModelStore::new(models_dir),
            engine: Mutex::new(None),
            engine_status: Mutex::new(EngineStatus::NoModel),
            cancel: CancelFlag::default(),
            phase: Mutex::new(Phase::Idle),
            downloads: Mutex::new(HashMap::new()),
            download_progress: Mutex::new(HashMap::new()),
            vad: Mutex::new(None),
            silero_model,
        }
    }

    pub fn settings(&self) -> Settings {
        lock(&self.settings).clone()
    }

    pub fn save_settings(&self, settings: Settings) -> anyhow::Result<()> {
        settings.save(&self.settings_path)?;
        *lock(&self.settings) = settings;
        Ok(())
    }

    pub fn phase(&self) -> Phase {
        *lock(&self.phase)
    }

    pub fn set_phase(&self, phase: Phase) {
        *lock(&self.phase) = phase;
    }

    /// Atomically move from `from` to `to`; false if the phase was different.
    pub fn transition(&self, from: Phase, to: Phase) -> bool {
        let mut phase = lock(&self.phase);
        if *phase == from {
            *phase = to;
            true
        } else {
            false
        }
    }

    pub fn engine_status(&self) -> EngineStatus {
        lock(&self.engine_status).clone()
    }

    pub fn engine(&self) -> MutexGuard<'_, Option<Box<dyn TranscriptionEngine>>> {
        lock(&self.engine)
    }

    pub fn is_downloading(&self, id: ModelId) -> bool {
        lock(&self.downloads).contains_key(&id)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelView {
    pub id: ModelId,
    pub name: &'static str,
    pub subtitle: &'static str,
    pub size_label: String,
    pub downloaded: bool,
    pub downloading: bool,
    /// Bytes downloaded so far (including an interrupted partial download).
    pub progress_bytes: u64,
    pub total_bytes: u64,
    pub selected: bool,
}

pub fn model_views(app: &AppHandle) -> Vec<ModelView> {
    let state = app.state::<AppState>();
    let selected = state.settings().model;
    let progress = lock(&state.download_progress).clone();
    ModelId::ALL
        .into_iter()
        .map(|id| {
            let spec = id.spec();
            let downloading = state.is_downloading(id);
            let progress_bytes = progress
                .get(&id)
                .map(|(have, _)| *have)
                .unwrap_or_else(|| state.store.partial_bytes(id));
            ModelView {
                id,
                name: spec.display_name,
                subtitle: spec.subtitle,
                size_label: format_size(spec.size_bytes),
                downloaded: state.store.is_downloaded(id),
                downloading,
                progress_bytes,
                total_bytes: spec.size_bytes,
                selected: id == selected,
            }
        })
        .collect()
}

pub fn notify_changed(app: &AppHandle) {
    let _ = app.emit(STATE_CHANGED_EVENT, ());
}

fn set_engine_status(app: &AppHandle, status: EngineStatus) {
    let state = app.state::<AppState>();
    *lock(&state.engine_status) = status;
    notify_changed(app);
}

/// Load the selected model in the background, replacing any loaded model.
pub fn load_selected_model(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(e) = load_selected_model_now(&app) {
            log::error!("Model load failed: {e:#}");
        }
    });
}

/// Make sure the selected model is loaded, loading it on this thread if not.
/// The previous model is dropped first so two models never share RAM, and the
/// engine lock is held throughout, so a dictation that starts meanwhile waits
/// for the new model instead of failing.
pub fn load_selected_model_now(app: &AppHandle) -> anyhow::Result<()> {
    let state = app.state::<AppState>();
    let model = state.settings().model;
    let mut engine = state.engine();
    if engine.as_ref().is_some_and(|e| e.model_id() == model) {
        return Ok(());
    }
    *engine = None;
    if !state.store.is_downloaded(model) {
        drop(engine);
        set_engine_status(app, EngineStatus::NoModel);
        anyhow::bail!("{} is not downloaded", model.spec().display_name);
    }
    set_engine_status(app, EngineStatus::Loading { model });
    match echolocal_engine::load_parakeet(model, &state.store.path(model)) {
        Ok(loaded) => {
            *engine = Some(loaded);
            drop(engine);
            set_engine_status(app, EngineStatus::Ready { model });
            Ok(())
        }
        Err(e) => {
            drop(engine);
            set_engine_status(
                app,
                EngineStatus::Failed {
                    model,
                    error: e.to_string(),
                },
            );
            Err(e)
        }
    }
}

#[derive(Clone, Serialize)]
struct ProgressPayload {
    id: ModelId,
    downloaded: u64,
    total: u64,
}

pub fn start_download(app: &AppHandle, id: ModelId) {
    let state = app.state::<AppState>();
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut downloads = lock(&state.downloads);
        if downloads.contains_key(&id) || state.store.is_downloaded(id) {
            return;
        }
        downloads.insert(id, cancel.clone());
    }
    notify_changed(app);

    let app = app.clone();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let mut last_emit = std::time::Instant::now() - std::time::Duration::from_secs(1);
        let result = state.store.download(id, &cancel, |downloaded, total| {
            lock(&state.download_progress).insert(id, (downloaded, total));
            if last_emit.elapsed().as_millis() >= 150 || downloaded == total {
                last_emit = std::time::Instant::now();
                let _ = app.emit(
                    DOWNLOAD_PROGRESS_EVENT,
                    ProgressPayload {
                        id,
                        downloaded,
                        total,
                    },
                );
            }
        });
        lock(&state.downloads).remove(&id);
        lock(&state.download_progress).remove(&id);
        match result {
            Ok(path) => {
                log::info!("Downloaded {}", path.display());
                if state.settings().model == id {
                    load_selected_model(&app);
                }
            }
            Err(DownloadError::Cancelled) => log::info!("Download of {id} cancelled"),
            Err(e) => {
                log::error!("Download of {id} failed: {e}");
                let _ = app.emit(
                    "download-failed",
                    serde_json::json!({ "id": id, "error": e.to_string() }),
                );
            }
        }
        notify_changed(&app);
    });
}

pub fn cancel_download(app: &AppHandle, id: ModelId) {
    let state = app.state::<AppState>();
    let flag = lock(&state.downloads).get(&id).cloned();
    if let Some(flag) = flag {
        flag.store(true, Ordering::Relaxed);
    }
}

pub fn delete_model(app: &AppHandle, id: ModelId) -> anyhow::Result<()> {
    let state = app.state::<AppState>();
    anyhow::ensure!(!state.is_downloading(id), "cancel the download first");
    if state.settings().model == id {
        let mut engine = state.engine();
        *engine = None;
        drop(engine);
        set_engine_status(app, EngineStatus::NoModel);
    }
    state.store.delete(id)?;
    notify_changed(app);
    Ok(())
}
