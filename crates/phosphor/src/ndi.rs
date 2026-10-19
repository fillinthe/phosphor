//! NDI output and input. Out: sends the composition over the network so MadMapper, Resolume, OBS, vMix or
//! any NDI app on another computer can use it as a source. In: lists NDI sources on the network and
//! receives one as a layer (`NdiReceiver`, newest frame wins). The NDI library (free, from ndi.video) is
//! loaded at runtime from Phosphor.app, the NDI Tools runtime or the NDI SDK (Mac), or the NDI Runtime
//! (Windows); without it Phosphor runs as usual and the panel says where to get it.

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

#[repr(C)]
struct FindCreate {
    show_local_sources: bool,
    p_groups: *const c_char,
    p_extra_ips: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Source {
    p_ndi_name: *const c_char,
    p_url_address: *const c_char,
}

#[repr(C)]
struct RecvCreateV3 {
    source_to_connect_to: Source,
    color_format: c_int,
    bandwidth: c_int,
    allow_video_fields: bool,
    p_ndi_recv_name: *const c_char,
}

const RECV_COLOR_RGBX_RGBA: c_int = 2;
const RECV_BANDWIDTH_HIGHEST: c_int = 100;
const FRAME_TYPE_VIDEO: c_int = 1;

/// The receiving half of the NDI API (older runtimes may lack it).
struct RecvFns {
    find_create: unsafe extern "C" fn(*const FindCreate) -> *mut c_void,
    find_sources: unsafe extern "C" fn(*mut c_void, *mut u32) -> *const Source,
    recv_create: unsafe extern "C" fn(*const RecvCreateV3) -> *mut c_void,
    recv_capture: unsafe extern "C" fn(*mut c_void, *mut VideoFrameV2, *mut c_void, *mut c_void, u32) -> c_int,
    recv_free_video: unsafe extern "C" fn(*mut c_void, *const VideoFrameV2),
    recv_destroy: unsafe extern "C" fn(*mut c_void),
}

struct Lib {
    path: PathBuf,
    send_create: unsafe extern "C" fn(*const SendCreate) -> *mut c_void,
    send_destroy: unsafe extern "C" fn(*mut c_void),
    send_video: unsafe extern "C" fn(*mut c_void, *const VideoFrameV2),
    connections: unsafe extern "C" fn(*mut c_void, u32) -> c_int,
    recv: Option<RecvFns>,
}

// SAFETY: plain C function pointers into a library that stays loaded for the process lifetime.
unsafe impl Send for Lib {}
unsafe impl Sync for Lib {}

#[cfg(windows)]
fn candidates() -> Vec<PathBuf> {
    const DLL: &str = "Processing.NDI.Lib.x64.dll";
    let mut out = Vec::new();
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.to_path_buf())) {
        out.push(dir.join(DLL));
    }
    // The NDI Runtime / NDI Tools installers set these.
    for var in ["NDI_RUNTIME_DIR_V6", "NDI_RUNTIME_DIR_V5"] {
        if let Some(dir) = std::env::var_os(var) {
            out.push(PathBuf::from(dir).join(DLL));
        }
    }
    let programs = std::env::var_os("ProgramFiles").map(PathBuf::from).unwrap_or_else(|| r"C:\Program Files".into());
    out.push(programs.join(r"NDI\NDI 6 Runtime\v6").join(DLL));
    out.push(programs.join(r"NDI\NDI 5 Runtime\v5").join(DLL));
    out
}

#[cfg(not(windows))]
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
    // NDI Tools for Mac doesn't install a shared runtime; its apps carry NDI's own official
    // library. Use that (only NDI's own apps, never other vendors' copies).
    for app in ["NDI Scan Converter", "NDI Test Patterns", "NDI Video Monitor", "NDI Discovery", "NDI Router"] {
        out.push(format!("/Applications/{app}.app/Contents/Frameworks/libndi.dylib").into());
        out.push(format!("/Applications/{app}.app/Contents/Frameworks/libndi_advanced.dylib").into());
    }
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
            let Some(h) = crate::dylib::Library::open(&path) else { continue };
            let sym = |name: &std::ffi::CStr| h.sym(name);
            // SAFETY: the documented NDI C entry points, cast to their declared signatures.
            unsafe {
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
                let recv = match (
                    sym(c"NDIlib_find_create_v2"),
                    sym(c"NDIlib_find_get_current_sources"),
                    sym(c"NDIlib_recv_create_v3"),
                    sym(c"NDIlib_recv_capture_v2"),
                    sym(c"NDIlib_recv_free_video_v2"),
                    sym(c"NDIlib_recv_destroy"),
                ) {
                    (Some(fc), Some(fs), Some(rc), Some(cap), Some(free), Some(rd)) => Some(RecvFns {
                        find_create: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*const FindCreate) -> *mut c_void>(fc),
                        find_sources: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*mut c_void, *mut u32) -> *const Source>(fs),
                        recv_create: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*const RecvCreateV3) -> *mut c_void>(rc),
                        recv_capture: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*mut c_void, *mut VideoFrameV2, *mut c_void, *mut c_void, u32) -> c_int>(cap),
                        recv_free_video: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*mut c_void, *const VideoFrameV2)>(free),
                        recv_destroy: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*mut c_void)>(rd),
                    }),
                    _ => None,
                };
                return Some(Lib {
                    path,
                    send_create: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*const SendCreate) -> *mut c_void>(create),
                    send_destroy: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*mut c_void)>(destroy),
                    send_video: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*mut c_void, *const VideoFrameV2)>(video),
                    connections: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*mut c_void, u32) -> c_int>(conns),
                    recv,
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
    pacer: crate::pacing::FramePacer,
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
        self.connections() > 0 && self.pacer.due(now, fps)
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

/// NDI sources on the network right now (names like "STUDIO-MAC (OBS)"). The first call starts a
/// background finder that keeps the list fresh; returns an empty list without NDI.
pub fn sources() -> Vec<String> {
    static LIST: OnceLock<Arc<std::sync::Mutex<Vec<String>>>> = OnceLock::new();
    let list = LIST.get_or_init(|| {
        let list = Arc::new(std::sync::Mutex::new(Vec::new()));
        if let Some(r) = lib().and_then(|l| l.recv.as_ref()) {
            let out = list.clone();
            let _ = std::thread::Builder::new().name("phosphor-ndi-find".into()).spawn(move || {
                let create = FindCreate { show_local_sources: true, p_groups: std::ptr::null(), p_extra_ips: std::ptr::null() };
                // SAFETY: NDI C API; the finder lives for the rest of the process on this thread.
                let finder = unsafe { (r.find_create)(&create) };
                if finder.is_null() {
                    return;
                }
                loop {
                    let mut n = 0u32;
                    // SAFETY: the returned array stays valid until the next call on this thread.
                    let names: Vec<String> = unsafe {
                        let p = (r.find_sources)(finder, &mut n);
                        (0..n as usize)
                            .filter_map(|i| {
                                let s = *p.add(i);
                                (!s.p_ndi_name.is_null()).then(|| std::ffi::CStr::from_ptr(s.p_ndi_name).to_string_lossy().into_owned())
                            })
                            .collect()
                    };
                    if let Ok(mut l) = out.lock() {
                        *l = names;
                    }
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            });
        }
        list
    });
    list.lock().map(|l| l.clone()).unwrap_or_default()
}

/// A picture from an NDI receiver: (width, height, RGBA rows tightly packed).
pub type NdiFrame = (u32, u32, Vec<u8>);

/// Receives one NDI source on its own thread; `take` hands out the newest frame.
pub struct NdiReceiver {
    latest: Arc<std::sync::Mutex<Option<NdiFrame>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl NdiReceiver {
    pub fn connect(source: &str) -> Result<Self, String> {
        let r = lib().ok_or("NDI isn't installed on this Mac (free NDI Tools from ndi.video)")?.recv.as_ref().ok_or("this NDI runtime can't receive")?;
        let latest = Arc::new(std::sync::Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let (latest2, stop2, name) = (latest.clone(), stop.clone(), CString::new(source).map_err(|_| "bad source name")?);
        let thread = std::thread::Builder::new()
            .name(format!("ndi in: {source}"))
            .spawn(move || {
                let recv_name = c"Phosphor";
                let create = RecvCreateV3 {
                    source_to_connect_to: Source { p_ndi_name: name.as_ptr(), p_url_address: std::ptr::null() },
                    color_format: RECV_COLOR_RGBX_RGBA,
                    bandwidth: RECV_BANDWIDTH_HIGHEST,
                    allow_video_fields: false,
                    p_ndi_recv_name: recv_name.as_ptr(),
                };
                // SAFETY: NDI C API; the receiver is used and destroyed on this thread only.
                let recv = unsafe { (r.recv_create)(&create) };
                if recv.is_null() {
                    return;
                }
                while !stop2.load(Ordering::Relaxed) {
                    // SAFETY: plain C struct the receiver fills in.
                    let mut f: VideoFrameV2 = unsafe { std::mem::zeroed() };
                    let kind = unsafe { (r.recv_capture)(recv, &mut f, std::ptr::null_mut(), std::ptr::null_mut(), 100) };
                    if kind != FRAME_TYPE_VIDEO {
                        continue;
                    }
                    let (w, h) = (f.xres.max(0) as usize, f.yres.max(0) as usize);
                    let stride = (f.line_stride_in_bytes.max(0) as usize).max(w * 4);
                    if w > 0 && h > 0 && !f.p_data.is_null() {
                        // SAFETY: `h` rows of `stride` bytes, valid until the frame is freed below.
                        let data = unsafe { std::slice::from_raw_parts(f.p_data, stride * h) };
                        let mut rgba = Vec::with_capacity(w * h * 4);
                        for row in data.chunks(stride) {
                            rgba.extend_from_slice(&row[..w * 4]);
                        }
                        // RGBX: make the unused byte opaque.
                        if f.four_cc == FOURCC_RGBX {
                            rgba.chunks_exact_mut(4).for_each(|p| p[3] = 255);
                        }
                        if let Ok(mut l) = latest2.lock() {
                            *l = Some((w as u32, h as u32, rgba));
                        }
                    }
                    // SAFETY: the frame came from this receiver's capture.
                    unsafe { (r.recv_free_video)(recv, &f) };
                }
                // SAFETY: valid receiver, destroyed once.
                unsafe { (r.recv_destroy)(recv) };
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { latest, stop, thread: Some(thread) })
    }

    /// The newest frame since the last call.
    pub fn take(&self) -> Option<NdiFrame> {
        self.latest.lock().ok()?.take()
    }
}

impl Drop for NdiReceiver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
