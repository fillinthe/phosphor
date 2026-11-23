//! Control Surface presets: a page's widget values saved by name and recalled with a fade (VDMX's section presets).

use super::{ControlSurface, ControlTarget, WidgetKind};
use serde::{Deserialize, Serialize};

/// The values of one page's widgets at the moment it was saved.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SurfacePreset {
    pub name: String,
    /// Which page it belongs to (index into `ControlSurface::pages`).
    pub page: usize,
    pub values: Vec<(ControlTarget, f32)>,
}

/// One value a recall sets: the widget's target, its value, and whether it glides (faders) or jumps (switches, choices).
#[derive(Clone, Debug, PartialEq)]
pub struct PresetStep {
    pub target: ControlTarget,
    pub value: f32,
    pub glides: bool,
}

impl ControlSurface {
    /// What a preset of `page` holds right now: every widget that keeps a value (not buttons, labels, text or computed
    /// ones), as the targets the widget answers to.
    pub fn snapshot(&self, page: usize) -> Vec<(ControlTarget, f32)> {
        let mut out = Vec::new();
        for w in self.widgets.iter().filter(|w| w.page == page) {
            match w.kind {
                WidgetKind::Fader | WidgetKind::Slider | WidgetKind::Knob | WidgetKind::Toggle | WidgetKind::Choices => out.push((ControlTarget::Surface(w.id), w.value)),
                WidgetKind::XyPad | WidgetKind::Color => {
                    out.push((ControlTarget::Surface(w.id), w.value));
                    out.push((ControlTarget::SurfaceY(w.id), w.value_y));
                }
                WidgetKind::Multi => out.extend(w.bars.iter().enumerate().map(|(i, b)| (ControlTarget::SurfaceBar { widget: w.id, bar: i as u8 }, b.value))),
                WidgetKind::Button | WidgetKind::Label | WidgetKind::Text | WidgetKind::Computed => {}
            }
        }
        out
    }

    /// Saves the page showing as a new preset; returns its index.
    pub fn save_preset(&mut self, name: String) -> usize {
        let page = self.current_page();
        self.presets.push(SurfacePreset { name, page, values: self.snapshot(page) });
        self.last_preset = Some(self.presets.len() - 1);
        self.presets.len() - 1
    }

    /// Saves the page's values again into preset `index` (keeps its name).
    pub fn update_preset(&mut self, index: usize) {
        if let Some(page) = self.presets.get(index).map(|p| p.page) {
            self.presets[index].values = self.snapshot(page);
        }
    }

    pub fn remove_preset(&mut self, index: usize) {
        if index < self.presets.len() {
            self.presets.remove(index);
            self.last_preset = None;
        }
    }

    /// The presets of `page`, as indexes into `presets`, in order.
    pub fn page_presets(&self, page: usize) -> Vec<usize> {
        (0..self.presets.len()).filter(|&i| self.presets[i].page == page).collect()
    }

    /// What recalling preset `index` sets (widgets removed since are left out).
    pub fn recall_steps(&self, index: usize) -> Vec<PresetStep> {
        let Some(p) = self.presets.get(index) else { return Vec::new() };
        p.values
            .iter()
            .filter_map(|(target, value)| {
                let id = match target {
                    ControlTarget::Surface(id) | ControlTarget::SurfaceY(id) => *id,
                    ControlTarget::SurfaceBar { widget, .. } => *widget,
                    _ => return None,
                };
                let w = self.widget(id)?;
                Some(PresetStep { target: target.clone(), value: *value, glides: !matches!(w.kind, WidgetKind::Toggle | WidgetKind::Choices) })
            })
            .collect()
    }

    /// The preset after the last one recalled, on the page showing (round to the first; None = the page has none).
    pub fn next_preset(&self) -> Option<usize> {
        let list = self.page_presets(self.current_page());
        let at = self.last_preset.and_then(|l| list.iter().position(|&i| i == l));
        match at {
            Some(k) => list.get((k + 1) % list.len()).copied(),
            None => list.first().copied(),
        }
    }

    /// Preset picked by a 0..1 value across the page's presets (a fader or a pick-one mapping).
    pub fn preset_at(&self, v: f32) -> Option<usize> {
        let list = self.page_presets(self.current_page());
        (!list.is_empty()).then(|| list[crate::pick(v, list.len())])
    }

    /// The 0..1 value that `preset_at` turns into the last recalled preset (controller feedback).
    pub fn preset_unit(&self) -> Option<f32> {
        let list = self.page_presets(self.current_page());
        let k = list.iter().position(|&i| Some(i) == self.last_preset)?;
        Some(crate::pick_unit(k, list.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preset_keeps_a_pages_values_and_steps_round() {
        let mut s = ControlSurface::default();
        let fader = s.add(WidgetKind::Fader);
        let toggle = s.add(WidgetKind::Toggle);
        let button = s.add(WidgetKind::Button);
        let multi = s.add(WidgetKind::Multi);
        s.widget_mut(multi).unwrap().set_bar_count(3);
        s.widget_mut(fader).unwrap().value = 0.25;
        s.widget_mut(toggle).unwrap().value = 1.0;
        let a = s.save_preset("A".into());
        let values = &s.presets[a].values;
        assert!(values.contains(&(ControlTarget::Surface(fader), 0.25)));
        assert!(values.contains(&(ControlTarget::SurfaceBar { widget: multi, bar: 2 }, 0.0)));
        assert!(!values.iter().any(|(t, _)| *t == ControlTarget::Surface(button)), "buttons hold no value");
        let steps = s.recall_steps(a);
        assert!(steps.iter().any(|p| p.target == ControlTarget::Surface(fader) && p.glides));
        assert!(steps.iter().any(|p| p.target == ControlTarget::Surface(toggle) && !p.glides), "switches jump");

        // Next goes round the page's presets; another page's presets aren't in the round.
        let b = s.save_preset("B".into());
        s.add_page();
        s.save_preset("Other page".into());
        s.page = 0;
        s.last_preset = Some(b);
        assert_eq!(s.next_preset(), Some(a));
        assert_eq!(s.preset_at(1.0), Some(b));
        assert_eq!(s.preset_unit().map(|u| s.preset_at(u)), Some(Some(b)));

        // A removed widget drops out of the recall; removing a page takes its presets along.
        s.remove(fader);
        assert!(!s.recall_steps(a).iter().any(|p| p.target == ControlTarget::Surface(fader)));
        s.remove_page(1);
        assert_eq!(s.presets.len(), 2);
    }
}
