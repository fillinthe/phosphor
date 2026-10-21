//! One FFGL plugin library: everything goes through its single exported function, `plugMain(code, value,
//! instance)` (FFGL 2, Resolume's SDK `FFGL.h`). Values are a union of a 32-bit number and a pointer.

use std::ffi::{CStr, c_char, c_void};
use std::path::Path;

use crate::dylib::Library;

// Function codes (FFGL.h).
const GET_INFO: u32 = 0;
const DEINITIALISE: u32 = 2;
const GET_NUM_PARAMETERS: u32 = 4;
const GET_PARAMETER_NAME: u32 = 5;
const GET_PARAMETER_DEFAULT: u32 = 6;
const SET_PARAMETER: u32 = 8;
const GET_PLUGIN_CAPS: u32 = 10;
const GET_PARAMETER_TYPE: u32 = 15;
const PROCESS_OPENGL: u32 = 17;
const INSTANTIATE_GL: u32 = 18;
const DEINSTANTIATE_GL: u32 = 19;
const SET_TIME: u32 = 20;
const CONNECT: u32 = 21;
const GET_NUM_PARAMETER_ELEMENTS: u32 = 31;
const INITIALISE_V2: u32 = 34;
const GET_PARAMETER_ELEMENT_NAME: u32 = 35;
const GET_PARAMETER_ELEMENT_VALUE: u32 = 36;
const SET_BEATINFO: u32 = 38;
const SET_HOSTINFO: u32 = 39;
const GET_PARAM_DISPLAY_NAME: u32 = 51;

const FAIL: u32 = 0xFFFF_FFFF;
const CAP_MINIMUM_INPUT_FRAMES: u32 = 10;

/// Plugin kinds.
pub const EFFECT: u32 = 0;
pub const SOURCE: u32 = 1;
pub const MIXER: u32 = 2;

/// Parameter types (FFGL.h `FF_TYPE_*`).
pub const TYPE_BOOLEAN: u32 = 0;
pub const TYPE_EVENT: u32 = 1;
pub const TYPE_OPTION: u32 = 11;
pub const TYPE_BUFFER: u32 = 12;
pub const TYPE_FILE: u32 = 14;
pub const TYPE_TEXT: u32 = 100;

#[repr(C)]
#[derive(Clone, Copy)]
pub union Mixed {
    pub uint: u32,
    pub ptr: *mut c_void,
}

impl Mixed {
    fn uint(v: u32) -> Mixed {
        // Clear the upper half too: some plugins read the whole pointer-sized value.
        let mut m = Mixed { ptr: std::ptr::null_mut() };
        m.uint = v;
        m
    }
    fn ptr<T>(p: *const T) -> Mixed {
        Mixed { ptr: p as *mut c_void }
    }
}

type PlugMain = unsafe extern "C" fn(u32, Mixed, *mut c_void) -> Mixed;

#[repr(C)]
struct PluginInfoStruct {
    api_major: u32,
    api_minor: u32,
    unique_id: [u8; 4],
    name: [u8; 16],
    kind: u32,
}

#[repr(C)]
struct SetParameterStruct {
    index: u32,
    value: Mixed,
}

#[repr(C)]
struct BeatInfo {
    bpm: f32,
    bar_phase: f32,
}

#[repr(C)]
struct HostInfo {
    name: *const c_char,
    version: *const c_char,
}

#[repr(C)]
pub struct Viewport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
pub struct GlTexture {
    pub width: u32,
    pub height: u32,
    pub hardware_width: u32,
    pub hardware_height: u32,
    pub handle: u32,
}

#[repr(C)]
pub struct ProcessGl {
    pub num_inputs: u32,
    pub inputs: *const *const GlTexture,
    pub host_fbo: u32,
}

#[repr(C)]
struct ElementQuery {
    param: u32,
    element: u32,
}

/// A loaded plugin library (stays loaded).
pub struct Plugin {
    main: PlugMain,
}

// SAFETY: only ever called from the thread that owns the GL context (the UI thread) or the probe process.
unsafe impl Send for Plugin {}

/// What the plugin says about itself.
#[derive(Clone, Debug)]
pub struct Info {
    pub name: String,
    pub kind: u32,
    /// Pictures it reads (effects 1, mixers 2, sources 0).
    pub inputs: u32,
}

/// A parameter as the plugin describes it.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ParamInfo {
    pub index: u32,
    pub name: String,
    pub kind: u32,
    pub default: f32,
    /// Option parameters: each choice's name and the value that picks it.
    pub options: Vec<(String, f32)>,
}

fn c_string(p: *mut c_void, max: usize) -> String {
    if p.is_null() || p as usize == FAIL as usize {
        return String::new();
    }
    // SAFETY: the plugin returns a NUL-terminated string it owns.
    let s = unsafe { CStr::from_ptr(p as *const c_char) }.to_string_lossy();
    s.chars().take(max).collect::<String>().trim().to_string()
}

impl Plugin {
    /// Loads a `.bundle` (its executable inside Contents/MacOS) or a plain library.
    pub fn load(path: &Path) -> Result<Plugin, String> {
        let exe = if path.is_dir() {
            let dir = path.join("Contents/MacOS");
            std::fs::read_dir(&dir).ok().and_then(|mut d| d.find_map(|e| e.ok().map(|e| e.path()))).ok_or("no executable inside the bundle")?
        } else {
            path.to_path_buf()
        };
        let lib = Library::open(&exe).ok_or("can't load it (built for another kind of Mac, or damaged)")?;
        let sym = lib.sym(c"plugMain").ok_or("not an FFGL plugin (no plugMain)")?;
        // SAFETY: plugMain has this signature in every FFGL plugin.
        let main: PlugMain = unsafe { std::mem::transmute::<*mut c_void, PlugMain>(sym) };
        Ok(Plugin { main })
    }

    fn call(&self, code: u32, value: Mixed, instance: *mut c_void) -> Mixed {
        // SAFETY: an FFGL call with the arguments FFGL.h documents for `code`.
        unsafe { (self.main)(code, value, instance) }
    }

    fn uint(&self, code: u32, value: Mixed, instance: *mut c_void) -> u32 {
        // SAFETY: these codes return a number.
        unsafe { self.call(code, value, instance).uint }
    }

    /// Initialises the library (once) and reads its name and kind.
    pub fn init(&self) -> Result<Info, String> {
        let p = unsafe { self.call(GET_INFO, Mixed::uint(0), std::ptr::null_mut()).ptr };
        if p.is_null() || p as usize == FAIL as usize {
            return Err("it didn't describe itself".into());
        }
        // SAFETY: GET_INFO returns a pointer to the plugin's static PluginInfoStruct.
        let info = unsafe { &*(p as *const PluginInfoStruct) };
        if info.api_major < 2 {
            return Err(format!("FFGL {}.{} (Phosphor needs FFGL 2)", info.api_major, info.api_minor));
        }
        if self.uint(INITIALISE_V2, Mixed::uint(0), std::ptr::null_mut()) == FAIL {
            return Err("it failed to start".into());
        }
        let name = String::from_utf8_lossy(&info.name).trim_end_matches('\0').trim().to_string();
        let min_inputs = self.uint(GET_PLUGIN_CAPS, Mixed::uint(CAP_MINIMUM_INPUT_FRAMES), std::ptr::null_mut());
        let inputs = match info.kind {
            SOURCE => 0,
            MIXER => 2,
            _ => min_inputs.clamp(1, 2),
        };
        Ok(Info { name, kind: info.kind, inputs })
    }

    pub fn params(&self) -> Vec<ParamInfo> {
        let null = std::ptr::null_mut();
        let n = self.uint(GET_NUM_PARAMETERS, Mixed::uint(0), null);
        if n == FAIL {
            return Vec::new();
        }
        (0..n.min(256))
            .map(|i| {
                let kind = self.uint(GET_PARAMETER_TYPE, Mixed::uint(i), null);
                // SAFETY: GET_PARAMETER_NAME returns a pointer, GET_PARAMETER_DEFAULT a float in the number half.
                let mut name = c_string(unsafe { self.call(GET_PARAMETER_NAME, Mixed::uint(i), null).ptr }, 64);
                let display = c_string(unsafe { self.call(GET_PARAM_DISPLAY_NAME, Mixed::uint(i), null).ptr }, 64);
                if !display.is_empty() {
                    name = display;
                }
                let default = if matches!(kind, TYPE_TEXT | TYPE_FILE | TYPE_BUFFER) { 0.0 } else { f32::from_bits(self.uint(GET_PARAMETER_DEFAULT, Mixed::uint(i), null)) };
                let mut options = Vec::new();
                if kind == TYPE_OPTION {
                    let count = self.uint(GET_NUM_PARAMETER_ELEMENTS, Mixed::uint(i), null);
                    for e in 0..if count == FAIL { 0 } else { count.min(64) } {
                        let q = ElementQuery { param: i, element: e };
                        // SAFETY: both take a pointer to {param, element}; the name is a pointer, the value a float.
                        let label = c_string(unsafe { self.call(GET_PARAMETER_ELEMENT_NAME, Mixed::ptr(&q), null).ptr }, 64);
                        let value = f32::from_bits(unsafe { self.call(GET_PARAMETER_ELEMENT_VALUE, Mixed::ptr(&q), null).uint });
                        options.push((label, value));
                    }
                }
                ParamInfo { index: i, name: if name.is_empty() { format!("Parameter {}", i + 1) } else { name }, kind, default: if default.is_finite() { default } else { 0.0 }, options }
            })
            .collect()
    }

    /// A new instance drawing at this size (a GL context must be current).
    pub fn instantiate(&self, width: u32, height: u32) -> Option<*mut c_void> {
        let vp = Viewport { x: 0, y: 0, width, height };
        // SAFETY: INSTANTIATE_GL takes a viewport pointer and returns the instance (or FAIL).
        let p = unsafe { self.call(INSTANTIATE_GL, Mixed::ptr(&vp), std::ptr::null_mut()).ptr };
        if p.is_null() || p as usize == FAIL as usize || p as u32 == FAIL {
            return None;
        }
        let host = HostInfo { name: c"Phosphor".as_ptr(), version: c"0.2".as_ptr() };
        self.call(SET_HOSTINFO, Mixed::ptr(&host), p);
        self.call(CONNECT, Mixed::uint(0), p);
        Some(p)
    }

    pub fn deinstantiate(&self, instance: *mut c_void) {
        self.call(DEINSTANTIATE_GL, Mixed::uint(0), instance);
    }

    pub fn set_param(&self, instance: *mut c_void, index: u32, value: f32) {
        let s = SetParameterStruct { index, value: Mixed::uint(value.to_bits()) };
        self.call(SET_PARAMETER, Mixed::ptr(&s), instance);
    }

    pub fn set_time(&self, instance: *mut c_void, seconds: f64) {
        self.call(SET_TIME, Mixed::ptr(&seconds), instance);
    }

    pub fn set_beat(&self, instance: *mut c_void, bpm: f32, bar_phase: f32) {
        let b = BeatInfo { bpm, bar_phase };
        self.call(SET_BEATINFO, Mixed::ptr(&b), instance);
    }

    /// Draws into the bound framebuffer `host_fbo`, reading `inputs`.
    pub fn process(&self, instance: *mut c_void, inputs: &[&GlTexture], host_fbo: u32) -> bool {
        let ptrs: Vec<*const GlTexture> = inputs.iter().map(|t| *t as *const GlTexture).collect();
        let p = ProcessGl { num_inputs: ptrs.len() as u32, inputs: ptrs.as_ptr(), host_fbo };
        self.uint(PROCESS_OPENGL, Mixed::ptr(&p), instance) != FAIL
    }

    #[allow(dead_code)]
    pub fn deinit(&self) {
        self.call(DEINITIALISE, Mixed::uint(0), std::ptr::null_mut());
    }
}
