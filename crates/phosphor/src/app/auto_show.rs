//! Auto show: the show runs itself. Settings are `Composition::auto_show`; this is the per-frame step:
//! the next column with clips every N bars (through the normal launch quantise), Surprise on a drop.

use super::{Quick, State, Trigger};

#[derive(Default)]
pub struct AutoShowRun {
    /// Bar number at the last check (None = not started).
    last_bar: Option<i64>,
    /// The music level averaged over a few seconds; a drop is a jump well above it.
    level_avg: f32,
    last_surprise: f64,
    last_now: Option<f64>,
    /// Columns fired so far (the bench prints it).
    pub fired: u32,
}

/// Seconds between two Surprises from drops, so a loud section doesn't flicker the look.
const SURPRISE_GAP: f64 = 8.0;

impl State {
    /// Once per frame.
    pub(crate) fn run_auto_show(&mut self, now: f64) {
        let a = self.comp.auto_show;
        if !a.enabled {
            self.auto_run = AutoShowRun::default();
            return;
        }
        let dt = self.auto_run.last_now.map_or(1.0 / 60.0, |t| (now - t).clamp(0.0, 0.25)) as f32;
        self.auto_run.last_now = Some(now);
        // Columns: on the first beat of every `bars`-th bar, the next column that has a clip.
        if a.columns {
            let bar = (self.clock.beats(now) / 4.0).floor() as i64;
            let first = self.auto_run.last_bar.is_none();
            if self.auto_run.last_bar != Some(bar) {
                self.auto_run.last_bar = Some(bar);
                if !first && bar.rem_euclid(a.bars.max(1) as i64) == 0
                    && let Some(col) = self.next_column_with_clips()
                {
                    self.trigger(Trigger::Column(col), now);
                    self.auto_run.fired += 1;
                }
            }
        }
        // Drops: the level jumps well above its recent average.
        if a.surprise_on_drops {
            let level = self.mod_inputs.audio_level;
            let k = (dt / 3.0).min(1.0);
            let avg = self.auto_run.level_avg;
            if level > avg * 1.8 + 0.15 && avg > 0.02 && now - self.auto_run.last_surprise > SURPRISE_GAP {
                self.auto_run.last_surprise = now;
                self.quick(Quick::Surprise, now);
            }
            self.auto_run.level_avg = avg + (level - avg) * k;
        }
    }

    /// The column after the one most layers are on, among columns that hold a clip somewhere (wraps around).
    fn next_column_with_clips(&self) -> Option<usize> {
        let cols = self.comp.clip_columns.max(1);
        let has_clip = |c: usize| self.comp.layers.iter().any(|l| l.clips.get(c).is_some_and(|k| !k.is_empty()));
        // Where are we? The most common active column, else -1.
        let mut counts = vec![0usize; cols];
        for l in &self.comp.layers {
            if let Some(active) = l.active_clip
                && let Some(c) = l.clips.iter().position(|k| k.id == active)
                && c < cols
            {
                counts[c] += 1;
            }
        }
        let current = counts.iter().enumerate().filter(|(_, n)| **n > 0).max_by_key(|(_, n)| **n).map(|(c, _)| c as i64).unwrap_or(-1);
        (1..=cols as i64).map(|k| ((current + k).rem_euclid(cols as i64)) as usize).find(|c| has_clip(*c))
    }
}
