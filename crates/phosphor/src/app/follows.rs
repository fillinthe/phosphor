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

    /// Dials (`MacroDial`) that moved since last frame, by hand, a controller or a follow: their targets follow.
    pub(crate) fn run_dials(&mut self, now: f64) {
        let sends = self.comp.dial_sends(&mut self.dial_seen);
        for (target, v) in sends {
            self.set_control(&target, v, now);
        }
    }

    /// A control carried onto another (`ui::link_drop`): a driver (surface widget, dial) carried onto a control
    /// drives it; a control carried onto a dial is driven by the dial; otherwise the control it landed on follows the
    /// one carried. The notice says what was linked.
    pub(crate) fn link(&mut self, source: ControlTarget, target: ControlTarget, now: f64) {
        let Some((driver, driven)) = phosphor_engine::link_roles(&source, &target) else {
            let said = format!("{} now follows {}", crate::control::target_label(&self.comp, &target), crate::control::target_label(&self.comp, &source));
            self.add_follow(source, target, now);
            self.notice = Some(said);
            return;
        };
        if !self.comp.link_driver(&driver, driven.clone()) {
            return;
        }
        // Nothing jumps: a dial's first setting gives the dial its value; anything else waits for the driver to move.
        let first = self.comp.driver_bindings_mut(&driver).is_some_and(|l| l.len() == 1);
        if first
            && matches!(driver, ControlTarget::Macro { .. })
            && let Some(v) = crate::control::get_target(&self.comp, &driven)
        {
            self.set_control(&driver, v, now);
        }
        self.notice = Some(format!("{} now drives {}", crate::control::target_label(&self.comp, &driver), crate::control::target_label(&self.comp, &driven)));
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
