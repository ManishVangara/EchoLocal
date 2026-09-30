//! Global push-to-talk shortcut, plus Escape to cancel while dictating.
//!
//! Escape is registered only while a recording is active, so EchoLocal never
//! swallows it from other apps the rest of the time.

use crate::dictation::Dictation;
use echolocal_core::dictation::Trigger;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Manager, Runtime, Wry};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState,
};

#[derive(Default)]
pub struct HotkeyState {
    dictation: Mutex<Option<Shortcut>>,
    cancel_enabled: AtomicBool,
}

fn escape() -> Shortcut {
    Shortcut::new(None, Code::Escape)
}

pub fn plugin() -> TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(handle)
        .build()
}

fn handle(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    let Some(hotkeys) = app.try_state::<HotkeyState>() else {
        return;
    };
    let Some(dictation) = app.try_state::<Dictation>() else {
        return;
    };
    let is_dictation = crate::state::lock(&hotkeys.dictation).as_ref() == Some(shortcut);
    if is_dictation {
        let trigger = match event.state() {
            ShortcutState::Pressed => Trigger::HotkeyPressed,
            ShortcutState::Released => Trigger::HotkeyReleased,
        };
        dictation.trigger(app, trigger);
    } else if *shortcut == escape() && event.state() == ShortcutState::Pressed {
        dictation.trigger(app, Trigger::Cancel);
    }
}

/// Check that `accelerator` parses and has a non-modifier key.
pub fn parse(accelerator: &str) -> Result<Shortcut, String> {
    let shortcut: Shortcut = accelerator
        .parse()
        .map_err(|e| format!("“{accelerator}” isn't a valid shortcut: {e}"))?;
    if shortcut == escape() {
        return Err("Escape is reserved for cancelling".into());
    }
    Ok(shortcut)
}

/// Register the push-to-talk shortcut, replacing the previous one. If the new
/// shortcut can't be registered (e.g. another app owns it), the previous one
/// is restored.
pub fn register_dictation<R: Runtime>(app: &AppHandle<R>, accelerator: &str) -> Result<(), String> {
    let hotkeys = app.state::<HotkeyState>();
    let shortcut = parse(accelerator)?;
    let mut current = crate::state::lock(&hotkeys.dictation);
    if current.as_ref() == Some(&shortcut) && app.global_shortcut().is_registered(shortcut) {
        return Ok(());
    }
    let previous = current.take();
    if let Some(previous) = previous {
        let _ = app.global_shortcut().unregister(previous);
    }
    match app.global_shortcut().register(shortcut) {
        Ok(()) => {
            log::info!("Push-to-talk shortcut: {accelerator}");
            *current = Some(shortcut);
            Ok(())
        }
        Err(e) => {
            if let Some(previous) = previous {
                if app.global_shortcut().register(previous).is_ok() {
                    *current = Some(previous);
                }
            }
            Err(format!(
                "Couldn't use {accelerator}: it may be taken by another app ({e})"
            ))
        }
    }
}

/// Enable or disable Escape-to-cancel.
pub fn set_cancel_enabled(app: &AppHandle, enabled: bool) {
    let hotkeys = app.state::<HotkeyState>();
    if hotkeys.cancel_enabled.swap(enabled, Ordering::SeqCst) == enabled {
        return;
    }
    let app = app.clone();
    // Hotkey registration talks to the OS event system; do it on the main thread.
    let _ = app.clone().run_on_main_thread(move || {
        let result = if enabled {
            app.global_shortcut().register(escape())
        } else {
            app.global_shortcut().unregister(escape())
        };
        if let Err(e) = result {
            log::warn!("Escape shortcut: {e}");
        }
    });
}
