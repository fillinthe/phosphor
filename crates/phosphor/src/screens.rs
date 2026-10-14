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
