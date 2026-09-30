//! Declarations for the C APIs used here that have no maintained Rust crate:
//! Accessibility (ApplicationServices), Text Input Sources (Carbon) and the
//! modifier state query from CoreGraphics.

#![allow(non_upper_case_globals, non_snake_case, dead_code)]

use core_foundation::base::{CFTypeID, CFTypeRef};
use core_foundation::dictionary::CFDictionaryRef;
use core_foundation::string::CFStringRef;
use std::ffi::c_void;

pub type AXUIElementRef = CFTypeRef;
pub type AXError = i32;
pub const kAXErrorSuccess: AXError = 0;
pub const kAXValueTypeCFRange: u32 = 4;

/// `Boolean` in the C APIs is an unsigned char.
pub type Boolean = u8;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    pub static kAXTrustedCheckOptionPrompt: CFStringRef;

    pub fn AXIsProcessTrusted() -> Boolean;
    pub fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> Boolean;

    pub fn AXUIElementGetTypeID() -> CFTypeID;
    pub fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    pub fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    pub fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    pub fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> AXError;
    pub fn AXUIElementIsAttributeSettable(
        element: AXUIElementRef,
        attribute: CFStringRef,
        settable: *mut Boolean,
    ) -> AXError;
    pub fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> AXError;
    pub fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout_seconds: f32)
        -> AXError;

    pub fn AXValueGetTypeID() -> CFTypeID;
    pub fn AXValueGetValue(value: CFTypeRef, the_type: u32, value_ptr: *mut c_void) -> Boolean;
}

pub type TISInputSourceRef = *mut c_void;

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    pub static kTISPropertyUnicodeKeyLayoutData: CFStringRef;
    pub static kTISPropertyInputSourceID: CFStringRef;

    pub fn TISCopyCurrentKeyboardLayoutInputSource() -> TISInputSourceRef;
    pub fn TISCopyCurrentASCIICapableKeyboardLayoutInputSource() -> TISInputSourceRef;
    pub fn TISGetInputSourceProperty(source: TISInputSourceRef, key: CFStringRef) -> *const c_void;
    pub fn LMGetKbdType() -> u8;
    pub fn UCKeyTranslate(
        key_layout: *const c_void,
        virtual_key_code: u16,
        key_action: u16,
        modifier_key_state: u32,
        keyboard_type: u32,
        key_translate_options: u32,
        dead_key_state: *mut u32,
        max_string_length: usize,
        actual_string_length: *mut usize,
        unicode_string: *mut u16,
    ) -> i32;
}

pub const kUCKeyActionDown: u16 = 0;
pub const kUCKeyTranslateNoDeadKeysMask: u32 = 1;
/// `cmdKey >> 8`, the form UCKeyTranslate expects.
pub const CMD_KEY_STATE: u32 = 1;

pub const kCGEventSourceStateHIDSystemState: i32 = 1;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    pub fn CGEventSourceFlagsState(state_id: i32) -> u64;
}
