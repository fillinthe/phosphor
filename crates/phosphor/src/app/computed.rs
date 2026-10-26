//! Computed values on the Control Surface: each frame, every Computed widget works out its formula from the
//! controls it reads and, when the answer changed, drives its targets. Nothing runs while there are none.

use phosphor_engine::{ControlTarget, WidgetKind};

use super::State;

impl State {
    pub(crate) fn run_computed(&mut self, now: f64) {
        if !self.comp.surface.widgets.iter().any(|w| w.kind == WidgetKind::Computed) {
            return;
        }
        let beats = self.clock.beats(now);
        let changed: Vec<(u64, f32)> = self
            .comp
            .surface
            .widgets
            .iter()
            .filter(|w| w.kind == WidgetKind::Computed)
            .filter_map(|w| {
                let inputs: Vec<f32> = w.inputs.iter().map(|t| crate::control::get_target(&self.comp, t).unwrap_or(0.0)).collect();
                let v = w.compute(&inputs, now, beats)?;
                ((v - w.value).abs() > 1e-4).then_some((w.id, v))
            })
            .collect();
        for (id, v) in changed {
            self.set_control(&ControlTarget::Surface(id), v, now);
        }
    }

    /// Faders and sliders that move by themselves: their targets follow where they are now (the fader keeps
    /// where you set it). Nothing runs while none moves.
    pub(crate) fn run_surface_motion(&mut self, now: f64) {
        let mut sends = Vec::new();
        for w in self.comp.surface.widgets.iter_mut().filter(|w| matches!(w.kind, WidgetKind::Fader | WidgetKind::Slider)) {
            let Some(m) = &w.motion else { continue };
            let v = phosphor_engine::control::modulate(w.value, 0.0, 1.0, Some(m), &self.mod_inputs);
            if (v - w.live).abs() > 1e-4 {
                w.live = v;
                sends.extend(w.bindings.iter().map(|b| (b.target.clone(), b.range.apply(v))));
            }
        }
        for (target, v) in sends {
            self.set_control(&target, v, now);
        }
    }
}
