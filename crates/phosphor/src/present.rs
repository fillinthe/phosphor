//! Which window waits for its screen's refresh. All of Phosphor's windows are painted one after another on one thread,
//! and with every window on vsync each waits for its own screen's next refresh: two or three outputs on screens that
//! don't refresh in step can take most of a frame between them (Windows reports of 2 to 10 frames a second with two
//! outputs). With "Screens don't wait for each other" on, one window keeps vsync and paces the show (the first output
//! open, else Phosphor's own window) and the others present without waiting: outputs by mailbox (the newest picture
//! at their own refresh, never torn) where their screen can, else vsync as before; Phosphor's own window without vsync.
//! The loop is held to the render rate by the paced window (and `ui`'s sleep). Off by default.
//! `PHOSPHOR_PRESENT=free` turns it on for a run (benches).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use eframe::egui::ViewportId;
use eframe::wgpu::PresentMode;

static ON: AtomicBool = AtomicBool::new(false);
/// The window that keeps vsync, by its viewport id's value; `CONTROL` = Phosphor's own window.
static PACING: AtomicU64 = AtomicU64::new(CONTROL);
const CONTROL: u64 = u64::MAX;

/// Tells the graphics layer to ask here for each window's present mode (once, before the first window opens).
pub fn install() {
    let _ = eframe::egui_wgpu::winit::PRESENT_MODE_FOR.set(mode_for);
    if std::env::var("PHOSPHOR_PRESENT").is_ok_and(|v| v == "free") {
        ON.store(true, Ordering::Relaxed);
    }
}

/// The setting, and the window that paces the show (None = Phosphor's own window). Cheap: called every frame.
pub fn set(on: bool, pacing: Option<ViewportId>) {
    if std::env::var_os("PHOSPHOR_PRESENT").is_none() {
        ON.store(on, Ordering::Relaxed);
    }
    PACING.store(pacing.map_or(CONTROL, |v| v.0.value()), Ordering::Relaxed);
}

fn mode_for(id: ViewportId) -> Option<PresentMode> {
    let pacing = match PACING.load(Ordering::Relaxed) {
        CONTROL => None,
        v => Some(v),
    };
    choose(ON.load(Ordering::Relaxed), pacing, id)
}

/// None = the configured mode (vsync) for every window while it's off.
fn choose(on: bool, pacing: Option<u64>, id: ViewportId) -> Option<PresentMode> {
    if !on {
        return None;
    }
    let control = id == ViewportId::ROOT;
    let paces = match pacing {
        Some(v) => id.0.value() == v,
        None => control,
    };
    Some(if paces {
        PresentMode::AutoVsync
    } else if control {
        PresentMode::AutoNoVsync
    } else {
        PresentMode::Mailbox
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_window_keeps_vsync_and_outputs_never_tear() {
        let out = |n: u64| ViewportId::from_hash_of(("phosphor_output", n));
        assert_eq!(choose(false, Some(out(1).0.value()), out(2)), None, "off: every window as configured");
        // Outputs 1 and 2 open: output 1 paces, output 2 and the control window don't wait.
        let first = Some(out(1).0.value());
        assert_eq!(choose(true, first, out(1)), Some(PresentMode::AutoVsync));
        assert_eq!(choose(true, first, out(2)), Some(PresentMode::Mailbox));
        assert_eq!(choose(true, first, ViewportId::ROOT), Some(PresentMode::AutoNoVsync));
        // No output open: Phosphor's own window paces.
        assert_eq!(choose(true, None, ViewportId::ROOT), Some(PresentMode::AutoVsync));
    }
}
