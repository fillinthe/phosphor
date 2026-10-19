//! How clips play (Resolume-style): trigger modes (normal, toggle, flash, piano) and autopilot
//! (next / previous / random clip when one ends or after N beats). Pure rules; the app calls them.

use serde::{Deserialize, Serialize};

use super::*;

/// What pressing a clip does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerMode {
    /// Press = play it.
    #[default]
    Normal,
    /// Press again = stop the layer.
    Toggle,
    /// Plays while held, then goes back to what was playing before.
    Flash,
    /// Plays while held, then stops the layer.
    Piano,
}

impl TriggerMode {
    pub const ALL: [TriggerMode; 4] = [TriggerMode::Normal, TriggerMode::Toggle, TriggerMode::Flash, TriggerMode::Piano];

    pub fn name(self) -> &'static str {
        match self {
            TriggerMode::Normal => "Normal",
            TriggerMode::Toggle => "Toggle",
            TriggerMode::Flash => "Flash (while held, then back)",
            TriggerMode::Piano => "Piano (while held, then stop)",
        }
    }
}

/// What a layer plays next by itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Autopilot {
    #[default]
    Off,
    Next,
    Previous,
    Random,
}

impl Autopilot {
    pub const ALL: [Autopilot; 4] = [Autopilot::Off, Autopilot::Next, Autopilot::Previous, Autopilot::Random];

    pub fn name(self) -> &'static str {
        match self {
            Autopilot::Off => "Off",
            Autopilot::Next => "Next clip",
            Autopilot::Previous => "Previous clip",
            Autopilot::Random => "Random clip",
        }
    }
}

/// Clips without an end (stills, generators, live) move on after this many beats when autopilot waits for the end.
pub const AUTOPILOT_DEFAULT_BEATS: f32 = 16.0;

impl LayerState {
    /// The layer's playback settings and effects, as a clip keeps them.
    pub fn clip_settings(&self) -> ClipSettings {
        ClipSettings {
            rate: self.rate,
            direction: self.direction,
            loop_in: self.loop_in,
            loop_out: self.loop_out,
            loop_beats: self.loop_beats,
            sync_beats: self.sync_beats,
            effects: self.effects.clone(),
        }
    }

    pub fn apply_clip_settings(&mut self, s: &ClipSettings) {
        self.rate = s.rate;
        self.direction = s.direction;
        self.loop_in = s.loop_in;
        self.loop_out = s.loop_out;
        self.loop_beats = s.loop_beats;
        self.sync_beats = s.sync_beats;
        self.effects = s.effects.clone();
    }

    /// While a clip with its own settings plays, changes made on the layer are saved into the clip.
    pub fn keep_clip_settings(&mut self) {
        let Some(slot) = self.active_slot() else { return };
        if self.clips[slot].own.is_none() {
            return;
        }
        let now = self.clip_settings();
        if self.clips[slot].own.as_ref() != Some(&now) {
            self.clips[slot].own = Some(now);
        }
    }

    /// The slot of the clip on the layer now.
    pub fn active_slot(&self) -> Option<usize> {
        let id = self.active_clip?;
        self.clips.iter().position(|c| c.id == id && !c.is_empty())
    }

    /// A clip button went down (mouse, key, pad). Returns true if something changed.
    pub fn press_clip(&mut self, slot: usize, now: f32, secs: f32) -> bool {
        let Some(clip) = self.clips.get(slot).filter(|c| !c.is_empty()) else { return false };
        let is_active = self.active_clip == Some(clip.id);
        match clip.trigger {
            TriggerMode::Toggle if is_active => {
                self.stop(now, secs);
                true
            }
            TriggerMode::Flash if !is_active => {
                self.flash_return = Some(Box::new(Clip {
                    id: self.active_clip.unwrap_or(0),
                    name: self.name.clone(),
                    source: self.source.clone(),
                    generator_values: self.generator_values.clone(),
                    mods: self.generator_mods.clone(),
                    trigger: TriggerMode::Normal,
                    // Speed, loop and effects come back too when the flash ends.
                    own: Some(self.clip_settings()),
                    muted: false,
                }));
                self.trigger_clip(slot, now, secs)
            }
            _ => self.trigger_clip(slot, now, secs),
        }
    }

    /// A clip button came up: flash clips go back to what played before, piano clips stop.
    pub fn release_clip(&mut self, slot: usize, now: f32, secs: f32) {
        let Some(clip) = self.clips.get(slot).filter(|c| !c.is_empty()) else { return };
        if self.active_clip != Some(clip.id) {
            return;
        }
        match clip.trigger {
            TriggerMode::Flash => match self.flash_return.take() {
                Some(back) if !back.is_empty() => {
                    self.start_transition(now, secs);
                    if let Some(own) = &back.own {
                        self.apply_clip_settings(own);
                    }
                    self.source = back.source;
                    self.generator_values = back.generator_values;
                    self.generator_mods = back.mods;
                    self.name = back.name;
                    self.active_clip = (back.id != 0).then_some(back.id);
                    self.playing = true;
                }
                _ => self.stop(now, secs),
            },
            TriggerMode::Piano => self.stop(now, secs),
            _ => {}
        }
    }

    /// Empties the layer (fading out over `secs`).
    pub fn stop(&mut self, now: f32, secs: f32) {
        self.start_transition(now, secs);
        self.source = SourceSpec::None;
        self.generator_values.clear();
        self.generator_mods.clear();
        self.active_clip = None;
    }

    /// The slot autopilot plays after the current one (`pick` in 0..1 chooses the random one), or None.
    pub fn autopilot_next(&self, pick: f32) -> Option<usize> {
        let filled: Vec<usize> = self.clips.iter().enumerate().filter(|(_, c)| !c.is_empty()).map(|(i, _)| i).collect();
        if filled.is_empty() {
            return None;
        }
        let at = self.active_slot().and_then(|s| filled.iter().position(|f| *f == s));
        let n = filled.len();
        let i = match (self.autopilot, at) {
            (Autopilot::Off, _) => return None,
            (Autopilot::Next, Some(a)) => (a + 1) % n,
            (Autopilot::Previous, Some(a)) => (a + n - 1) % n,
            (Autopilot::Next | Autopilot::Previous, None) => 0,
            (Autopilot::Random, Some(a)) if n > 1 => {
                // Any clip but this one.
                let r = ((pick.clamp(0.0, 0.999) * (n - 1) as f32) as usize).min(n - 2);
                if r >= a { r + 1 } else { r }
            }
            (Autopilot::Random, _) => ((pick.clamp(0.0, 0.999) * n as f32) as usize).min(n - 1),
        };
        Some(filled[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer_with(n: usize) -> LayerState {
        let mut l = LayerState::default();
        for i in 0..n {
            let c = l.clip_slot_mut(i * 2); // gaps between clips are skipped
            c.id = 100 + i as u64;
            c.source = SourceSpec::Generator(format!("g{i}"));
        }
        l
    }

    #[test]
    fn autopilot_walks_filled_slots_and_random_never_repeats() {
        let mut l = layer_with(3);
        l.autopilot = Autopilot::Next;
        assert_eq!(l.autopilot_next(0.0), Some(0));
        l.trigger_clip(4, 0.0, 0.0);
        assert_eq!(l.autopilot_next(0.0), Some(0));
        l.autopilot = Autopilot::Previous;
        assert_eq!(l.autopilot_next(0.0), Some(2));
        l.autopilot = Autopilot::Random;
        for k in 0..10 {
            assert_ne!(l.autopilot_next(k as f32 / 10.0), Some(4));
        }
    }

    #[test]
    fn toggle_flash_and_piano() {
        let mut l = layer_with(2);
        l.trigger_clip(0, 0.0, 0.0);
        l.clips[2].trigger = TriggerMode::Flash;
        l.press_clip(2, 1.0, 0.0);
        assert_eq!(l.active_slot(), Some(2));
        l.release_clip(2, 2.0, 0.0);
        assert_eq!(l.active_slot(), Some(0), "flash goes back");
        l.clips[0].trigger = TriggerMode::Toggle;
        l.press_clip(0, 3.0, 0.0);
        assert_eq!(l.source, SourceSpec::None, "toggle stops");
        l.clips[2].trigger = TriggerMode::Piano;
        l.press_clip(2, 4.0, 0.0);
        l.release_clip(2, 5.0, 0.0);
        assert_eq!(l.source, SourceSpec::None, "piano stops on release");
    }

    #[test]
    fn clips_keep_their_own_settings() {
        let mut l = layer_with(2);
        l.clips[0].own = Some(ClipSettings { rate: 2.0, ..l.clip_settings() });
        l.trigger_clip(0, 0.0, 0.0);
        assert_eq!(l.rate, 2.0, "triggering brings the clip's speed");
        l.rate = 3.0;
        l.keep_clip_settings();
        l.trigger_clip(2, 1.0, 0.0);
        assert_eq!(l.rate, 3.0, "a clip without its own settings leaves the layer as is");
        l.rate = 1.0;
        l.trigger_clip(0, 2.0, 0.0);
        assert_eq!(l.rate, 3.0, "the change made while it played was kept");
    }
}

#[cfg(test)]
mod sync_tests {
    use super::*;

    #[test]
    fn bpm_sync_fits_the_clip_to_beats() {
        let mut l = LayerState { sync_beats: 8.0, ..Default::default() };
        // A 4 s clip over 8 beats at 120 BPM (4 s) plays at normal speed; at 240 BPM twice as fast.
        assert!((l.play_rate(Some(4.0), 120.0) - 1.0).abs() < 1e-4);
        assert!((l.play_rate(Some(4.0), 240.0) - 2.0).abs() < 1e-4);
        l.sync_beats = 0.0;
        l.rate = 0.5;
        assert_eq!(l.play_rate(Some(4.0), 240.0), 0.5);
    }
}
