//! Synthesized keyboard input.

use super::ffi::*;
use crate::utf16::utf16_chunks;
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use std::time::{Duration, Instant};

/// Unicode strings attached to one key event are truncated beyond 20 UTF-16
/// units by some apps, so text is sent in chunks of at most this size.
const MAX_UNITS_PER_EVENT: usize = 20;
const CHUNK_DELAY: Duration = Duration::from_millis(2);

const MODIFIER_MASK: u64 = 0x0010_0000 // command
    | 0x0002_0000 // shift
    | 0x0008_0000 // option
    | 0x0004_0000 // control
    | 0x0080_0000; // fn

/// Wait (up to `timeout`) until no modifier keys are physically held, so a
/// still-held hotkey modifier (e.g. ⌥) can't alter the inserted text.
/// Returns false on timeout.
pub fn wait_for_modifiers_released(timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        let flags = unsafe { CGEventSourceFlagsState(kCGEventSourceStateHIDSystemState) };
        if flags & MODIFIER_MASK == 0 {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn event_source() -> anyhow::Result<CGEventSource> {
    // A private source doesn't merge in the physical modifier state.
    CGEventSource::new(CGEventSourceStateID::Private)
        .map_err(|_| anyhow::anyhow!("could not create an event source"))
}

/// Type `text` as Unicode keyboard events. Independent of keyboard layout.
/// With a PID, events go straight to that process; otherwise to the focused app.
pub fn type_text(text: &str, pid: Option<i32>) -> anyhow::Result<()> {
    let source = event_source()?;
    let units: Vec<u16> = text.encode_utf16().collect();
    for chunk in utf16_chunks(&units, MAX_UNITS_PER_EVENT) {
        for key_down in [true, false] {
            let event = CGEvent::new_keyboard_event(source.clone(), 0, key_down)
                .map_err(|_| anyhow::anyhow!("could not create a keyboard event"))?;
            event.set_flags(CGEventFlags::empty());
            event.set_string_from_utf16_unchecked(chunk);
            match pid {
                Some(pid) => event.post_to_pid(pid),
                None => event.post(CGEventTapLocation::HID),
            }
        }
        std::thread::sleep(CHUNK_DELAY);
    }
    Ok(())
}

/// Send ⌘ + `key_code` to the focused application.
pub fn send_command_key(key_code: u16) -> anyhow::Result<()> {
    let source = event_source()?;
    for key_down in [true, false] {
        let event = CGEvent::new_keyboard_event(source.clone(), key_code, key_down)
            .map_err(|_| anyhow::anyhow!("could not create a keyboard event"))?;
        event.set_flags(CGEventFlags::CGEventFlagCommand);
        event.post(CGEventTapLocation::HID);
    }
    Ok(())
}
