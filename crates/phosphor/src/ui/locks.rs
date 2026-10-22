//! Lock and snap points for any mappable control, from its right-click menu. The project keeps them
//! (`Composition::control_prefs`); the app publishes a copy each frame (`CONTROL_PREFS`) and applies the
//! changes widgets ask for (`PREFS_REQUEST`). Locked controls ignore the mouse here and every other input in
//! `control::set_target`.

use super::*;
use phosphor_engine::ControlPrefs;

pub const CONTROL_PREFS: &str = "phosphor_control_prefs";

pub const PREFS_REQUEST: &str = "phosphor_prefs_request";

/// A change a widget asks the app to make to `Composition::control_prefs`.
#[derive(Clone, Debug)]
pub enum PrefChange {
    Lock(ControlTarget, bool),
    AddSnap(ControlTarget, f32),
    RemoveSnap(ControlTarget, f32),
    ClearSnaps(ControlTarget),
    /// Set the low (true) or high limit to a 0..1 point, or clear both (None).
    Limit(ControlTarget, bool, Option<f32>),
}

impl PrefChange {
    pub fn apply(self, p: &mut ControlPrefs) {
        match self {
            PrefChange::Lock(t, on) => p.set_locked(&t, on),
            PrefChange::AddSnap(t, u) => p.add_snap(&t, u),
            PrefChange::RemoveSnap(t, u) => p.remove_snap(&t, u),
            PrefChange::ClearSnaps(t) => p.clear_snaps(&t),
            PrefChange::Limit(t, low, u) => p.set_limit(&t, low, u),
        }
    }
}

fn prefs(ui: &egui::Ui) -> Option<std::sync::Arc<ControlPrefs>> {
    ui.ctx().data(|d| d.get_temp::<std::sync::Arc<ControlPrefs>>(egui::Id::new(CONTROL_PREFS)))
}

pub(crate) fn is_locked(ui: &egui::Ui, target: &ControlTarget) -> bool {
    prefs(ui).is_some_and(|p| p.is_locked(target))
}

/// A control's limits (0..1 of its range), if it has any.
pub(crate) fn limits(ui: &egui::Ui, target: &ControlTarget) -> Option<(f32, f32)> {
    prefs(ui).and_then(|p| p.limits_of(target))
}

/// "Limits" for a slider's right-click menu: the lowest / highest it can go, set at the current value.
pub(crate) fn limit_items(ui: &mut egui::Ui, target: &ControlTarget, unit: f32, show: &dyn Fn(f32) -> String) {
    let now = limits(ui, target);
    ui.menu_button(label(icon::ARROWS_IN_LINE_HORIZONTAL, "Limits…"), |ui| {
        ui.set_min_width(240.0);
        hint(ui, "Use only part of the slider: the mouse stays inside, and a controller's whole travel covers just that part.");
        if let Some((lo, hi)) = now {
            ui.label(format!("Now: {} to {}", show(lo), show(hi)));
        }
        if ui.button(format!("Lowest = here ({})", show(unit))).clicked() {
            ask(ui, PrefChange::Limit(target.clone(), true, Some(unit)));
            ui.close();
        }
        if ui.button(format!("Highest = here ({})", show(unit))).clicked() {
            ask(ui, PrefChange::Limit(target.clone(), false, Some(unit)));
            ui.close();
        }
        if now.is_some() && ui.button(label(icon::X, "No limits")).clicked() {
            ask(ui, PrefChange::Limit(target.clone(), true, None));
            ui.close();
        }
    });
}

/// A control's own snap points (0..1 of its range).
pub(crate) fn snap_points(ui: &egui::Ui, target: &ControlTarget) -> Vec<f32> {
    prefs(ui).map(|p| p.snaps_of(target).to_vec()).unwrap_or_default()
}

pub(crate) fn ask(ui: &egui::Ui, change: PrefChange) {
    ui.ctx().data_mut(|d| d.get_temp_mut_or_default::<Vec<PrefChange>>(egui::Id::new(PREFS_REQUEST)).push(change));
}

/// "Lock" / "Unlock" for a control's right-click menu.
pub(crate) fn lock_item(ui: &mut egui::Ui, target: &ControlTarget) {
    let locked = is_locked(ui, target);
    let (glyph, text, tip) = if locked {
        (icon::LOCK_OPEN, "Unlock", "Let it move again")
    } else {
        (icon::LOCK, "Lock", "Nothing moves it: not the mouse, a controller, OSC or a phone")
    };
    if ui.button(label(glyph, text)).on_hover_text(tip).clicked() {
        ask(ui, PrefChange::Lock(target.clone(), !locked));
        ui.close();
    }
}

/// A small lock on a locked control (drawn with the mapped badge).
pub(crate) fn lock_badge(ui: &egui::Ui, resp: &egui::Response, target: &ControlTarget) {
    if is_locked(ui, target) {
        let at = resp.rect.right_top() + egui::vec2(-6.0, 6.0);
        ui.painter().circle_filled(at, 6.0, Color32::from_black_alpha(200));
        ui.painter().text(at, egui::Align2::CENTER_CENTER, icon::LOCK, egui::FontId::proportional(9.0), crate::theme::WARN);
    }
}

/// "Snap points" for a slider's right-click menu: add one at the current value, remove one, clear.
/// `unit` = the current value as 0..1 of the range; `show` writes a 0..1 point the way the slider shows values.
pub(crate) fn snap_items(ui: &mut egui::Ui, target: &ControlTarget, unit: f32, show: &dyn Fn(f32) -> String) {
    let points = snap_points(ui, target);
    ui.menu_button(label(icon::MAGNET, "Snap points…"), |ui| {
        ui.set_min_width(220.0);
        hint(ui, "Hold Shift while dragging to land on a snap point (the ends, the default and 100% always count)");
        if ui.button(label(icon::PLUS, &format!("Add one here ({})", show(unit)))).clicked() {
            ask(ui, PrefChange::AddSnap(target.clone(), unit));
        }
        if !points.is_empty() {
            ui.separator();
            for &u in &points {
                if ui.button(label(icon::X, &show(u))).on_hover_text("Remove this snap point").clicked() {
                    ask(ui, PrefChange::RemoveSnap(target.clone(), u));
                }
            }
            if danger_button(ui, "Remove all").clicked() {
                ask(ui, PrefChange::ClearSnaps(target.clone()));
                ui.close();
            }
        }
    });
}

/// An on/off toggle mapped to a control target: right-click to map or lock; ignores clicks while locked.
pub(crate) fn toggle_mapped(ui: &mut egui::Ui, on: &mut bool, text: impl Into<String>, target: ControlTarget) -> egui::Response {
    let r = toggle_locked(ui, on, text, is_locked(ui, &target));
    map_menu(ui, &r, target);
    r
}

/// `choice_buttons` mapped to a control target (right-click to map or lock; no change while locked).
pub(crate) fn choice_mapped<T: phosphor_engine::Choice>(ui: &mut egui::Ui, value: &mut T, target: ControlTarget) {
    let before = *value;
    choice_buttons(ui, value, |ui, r| map_menu(ui, r, target.clone()));
    if is_locked(ui, &target) {
        *value = before;
    }
}
