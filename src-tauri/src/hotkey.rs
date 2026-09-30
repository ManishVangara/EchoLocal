//! Global push-to-talk shortcut (via `handy-keys`), Escape to cancel, and
//! recording a new shortcut from the settings window.
//!
//! One thread owns the `HotkeyManager`. It needs Accessibility permission, so
//! until that is granted the thread keeps retrying and the settings window
//! shows the shortcut as waiting for permission. The permission is re-checked
//! every second: if it is revoked the listener is dropped, and a new one is
//! created as soon as it is granted again.
//!
//! Key-combination shortcuts (⌃⌥Space) are *blocked* from reaching other
//! apps. Modifier-only shortcuts (hold Right ⌥) are not — blocking a modifier
//! would break typing characters like ⌥E.

use crate::dictation::Dictation;
use echolocal_core::dictation::Trigger;
use echolocal_core::shortcut::{self, CaptureStep, ShortcutCapture};
use handy_keys::{Hotkey, HotkeyId, HotkeyManager, HotkeyState, Key, KeyboardListener, Modifiers};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

// `echolocal_core::shortcut::mods` mirrors handy-keys' bit layout.
const _: () = assert!(Modifiers::OPT_RIGHT.bits() == shortcut::mods::OPT_RIGHT);
const _: () = assert!(Modifiers::CMD_LEFT.bits() == shortcut::mods::CMD_LEFT);
const _: () = assert!(Modifiers::FN.bits() == shortcut::mods::FN);
const _: () = assert!(Modifiers::CTRL_RIGHT.bits() == shortcut::mods::CTRL_RIGHT);

const POLL: Duration = Duration::from_millis(4);
const PERMISSION_RETRY: Duration = Duration::from_secs(1);

/// Whether the push-to-talk shortcut is live, for the settings window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", content = "detail", rename_all = "snake_case")]
pub enum HotkeyStatus {
    Active,
    NeedsAccessibility,
    Error(String),
}

enum Command {
    SetShortcut(String, Sender<Result<(), String>>),
    SetCancel(bool),
    /// Pause/resume the dictation shortcut while a new one is being recorded.
    Pause(bool),
}

pub struct Hotkeys {
    commands: Mutex<Sender<Command>>,
    status: Arc<Mutex<HotkeyStatus>>,
    capturing: Arc<AtomicBool>,
}

impl Hotkeys {
    pub fn start(app: &AppHandle, shortcut: String) -> Self {
        let (tx, rx) = mpsc::channel();
        let status = Arc::new(Mutex::new(HotkeyStatus::NeedsAccessibility));
        let worker = Worker {
            app: app.clone(),
            shortcut,
            manager: None,
            dictation_id: None,
            escape_id: None,
            cancel_enabled: false,
            paused: false,
            status: status.clone(),
        };
        std::thread::Builder::new()
            .name("echolocal-hotkeys".into())
            .spawn(move || worker.run(rx))
            .expect("failed to start hotkey thread");
        Self {
            commands: Mutex::new(tx),
            status,
            capturing: Arc::new(AtomicBool::new(false)),
        }
    }

    fn send(&self, command: Command) {
        let _ = crate::state::lock(&self.commands).send(command);
    }

    pub fn status(&self) -> HotkeyStatus {
        crate::state::lock(&self.status).clone()
    }

    /// Switch the dictation shortcut. Fails (keeping the old one) if the
    /// string isn't a usable shortcut.
    pub fn set_shortcut(&self, shortcut: &str) -> Result<(), String> {
        validate(shortcut)?;
        let (tx, rx) = mpsc::channel();
        self.send(Command::SetShortcut(shortcut.to_string(), tx));
        rx.recv_timeout(Duration::from_secs(3))
            .unwrap_or_else(|_| Err("The shortcut service didn't respond".into()))
    }

    pub fn set_cancel_enabled(&self, enabled: bool) {
        self.send(Command::SetCancel(enabled));
    }
}

/// Parse and sanity-check a shortcut string.
pub fn validate(shortcut: &str) -> Result<Hotkey, String> {
    let hotkey: Hotkey = shortcut
        .parse()
        .map_err(|e| format!("“{shortcut}” isn't a valid shortcut ({e})"))?;
    let key_name = hotkey.key.map(|k| k.to_string());
    shortcut::check_combo(hotkey.modifiers.bits(), key_name.as_deref()).map_err(str::to_string)?;
    if hotkey.key == Some(Key::Escape) && hotkey.modifiers.is_empty() {
        return Err("Escape is reserved for cancelling".into());
    }
    Ok(hotkey)
}

struct Worker {
    app: AppHandle,
    shortcut: String,
    manager: Option<(HotkeyManager, bool)>, // (manager, blocking)
    dictation_id: Option<HotkeyId>,
    escape_id: Option<HotkeyId>,
    cancel_enabled: bool,
    paused: bool,
    status: Arc<Mutex<HotkeyStatus>>,
}

impl Worker {
    fn set_status(&self, status: HotkeyStatus) {
        let changed = {
            let mut current = crate::state::lock(&self.status);
            let changed = *current != status;
            *current = status;
            changed
        };
        if changed {
            crate::state::notify_changed(&self.app);
        }
    }

    fn run(mut self, rx: Receiver<Command>) {
        let mut next_attempt = Instant::now();
        loop {
            if Instant::now() >= next_attempt {
                next_attempt = Instant::now() + PERMISSION_RETRY;
                if self.manager.is_none() {
                    self.ensure_manager();
                } else if !echolocal_macos::accessibility_trusted() {
                    // Permission was turned off: macOS disables the event tap
                    // and doesn't restore it when permission comes back, so
                    // drop the listener and build a fresh one once granted.
                    log::warn!("Accessibility permission was revoked; shortcut paused");
                    self.manager = None;
                    self.dictation_id = None;
                    self.escape_id = None;
                    self.set_status(HotkeyStatus::NeedsAccessibility);
                }
            }
            // Deliver hotkey events.
            let mut events = Vec::new();
            if let Some((manager, _)) = &self.manager {
                while let Some(event) = manager.try_recv() {
                    events.push(event);
                }
            }
            for event in events {
                self.dispatch(event.id, event.state);
            }
            match rx.recv_timeout(POLL) {
                Ok(command) => self.handle(command),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    fn dispatch(&self, id: HotkeyId, state: HotkeyState) {
        let Some(dictation) = self.app.try_state::<Dictation>() else {
            return;
        };
        if Some(id) == self.dictation_id {
            let trigger = match state {
                HotkeyState::Pressed => Trigger::HotkeyPressed,
                HotkeyState::Released => Trigger::HotkeyReleased,
            };
            dictation.trigger(&self.app, trigger);
        } else if Some(id) == self.escape_id && state == HotkeyState::Pressed {
            dictation.trigger(&self.app, Trigger::Cancel);
        }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::SetShortcut(shortcut, reply) => {
                let previous = std::mem::replace(&mut self.shortcut, shortcut);
                let result = self.rebuild();
                if let Err(e) = &result {
                    log::error!("Shortcut change failed: {e}");
                    self.shortcut = previous;
                    let _ = self.rebuild();
                }
                let _ = reply.send(result);
            }
            Command::SetCancel(enabled) => {
                self.cancel_enabled = enabled;
                self.sync_escape();
            }
            Command::Pause(paused) => {
                self.paused = paused;
                let _ = self.sync_dictation();
            }
        }
    }

    /// Create the manager (blocking or not, depending on the shortcut) and
    /// register everything. Needs Accessibility permission.
    fn ensure_manager(&mut self) {
        if !echolocal_macos::accessibility_trusted() {
            self.set_status(HotkeyStatus::NeedsAccessibility);
            return;
        }
        let blocking = !shortcut::is_modifier_only(&self.shortcut);
        let created = if blocking {
            HotkeyManager::new_with_blocking()
        } else {
            HotkeyManager::new()
        };
        match created {
            Ok(manager) => {
                self.manager = Some((manager, blocking));
                self.dictation_id = None;
                self.escape_id = None;
                match self.sync_dictation() {
                    Ok(()) => {
                        log::info!("Push-to-talk shortcut: {}", self.shortcut);
                        self.set_status(HotkeyStatus::Active);
                    }
                    Err(e) => self.set_status(HotkeyStatus::Error(e)),
                }
                self.sync_escape();
            }
            Err(e) => {
                log::warn!("Hotkey listener unavailable: {e}");
                self.set_status(HotkeyStatus::NeedsAccessibility);
            }
        }
    }

    /// Re-create the manager if the blocking mode must change, then
    /// re-register the dictation shortcut.
    fn rebuild(&mut self) -> Result<(), String> {
        let blocking = !shortcut::is_modifier_only(&self.shortcut);
        if self.manager.as_ref().is_some_and(|(_, b)| *b != blocking) {
            self.manager = None;
        }
        if self.manager.is_none() {
            self.ensure_manager();
            return match &*crate::state::lock(&self.status) {
                HotkeyStatus::Error(e) => Err(e.clone()),
                // Saved; it becomes active once permission is granted.
                _ => Ok(()),
            };
        }
        self.sync_dictation()
    }

    fn sync_dictation(&mut self) -> Result<(), String> {
        let Some((manager, _)) = &self.manager else {
            return Ok(());
        };
        if let Some(id) = self.dictation_id.take() {
            let _ = manager.unregister(id);
        }
        if self.paused {
            return Ok(());
        }
        let hotkey = validate(&self.shortcut)?;
        let id = manager.register(hotkey).map_err(|e| e.to_string())?;
        self.dictation_id = Some(id);
        Ok(())
    }

    fn sync_escape(&mut self) {
        let Some((manager, _)) = &self.manager else {
            return;
        };
        match (self.cancel_enabled, self.escape_id) {
            (true, None) => match Hotkey::new(Modifiers::empty(), Key::Escape) {
                Ok(hotkey) => self.escape_id = manager.register(hotkey).ok(),
                Err(e) => log::warn!("Escape shortcut: {e}"),
            },
            (false, Some(id)) => {
                let _ = manager.unregister(id);
                self.escape_id = None;
            }
            _ => {}
        }
    }
}

/// Live state of shortcut recording, sent to the settings window.
#[derive(Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CaptureEvent {
    Pending { label: String },
    Done { shortcut: String, label: String },
    Invalid { message: String },
    Cancelled,
}

pub const CAPTURE_EVENT: &str = "shortcut-capture";

/// Start recording a new shortcut: the next key combination (or a single
/// modifier pressed and released) is reported through [`CAPTURE_EVENT`].
/// The current shortcut is paused meanwhile so pressing it doesn't dictate.
pub fn start_capture(app: &AppHandle) -> Result<(), String> {
    let hotkeys = app.state::<Hotkeys>();
    if hotkeys.capturing.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let listener = match KeyboardListener::new() {
        Ok(listener) => listener,
        Err(e) => {
            hotkeys.capturing.store(false, Ordering::SeqCst);
            return Err(if echolocal_macos::accessibility_trusted() {
                format!("Couldn't listen to the keyboard: {e}")
            } else {
                "Allow Accessibility access first".into()
            });
        }
    };
    hotkeys.send(Command::Pause(true));
    let app = app.clone();
    std::thread::spawn(move || {
        let hotkeys = app.state::<Hotkeys>();
        let mut capture = ShortcutCapture::new();
        let emit = |event: CaptureEvent| {
            let _ = app.emit(CAPTURE_EVENT, event);
        };
        while hotkeys.capturing.load(Ordering::SeqCst) {
            let Ok(event) = listener.recv_timeout(Duration::from_millis(50)) else {
                continue;
            };
            let key_name = event.key.map(|k| k.to_string());
            let step = capture.on_event(
                event.modifiers.bits(),
                event.key,
                event.is_key_down,
                event.key == Some(Key::Escape),
            );
            match step {
                CaptureStep::Pending { modifiers } => {
                    let label = Hotkey::new(Modifiers::from_bits_truncate(modifiers), None)
                        .map(|h| shortcut::shortcut_label(&h.to_string()))
                        .unwrap_or_default();
                    emit(CaptureEvent::Pending { label });
                }
                CaptureStep::Cancelled => {
                    emit(CaptureEvent::Cancelled);
                    break;
                }
                CaptureStep::Done { modifiers, key } => {
                    let key_name = key.map(|_| key_name.clone().unwrap_or_default());
                    if let Err(message) = shortcut::check_combo(modifiers, key_name.as_deref()) {
                        emit(CaptureEvent::Invalid {
                            message: message.to_string(),
                        });
                        continue;
                    }
                    match Hotkey::new(Modifiers::from_bits_truncate(modifiers), key) {
                        Ok(hotkey) => {
                            let shortcut = hotkey.to_string();
                            let label = shortcut::shortcut_label(&shortcut);
                            emit(CaptureEvent::Done { shortcut, label });
                            break;
                        }
                        Err(e) => emit(CaptureEvent::Invalid {
                            message: e.to_string(),
                        }),
                    }
                }
            }
        }
        hotkeys.capturing.store(false, Ordering::SeqCst);
        hotkeys.send(Command::Pause(false));
    });
    Ok(())
}

/// Stop recording without choosing a shortcut.
pub fn stop_capture(app: &AppHandle) {
    app.state::<Hotkeys>()
        .capturing
        .store(false, Ordering::SeqCst);
}
