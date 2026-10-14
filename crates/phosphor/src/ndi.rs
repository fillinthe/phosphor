//! NDI output: sends the composition over the network so MadMapper, Resolume, OBS, vMix or any
//! NDI app on another computer can use it as a source. The NDI library (free, from ndi.video) is
//! loaded at runtime from Phosphor.app, the NDI Tools runtime or the NDI SDK; without it Phosphor
//! runs as usual and the panel says where to get it.

use std::ffi::{CString, c_char, c_int, c_void};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, OnceLock};

#[repr(C)]
struct SendCreate {
    p_ndi_name: *const c_char,
    p_groups: *const c_char,
    clock_video: bool,
    clock_audio: bool,
}

#[repr(C)]
struct VideoFrameV2 {
    xres: c_int,
    yres: c_int,
    four_cc: u32,
    frame_rate_n: c_int,
    frame_rate_d: c_int,
    picture_aspect_ratio: f32,
    frame_format_type: c_int,
    timecode: i64,
    p_data: *const u8,
    line_stride_in_bytes: c_int,
    p_metadata: *const c_char,
    timestamp: i64,
}

const fn fourcc(a: u8, b: u8, c: u8, d: u8) -> u32 {
    a as u32 | (b as u32) << 8 | (c as u32) << 16 | (d as u32) << 24
}
/// RGBA with the alpha ignored: receivers always get an opaque picture.
const FOURCC_RGBX: u32 = fourcc(b'R', b'G', b'B', b'X');
const FORMAT_PROGRESSIVE: c_int = 1;
const TIMECODE_SYNTHESIZE: i64 = i64::MAX;

struct Lib {
    path: PathBuf,
    send_create: unsafe extern "C" fn(*const SendCreate) -> *mut c_void,
    send_destroy: unsafe extern "C" fn(*mut c_void),
    send_video: unsafe extern "C" fn(*mut c_void, *const VideoFrameV2),
    connections: unsafe extern "C" fn(*mut c_void, u32) -> c_int,
}

// SAFETY: plain C function pointers into a library that stays loaded for the process lifetime.
unsafe impl Send for Lib {}
unsafe impl Sync for Lib {}

fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("../Frameworks"))) {
        out.push(dir.join("libndi.dylib"));
    }
    for var in ["NDI_RUNTIME_DIR_V6", "NDI_RUNTIME_DIR_V5"] {
        if let Some(dir) = std::env::var_os(var) {
            out.push(PathBuf::from(dir).join("libndi.dylib"));
        }
    }
    out.push("/usr/local/lib/libndi.dylib".into());
    out.push("/Library/NDI SDK for Apple/lib/macOS/libndi.dylib".into());
    out.push("/Library/Application Support/NDI/lib/libndi.dylib".into());
    out
}

/// Loads the NDI library once; `None` if it isn't installed.
fn lib() -> Option<&'static Lib> {
    static LIB: OnceLock<Option<Lib>> = OnceLock::new();
    LIB.get_or_init(|| {
        for path in candidates() {
            if !path.is_file() {
                continue;
            }
            let Ok(c) = CString::new(path.to_string_lossy().as_bytes()) else { continue };
            // SAFETY: loading a shared library and looking up the documented NDI C entry points.
            unsafe {
                let h = libc::dlopen(c.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
                if h.is_null() {
                    continue;
                }
                let sym = |name: &std::ffi::CStr| {
                    let p = libc::dlsym(h, name.as_ptr());
                    (!p.is_null()).then_some(p)
                };
                let (Some(init), Some(create), Some(destroy), Some(video), Some(conns)) = (
                    sym(c"NDIlib_initialize"),
                    sym(c"NDIlib_send_create"),
                    sym(c"NDIlib_send_destroy"),
                    sym(c"NDIlib_send_send_video_v2"),
                    sym(c"NDIlib_send_get_no_connections"),
                ) else {
                    continue;
                };
                let init: unsafe extern "C" fn() -> bool = std::mem::transmute(init);
                if !init() {
                    continue;
                }
                return Some(Lib {
                    path,
                    send_create: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*const SendCreate) -> *mut c_void>(create),
                    send_destroy: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*mut c_void)>(destroy),
                    send_video: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*mut c_void, *const VideoFrameV2)>(video),
                    connections: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*mut c_void, u32) -> c_int>(conns),
                });
            }
        }
        None
    })
    .as_ref()
}

struct Frame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    fps: u32,
}

struct Running {
    name: String,
    tx: SyncSender<Frame>,
    stop: Arc<AtomicBool>,
    connections: Arc<AtomicI32>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[derive(Default)]
pub struct Ndi {
    running: Option<Running>,
    next_frame: f64,
    pub error: Option<String>,
}

impl Ndi {
    /// Where the NDI library was found, or `None` if NDI isn't installed.
    pub fn library() -> Option<PathBuf> {
        lib().map(|l| l.path.clone())
    }

    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    /// Receivers watching right now.
    pub fn connections(&self) -> i32 {
        self.running.as_ref().map_or(0, |r| r.connections.load(Ordering::Relaxed))
    }

    /// Starts, renames or stops the sender to match the settings.
    pub fn sync(&mut self, enabled: bool, name: &str) {
        let name = if name.trim().is_empty() { "Phosphor" } else { name.trim() };
        if !enabled {
            self.running = None;
            return;
        }
        if self.running.as_ref().is_some_and(|r| r.name == name) {
            return;
        }
        self.running = None;
        let Some(lib) = lib() else {
            self.error = Some("NDI isn't installed on this Mac".into());
            return;
        };
        let (tx, rx) = sync_channel::<Frame>(2);
        let stop = Arc::new(AtomicBool::new(false));
        let connections = Arc::new(AtomicI32::new(0));
        let (stop2, conns2, sender_name) = (stop.clone(), connections.clone(), name.to_string());
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let thread = std::thread::Builder::new().name("phosphor-ndi".into()).spawn(move || {
            let cname = CString::new(sender_name).unwrap_or_default();
            let create = SendCreate { p_ndi_name: cname.as_ptr(), p_groups: std::ptr::null(), clock_video: false, clock_audio: false };
            // SAFETY: NDI C API; the instance is used and destroyed on this thread only.
            let inst = unsafe { (lib.send_create)(&create) };
            if inst.is_null() {
                let _ = ready_tx.send(Err("NDI couldn't create the sender".into()));
                return;
            }
            let _ = ready_tx.send(Ok(()));
            while !stop2.load(Ordering::Relaxed) {
                // SAFETY: valid instance; timeout 0 just reads the count.
                conns2.store(unsafe { (lib.connections)(inst, 0) }, Ordering::Relaxed);
                let Ok(f) = rx.recv_timeout(std::time::Duration::from_millis(250)) else { continue };
                let frame = VideoFrameV2 {
                    xres: f.width as c_int,
                    yres: f.height as c_int,
                    four_cc: FOURCC_RGBX,
                    frame_rate_n: f.fps as c_int,
                    frame_rate_d: 1,
                    picture_aspect_ratio: 0.0,
                    frame_format_type: FORMAT_PROGRESSIVE,
                    timecode: TIMECODE_SYNTHESIZE,
                    p_data: f.rgba.as_ptr(),
                    line_stride_in_bytes: (f.width * 4) as c_int,
                    p_metadata: std::ptr::null(),
                    timestamp: 0,
                };
                // SAFETY: synchronous send; NDI is done with `f.rgba` when this returns.
                unsafe { (lib.send_video)(inst, &frame) };
            }
            // SAFETY: valid instance, destroyed once.
            unsafe { (lib.send_destroy)(inst) };
        });
        let thread = match thread {
            Ok(t) => t,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        match ready_rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(Ok(())) => {
                self.error = None;
                self.running = Some(Running { name: name.to_string(), tx, stop, connections, thread: Some(thread) });
            }
            Ok(Err(e)) => self.error = Some(e),
            Err(_) => self.error = Some("NDI didn't start".into()),
        }
    }

    /// True when a frame should be captured for NDI now (only while someone is watching).
    pub fn frame_due(&mut self, now: f64, fps: u32) -> bool {
        if self.connections() <= 0 || now < self.next_frame {
            return false;
        }
        let step = 1.0 / fps.max(1) as f64;
        self.next_frame = (self.next_frame + step).max(now - step);
        true
    }

    /// Hands a frame to the sender; skipped if it's still busy with the last one.
    pub fn push(&mut self, width: u32, height: u32, rgba: Vec<u8>, fps: u32) {
        let Some(r) = &self.running else { return };
        if let Err(TrySendError::Disconnected(_)) = r.tx.try_send(Frame { width, height, rgba, fps }) {
            self.running = None;
            self.error = Some("NDI sender stopped".into());
        }
    }
}
