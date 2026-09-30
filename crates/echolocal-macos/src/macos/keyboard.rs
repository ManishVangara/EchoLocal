//! Keyboard layout awareness.
//!
//! ⌘V must be sent with the virtual key code that produces "v" in the user's
//! layout — on Dvorak that is not the QWERTY "V" key. Layouts such as
//! "Dvorak – QWERTY ⌘" switch to QWERTY while ⌘ is held, so the lookup
//! translates with the command modifier applied.
//!
//! Text Input Source APIs must be called on the main thread, so the result is
//! cached: call [`refresh`] from the main thread (at launch and when a
//! dictation starts); [`paste_key_code`] can then be read from any thread.

use super::ffi::*;
use core_foundation::base::{CFRelease, TCFType};
use core_foundation::data::{CFData, CFDataRef};
use std::sync::atomic::{AtomicU16, Ordering};

/// `kVK_ANSI_V`, correct for QWERTY-family layouts.
const ANSI_V: u16 = 9;
const UNSET: u16 = u16::MAX;

static PASTE_KEY_CODE: AtomicU16 = AtomicU16::new(UNSET);

/// Virtual key code to combine with ⌘ for paste.
pub fn paste_key_code() -> u16 {
    match PASTE_KEY_CODE.load(Ordering::Relaxed) {
        UNSET => ANSI_V,
        code => code,
    }
}

/// Re-resolve the paste key for the current layout. Main thread only.
pub fn refresh() {
    let code = resolve_key_code_for('v').unwrap_or(ANSI_V);
    let previous = PASTE_KEY_CODE.swap(code, Ordering::Relaxed);
    if previous != code {
        log::info!("Paste key code for current keyboard layout: {code}");
    }
}

fn resolve_key_code_for(target: char) -> Option<u16> {
    // Input methods (e.g. Japanese) have no key layout data; fall back to the
    // ASCII-capable layout they type Latin characters with.
    for copy in [
        TISCopyCurrentKeyboardLayoutInputSource as unsafe extern "C" fn() -> TISInputSourceRef,
        TISCopyCurrentASCIICapableKeyboardLayoutInputSource,
    ] {
        let source = unsafe { copy() };
        if source.is_null() {
            continue;
        }
        let result = key_code_in_source(source, target);
        unsafe { CFRelease(source as _) };
        if result.is_some() {
            return result;
        }
    }
    None
}

fn key_code_in_source(source: TISInputSourceRef, target: char) -> Option<u16> {
    let data = unsafe { TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData) };
    if data.is_null() {
        return None;
    }
    // Get rule: the source owns the data; it lives as long as `source`.
    let data = unsafe { CFData::wrap_under_get_rule(data as CFDataRef) };
    let layout = data.bytes().as_ptr() as *const std::ffi::c_void;
    let keyboard_type = unsafe { LMGetKbdType() } as u32;

    let translate = |code: u16, modifiers: u32| -> Option<char> {
        let mut dead_state = 0u32;
        let mut buf = [0u16; 4];
        let mut len = 0usize;
        let status = unsafe {
            UCKeyTranslate(
                layout,
                code,
                kUCKeyActionDown,
                modifiers,
                keyboard_type,
                kUCKeyTranslateNoDeadKeysMask,
                &mut dead_state,
                buf.len(),
                &mut len,
                buf.as_mut_ptr(),
            )
        };
        if status != 0 || len != 1 {
            return None;
        }
        char::from_u32(buf[0] as u32).map(|c| c.to_ascii_lowercase())
    };

    for modifiers in [CMD_KEY_STATE, 0] {
        if let Some(code) = (0u16..128).find(|&code| translate(code, modifiers) == Some(target)) {
            return Some(code);
        }
    }
    None
}
