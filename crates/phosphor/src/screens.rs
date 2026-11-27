//! Lists the connected displays so the output can be sent to a specific screen.

use eframe::egui;

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
    /// Top-left corner on the desktop in pixels: where a window is, whatever each screen's scale (Windows and Linux
    /// lay screens out in pixels; points from screens with different scales overlap or leave gaps). The Mac: points.
    pub at_px: (i32, i32),
    pub main: bool,
    /// The key older versions saved for this screen (Windows: "Display 2" by enumeration order), still found by
    /// `find_screen` so saved projects keep their screens. Empty when it's the same as now.
    pub old_key: String,
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

/// The connected screen saved as `key` (see `screen_key`), or by the key an older version saved for it.
pub fn find_screen(all: &[Display], key: &str) -> Option<Display> {
    if key.is_empty() {
        return None;
    }
    all.iter().find(|d| screen_key(all, d) == key).or_else(|| all.iter().find(|d| d.old_key == key)).cloned()
}

/// The display the point is on (None when it's on none of them).
pub fn display_of(all: &[Display], x: f32, y: f32) -> Option<u32> {
    all.iter().find(|d| x >= d.x && x < d.x + d.w && y >= d.y && y < d.y + d.h).map(|d| d.id)
}

/// The display a desktop pixel is on (Windows, Linux).
pub fn display_of_px(all: &[Display], x: i32, y: i32) -> Option<u32> {
    all.iter()
        .find(|d| x >= d.at_px.0 && y >= d.at_px.1 && (x - d.at_px.0) < d.pixels.0 as i32 && (y - d.at_px.1) < d.pixels.1 as i32)
        .map(|d| d.id)
}

/// The display the window of `ctx` is on (its centre). On Windows and Linux by pixels: egui gives a window's place in
/// its own screen's points, which don't line up across screens with different scales (a window on a 150 % screen
/// was taken for one on the 100 % screen beside it).
pub fn display_of_window(all: &[Display], ctx: &egui::Context) -> Option<u32> {
    let c = ctx.input(|i| i.viewport().outer_rect)?.center();
    if cfg!(target_os = "macos") {
        // egui's points are the Mac's divided by the zoom.
        let zoom = ctx.zoom_factor();
        display_of(all, c.x * zoom, c.y * zoom)
    } else {
        let ppp = ctx.pixels_per_point();
        display_of_px(all, (c.x * ppp).round() as i32, (c.y * ppp).round() as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_is_on_the_display_that_holds_it() {
        let d = |id, x| Display { id, name: String::new(), x, y: 0.0, w: 100.0, h: 100.0, pixels: (1920, 1080), at_px: (0, 0), main: false, old_key: String::new() };
        let all = vec![d(1, 0.0), d(2, 100.0)];
        assert_eq!(display_of(&all, 50.0, 50.0), Some(1));
        assert_eq!(display_of(&all, 100.0, 50.0), Some(2));
        assert_eq!(display_of(&all, 250.0, 50.0), None);
    }

    /// Windows, 1920 px at 100 % with a 3840 px screen at 150 % to its right: in points the second starts at 1280,
    /// inside the first. By pixels each point of the desktop is on one screen.
    #[test]
    fn screens_with_different_scales_are_told_apart_by_pixels() {
        let d = |id, at: i32, px: u32, scale: f32| Display {
            id,
            name: String::new(),
            x: at as f32 / scale,
            y: 0.0,
            w: px as f32 / scale,
            h: 1080.0,
            pixels: (px, 2160),
            at_px: (at, 0),
            main: id == 1,
            old_key: String::new(),
        };
        let all = vec![d(1, 0, 1920, 1.0), d(2, 1920, 3840, 1.5)];
        // A window centred at pixel 2000 (on the 150 % screen) is at 1333 points: the first screen in points.
        assert_eq!(display_of(&all, 2000.0 / 1.5, 500.0), Some(1), "points overlap: the old, wrong answer");
        assert_eq!(display_of_px(&all, 2000, 500), Some(2));
        assert_eq!(display_of_px(&all, 1919, 500), Some(1));
        assert_eq!(display_of_px(&all, 5760, 500), None);
    }

    #[test]
    fn winits_screens_become_displays() {
        let m = |name: &str, x: i32, w: u32, scale: f64, primary: bool| eframe::Monitor { name: name.into(), position: [x, 0], size: [w, w * 9 / 16], scale, primary };
        let all = from_monitors(&[m("HDMI-1", 0, 1920, 1.0, false), m("", 1920, 3840, 2.0, false), m("HDMI-1", 5760, 1920, 1.0, false)]);
        assert_eq!(all.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["HDMI-1", "Display 2", "HDMI-1"]);
        assert!(all[0].main && !all[1].main, "no primary (Wayland): the first is the main one");
        assert_eq!((all[1].x, all[1].w, all[1].at_px, all[1].pixels), (960.0, 1920.0, (1920, 0), (3840, 2160)));
        assert_eq!(display_of_px(&all, 2000, 10), Some(all[1].id));
        assert_eq!(screen_key(&all, &all[2]), "HDMI-1 #2");
        let primary = from_monitors(&[m("DP-1", 0, 1920, 1.0, false), m("DP-2", 1920, 1920, 1.0, true)]);
        assert!(!primary[0].main && primary[1].main);
    }

    #[test]
    fn screens_saved_under_their_old_names_are_still_found() {
        let d = |id, name: &str, old: &str| Display { id, name: name.into(), x: 0.0, y: 0.0, w: 100.0, h: 100.0, pixels: (1920, 1080), at_px: (0, 0), main: false, old_key: old.into() };
        let all = vec![d(1, "DELL U2720Q", "Display 1 (main)"), d(2, "EPSON PJ", "Display 2")];
        assert_eq!(find_screen(&all, "EPSON PJ").map(|d| d.id), Some(2));
        assert_eq!(find_screen(&all, "Display 2").map(|d| d.id), Some(2), "a project saved before the real names");
        assert_eq!(find_screen(&all, "Display 1 (main)").map(|d| d.id), Some(1));
        assert_eq!(find_screen(&all, "Display 3"), None);
    }

    #[test]
    fn same_named_screens_are_numbered_left_to_right() {
        let d = |id, name: &str, x| Display { id, name: name.into(), x, y: 0.0, w: 100.0, h: 100.0, pixels: (1920, 1080), at_px: (0, 0), main: false, old_key: String::new() };
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
                    at_px: (b.origin.x as i32, b.origin.y as i32),
                    main: id == main,
                    old_key: String::new(),
                }
            })
            .collect()
    }
}

/// Windows: every monitor (EnumDisplayMonitors), in points (each monitor's own DPI scale) and in pixels, by its real
/// name ("DELL U2720Q"; `monitor_names`). The id comes from the monitor's device name (`\\.\DISPLAY2`…), which stays
/// the same when another monitor is unplugged or this one is plugged back into the same port (an index would shift).
/// Older versions called them "Display N" by enumeration order, and saved that: kept as `old_key`.
#[cfg(windows)]
pub fn displays() -> Vec<Display> {
    use windows_sys::Win32::Foundation::{LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW};
    use windows_sys::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    unsafe extern "system" fn each(monitor: HMONITOR, _dc: HDC, _rect: *mut RECT, data: LPARAM) -> i32 {
        // SAFETY: `data` is the Vec we passed below; the monitor handle comes from the enumeration.
        unsafe {
            let out = &mut *(data as *mut Vec<(HMONITOR, MONITORINFOEXW, f32)>);
            let mut info: MONITORINFOEXW = std::mem::zeroed();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(monitor, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) != 0 {
                let (mut dx, mut dy) = (96u32, 96u32);
                let scale = if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) == 0 { dx as f32 / 96.0 } else { 1.0 };
                out.push((monitor, info, scale.max(0.5)));
            }
        }
        1
    }
    let mut found: Vec<(HMONITOR, MONITORINFOEXW, f32)> = Vec::new();
    // SAFETY: the callback only writes into `found`, which outlives the call.
    unsafe { EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(each), &mut found as *mut _ as LPARAM) };
    let names = monitor_names();
    found
        .iter()
        .enumerate()
        .map(|(i, (_, ex, scale))| {
            let info = &ex.monitorInfo;
            let r = info.rcMonitor;
            let device = wide_text(&ex.szDevice);
            // FNV-1a of the device name; never 0.
            let id = device.encode_utf16().fold(0x811c_9dc5u32, |h, c| (h ^ c as u32).wrapping_mul(0x0100_0193)).max(1);
            let (pw, ph) = ((r.right - r.left) as u32, (r.bottom - r.top) as u32);
            let main = info.dwFlags & MONITORINFOF_PRIMARY != 0;
            let old_key = if main { format!("Display {} (main)", i + 1) } else { format!("Display {}", i + 1) };
            let name = names.iter().find(|(d, _)| *d == device).map(|(_, n)| n.clone()).unwrap_or_else(|| old_key.clone());
            Display {
                id,
                name,
                x: r.left as f32 / scale,
                y: r.top as f32 / scale,
                w: pw as f32 / scale,
                h: ph as f32 / scale,
                pixels: (pw, ph),
                at_px: (r.left, r.top),
                main,
                old_key,
            }
        })
        .collect()
}

/// UTF-16 up to its first 0.
#[cfg(windows)]
fn wide_text(w: &[u16]) -> String {
    String::from_utf16_lossy(&w[..w.iter().position(|&c| c == 0).unwrap_or(w.len())])
}

/// Windows: (GDI device name `\\.\DISPLAY1`, the monitor's own name) for each active screen, from the display
/// configuration (the name the monitor reports, as Windows' Display settings show it). A laptop's own panel often
/// has no name: "Built-in display", as on the Mac.
#[cfg(windows)]
fn monitor_names() -> Vec<(String, String)> {
    use windows_sys::Win32::Devices::Display::{
        DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, DISPLAYCONFIG_DEVICE_INFO_HEADER,
        DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
        DISPLAYCONFIG_TARGET_DEVICE_NAME, DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
    };
    let (mut paths_n, mut modes_n) = (0u32, 0u32);
    // SAFETY: Win32 display-configuration calls into buffers sized as the first call says; each info packet's header
    // carries its own size and type.
    unsafe {
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut paths_n, &mut modes_n) != 0 {
            return Vec::new();
        }
        let mut paths: Vec<DISPLAYCONFIG_PATH_INFO> = vec![std::mem::zeroed(); paths_n as usize];
        let mut modes: Vec<DISPLAYCONFIG_MODE_INFO> = vec![std::mem::zeroed(); modes_n as usize];
        if QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS, &mut paths_n, paths.as_mut_ptr(), &mut modes_n, modes.as_mut_ptr(), std::ptr::null_mut()) != 0 {
            return Vec::new();
        }
        paths.truncate(paths_n as usize);
        paths
            .iter()
            .filter_map(|p| {
                let mut source: DISPLAYCONFIG_SOURCE_DEVICE_NAME = std::mem::zeroed();
                source.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                    size: std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                    adapterId: p.sourceInfo.adapterId,
                    id: p.sourceInfo.id,
                };
                if DisplayConfigGetDeviceInfo(&mut source.header) != 0 {
                    return None;
                }
                let mut target: DISPLAYCONFIG_TARGET_DEVICE_NAME = std::mem::zeroed();
                target.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                    size: std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                    adapterId: p.targetInfo.adapterId,
                    id: p.targetInfo.id,
                };
                if DisplayConfigGetDeviceInfo(&mut target.header) != 0 {
                    return None;
                }
                let name = wide_text(&target.monitorFriendlyDeviceName);
                let name = match name.trim() {
                    "" if target.outputTechnology == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL => "Built-in display".to_string(),
                    "" => return None,
                    n => n.to_string(),
                };
                Some((wide_text(&source.viewGdiDeviceName), name))
            })
            .collect()
    }
}

/// Linux: the screens winit knows (X11 or Wayland; `eframe::monitors`, a Phosphor patch). Asked inside the event
/// loop's callbacks only (where the app runs); outside them the last list is kept.
#[cfg(target_os = "linux")]
pub fn displays() -> Vec<Display> {
    static LAST: std::sync::Mutex<Vec<Display>> = std::sync::Mutex::new(Vec::new());
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(monitors) = eframe::monitors() {
        *last = from_monitors(&monitors);
    }
    last.clone()
}

/// winit's screens as `Display`s, in winit's order (the order `ViewportCommand::SetMonitor` counts in): points are
/// each screen's pixels over its scale; the id is the screen's name (X11: its output, "HDMI-1"), else its place; a
/// screen with no name is "Display N"; with no primary screen (Wayland) the first is the main one.
#[cfg(any(target_os = "linux", test))]
fn from_monitors(monitors: &[eframe::Monitor]) -> Vec<Display> {
    let any_primary = monitors.iter().any(|m| m.primary);
    monitors
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let scale = m.scale.max(0.5) as f32;
            let name = if m.name.trim().is_empty() { format!("Display {}", i + 1) } else { m.name.trim().to_string() };
            // (The second screen of a name counts as another one.)
            let before = monitors[..i].iter().filter(|o| o.name.trim() == m.name.trim()).count();
            let what = if m.name.trim().is_empty() { format!("@{},{}", m.position[0], m.position[1]) } else { format!("{name}#{before}") };
            let id = what.bytes().fold(0x811c_9dc5u32, |h, c| (h ^ c as u32).wrapping_mul(0x0100_0193)).max(1);
            Display {
                id,
                name,
                x: m.position[0] as f32 / scale,
                y: m.position[1] as f32 / scale,
                w: m.size[0] as f32 / scale,
                h: m.size[1] as f32 / scale,
                pixels: (m.size[0], m.size[1]),
                at_px: (m.position[0], m.position[1]),
                main: m.primary || (!any_primary && i == 0),
                old_key: String::new(),
            }
        })
        .collect()
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
pub fn displays() -> Vec<Display> {
    Vec::new()
}

/// Puts the window titled `title` above the menu bar and Dock and keeps it out of Spaces
/// (`cover`), or back to a normal window. Output windows fill a display this way instead of
/// macOS full screen, which blacks out every other display when "Displays have separate
/// Spaces" is off.
#[cfg(target_os = "macos")]
pub fn cover_window(_ctx: &egui::Context, title: &str, cover: Option<&Display>) -> bool {
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
                    // Exactly the display, set directly so nothing nudges it below the menu bar.
                    let _: () = msg_send![&*w, setFrame: appkit_frame(d), display: true];
                }
                return true;
            }
        }
    }
    false
}

/// A display's whole area in AppKit coordinates (bottom-left origin, the main display's height as the reference).
#[cfg(target_os = "macos")]
fn appkit_frame(d: &Display) -> objc2_foundation::NSRect {
    let main_h = displays().iter().find(|m| m.main).map_or(d.h, |m| m.h) as f64;
    objc2_foundation::NSRect::new(
        objc2_foundation::NSPoint::new(d.x as f64, main_h - (d.y + d.h) as f64),
        objc2_foundation::NSSize::new(d.w as f64, d.h as f64),
    )
}

/// Does the window titled `title` still cover display `d` exactly? macOS moves a window back under the menu bar now
/// and then (displays or Spaces changing), which slid a mapped output down by the menu bar's height and showed the
/// desktop above it (report 2026-11-26: slices out of place, then back again later). None = window not found.
#[cfg(target_os = "macos")]
pub fn covers(title: &str, d: &Display) -> Option<bool> {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::{NSArray, NSString};
    let want = appkit_frame(d);
    let app_class = AnyClass::get(c"NSApplication")?;
    // SAFETY: AppKit reads on the main thread (the UI thread), on NSApp's own windows.
    unsafe {
        let app: Option<Retained<AnyObject>> = msg_send![app_class, sharedApplication];
        let windows: Option<Retained<NSArray<AnyObject>>> = msg_send![&*app?, windows];
        for w in windows.iter().flat_map(|a| a.iter()) {
            let t: Option<Retained<NSString>> = msg_send![&*w, title];
            if t.is_some_and(|t| t.to_string() == title) {
                let f: objc2_foundation::NSRect = msg_send![&*w, frame];
                let near = |a: f64, b: f64| (a - b).abs() < 1.0;
                return Some(near(f.origin.x, want.origin.x) && near(f.origin.y, want.origin.y) && near(f.size.width, want.size.width) && near(f.size.height, want.size.height));
            }
        }
    }
    None
}

/// Elsewhere a covered window stays where it was put.
#[cfg(not(target_os = "macos"))]
pub fn covers(_title: &str, _d: &Display) -> Option<bool> {
    Some(true)
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

/// Windows: where the mouse is on the desktop, in pixels (not points: each screen has its own scale).
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

/// Where the mouse is in the window of `ctx`, in egui points, asked of the system: during a drag from the Finder or
/// Explorer the window gets no mouse moves. None where the system can't say (Linux).
pub fn mouse_in_window(ctx: &egui::Context) -> Option<egui::Pos2> {
    let (x, y) = mouse_location()?;
    let inner = ctx.input(|i| i.viewport().inner_rect)?;
    // The Mac answers in points, divided here by egui's zoom; Windows in pixels, which are this window's points
    // times its pixels per point (its screen's scale and the zoom: at 125-150 % a drop landed on the wrong cell).
    let per = if cfg!(windows) { ctx.pixels_per_point() } else { ctx.zoom_factor() };
    Some(egui::pos2(x / per, y / per) - inner.min.to_vec2())
}

/// Windows: the borderless window still has a resize frame (Windows 11 draws it as a thin white line round the
/// window) and sits under the taskbar. Covering = a plain popup (no frame, no DWM border, square corners), topmost,
/// exactly the monitor's pixels, so Windows treats it as full screen and the taskbar goes behind it. Back to a window
/// = the styles it had before. (Bug report 2026-11-12: taskbar and a white border on the projector.)
#[cfg(windows)]
pub fn cover_window(_ctx: &egui::Context, title: &str, cover: Option<&Display>) -> bool {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GWL_EXSTYLE, GWL_STYLE, GetWindowLongPtrW, HWND_NOTOPMOST, HWND_TOPMOST, SWP_FRAMECHANGED,
        SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowLongPtrW, SetWindowPos, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME,
        WS_EX_STATICEDGE, WS_EX_WINDOWEDGE, WS_POPUP, WS_VISIBLE,
    };
    // Styles before covering, by window handle, to put back.
    static SAVED: Mutex<Option<HashMap<isize, (isize, isize)>>> = Mutex::new(None);
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWA_BORDER_COLOR: u32 = 34;
    let wide: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    // SAFETY: plain Win32 calls on our own window, found by its (unique) title; the attribute values are u32s we own.
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), wide.as_ptr());
        if hwnd.is_null() {
            return false;
        }
        let key = hwnd as isize;
        let dwm = |attr: u32, value: u32| {
            DwmSetWindowAttribute(hwnd, attr as _, &value as *const u32 as *const _, 4);
        };
        let mut saved = SAVED.lock().unwrap();
        let saved = saved.get_or_insert_with(HashMap::new);
        match cover {
            Some(d) => {
                saved.entry(key).or_insert_with(|| (GetWindowLongPtrW(hwnd, GWL_STYLE), GetWindowLongPtrW(hwnd, GWL_EXSTYLE)));
                let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & !((WS_EX_CLIENTEDGE | WS_EX_DLGMODALFRAME | WS_EX_STATICEDGE | WS_EX_WINDOWEDGE) as isize);
                SetWindowLongPtrW(hwnd, GWL_STYLE, (WS_POPUP | WS_VISIBLE) as isize);
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
                dwm(DWMWA_BORDER_COLOR, 0xFFFF_FFFE); // DWMWA_COLOR_NONE
                dwm(DWMWA_WINDOW_CORNER_PREFERENCE, 1); // DWMWCP_DONOTROUND
                // Exactly the display's pixels.
                let (x, y) = d.at_px;
                let (w, h) = (d.pixels.0 as i32, d.pixels.1 as i32);
                // Twice: moving onto a monitor with another scale makes the window resize itself for the new DPI
                // during the first call; the second one lands exactly.
                for _ in 0..2 {
                    SetWindowPos(hwnd, HWND_TOPMOST, x, y, w, h, SWP_FRAMECHANGED | SWP_SHOWWINDOW);
                }
            }
            None => {
                if let Some((style, ex)) = saved.remove(&key) {
                    SetWindowLongPtrW(hwnd, GWL_STYLE, style);
                    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
                }
                dwm(DWMWA_BORDER_COLOR, 0xFFFF_FFFF); // DWMWA_COLOR_DEFAULT
                dwm(DWMWA_WINDOW_CORNER_PREFERENCE, 0); // DWMWCP_DEFAULT
                SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED | SWP_SHOWWINDOW);
            }
        }
    }
    true
}

/// Linux: the output window (of `ctx`) goes full screen on exactly that screen (winit's borderless full screen on a
/// given monitor: X11 and Wayland alike; Wayland doesn't let a window place itself any other way), or back to a
/// window. False when the screen is gone.
#[cfg(target_os = "linux")]
pub fn cover_window(ctx: &egui::Context, _title: &str, cover: Option<&Display>) -> bool {
    match cover {
        Some(d) => {
            // (`displays` is in winit's order: the order SetMonitor counts in.)
            let Some(index) = displays().iter().position(|m| m.id == d.id) else { return false };
            ctx.send_viewport_cmd(egui::ViewportCommand::SetMonitor(index));
        }
        None => ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false)),
    }
    true
}

/// The browser (and any other system): nothing to cover with.
#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
pub fn cover_window(_ctx: &egui::Context, _title: &str, _cover: Option<&Display>) -> bool {
    false
}

#[cfg(all(test, target_os = "macos"))]
#[test]
fn mouse_location_is_on_the_desktop() {
    // With the screen locked there is no pointer on any desktop display (and no location at all): not a failure.
    let locked = crate::os::command("sh")
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

/// Keeps the displays awake (no display sleep, no screen saver, so no lock) while an output window is open: a show
/// mustn't go dark because nobody touched the Mac. Off again when the last output closes. (macOS: an
/// NSProcessInfo activity with NSActivityIdleDisplaySleepDisabled; other systems: nothing yet.)
#[cfg(target_os = "macos")]
pub fn keep_display_awake(on: bool) {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use std::sync::Mutex;
    /// The activity token, as a pointer (an Objective-C object isn't Send; it's only ever used from the main thread).
    struct Token(*mut AnyObject);
    // SAFETY: the token is created and ended on the main thread (this runs from the UI loop); the lock only holds it.
    unsafe impl Send for Token {}
    static TOKEN: Mutex<Option<Token>> = Mutex::new(None);
    let mut token = TOKEN.lock().unwrap_or_else(|e| e.into_inner());
    if on == token.is_some() {
        return;
    }
    let Some(class) = AnyClass::get(c"NSProcessInfo") else { return };
    // SAFETY: +processInfo, -beginActivityWithOptions:reason: and -endActivity: per NSProcessInfo.h.
    unsafe {
        let info: Option<Retained<AnyObject>> = msg_send![class, processInfo];
        let Some(info) = info else { return };
        if on {
            // NSActivityIdleDisplaySleepDisabled | NSActivityUserInitiated
            const OPTIONS: u64 = (1 << 40) | 0x00FF_FFFF;
            let reason = objc2_foundation::NSString::from_str("Phosphor has an output window open");
            let t: Option<Retained<AnyObject>> = msg_send![&*info, beginActivityWithOptions: OPTIONS, reason: &*reason];
            *token = t.map(|t| Token(Retained::into_raw(t)));
        } else if let Some(Token(raw)) = token.take() {
            let t: Retained<AnyObject> = Retained::from_raw(raw).expect("the token we made");
            let _: () = msg_send![&*info, endActivity: &*t];
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn keep_display_awake(_on: bool) {}
