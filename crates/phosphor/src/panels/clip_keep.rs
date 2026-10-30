//! "Keep speed, loop and direction with this clip": one switch for the Inspector's Playback and the clip's
//! right-click menu. Off by default; while on it says what's saved, and switching it says what just happened.

use eframe::egui;
use phosphor_engine::{ClipSettings, PlayDirection};

pub(crate) const KEEP_LABEL: &str = "Keep speed, loop and direction with this clip";

/// The switch (`own` = the clip's saved settings, `current` = the layer's now, `name` = the clip's name) and, while
/// it's on, a line naming what's saved.
pub(crate) fn keep_with_clip(ui: &mut egui::Ui, own: &mut Option<ClipSettings>, current: ClipSettings, name: &str) {
    let mut on = own.is_some();
    if crate::ui::toggle(ui, &mut on, KEEP_LABEL)
        .on_hover_text("On: this clip's speed, direction (reverse, ping-pong), loop section and BPM sync are saved with it, come back each time it's triggered (on any layer), and changes you make while it plays are saved into it. Off: it plays with the layer's settings")
        .changed()
    {
        *own = on.then_some(current);
        let name = if name.is_empty() { "This clip".to_string() } else { format!("\u{201c}{name}\u{201d}") };
        crate::ui::notice::tell(
            ui.ctx(),
            if on { format!("{name} now keeps its speed, loop and direction: they come back each time it plays") } else { format!("{name} now plays with the layer's speed, loop and direction") },
        );
    }
    if let Some(o) = own {
        crate::ui::empty_note(ui, &format!("Saved with this clip: {}. Changes made while it plays are saved into it.", summary(o)));
    }
}

/// What a clip's saved settings do, in words ("speed 1.5×, ping-pong, loop 10–80 %").
fn summary(o: &ClipSettings) -> String {
    let mut parts = Vec::new();
    if o.rate > 0.0 && (o.rate - 1.0).abs() > 0.005 {
        parts.push(format!("speed {:.2}×", o.rate));
    }
    match o.direction {
        PlayDirection::Forward => {}
        d => parts.push(d.name().to_lowercase()),
    }
    if o.loop_beats > 0.0 {
        parts.push(format!("a {} beat loop", o.loop_beats));
    } else if o.loop_in > 0.001 || o.loop_out < 0.999 {
        parts.push(format!("loop {:.0}–{:.0} %", o.loop_in * 100.0, o.loop_out * 100.0));
    }
    if o.sync_beats > 0.0 {
        parts.push(format!("stretched to {} beats", o.sync_beats));
    }
    if parts.is_empty() { "normal speed, forward, the whole movie".to_string() } else { parts.join(", ") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_what_is_kept() {
        let plain = phosphor_engine::LayerState::default().clip_settings();
        assert_eq!(summary(&plain), "normal speed, forward, the whole movie");
        let kept = ClipSettings { rate: 1.5, direction: PlayDirection::PingPong, loop_in: 0.1, loop_out: 0.8, ..plain };
        assert_eq!(summary(&kept), "speed 1.50×, ping-pong, loop 10–80 %");
    }
}
