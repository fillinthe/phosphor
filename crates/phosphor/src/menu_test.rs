//! Bench `--quick menutest` (macOS): does rendering go on while a menu is open? A real NSMenu is popped up from the
//! main run loop (as a menu-bar click would: AppKit then runs its own tracking loop until the menu closes), closed
//! by itself after 3 s, and the engine frames rendered meanwhile are counted (MENU_TEST line).

use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Sel};
use objc2_foundation::NSString;

/// Engine frames rendered so far (`PhosphorApp::render`).
pub static RENDERS: AtomicU64 = AtomicU64::new(0);

unsafe extern "C" {
    static _dispatch_main_q: c_void;
    fn dispatch_time(when: u64, delta: i64) -> u64;
    fn dispatch_after_f(when: u64, queue: *const c_void, context: *mut c_void, work: extern "C" fn(*mut c_void));
}

/// Opens the menu `secs` from now (from the run loop, outside any frame).
pub fn schedule(secs: f64) {
    // SAFETY: libdispatch's main queue and a plain C callback with no context.
    unsafe { dispatch_after_f(dispatch_time(0, (secs * 1e9) as i64), &_dispatch_main_q, std::ptr::null_mut(), open_menu) };
}

extern "C" fn open_menu(_: *mut c_void) {
    let before = RENDERS.load(Ordering::Relaxed);
    let t = std::time::Instant::now();
    // SAFETY: AppKit on the main thread (the main dispatch queue).
    unsafe {
        // In front, as when the user clicks the menu bar (a menu in a background app closes at once).
        if let Some(app_class) = AnyClass::get(c"NSApplication") {
            let app: *mut AnyObject = msg_send![app_class, sharedApplication];
            let _: () = msg_send![app, activateIgnoringOtherApps: true];
        }
        let Some(menu_class) = AnyClass::get(c"NSMenu") else { return };
        let menu: Retained<AnyObject> = msg_send![menu_class, new];
        let title = NSString::from_str("Phosphor menu test");
        let empty = NSString::from_str("");
        let _: *mut AnyObject = msg_send![&*menu, addItemWithTitle: &*title, action: None::<Sel>, keyEquivalent: &*empty];
        // Closes itself in 3 s, from inside the menu's own tracking loop.
        let Some(array_class) = AnyClass::get(c"NSArray") else { return };
        let mode = NSString::from_str("NSEventTrackingRunLoopMode");
        let modes: Retained<AnyObject> = msg_send![array_class, arrayWithObject: &*mode];
        let nil: *const AnyObject = std::ptr::null();
        let _: () = msg_send![&*menu, performSelector: objc2::sel!(cancelTracking), withObject: nil, afterDelay: 3.0f64, inModes: &*modes];
        let at = objc2_foundation::NSPoint::new(200.0, 200.0);
        let item: *const AnyObject = std::ptr::null();
        let view: *const AnyObject = std::ptr::null();
        let _: bool = msg_send![&*menu, popUpMenuPositioningItem: item, atLocation: at, inView: view];
    }
    let n = RENDERS.load(Ordering::Relaxed) - before;
    let secs = t.elapsed().as_secs_f64();
    println!("MENU_TEST renders={n} in {secs:.2} s ({:.1} fps while the menu was open)", n as f64 / secs.max(0.001));
}
