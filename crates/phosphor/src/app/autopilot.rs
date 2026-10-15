//! Autopilot: layers that play their next / previous / a random clip by themselves, when the clip ends
//! or after a number of beats (rules: `LayerState::autopilot_next`).

use super::*;

/// What autopilot remembers about a layer's current clip.
#[derive(Default)]
pub struct AutoRun {
    clip: Option<u64>,
    started_beats: f64,
    passes: Option<u64>,
}

impl State {
    /// Moves autopilot layers on when their clip is done. Call once per frame.
    pub(crate) fn run_autopilot(&mut self, now: f64) {
        let beats = self.clock.beats(now);
        let mut fire: Vec<(u64, usize)> = Vec::new();
        for layer in self.comp.layers.iter().filter(|l| l.autopilot != phosphor_engine::Autopilot::Off) {
            let passes = self.media.passes(layer.id);
            let run = self.autopilot.entry(layer.id).or_default();
            if run.clip != layer.active_clip {
                *run = AutoRun { clip: layer.active_clip, started_beats: beats, passes };
            }
            // Only moves on from a clip it (or you) started; an empty layer waits for you.
            if layer.active_clip.is_none() || !layer.enabled {
                continue;
            }
            let played = beats - run.started_beats;
            let due = if layer.autopilot_beats > 0.0 {
                played >= layer.autopilot_beats as f64
            } else if let (Some(now_passes), Some(at_start)) = (passes, run.passes) {
                now_passes > at_start
            } else {
                played >= phosphor_engine::AUTOPILOT_DEFAULT_BEATS as f64
            };
            if !due {
                continue;
            }
            // A cheap random number is plenty for "pick another clip".
            let pick = ((now * 7919.0).fract() * 0.618_034 + (beats * 0.137).fract()).fract() as f32;
            if let Some(slot) = layer.autopilot_next(pick) {
                fire.push((layer.id, slot));
            }
            // Wait for the new clip (or, if the same one is picked again, a fresh count).
            *run = AutoRun { clip: layer.active_clip, started_beats: beats, passes: self.media.passes(layer.id) };
        }
        for (layer, slot) in fire {
            self.comp.trigger_clip(layer, slot, now as f32);
        }
        let live: std::collections::HashSet<u64> = self.comp.layers.iter().map(|l| l.id).collect();
        self.autopilot.retain(|id, _| live.contains(id));
    }
}
