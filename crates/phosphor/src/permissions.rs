//! What macOS lets Phosphor use (camera, microphone, screen recording), for the Devices panel, and where in System
//! Settings to change it. Asking costs nothing (no prompt): it only reads the status.

/// One permission's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Allowed,
    Denied,
    /// Not asked yet: macOS asks the first time Phosphor uses it.
    NotAsked,
    /// Blocked by a profile (school / company Mac).
    Restricted,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Allowed => "allowed",
            Status::Denied => "not allowed",
            Status::NotAsked => "not asked yet",
            Status::Restricted => "blocked by this Mac's settings",
        }
    }
}

/// A permission, what it's for, and its System Settings page.
pub struct Permission {
    pub name: &'static str,
    pub why: &'static str,
    pub status: Status,
    pub settings: &'static str,
}

/// Camera, microphone and screen recording, as macOS sees them now.
pub fn all() -> Vec<Permission> {
    vec![
        Permission { name: "Camera", why: "cameras and capture cards as layers, video tracking", status: av_status(c"vide"), settings: "x-apple.systempreferences:com.apple.preference.security?Privacy_Camera" },
        Permission { name: "Microphone", why: "sound input for audio-reactive visuals and beat detection", status: av_status(c"soun"), settings: "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone" },
        Permission { name: "Screen recording", why: "screens and windows as layers", status: screen_status(), settings: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture" },
    ]
}

/// AVCaptureDevice's authorization for a media type (`vide` = AVMediaTypeVideo, `soun` = AVMediaTypeAudio).
fn av_status(media: &std::ffi::CStr) -> Status {
    use objc2::msg_send;
    use objc2::runtime::AnyClass;
    use objc2_foundation::NSString;
    let Some(class) = AnyClass::get(c"AVCaptureDevice") else { return Status::NotAsked };
    let kind = NSString::from_str(&media.to_string_lossy());
    // SAFETY: a class method that only reads the status; `kind` is an AVMediaType string.
    let raw: isize = unsafe { msg_send![class, authorizationStatusForMediaType: &*kind] };
    match raw {
        3 => Status::Allowed,
        2 => Status::Denied,
        1 => Status::Restricted,
        _ => Status::NotAsked,
    }
}

fn screen_status() -> Status {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
    }
    // SAFETY: no arguments; reads the status without asking.
    if unsafe { CGPreflightScreenCaptureAccess() } { Status::Allowed } else { Status::Denied }
}

#[cfg(test)]
mod tests {
    #[test]
    fn statuses_can_be_read() {
        let all = super::all();
        assert_eq!(all.len(), 3);
        for p in &all {
            assert!(!p.status.label().is_empty() && p.settings.starts_with("x-apple.systempreferences:"));
        }
    }
}
