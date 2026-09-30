//! Microphone (TCC) permission status and request.

use crate::Permission;
use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

pub fn status() -> Permission {
    let Some(media) = (unsafe { AVMediaTypeAudio }) else {
        return Permission::Unknown;
    };
    let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media) };
    match status {
        AVAuthorizationStatus::Authorized => Permission::Granted,
        AVAuthorizationStatus::Denied | AVAuthorizationStatus::Restricted => Permission::Denied,
        AVAuthorizationStatus::NotDetermined => Permission::NotDetermined,
        _ => Permission::Unknown,
    }
}

/// Show the system prompt if the user hasn't decided yet. `done` is called
/// (on an arbitrary thread) with whether access was granted.
pub fn request(done: impl Fn(bool) + Send + 'static) {
    let Some(media) = (unsafe { AVMediaTypeAudio }) else {
        done(false);
        return;
    };
    let block = RcBlock::new(move |granted: Bool| done(granted.as_bool()));
    unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &block) };
}
