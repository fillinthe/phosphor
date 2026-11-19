//! Files opened from outside the app while it starts or runs: a project or packed show double-clicked in Finder or
//! dropped on the Dock icon. macOS hands these to the app delegate (`application:openURLs:`), which winit doesn't
//! answer; `install` adds that method to winit's delegate (eframe's `AFTER_EVENT_LOOP` hook, before launch finishes, so
//! the file that started the app arrives too). The app takes them with `take` each frame. Windows / Linux pass the
//! file on the command line instead (`init.rs`).

use std::path::PathBuf;
use std::sync::Mutex;

static OPENED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Files opened since the last call (cheap: nothing to do most frames).
pub fn take() -> Vec<PathBuf> {
    OPENED.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default()
}

/// What the app does with an opened file.
#[derive(Debug, PartialEq)]
pub enum Opened {
    Project(PathBuf),
    Pack(PathBuf),
    Other,
}

pub fn kind(path: &std::path::Path) -> Opened {
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_lowercase);
    match ext.as_deref() {
        Some("phosphor") => Opened::Project(path.to_path_buf()),
        Some("phosphorpack") => Opened::Pack(path.to_path_buf()),
        _ => Opened::Other,
    }
}

#[cfg(target_os = "macos")]
pub fn install() {
    use objc2::runtime::{AnyClass, AnyObject, Sel};
    use objc2::{msg_send, sel};
    use objc2_foundation::NSString;

    unsafe extern "C-unwind" fn open_urls(_this: *mut AnyObject, _sel: Sel, _app: *mut AnyObject, urls: *mut AnyObject) {
        if urls.is_null() {
            return;
        }
        let mut got = Vec::new();
        unsafe {
            let n: usize = msg_send![urls, count];
            for i in 0..n {
                let url: *mut AnyObject = msg_send![urls, objectAtIndex: i];
                let is_file: bool = msg_send![url, isFileURL];
                if !is_file {
                    continue;
                }
                let path: *mut NSString = msg_send![url, path];
                if let Some(p) = path.as_ref() {
                    got.push(PathBuf::from(p.to_string()));
                }
            }
        }
        if let Ok(mut v) = OPENED.lock() {
            v.extend(got);
        }
    }

    unsafe {
        let Some(app_class) = AnyClass::get(c"NSApplication") else { return };
        let app: *mut AnyObject = msg_send![app_class, sharedApplication];
        if app.is_null() {
            return;
        }
        let delegate: *mut AnyObject = msg_send![app, delegate];
        let Some(delegate) = delegate.as_ref() else { return };
        let class = delegate.class() as *const AnyClass as *mut AnyClass;
        let imp: objc2::runtime::Imp = std::mem::transmute(open_urls as unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject, *mut AnyObject));
        // v = returns nothing; @ self, : selector, @ the app, @ the URL list.
        objc2::ffi::class_addMethod(class, sel!(application:openURLs:), imp, c"v@:@@".as_ptr());
    }
}

#[cfg(not(target_os = "macos"))]
pub fn install() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_and_packs_are_told_apart() {
        assert_eq!(kind(std::path::Path::new("/a/Show.PHOSPHOR")), Opened::Project("/a/Show.PHOSPHOR".into()));
        assert_eq!(kind(std::path::Path::new("/a/Show.phosphorpack")), Opened::Pack("/a/Show.phosphorpack".into()));
        assert_eq!(kind(std::path::Path::new("/a/clip.mov")), Opened::Other);
    }
}
