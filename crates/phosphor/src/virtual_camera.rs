//! Virtual camera (macOS): Phosphor's program as a camera called "Phosphor" in Zoom, OBS, Teams… The camera itself is
//! a Core Media I/O camera extension inside the app (macos/PhosphorCamera, scripts/build-camera-extension.sh); this
//! side installs it (`activate`) and sends it frames (`Sender`) into its hidden sink stream, 1920×1080 BGRA.
//!
//! macOS runs the extension only when the app is in /Applications and both are signed with a Developer ID and
//! provisioning profiles that allow it: until then `activate` reports why it can't, and nothing else happens.

use std::ffi::c_void;
use std::sync::Mutex;

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, NSObject, NSObjectProtocol};
use objc2::{define_class, msg_send, AllocAnyThread};
use objc2_foundation::NSString;

/// The extension's bundle id (the app's id + ".camera").
pub const EXTENSION_ID: &str = "co.fillintheblank.phosphor.camera";
/// What the camera offers (Provider.swift says the same).
pub const WIDTH: u32 = 1920;
pub const HEIGHT: u32 = 1080;
pub const FPS: u32 = 30;

/// What installing the camera last said, for the UI.
static STATUS: Mutex<String> = Mutex::new(String::new());

pub fn status() -> String {
    STATUS.lock().map(|s| s.clone()).unwrap_or_default()
}

fn set_status(s: impl Into<String>) {
    if let Ok(mut st) = STATUS.lock() {
        *st = s.into();
    }
}

/// The extension is inside this copy of the app (built by scripts/build-camera-extension.sh).
pub fn bundled() -> bool {
    std::env::current_exe().ok().and_then(|e| Some(e.parent()?.parent()?.join("Library/SystemExtensions/PhosphorCamera.systemextension"))).is_some_and(|p| p.exists())
}

#[link(name = "SystemExtensions", kind = "framework")]
unsafe extern "C" {}

unsafe extern "C" {
    static _dispatch_main_q: c_void;
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; no Drop.
    #[unsafe(super(NSObject))]
    #[name = "PhosphorCameraInstallDelegate"]
    struct InstallDelegate;

    unsafe impl NSObjectProtocol for InstallDelegate {}

    impl InstallDelegate {
        /// An older copy is installed: replace it with this one.
        #[unsafe(method(request:actionForReplacingExtension:withExtension:))]
        fn replace(&self, _request: &AnyObject, _old: &AnyObject, _new: &AnyObject) -> isize {
            1 // OSSystemExtensionReplacementActionReplace
        }

        #[unsafe(method(requestNeedsUserApproval:))]
        fn needs_approval(&self, _request: &AnyObject) {
            set_status("Allow it in System Settings › General › Login Items & Extensions (Camera Extensions), then turn the camera on again.");
        }

        #[unsafe(method(request:didFinishWithResult:))]
        fn finished(&self, _request: &AnyObject, result: isize) {
            set_status(if result == 0 { "Installed: apps can now pick the camera \"Phosphor\"." } else { "Installed; it works after a restart." });
        }

        #[unsafe(method(request:didFailWithError:))]
        fn failed(&self, _request: &AnyObject, error: &AnyObject) {
            // SAFETY: an NSError.
            let text: Option<Retained<NSString>> = unsafe { msg_send![error, localizedDescription] };
            set_status(format!("macOS didn't install the camera: {}", text.map(|t| t.to_string()).unwrap_or_default()));
        }
    }
);

/// Asks macOS to install (or update) the camera extension. The answer arrives later in `status()`.
pub fn activate() {
    if !bundled() {
        set_status("This copy of Phosphor has no camera extension (it's built by scripts/build-camera-extension.sh).");
        return;
    }
    set_status("Asking macOS…");
    // SAFETY: SystemExtensions API on the main thread; the delegate is kept alive for the app's lifetime (leaked once).
    unsafe {
        let (Some(request_class), Some(manager_class)) = (AnyClass::get(c"OSSystemExtensionRequest"), AnyClass::get(c"OSSystemExtensionManager")) else {
            set_status("System extensions aren't available on this macOS.");
            return;
        };
        let id = NSString::from_str(EXTENSION_ID);
        let queue = &_dispatch_main_q as *const c_void as *const AnyObject;
        let request: Retained<AnyObject> = msg_send![request_class, activationRequestForExtension: &*id, queue: queue];
        let delegate: Retained<InstallDelegate> = msg_send![InstallDelegate::alloc(), init];
        let _: () = msg_send![&*request, setDelegate: &*delegate];
        std::mem::forget(delegate);
        let manager: Retained<AnyObject> = msg_send![manager_class, sharedManager];
        let _: () = msg_send![&*manager, submitRequest: &*request];
    }
}

// ---- sending frames: Core Media I/O's C API ----

#[repr(C)]
#[derive(Clone, Copy)]
struct PropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

const fn fourcc(s: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*s)
}

const SYSTEM_OBJECT: u32 = 1;
const GLOBAL: u32 = fourcc(b"glob");
const DEVICES: u32 = fourcc(b"dev#");
const NAME: u32 = fourcc(b"lnam");
const STREAMS: u32 = fourcc(b"stm#");

#[link(name = "CoreMediaIO", kind = "framework")]
unsafe extern "C" {
    fn CMIOObjectGetPropertyDataSize(object: u32, address: *const PropertyAddress, qualifier_size: u32, qualifier: *const c_void, size: *mut u32) -> i32;
    fn CMIOObjectGetPropertyData(object: u32, address: *const PropertyAddress, qualifier_size: u32, qualifier: *const c_void, size: u32, used: *mut u32, data: *mut c_void) -> i32;
    fn CMIOStreamCopyBufferQueue(stream: u32, altered: *const c_void, refcon: *mut c_void, queue: *mut *mut c_void) -> i32;
    fn CMIODeviceStartStream(device: u32, stream: u32) -> i32;
    fn CMIODeviceStopStream(device: u32, stream: u32) -> i32;
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}

#[repr(C)]
struct SampleTiming {
    duration: CMTime,
    pts: CMTime,
    dts: CMTime,
}

#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    fn CMSimpleQueueEnqueue(queue: *mut c_void, element: *const c_void) -> i32;
    fn CMSimpleQueueGetCount(queue: *mut c_void) -> i32;
    fn CMSimpleQueueGetCapacity(queue: *mut c_void) -> i32;
    fn CMClockGetHostTimeClock() -> *mut c_void;
    fn CMClockGetTime(clock: *mut c_void) -> CMTime;
    fn CMVideoFormatDescriptionCreateForImageBuffer(alloc: *const c_void, image: *mut c_void, out: *mut *mut c_void) -> i32;
    fn CMSampleBufferCreateReadyWithImageBuffer(alloc: *const c_void, image: *mut c_void, format: *mut c_void, timing: *const SampleTiming, out: *mut *mut c_void) -> i32;
}

#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    fn CVPixelBufferCreate(alloc: *const c_void, w: usize, h: usize, format: u32, attrs: *const c_void, out: *mut *mut c_void) -> i32;
    fn CVPixelBufferLockBaseAddress(pb: *mut c_void, flags: u64) -> i32;
    fn CVPixelBufferUnlockBaseAddress(pb: *mut c_void, flags: u64) -> i32;
    fn CVPixelBufferGetBaseAddress(pb: *mut c_void) -> *mut u8;
    fn CVPixelBufferGetBytesPerRow(pb: *mut c_void) -> usize;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
    fn CFStringGetLength(s: *const c_void) -> isize;
    fn CFStringGetCString(s: *const c_void, buf: *mut u8, size: isize, encoding: u32) -> bool;
}

/// An object's list property (devices of the system, streams of a device).
fn ids(object: u32, selector: u32) -> Vec<u32> {
    let addr = PropertyAddress { selector, scope: GLOBAL, element: 0 };
    let mut size = 0u32;
    // SAFETY: plain C calls with valid out-pointers.
    unsafe {
        if CMIOObjectGetPropertyDataSize(object, &addr, 0, std::ptr::null(), &mut size) != 0 || size == 0 {
            return Vec::new();
        }
        let mut out = vec![0u32; size as usize / 4];
        let mut used = 0u32;
        if CMIOObjectGetPropertyData(object, &addr, 0, std::ptr::null(), size, &mut used, out.as_mut_ptr() as *mut c_void) != 0 {
            return Vec::new();
        }
        out.truncate(used as usize / 4);
        out
    }
}

/// An object's name.
fn name(object: u32) -> String {
    let addr = PropertyAddress { selector: NAME, scope: GLOBAL, element: 0 };
    let mut cf: *const c_void = std::ptr::null();
    let mut used = 0u32;
    // SAFETY: the property is a CFStringRef we own (released below).
    unsafe {
        if CMIOObjectGetPropertyData(object, &addr, 0, std::ptr::null(), std::mem::size_of::<*const c_void>() as u32, &mut used, &mut cf as *mut _ as *mut c_void) != 0 || cf.is_null() {
            return String::new();
        }
        let mut buf = vec![0u8; (CFStringGetLength(cf) * 4 + 1).max(1) as usize];
        let ok = CFStringGetCString(cf, buf.as_mut_ptr(), buf.len() as isize, 0x0800_0100);
        CFRelease(cf);
        if !ok {
            return String::new();
        }
        String::from_utf8_lossy(&buf[..buf.iter().position(|b| *b == 0).unwrap_or(0)]).to_string()
    }
}

/// Feeds the camera: connected to its sink stream while the virtual camera is on.
pub struct Sender {
    device: u32,
    stream: u32,
    queue: *mut c_void,
    scaler: Option<ffmpeg_next::software::scaling::Context>,
    size: (u32, u32),
}

// SAFETY: the queue and ids are only used from the thread that owns the Sender (the UI thread).
unsafe impl Send for Sender {}

impl Sender {
    /// Finds the "Phosphor" camera and opens its sink ("Phosphor in"). None when the extension isn't running.
    pub fn connect() -> Option<Sender> {
        let device = ids(SYSTEM_OBJECT, DEVICES).into_iter().find(|d| name(*d) == "Phosphor")?;
        let stream = ids(device, STREAMS).into_iter().find(|s| name(*s) == "Phosphor in")?;
        let mut queue: *mut c_void = std::ptr::null_mut();
        // SAFETY: valid ids from CMIO; the queue is retained for us.
        unsafe {
            if CMIOStreamCopyBufferQueue(stream, std::ptr::null(), std::ptr::null_mut(), &mut queue) != 0 || queue.is_null() {
                return None;
            }
            if CMIODeviceStartStream(device, stream) != 0 {
                CFRelease(queue);
                return None;
            }
        }
        Some(Sender { device, stream, queue, scaler: None, size: (0, 0) })
    }

    /// One frame of the program (RGBA, any size): scaled to the camera's 1920×1080 BGRA and queued, unless the
    /// queue is full (the camera's consumer is behind: this frame is skipped).
    pub fn send(&mut self, width: u32, height: u32, rgba: &[u8]) {
        use ffmpeg_next::format::Pixel;
        // SAFETY: the queue stays valid while the Sender lives.
        if unsafe { CMSimpleQueueGetCount(self.queue) >= CMSimpleQueueGetCapacity(self.queue) } {
            return;
        }
        if self.size != (width, height) || self.scaler.is_none() {
            self.scaler = ffmpeg_next::software::scaling::Context::get(Pixel::RGBA, width, height, Pixel::BGRA, WIDTH, HEIGHT, ffmpeg_next::software::scaling::Flags::BILINEAR).ok();
            self.size = (width, height);
        }
        let Some(scaler) = self.scaler.as_mut() else { return };
        let mut src = ffmpeg_next::frame::Video::new(Pixel::RGBA, width, height);
        let stride = src.stride(0);
        for (y, row) in rgba.chunks_exact(width as usize * 4).take(height as usize).enumerate() {
            src.data_mut(0)[y * stride..y * stride + row.len()].copy_from_slice(row);
        }
        let mut dst = ffmpeg_next::frame::Video::new(Pixel::BGRA, WIDTH, HEIGHT);
        if scaler.run(&src, &mut dst).is_err() {
            return;
        }
        // SAFETY: CoreVideo / CoreMedia objects created and handed over here; the sample buffer's +1 goes to the
        // queue (the extension releases it), everything else is released.
        unsafe {
            let mut pb: *mut c_void = std::ptr::null_mut();
            if CVPixelBufferCreate(std::ptr::null(), WIDTH as usize, HEIGHT as usize, u32::from_be_bytes(*b"BGRA"), std::ptr::null(), &mut pb) != 0 || pb.is_null() {
                return;
            }
            CVPixelBufferLockBaseAddress(pb, 0);
            let base = CVPixelBufferGetBaseAddress(pb);
            let bpr = CVPixelBufferGetBytesPerRow(pb);
            let src_stride = dst.stride(0);
            let data = dst.data(0);
            for y in 0..HEIGHT as usize {
                std::ptr::copy_nonoverlapping(data.as_ptr().add(y * src_stride), base.add(y * bpr), WIDTH as usize * 4);
            }
            CVPixelBufferUnlockBaseAddress(pb, 0);
            let mut format: *mut c_void = std::ptr::null_mut();
            CMVideoFormatDescriptionCreateForImageBuffer(std::ptr::null(), pb, &mut format);
            let now = CMClockGetTime(CMClockGetHostTimeClock());
            let timing = SampleTiming { duration: CMTime { value: 1, timescale: FPS as i32, flags: 1, epoch: 0 }, pts: now, dts: CMTime { value: 0, timescale: 0, flags: 0, epoch: 0 } };
            let mut sample: *mut c_void = std::ptr::null_mut();
            let ok = !format.is_null() && CMSampleBufferCreateReadyWithImageBuffer(std::ptr::null(), pb, format, &timing, &mut sample) == 0 && !sample.is_null();
            if !format.is_null() {
                CFRelease(format);
            }
            CFRelease(pb);
            if ok && CMSimpleQueueEnqueue(self.queue, sample) != 0 {
                CFRelease(sample);
            }
        }
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        // SAFETY: what `connect` started and retained.
        unsafe {
            CMIODeviceStopStream(self.device, self.stream);
            CFRelease(self.queue);
        }
    }
}

/// The virtual camera as the app holds it: on/off, the connection to the camera (made while it's on, retried every
/// 2 s while the extension isn't there yet) and its 30 fps pacing. Does nothing while off.
#[derive(Default)]
pub struct VirtualCamera {
    pub on: bool,
    sender: Option<Sender>,
    pacer: crate::pacing::FramePacer,
    retry_at: f64,
}

impl VirtualCamera {
    /// A frame is wanted now (connects first when it can).
    pub fn frame_due(&mut self, now: f64) -> bool {
        if !self.on {
            self.sender = None;
            return false;
        }
        if self.sender.is_none() && now >= self.retry_at {
            self.retry_at = now + 2.0;
            self.sender = Sender::connect();
        }
        self.sender.is_some() && self.pacer.due(now, FPS)
    }

    pub fn push(&mut self, width: u32, height: u32, rgba: &[u8]) {
        if let Some(s) = self.sender.as_mut() {
            s.send(width, height, rgba);
        }
    }

    /// Sending to the camera right now.
    pub fn connected(&self) -> bool {
        self.sender.is_some()
    }
}
