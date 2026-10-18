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

/// A name that finds this screen again after a restart or re-plug (display ids can change): its name, plus
/// " #2", " #3"… when several screens share it, counted left to right.
pub fn screen_key(all: &[Display], d: &Display) -> String {
    let mut same: Vec<&Display> = all.iter().filter(|o| o.name == d.name).collect();
    if same.len() < 2 {
        return d.name.clone();
    }
    same.sort_by(|a, b| (a.x, a.y).partial_cmp(&(b.x, b.y)).unwrap_or(std::cmp::Ordering::Equal));
    let n = same.iter().position(|o| o.id == d.id).unwrap_or(0) + 1;
    if n == 1 { d.name.clone() } else { format!("{} #{n}", d.name) }
}

/// The connected screen saved as `key` (see `screen_key`).
pub fn find_screen(all: &[Display], key: &str) -> Option<Display> {
    if key.is_empty() {
        return None;
    }
    all.iter().find(|d| screen_key(all, d) == key).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_named_screens_are_numbered_left_to_right() {
        let d = |id, name: &str, x| Display { id, name: name.into(), x, y: 0.0, w: 100.0, h: 100.0, pixels: (1920, 1080), main: false };
        let all = vec![d(7, "EPSON", 300.0), d(3, "Built-in", 0.0), d(9, "EPSON", 100.0)];
        assert_eq!(screen_key(&all, &all[1]), "Built-in");
        assert_eq!(screen_key(&all, &all[2]), "EPSON");
        assert_eq!(screen_key(&all, &all[0]), "EPSON #2");
        assert_eq!(find_screen(&all, "EPSON #2").map(|d| d.id), Some(7));
        assert_eq!(find_screen(&all, "Gone"), None);
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

/// Windows: every monitor (EnumDisplayMonitors), in points (each monitor's own DPI scale), primary first-class.
#[cfg(windows)]
pub fn displays() -> Vec<Display> {
    use windows_sys::Win32::Foundation::{LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
    use windows_sys::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    unsafe extern "system" fn each(monitor: HMONITOR, _dc: HDC, _rect: *mut RECT, data: LPARAM) -> i32 {
        // SAFETY: `data` is the Vec we passed below; the monitor handle comes from the enumeration.
        unsafe {
            let out = &mut *(data as *mut Vec<(HMONITOR, MONITORINFO, f32)>);
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info) != 0 {
                let (mut dx, mut dy) = (96u32, 96u32);
                let scale = if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) == 0 { dx as f32 / 96.0 } else { 1.0 };
                out.push((monitor, info, scale.max(0.5)));
            }
        }
        1
    }
    let mut found: Vec<(HMONITOR, MONITORINFO, f32)> = Vec::new();
    // SAFETY: the callback only writes into `found`, which outlives the call.
    unsafe { EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(each), &mut found as *mut _ as LPARAM) };
    found
        .iter()
        .enumerate()
        .map(|(i, (_, info, scale))| {
            let r = info.rcMonitor;
            let (pw, ph) = ((r.right - r.left) as u32, (r.bottom - r.top) as u32);
            let main = info.dwFlags & MONITORINFOF_PRIMARY != 0;
            Display {
                id: i as u32 + 1,
                name: if main { format!("Display {} (main)", i + 1) } else { format!("Display {}", i + 1) },
                x: r.left as f32 / scale,
                y: r.top as f32 / scale,
                w: pw as f32 / scale,
                h: ph as f32 / scale,
                pixels: (pw, ph),
                main,
            }
        })
        .collect()
}

#[cfg(not(any(target_os = "macos", windows)))]
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

/// Where the mouse is on the desktop, in points from the main display's top-left (the coordinates egui's
/// viewport rects use, before zoom). Works while macOS runs a drag from the Finder, when windows get no mouse events.
#[cfg(target_os = "macos")]
pub fn mouse_location() -> Option<(f32, f32)> {
    use objc2::msg_send;
    use objc2::runtime::AnyClass;
    let class = AnyClass::get(c"NSEvent")?;
    // SAFETY: a class method returning a plain struct.
    let p: objc2_foundation::NSPoint = unsafe { msg_send![class, mouseLocation] };
    let main_h = displays().iter().find(|m| m.main)?.h as f64;
    Some((p.x as f32, (main_h - p.y) as f32))
}

#[cfg(windows)]
pub fn mouse_location() -> Option<(f32, f32)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT { x: 0, y: 0 };
    // SAFETY: a POINT we own.
    (unsafe { GetCursorPos(&mut p) } != 0).then(|| (p.x as f32, p.y as f32))
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn mouse_location() -> Option<(f32, f32)> {
    None
}

/// Off the Mac the output window is already borderless and sized to the display by the caller; nothing more to do.
#[cfg(not(target_os = "macos"))]
pub fn cover_window(_title: &str, _cover: Option<&Display>) -> bool {
    true
}

#[cfg(all(test, target_os = "macos"))]
#[test]
fn mouse_location_is_on_the_desktop() {
    // With the screen locked there is no pointer on any desktop display (and no location at all): not a failure.
    let locked = std::process::Command::new("sh")
        .args(["-c", "ioreg -n Root -d1 | grep -c 'CGSSessionScreenIsLocked\\\"=Yes'"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() != "0")
        .unwrap_or(false);
    if locked {
        eprintln!("screen locked: skipping");
        return;
    }
    let (x, y) = mouse_location().expect("mouse location");
    let ds = displays();
    let (x0, y0) = ds.iter().fold((f32::MAX, f32::MAX), |(a, b), d| (a.min(d.x as f32), b.min(d.y as f32)));
    let (x1, y1) = ds.iter().fold((f32::MIN, f32::MIN), |(a, b), d| (a.max((d.x + d.w) as f32), b.max((d.y + d.h) as f32)));
    assert!(x >= x0 && x <= x1 && y >= y0 && y <= y1, "({x}, {y}) outside {x0},{y0}–{x1},{y1}");
}
