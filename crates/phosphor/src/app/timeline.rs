//! Playing the keyframe timeline: the playhead runs while playing (markers can pause or loop it), and every track
//! sets its control through `State::set_control` whenever the playhead moves (playing or dragged).

use phosphor_engine::Step;

use super::State;

impl State {
    /// Play from the playhead, or pause.
    pub(crate) fn timeline_toggle(&mut self, now: f64) {
        self.timeline_play = match self.timeline_play {
            Some(_) => None,
            None => {
                // At the end: from the start again.
                if self.timeline_pos >= self.comp.timeline.length {
                    self.timeline_pos = 0.0;
                }
                Some((now, self.timeline_pos))
            }
        };
    }

    /// Puts the playhead at `t` (playing carries on from there).
    pub(crate) fn timeline_seek(&mut self, t: f64, now: f64) {
        self.timeline_pos = t.clamp(0.0, self.comp.timeline.length.max(0.0));
        if self.timeline_play.is_some() {
            self.timeline_play = Some((now, self.timeline_pos));
        }
    }

    /// Every frame: moves the playhead while playing, and sets the tracks' controls when it has moved.
    pub(crate) fn run_timeline(&mut self, now: f64) {
        if let Some((start, from)) = self.timeline_play {
            let want = from + (now - start);
            match self.comp.timeline.advance(self.timeline_pos, want) {
                Step::At(t) => {
                    if (t - want).abs() > 1e-6 {
                        // Looped or jumped: carry on from there.
                        self.timeline_play = Some((now, t));
                    }
                    self.timeline_pos = t;
                }
                Step::Stop(t) => {
                    self.timeline_pos = t;
                    self.timeline_play = None;
                }
            }
        }
        if self.comp.timeline.tracks.is_empty() || self.timeline_applied == Some(self.timeline_pos) {
            return;
        }
        // Recording: a control moved by hand since the timeline last set it gets a key here instead.
        let mut moved: Vec<phosphor_engine::ControlTarget> = Vec::new();
        if self.timeline_record && self.timeline_play.is_some() {
            let t = self.timeline_pos;
            for (target, was) in std::mem::take(&mut self.timeline_set) {
                if let Some(v) = crate::control::get_target(&self.comp, &target)
                    && (v - was).abs() > 0.004
                {
                    self.comp.timeline.track_mut(&target).set_key(t, v);
                    moved.push(target);
                }
            }
        }
        self.timeline_applied = Some(self.timeline_pos);
        let values: Vec<(phosphor_engine::ControlTarget, f32)> =
            self.comp.timeline.values_at(self.timeline_pos).into_iter().filter(|(t, _)| !moved.contains(t)).map(|(t, v)| (t.clone(), v)).collect();
        self.timeline_set.clear();
        for (target, v) in values {
            self.set_control(&target, v, now);
            // What it actually is now (locks and ranges can change it).
            let now_v = crate::control::get_target(&self.comp, &target).unwrap_or(v);
            self.timeline_set.push((target, now_v));
        }
        for target in moved {
            if let Some(v) = crate::control::get_target(&self.comp, &target) {
                self.timeline_set.push((target, v));
            }
        }
    }

    /// A key at the playhead for `target`, with its value now (any control's menu › Key on the timeline).
    pub(crate) fn key_now(&mut self, target: &phosphor_engine::ControlTarget) {
        let Some(v) = crate::control::get_target(&self.comp, target) else {
            self.notice = Some("That control has no value to keep (a trigger)".into());
            return;
        };
        let t = self.timeline_pos;
        self.comp.timeline.track_mut(target).set_key(t, v);
        // Don't let playback snap it back this frame.
        self.timeline_applied = Some(t);
    }
}
