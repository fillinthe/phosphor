//! Controls that follow other controls (`Composition::follows`): each frame, a source whose value changed hands
//! it on to its followers through their fader part, value effects and range. Nothing runs while there are none,
//! and a follower can still be moved by hand between its source's moves.

use phosphor_engine::{ControlTarget, Follow};

use super::State;

impl State {
    pub(crate) fn run_follows(&mut self, now: f64) {
        if self.comp.follows.is_empty() {
            if !self.follow_seen.is_empty() {
                self.follow_seen.clear();
            }
            return;
        }
        let sends = crate::control::follow_sends(&self.comp, &mut self.follow_seen);
        for (target, v) in sends {
            self.set_control(&target, v, now);
        }
    }

    /// `target` follows `source` from now on (and takes its value straight away). A control can't follow itself,
    /// and following the same control twice changes nothing.
    pub(crate) fn add_follow(&mut self, source: ControlTarget, target: ControlTarget, now: f64) {
        if source == target || self.comp.follows.iter().any(|f| f.source == source && f.to.target == target) {
            return;
        }
        let to = phosphor_engine::Binding::new(target);
        if let Some(v) = crate::control::get_target(&self.comp, &source) {
            self.set_control(&to.target.clone(), to.value(v), now);
        }
        self.comp.follows.push(Follow { source, to });
    }
}
