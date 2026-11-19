//! Notices a projector / display unplugged and plugged back in mid-show. An output filling a screen that goes away
//! is hidden (the system would otherwise drop it onto another screen, often over the control screen); when that
//! screen comes back, the output fills it again.
//!
//! Cheap: macOS tells us when the display setup changes (a CoreGraphics callback sets a flag), with a slow poll as a
//! fallback; elsewhere the display list is read about once a second. Nothing is read while no output window is open
//! and none is waiting for its screen.

use crate::screens::{Display, screen_key};

/// An output window as the watcher sees it.
#[derive(Clone, Debug)]
pub struct Placed {
    pub output: u64,
    pub name: String,
    pub open: bool,
    /// The display it fills (or is being placed on), if any.
    pub filling: Option<u32>,
    /// The screen saved for it in the project (`screen_key`).
    pub screen: String,
}

/// An output hidden because its screen went away.
#[derive(Clone, Debug, PartialEq)]
pub struct Away {
    pub output: u64,
    pub name: String,
    pub display: u32,
    pub key: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    /// Its screen is gone: hide the window.
    Hide(Away),
    /// Its screen is back: fill it again.
    Restore { output: u64, name: String, display: Display },
}

/// How often the display list is read without a change notice.
#[cfg(target_os = "macos")]
const POLL_SECS: f64 = 3.0;
#[cfg(not(target_os = "macos"))]
const POLL_SECS: f64 = 1.0;
/// After a change notice, look again a little later too (the new layout takes a moment to settle).
const SETTLE_SECS: f64 = 0.7;

#[derive(Default)]
pub struct ScreenWatch {
    /// The displays at the last look.
    known: Vec<Display>,
    /// Outputs hidden until their screen is back.
    pub away: Vec<Away>,
    next: f64,
    settle: Option<f64>,
}

impl ScreenWatch {
    /// Whether to read the display list now. `watching` = some output window is open (or waiting for its screen).
    pub fn due(&mut self, now: f64, watching: bool) -> bool {
        let changed = take_changed();
        if !watching && self.away.is_empty() {
            self.known.clear();
            return false;
        }
        if changed {
            self.settle = Some(now + SETTLE_SECS);
            return true;
        }
        if self.settle.is_some_and(|t| now >= t) {
            self.settle = None;
            return true;
        }
        if now >= self.next {
            self.next = now + POLL_SECS;
            return true;
        }
        false
    }

    /// What to do now that the displays are `now`, given the outputs. Remembers the hidden ones. Pure: no system
    /// calls, so it can be tested with made-up screens.
    pub fn update(&mut self, now: &[Display], outputs: &[Placed]) -> Vec<Act> {
        // An empty list happens for a moment while the setup changes; wait for the real one.
        if now.is_empty() {
            return Vec::new();
        }
        let old = std::mem::replace(&mut self.known, now.to_vec());
        // Opened again by hand (or deleted) meanwhile: no longer waiting.
        self.away.retain(|a| outputs.iter().any(|o| o.output == a.output && !o.open));
        let mut acts = Vec::new();
        if old.is_empty() {
            return acts;
        }
        for o in outputs.iter().filter(|o| o.open) {
            let Some(id) = o.filling else { continue };
            if now.iter().any(|d| d.id == id) {
                continue;
            }
            let key = old.iter().find(|d| d.id == id).map_or_else(|| o.screen.clone(), |d| screen_key(&old, d));
            let away = Away { output: o.output, name: o.name.clone(), display: id, key };
            self.away.push(away.clone());
            acts.push(Act::Hide(away));
        }
        // Back: the same display, or a newly connected one with the same screen name (ids can change on re-plug).
        let mut back = Vec::new();
        for (i, a) in self.away.iter().enumerate() {
            let same = now.iter().find(|d| d.id == a.display);
            let named = || now.iter().find(|d| !old.iter().any(|o| o.id == d.id) && screen_key(now, d) == a.key);
            if let Some(d) = same.or_else(named) {
                back.push(i);
                acts.push(Act::Restore { output: a.output, name: a.name.clone(), display: d.clone() });
            }
        }
        for i in back.into_iter().rev() {
            self.away.remove(i);
        }
        acts
    }
}

/// The notice for what happened (None when nothing did).
pub fn notice(acts: &[Act]) -> Option<String> {
    let names = |hide: bool| -> Vec<&str> {
        acts.iter()
            .filter_map(|a| match a {
                Act::Hide(w) if hide => Some(w.name.as_str()),
                Act::Restore { name, .. } if !hide => Some(name.as_str()),
                _ => None,
            })
            .collect()
    };
    let (gone, back) = (names(true), names(false));
    let mut parts = Vec::new();
    if !gone.is_empty() {
        parts.push(format!("{}: screen unplugged, output hidden", gone.join(", ")));
    }
    if !back.is_empty() {
        parts.push(format!("{}: screen back, output restored", back.join(", ")));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

#[cfg(target_os = "macos")]
mod cg {
    use std::sync::Once;
    use std::sync::atomic::{AtomicBool, Ordering};

    pub static CHANGED: AtomicBool = AtomicBool::new(false);
    static REGISTER: Once = Once::new();

    type Callback = extern "C" fn(display: u32, flags: u32, user: *mut std::ffi::c_void);
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGDisplayRegisterReconfigurationCallback(callback: Callback, user: *mut std::ffi::c_void) -> i32;
    }

    extern "C" fn changed(_display: u32, _flags: u32, _user: *mut std::ffi::c_void) {
        CHANGED.store(true, Ordering::Relaxed);
    }

    /// Registers the callback once (it runs on the main thread's run loop and only sets a flag).
    pub fn take() -> bool {
        // SAFETY: a plain C callback with no user data, registered once for the life of the app.
        REGISTER.call_once(|| unsafe {
            CGDisplayRegisterReconfigurationCallback(changed, std::ptr::null_mut());
        });
        CHANGED.swap(false, Ordering::Relaxed)
    }
}

#[cfg(target_os = "macos")]
fn take_changed() -> bool {
    cg::take()
}

#[cfg(not(target_os = "macos"))]
fn take_changed() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(id: u32, name: &str, x: f32) -> Display {
        Display { id, name: name.into(), x, y: 0.0, w: 1920.0, h: 1080.0, pixels: (1920, 1080), main: id == 1 }
    }
    fn out(output: u64, filling: Option<u32>, open: bool) -> Placed {
        Placed { output, name: format!("Projector {output}"), open, filling, screen: String::new() }
    }

    #[test]
    fn unplugged_screen_hides_its_output_and_brings_it_back() {
        let mut w = ScreenWatch::default();
        let all = vec![d(1, "Built-in display", 0.0), d(5, "External display 1", 1920.0), d(6, "External display 2", 3840.0)];
        assert!(w.update(&all, &[out(1, Some(5), true), out(2, Some(6), true)]).is_empty());
        // Projector 1's screen goes; the other one is renamed "External display 1" but its output stays.
        let less = vec![d(1, "Built-in display", 0.0), d(6, "External display 1", 3840.0)];
        let acts = w.update(&less, &[out(1, Some(5), true), out(2, Some(6), true)]);
        assert_eq!(acts, vec![Act::Hide(Away { output: 1, name: "Projector 1".into(), display: 5, key: "External display 1".into() })]);
        assert_eq!(notice(&acts).as_deref(), Some("Projector 1: screen unplugged, output hidden"));
        // Hidden now; nothing more happens while the screen stays away (the renamed screen isn't taken for it).
        assert!(w.update(&less, &[out(1, None, false), out(2, Some(6), true)]).is_empty());
        // Back (same id): fill it again.
        let acts = w.update(&all, &[out(1, None, false), out(2, Some(6), true)]);
        assert_eq!(acts, vec![Act::Restore { output: 1, name: "Projector 1".into(), display: all[1].clone() }]);
        assert!(w.away.is_empty());
    }

    #[test]
    fn a_screen_back_with_a_new_id_is_found_by_name() {
        let mut w = ScreenWatch::default();
        w.update(&[d(1, "Built-in display", 0.0), d(5, "External display 1", 1920.0)], &[out(1, Some(5), true)]);
        let alone = vec![d(1, "Built-in display", 0.0)];
        assert_eq!(w.update(&alone, &[out(1, Some(5), true)]).len(), 1);
        let back = d(9, "External display 1", 1920.0);
        let acts = w.update(&[alone[0].clone(), back.clone()], &[out(1, None, false)]);
        assert_eq!(acts, vec![Act::Restore { output: 1, name: "Projector 1".into(), display: back }]);
    }

    #[test]
    fn windows_not_filling_a_screen_and_outputs_reopened_by_hand_are_left_alone() {
        let mut w = ScreenWatch::default();
        let all = vec![d(1, "Built-in display", 0.0), d(5, "External display 1", 1920.0)];
        w.update(&all, &[out(1, None, true), out(2, Some(5), true)]);
        let alone = vec![d(1, "Built-in display", 0.0)];
        // Output 1 is a plain window: not hidden.
        assert_eq!(w.update(&alone, &[out(1, None, true), out(2, Some(5), true)]).len(), 1);
        // Output 2 opened again by hand on the built-in screen: it no longer waits, so the projector's return leaves it be.
        w.update(&alone, &[out(1, None, true), out(2, Some(1), true)]);
        assert!(w.update(&all, &[out(1, None, true), out(2, Some(1), true)]).is_empty());
    }

    #[test]
    fn an_empty_list_while_screens_change_is_ignored() {
        let mut w = ScreenWatch::default();
        let all = vec![d(1, "Built-in display", 0.0), d(5, "External display 1", 1920.0)];
        w.update(&all, &[out(1, Some(5), true)]);
        assert!(w.update(&[], &[out(1, Some(5), true)]).is_empty());
        assert!(w.update(&all, &[out(1, Some(5), true)]).is_empty());
    }

    #[test]
    fn nothing_is_read_while_no_output_is_open() {
        let mut w = ScreenWatch::default();
        assert!(!w.due(0.0, false));
        assert!(w.due(0.0, true));
        assert!(!w.due(0.5, true));
        assert!(w.due(POLL_SECS + 0.1, true));
    }
}
