//! Which window waits for its screen's refresh. All of Phosphor's windows are painted one after another on one thread,
//! and with every window on vsync each waits for its own screen's next refresh: two or three outputs on screens that
//! don't refresh in step can take most of a frame between them (Windows reports of 2 to 10 frames a second with two
//! outputs). With "Screens don't wait for each other" on, one window keeps vsync and paces the show (the first output
//! open, else Phosphor's own window) and the others present without waiting: outputs by mailbox (the newest picture
//! at their own refresh, never torn) where their screen can, else vsync as before; Phosphor's own window without vsync.
//! The loop is held to the render rate by the paced window (and `ui`'s sleep). Off by default.
//! `PHOSPHOR_PRESENT=free` turns it on for a run (benches).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use eframe::egui::ViewportId;
use eframe::wgpu::PresentMode;

static ON: AtomicBool = AtomicBool::new(false);
/// The window that keeps vsync, by its viewport id's value; `CONTROL` = Phosphor's own window.
static PACING: AtomicU64 = AtomicU64::new(CONTROL);
const CONTROL: u64 = u64::MAX;

/// Tells the graphics layer to ask here for each window's present mode (once, before the first window opens).
pub fn install() {
    let _ = eframe::egui_wgpu::winit::PRESENT_MODE_FOR.set(mode_for);
    let _ = eframe::egui_wgpu::winit::PRESENTED.set(note);
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

/// Bench: when each window showed a new frame, by viewport id (`start_log` / `take_log`). Off (nothing kept) otherwise.
static LOGGING: AtomicBool = AtomicBool::new(false);
static LOG: Mutex<Option<(web_time::Instant, HashMap<u64, Vec<f64>>)>> = Mutex::new(None);

/// Starts keeping present times (the bench, once the show has settled).
pub fn start_log() {
    *LOG.lock().unwrap_or_else(PoisonError::into_inner) = Some((web_time::Instant::now(), HashMap::new()));
    LOGGING.store(true, Ordering::Release);
}

/// A window presented a frame (egui's painter, or the render thread for the outputs it presents).
pub fn note(id: ViewportId) {
    note_key(id.0.value());
}

fn note_key(key: u64) {
    if !LOGGING.load(Ordering::Acquire) {
        return;
    }
    if let Some((start, times)) = LOG.lock().unwrap_or_else(PoisonError::into_inner).as_mut() {
        times.entry(key).or_default().push(start.elapsed().as_secs_f64());
    }
}

/// The key `take_log` gives the engine's frames under (each frame drawn, wherever it was drawn).
pub const ENGINE_FRAMES: u64 = 1;

/// The engine drew a frame (bench timing, as `note`).
pub fn note_engine() {
    note_key(ENGINE_FRAMES);
}

/// The present times kept since `start_log` (seconds), by viewport id value; stops keeping them.
pub fn take_log() -> HashMap<u64, Vec<f64>> {
    LOGGING.store(false, Ordering::Release);
    LOG.lock().unwrap_or_else(PoisonError::into_inner).take().map(|(_, t)| t).unwrap_or_default()
}

/// Gaps between frames (ms): (median, 99th percentile, longest, count). None with fewer than two frames.
pub fn gaps(times: &[f64]) -> Option<(f64, f64, f64, usize)> {
    let mut gaps: Vec<f64> = times.windows(2).map(|w| (w[1] - w[0]) * 1000.0).collect();
    if gaps.is_empty() {
        return None;
    }
    gaps.sort_by(f64::total_cmp);
    let at = |q: f64| gaps[((gaps.len() - 1) as f64 * q).round() as usize];
    Some((at(0.5), at(0.99), gaps[gaps.len() - 1], gaps.len()))
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

    #[test]
    fn frame_gaps_give_median_tail_and_longest() {
        assert_eq!(gaps(&[1.0]), None, "one frame has no gap");
        // 100 frames 1/60 s apart, then one 200 ms hole.
        let mut t: Vec<f64> = (0..100).map(|i| i as f64 / 60.0).collect();
        t.push(t[99] + 0.2);
        let (p50, p99, max, n) = gaps(&t).unwrap();
        assert_eq!(n, 100);
        assert!((p50 - 1000.0 / 60.0).abs() < 1e-6, "{p50}");
        assert!((max - 200.0).abs() < 1e-6, "{max}");
        assert!(p99 < 20.0, "one hole in a hundred gaps is past the 99th percentile's reach: {p99}");
    }
}
