//! Per-control settings kept with the show: locked controls (nothing moves them: mouse, controllers, phones) and
//! snap points (Shift-drag on a slider lands on them). Keyed by `ControlTarget`, so any mappable control has them.

use serde::{Deserialize, Serialize};

use super::{Composition, ControlTarget};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ControlPrefs {
    pub locked: Vec<ControlTarget>,
    /// A control's own snap points (0..1 of its range), besides the built-in ones (ends, default, 100%).
    pub snaps: Vec<(ControlTarget, Vec<f32>)>,
}

impl ControlPrefs {
    pub fn is_locked(&self, target: &ControlTarget) -> bool {
        self.locked.contains(target)
    }

    pub fn set_locked(&mut self, target: &ControlTarget, on: bool) {
        self.locked.retain(|t| t != target);
        if on {
            self.locked.push(target.clone());
        }
    }

    pub fn snaps_of(&self, target: &ControlTarget) -> &[f32] {
        self.snaps.iter().find(|(t, _)| t == target).map_or(&[], |(_, s)| s.as_slice())
    }

    /// Adds a snap point at `unit` (0..1; ignored if one is already that close), kept in order.
    pub fn add_snap(&mut self, target: &ControlTarget, unit: f32) {
        let unit = unit.clamp(0.0, 1.0);
        let i = match self.snaps.iter().position(|(t, _)| t == target) {
            Some(i) => i,
            None => {
                self.snaps.push((target.clone(), Vec::new()));
                self.snaps.len() - 1
            }
        };
        let s = &mut self.snaps[i].1;
        if !s.iter().any(|&x| (x - unit).abs() < 1e-3) {
            s.push(unit);
            s.sort_by(f32::total_cmp);
        }
    }

    pub fn remove_snap(&mut self, target: &ControlTarget, unit: f32) {
        for (_, s) in self.snaps.iter_mut().filter(|(t, _)| t == target) {
            s.retain(|&x| (x - unit).abs() >= 1e-3);
        }
        self.snaps.retain(|(_, s)| !s.is_empty());
    }

    pub fn clear_snaps(&mut self, target: &ControlTarget) {
        self.snaps.retain(|(t, _)| t != target);
    }
}

impl Composition {
    /// Locked controls ignore every input (the UI, controllers, OSC, phones).
    pub fn control_locked(&self, target: &ControlTarget) -> bool {
        self.control_prefs.is_locked(target)
    }
}

/// The point among `points` nearest `x` (Shift-drag on a slider).
pub fn nearest_snap(x: f32, points: &[f32]) -> Option<f32> {
    points.iter().copied().min_by(|a, b| (a - x).abs().total_cmp(&(b - x).abs()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locks_and_snaps_per_control() {
        let mut p = ControlPrefs::default();
        let a = ControlTarget::LayerOpacity(1);
        let b = ControlTarget::LayerOpacity(2);
        p.set_locked(&a, true);
        p.set_locked(&a, true);
        assert!(p.is_locked(&a) && !p.is_locked(&b));
        assert_eq!(p.locked.len(), 1);
        p.set_locked(&a, false);
        assert!(!p.is_locked(&a));
        p.add_snap(&a, 0.75);
        p.add_snap(&a, 0.25);
        p.add_snap(&a, 0.2501);
        assert_eq!(p.snaps_of(&a), &[0.25, 0.75]);
        assert!(p.snaps_of(&b).is_empty());
        p.remove_snap(&a, 0.25);
        assert_eq!(p.snaps_of(&a), &[0.75]);
        p.clear_snaps(&a);
        assert!(p.snaps.is_empty());
        assert_eq!(nearest_snap(0.6, &[0.0, 0.5, 1.0]), Some(0.5));
        assert_eq!(nearest_snap(0.9, &[0.0, 0.5, 1.0]), Some(1.0));
    }
}
