//! A small safe wrapper over Accessibility (AXUIElement) calls.

use super::ffi::*;
use core_foundation::base::{CFGetTypeID, CFHash, CFRelease, CFRetain, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use echolocal_core::insertion::FieldSnapshot;
use std::ffi::c_void;

/// Keep AX calls from blocking for the 6 s default when an app is hung.
const MESSAGING_TIMEOUT_SECS: f32 = 0.3;
/// Field values larger than this are not read (snapshots use counts instead).
const MAX_READ_CHARS: i64 = 60_000;

pub fn is_trusted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

/// Like [`is_trusted`], but shows the system prompt if not yet granted.
pub fn request_trust() -> bool {
    use core_foundation::dictionary::CFDictionary;
    let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
    let options =
        CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFBoolean::true_value().as_CFType())]);
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) != 0 }
}

/// An owned (retained) AXUIElementRef.
#[derive(Debug)]
pub struct AxElement(AXUIElementRef);

// SAFETY: AXUIElementRef is an immutable CF object; the AX API may be called
// from any thread.
unsafe impl Send for AxElement {}
unsafe impl Sync for AxElement {}

impl Clone for AxElement {
    fn clone(&self) -> Self {
        unsafe { CFRetain(self.0) };
        AxElement(self.0)
    }
}

impl Drop for AxElement {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}

impl PartialEq for AxElement {
    fn eq(&self, other: &Self) -> bool {
        unsafe { core_foundation::base::CFEqual(self.0, other.0) != 0 }
    }
}

fn attr(name: &'static str) -> CFString {
    CFString::from_static_string(name)
}

impl AxElement {
    pub fn system_wide() -> Self {
        let element = AxElement(unsafe { AXUIElementCreateSystemWide() });
        unsafe { AXUIElementSetMessagingTimeout(element.0, MESSAGING_TIMEOUT_SECS) };
        element
    }

    /// The element with keyboard focus anywhere on the system.
    pub fn focused() -> Option<Self> {
        let system = Self::system_wide();
        if let Some(element) = system.element_attr("AXFocusedUIElement") {
            return Some(element);
        }
        // Some apps only answer through their application element.
        system
            .element_attr("AXFocusedApplication")
            .and_then(|app| app.element_attr("AXFocusedUIElement"))
    }

    fn copy_attr(&self, name: &'static str) -> Option<CFTypeRef> {
        let mut value: CFTypeRef = std::ptr::null();
        let err = unsafe {
            AXUIElementCopyAttributeValue(self.0, attr(name).as_concrete_TypeRef(), &mut value)
        };
        (err == kAXErrorSuccess && !value.is_null()).then_some(value)
    }

    pub fn element_attr(&self, name: &'static str) -> Option<AxElement> {
        let value = self.copy_attr(name)?;
        if unsafe { CFGetTypeID(value) } == unsafe { AXUIElementGetTypeID() } {
            let element = AxElement(value);
            unsafe { AXUIElementSetMessagingTimeout(element.0, MESSAGING_TIMEOUT_SECS) };
            Some(element)
        } else {
            unsafe { CFRelease(value) };
            None
        }
    }

    pub fn string_attr(&self, name: &'static str) -> Option<String> {
        let value = self.copy_attr(name)?;
        if unsafe { CFGetTypeID(value) } == CFString::type_id() {
            Some(unsafe { CFString::wrap_under_create_rule(value as _) }.to_string())
        } else {
            unsafe { CFRelease(value) };
            None
        }
    }

    pub fn i64_attr(&self, name: &'static str) -> Option<i64> {
        let value = self.copy_attr(name)?;
        if unsafe { CFGetTypeID(value) } == CFNumber::type_id() {
            unsafe { CFNumber::wrap_under_create_rule(value as _) }.to_i64()
        } else {
            unsafe { CFRelease(value) };
            None
        }
    }

    /// A CFRange attribute such as `AXSelectedTextRange`, as (location, length).
    pub fn range_attr(&self, name: &'static str) -> Option<(i64, i64)> {
        let value = self.copy_attr(name)?;
        let mut range = core_foundation::base::CFRange {
            location: 0,
            length: 0,
        };
        let ok = unsafe {
            CFGetTypeID(value) == AXValueGetTypeID()
                && AXValueGetValue(
                    value,
                    kAXValueTypeCFRange,
                    &mut range as *mut _ as *mut c_void,
                ) != 0
        };
        unsafe { CFRelease(value) };
        ok.then_some((range.location as i64, range.length as i64))
    }

    pub fn is_settable(&self, name: &'static str) -> bool {
        let mut settable: Boolean = 0;
        let err = unsafe {
            AXUIElementIsAttributeSettable(self.0, attr(name).as_concrete_TypeRef(), &mut settable)
        };
        err == kAXErrorSuccess && settable != 0
    }

    pub fn set_string_attr(&self, name: &'static str, value: &str) -> Result<(), AXError> {
        let value = CFString::new(value);
        let err = unsafe {
            AXUIElementSetAttributeValue(
                self.0,
                attr(name).as_concrete_TypeRef(),
                value.as_CFTypeRef(),
            )
        };
        if err == kAXErrorSuccess {
            Ok(())
        } else {
            Err(err)
        }
    }

    pub fn pid(&self) -> Option<i32> {
        let mut pid = 0;
        let err = unsafe { AXUIElementGetPid(self.0, &mut pid) };
        (err == kAXErrorSuccess && pid > 0).then_some(pid)
    }

    pub fn role(&self) -> Option<String> {
        self.string_attr("AXRole")
    }

    pub fn subrole(&self) -> Option<String> {
        self.string_attr("AXSubrole")
    }

    pub fn is_secure(&self) -> bool {
        self.subrole().as_deref() == Some("AXSecureTextField")
    }

    /// Observable state of this element, for insertion verification.
    pub fn snapshot(&self) -> FieldSnapshot {
        let char_count = self.i64_attr("AXNumberOfCharacters");
        let value = if self.is_secure() || char_count.unwrap_or(0) > MAX_READ_CHARS {
            None
        } else {
            self.string_attr("AXValue")
        };
        FieldSnapshot {
            element_id: unsafe { CFHash(self.0) } as u64,
            pid: self.pid().unwrap_or(0),
            value,
            char_count,
            selection: self.range_attr("AXSelectedTextRange"),
        }
    }
}
