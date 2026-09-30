mod apps;
mod ax;
mod events;
mod ffi;
pub mod keyboard;
pub mod microphone;
mod pasteboard;

use crate::{FocusTarget, InsertOptions, InsertionReport};
use ax::AxElement;
use echolocal_core::insertion::{self, FieldSnapshot, InsertionMethod, Landed, TargetInfo};
use echolocal_core::text::{join_with_preceding, utf16_len};
use std::time::{Duration, Instant};

pub use apps::{activate, frontmost_pid};
pub use ax::{is_trusted as accessibility_trusted, request_trust as request_accessibility};

/// Platform half of [`FocusTarget`].
#[derive(Debug, Clone)]
pub struct NativeTarget {
    element: Option<AxElement>,
}

pub fn capture_focus_target() -> FocusTarget {
    let started = Instant::now();
    let element = if ax::is_trusted() {
        AxElement::focused()
    } else {
        None
    };
    let pid = element
        .as_ref()
        .and_then(AxElement::pid)
        .or_else(frontmost_pid)
        .unwrap_or(0);
    let info = TargetInfo {
        bundle_id: (pid > 0).then(|| apps::bundle_id(pid)).flatten(),
        role: element.as_ref().and_then(AxElement::role),
        selected_text_settable: element
            .as_ref()
            .is_some_and(|e| e.is_settable("AXSelectedText")),
        is_secure: element.as_ref().is_some_and(AxElement::is_secure),
    };
    let target = FocusTarget {
        pid,
        app_name: (pid > 0).then(|| apps::app_name(pid)).flatten(),
        info,
        native: NativeTarget { element },
    };
    log::debug!(
        "Captured target {:?} ({:?}, role {:?}) in {} ms",
        target.app_name,
        target.info.bundle_id,
        target.info.role,
        started.elapsed().as_millis()
    );
    target
}

/// Poll the focused element until `inserted` shows up or `timeout` passes.
/// Apps process synthesized events asynchronously, so "unchanged" is only
/// believed after giving them time.
fn await_landing(
    element: &AxElement,
    before: &FieldSnapshot,
    inserted_len: usize,
    timeout: Duration,
) -> Landed {
    let deadline = Instant::now() + timeout;
    loop {
        let verdict = insertion::verify(before, &element.snapshot(), inserted_len);
        if verdict != Landed::No || Instant::now() >= deadline {
            return verdict;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

pub fn insert_text(
    target: &FocusTarget,
    text: &str,
    options: &InsertOptions,
) -> anyhow::Result<InsertionReport> {
    if !events::wait_for_modifiers_released(Duration::from_millis(600)) {
        log::warn!("Modifier keys still held; inserting anyway");
    }
    if target.pid > 0 && !activate(target.pid, Duration::from_millis(400)) {
        log::warn!("Could not bring {:?} back to the front", target.app_name);
    }

    // Prefer the element captured at key-down; if focus moved within the app,
    // use whatever is focused now.
    let current = AxElement::focused();
    let element = match (&target.native.element, current) {
        (Some(captured), Some(now)) if *captured == now => Some(now),
        (_, Some(now)) if now.pid() == Some(target.pid) => Some(now),
        (captured, _) => captured.clone(),
    };
    let secure = target.info.is_secure;
    let before = element
        .as_ref()
        .filter(|_| !secure)
        .map(AxElement::snapshot);

    let text = match (&before, options.smart_spacing) {
        (Some(snapshot), true) => {
            join_with_preceding(insertion::char_before_selection(snapshot), text)
        }
        _ => text.to_string(),
    };
    let inserted_len = utf16_len(&text);
    let verify = |timeout: Duration| match (&element, &before) {
        (Some(element), Some(before)) => await_landing(element, before, inserted_len, timeout),
        _ => Landed::Unknown,
    };

    for method in insertion::insertion_plan(&target.info) {
        let started = Instant::now();
        let landed = match method {
            InsertionMethod::Accessibility => {
                let Some(element) = &element else { continue };
                if let Err(code) = element.set_string_attr("AXSelectedText", &text) {
                    log::info!("Accessibility insert failed (AXError {code})");
                    continue;
                }
                verify(Duration::from_millis(150))
            }
            InsertionMethod::KeyEvents => {
                let pid = (target.pid > 0).then_some(target.pid);
                if let Err(e) = events::type_text(&text, pid) {
                    log::info!("Key event insert failed: {e}");
                    continue;
                }
                verify(Duration::from_millis(400))
            }
            InsertionMethod::ClipboardPaste => {
                let saved = pasteboard::save();
                let count = pasteboard::set_text(&text, true)?;
                events::send_command_key(keyboard::paste_key_code())?;
                let landed = verify(Duration::from_millis(400));
                // Give the app time to read the clipboard before restoring it.
                let settle = options
                    .clipboard_restore_delay
                    .saturating_sub(started.elapsed());
                std::thread::sleep(settle);
                pasteboard::restore(saved, count);
                landed
            }
            InsertionMethod::ClipboardOnly => {
                pasteboard::set_text(&text, false)?;
                Landed::Unknown
            }
        };
        log::info!(
            "Insert via {method:?}: {landed:?} in {} ms",
            started.elapsed().as_millis()
        );
        if landed != Landed::No {
            return Ok(InsertionReport { method, landed });
        }
    }
    anyhow::bail!("no insertion method worked")
}

pub fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    pasteboard::set_text(text, false).map(|_| ())
}
