//! Running-application queries.

use objc2_app_kit::{
    NSApplicationActivationOptions, NSBitmapImageFileType, NSBitmapImageRep, NSRunningApplication,
    NSWorkspace,
};
use objc2_foundation::NSDictionary;
use std::time::{Duration, Instant};

pub fn bundle_id(pid: i32) -> Option<String> {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
        .and_then(|app| app.bundleIdentifier())
        .map(|id| id.to_string())
}

pub fn app_name(pid: i32) -> Option<String> {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
        .and_then(|app| app.localizedName())
        .map(|name| name.to_string())
}

pub fn frontmost_pid() -> Option<i32> {
    NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|app| app.processIdentifier())
}

/// Bring `pid` to the front if it isn't already, waiting up to `timeout`.
/// Returns whether it is frontmost afterwards.
pub fn activate(pid: i32, timeout: Duration) -> bool {
    if frontmost_pid() == Some(pid) {
        return true;
    }
    let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) else {
        return false;
    };
    #[allow(deprecated)]
    app.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if frontmost_pid() == Some(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

/// The app's icon as PNG, using the smallest bitmap at least `min_px` wide.
pub fn icon_png(pid: i32, min_px: isize) -> Option<Vec<u8>> {
    let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?;
    let tiff = app.icon()?.TIFFRepresentation()?;
    let reps = NSBitmapImageRep::imageRepsWithData(&tiff);
    let bitmaps: Vec<_> = reps
        .iter()
        .filter_map(|rep| rep.downcast::<NSBitmapImageRep>().ok())
        .collect();
    let widths: Vec<isize> = bitmaps.iter().map(|b| b.pixelsWide()).collect();
    let bitmap = &bitmaps[crate::icon::pick_size(&widths, min_px)?];
    let png = unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }?;
    Some(png.to_vec())
}
