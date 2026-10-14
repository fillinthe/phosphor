//! Lists the connected displays so the output can be sent to a specific screen.

#[derive(Clone, Debug, PartialEq)]
pub struct Display {
    pub id: u32,
    pub name: String,
    /// Global position and size in points (top-left origin, main display at 0,0).
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Native pixel size.
    pub pixels: (u32, u32),
    pub main: bool,
}

impl Display {
    pub fn label(&self) -> String {
        format!("{} · {}×{}", self.name, self.pixels.0, self.pixels.1)
    }
}

#[cfg(target_os = "macos")]
mod cg {
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct CGPoint {
        pub x: f64,
        pub y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct CGSize {
        pub width: f64,
        pub height: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct CGRect {
        pub origin: CGPoint,
        pub size: CGSize,
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        pub fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
        pub fn CGDisplayBounds(display: u32) -> CGRect;
        pub fn CGDisplayIsBuiltin(display: u32) -> u32;
        pub fn CGMainDisplayID() -> u32;
        pub fn CGDisplayPixelsWide(display: u32) -> usize;
        pub fn CGDisplayPixelsHigh(display: u32) -> usize;
        pub fn CGDisplayCopyDisplayMode(display: u32) -> *const std::ffi::c_void;
        pub fn CGDisplayModeGetPixelWidth(mode: *const std::ffi::c_void) -> usize;
        pub fn CGDisplayModeGetPixelHeight(mode: *const std::ffi::c_void) -> usize;
        pub fn CGDisplayModeRelease(mode: *const std::ffi::c_void);
    }
}

#[cfg(target_os = "macos")]
pub fn displays() -> Vec<Display> {
    let mut ids = [0u32; 16];
    let mut count = 0u32;
    // SAFETY: plain CoreGraphics queries into stack buffers sized as declared.
    unsafe {
        if cg::CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) != 0 {
            return Vec::new();
        }
        let main = cg::CGMainDisplayID();
        let mut external = 0;
        ids[..count as usize]
            .iter()
            .map(|&id| {
                let b = cg::CGDisplayBounds(id);
                let builtin = cg::CGDisplayIsBuiltin(id) != 0;
                let mode = cg::CGDisplayCopyDisplayMode(id);
                let pixels = if mode.is_null() {
                    (cg::CGDisplayPixelsWide(id) as u32, cg::CGDisplayPixelsHigh(id) as u32)
                } else {
                    let p = (cg::CGDisplayModeGetPixelWidth(mode) as u32, cg::CGDisplayModeGetPixelHeight(mode) as u32);
                    cg::CGDisplayModeRelease(mode);
                    p
                };
                let name = if builtin {
                    "Built-in display".to_string()
                } else {
                    external += 1;
                    format!("External display {external}")
                };
                Display {
                    id,
                    name,
                    x: b.origin.x as f32,
                    y: b.origin.y as f32,
                    w: b.size.width as f32,
                    h: b.size.height as f32,
                    pixels,
                    main: id == main,
                }
            })
            .collect()
    }
}

#[cfg(not(target_os = "macos"))]
pub fn displays() -> Vec<Display> {
    Vec::new()
}

/// The display containing a global point (points, top-left origin), e.g. a window's centre.
pub fn display_at(x: f32, y: f32) -> Option<Display> {
    let all = displays();
    all.iter().find(|d| x >= d.x && x < d.x + d.w && y >= d.y && y < d.y + d.h).cloned().or_else(|| all.into_iter().next())
}

/// Puts the window titled `title` above the menu bar and Dock and keeps it out of Spaces
/// (`cover`), or back to a normal window. Output windows fill a display this way instead of
/// macOS full screen, which blacks out every other display when "Displays have separate
/// Spaces" is off.
#[cfg(target_os = "macos")]
pub fn cover_window(title: &str, cover: Option<&Display>) -> bool {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::{NSArray, NSString};
    // NSMainMenuWindowLevel (24) + 1, so the menu bar and Dock stay underneath.
    const COVER_LEVEL: isize = 25;
    // canJoinAllSpaces | stationary | ignoresCycle | fullScreenAuxiliary
    const COVER_BEHAVIOR: usize = 1 | 16 | 64 | 256;
    let Some(app_class) = AnyClass::get(c"NSApplication") else { return false };
    // SAFETY: AppKit calls on the main thread (the UI thread), on NSApp's own windows.
    unsafe {
        let app: Option<Retained<AnyObject>> = msg_send![app_class, sharedApplication];
        let Some(app) = app else { return false };
        let windows: Option<Retained<NSArray<AnyObject>>> = msg_send![&*app, windows];
        for w in windows.iter().flat_map(|a| a.iter()) {
            let t: Option<Retained<NSString>> = msg_send![&*w, title];
            if t.is_some_and(|t| t.to_string() == title) {
                let on = cover.is_some();
                let _: () = msg_send![&*w, setLevel: if on { COVER_LEVEL } else { 0isize }];
                let _: () = msg_send![&*w, setCollectionBehavior: if on { COVER_BEHAVIOR } else { 0usize }];
                let _: () = msg_send![&*w, setHasShadow: !on];
                if let Some(d) = cover {
                    // Exactly the display, in AppKit coordinates (bottom-left origin, main display
                    // height as the reference), set directly so nothing nudges it below the menu bar.
                    let main_h = displays().iter().find(|m| m.main).map_or(d.h, |m| m.h) as f64;
                    let frame = objc2_foundation::NSRect::new(
                        objc2_foundation::NSPoint::new(d.x as f64, main_h - (d.y + d.h) as f64),
                        objc2_foundation::NSSize::new(d.w as f64, d.h as f64),
                    );
                    let _: () = msg_send![&*w, setFrame: frame, display: true];
                }
                return true;
            }
        }
    }
    false
}

#[cfg(not(target_os = "macos"))]
pub fn cover_window(_title: &str, _cover: Option<&Display>) -> bool {
    false
}
