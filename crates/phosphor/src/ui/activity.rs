//! Activity lights: a dot that lights while something keeps arriving or going out (the status line's IN / OUT).

use eframe::egui::{self, Color32, RichText};
use egui_phosphor::regular as icon;

/// How long a light stays on after the last change (seconds).
const HOLD: f64 = 1.5;

/// What lights a light: a counter that goes up while things flow (frames, packets), or plain on / off.
pub(crate) enum Flow {
    Count(u64),
    On(bool),
}

/// When a counter last changed.
#[derive(Clone, Copy, Default)]
struct Pulse {
    count: u64,
    at: f64,
}

impl Pulse {
    /// Takes the counter now; true while it changed in the last `HOLD` seconds.
    fn feed(&mut self, count: u64, now: f64) -> bool {
        if count != self.count {
            self.count = count;
            self.at = now;
        }
        self.at > 0.0 && now - self.at < HOLD
    }
}

/// One light with its name ("Media", "NDI"); `tip` says what it watches.
pub(crate) fn light(ui: &mut egui::Ui, name: &str, flow: Flow, tip: &str) -> egui::Response {
    let lit = match flow {
        Flow::On(on) => on,
        Flow::Count(count) => {
            let now = ui.input(|i| i.time).max(1e-6);
            let id = egui::Id::new(("activity_light", name));
            ui.ctx().data_mut(|d| d.get_temp_mut_or_default::<Pulse>(id).feed(count, now))
        }
    };
    let colour = if lit { crate::theme::accent() } else { Color32::from_gray(70) };
    ui.label(RichText::new(format!("{} {name}", icon::CIRCLE)).small().color(colour)).on_hover_text(tip)
}

/// A group's heading ("IN", "OUT").
pub(crate) fn heading(ui: &mut egui::Ui, name: &str, tip: &str) {
    ui.label(RichText::new(name).small().strong().color(Color32::GRAY)).on_hover_text(tip);
}

#[cfg(test)]
mod tests {
    use super::Pulse;

    #[test]
    fn a_light_stays_on_a_moment_after_the_last_change() {
        let mut p = Pulse::default();
        assert!(!p.feed(0, 1.0), "nothing has come in yet");
        assert!(p.feed(3, 2.0));
        assert!(p.feed(3, 3.0), "still lit just after");
        assert!(!p.feed(3, 4.0), "out once it stops");
        assert!(p.feed(4, 5.0), "back on when more comes");
    }
}
