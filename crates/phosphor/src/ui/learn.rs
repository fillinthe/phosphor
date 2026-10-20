//! Controller mapping for any control: Learn, built-in OSC addresses, typed OSC/MIDI/key
//! mappings, the mapped badge and Map mode.

use super::*;

pub const LEARN_REQUEST: &str = "phosphor_learn_request";

pub const UNLEARN_REQUEST: &str = "phosphor_unlearn_request";

pub const MAP_REQUEST: &str = "phosphor_map_request";

pub const MAPPED_TARGETS: &str = "phosphor_mapped_targets";

pub(crate) fn request(ui: &egui::Ui, key: &str, target: ControlTarget) {
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(key), target));
}

/// Mappings for the UI: (target, source description).
pub type MappedList = std::sync::Arc<Vec<(ControlTarget, String)>>;

pub(crate) fn mapped_sources(ui: &egui::Ui, target: &ControlTarget) -> Vec<String> {
    ui.ctx()
        .data(|d| d.get_temp::<MappedList>(egui::Id::new(MAPPED_TARGETS)))
        .map(|v| v.iter().filter(|(t, _)| t == target).map(|(_, s)| s.clone()).collect())
        .unwrap_or_default()
}

pub(crate) fn is_mapped(ui: &egui::Ui, target: &ControlTarget) -> bool {
    !mapped_sources(ui, target).is_empty()
}

/// Typed-in mapping drafts, remembered per control while the menu is open.
/// The "Map" section of any control's right-click menu: Learn, the built-in OSC address, typed
/// OSC / MIDI CC / key mappings, and Forget.
pub(crate) fn learn_items(ui: &mut egui::Ui, target: &ControlTarget) {
    ui.set_min_width(300.0);
    lock_item(ui, target);
    if ui.button(label(icon::EAR, "Listen: move a knob, send OSC, or press a key")).clicked() {
        request(ui, LEARN_REQUEST, target.clone());
        ui.close();
    }
    let address = crate::control::default_osc_address(target);
    ui.horizontal(|ui| {
        ui.label(RichText::new("OSC").small().color(Color32::GRAY));
        ui.monospace(&address).on_hover_text("Always works, no mapping needed. Send 0–1 (1 = press for buttons and clips).");
        if ui.small_button(icon::COPY).on_hover_text("Copy address").clicked() {
            ui.ctx().copy_text(address.clone());
        }
    });
    ui.menu_button(label(icon::PLUG, "Map to…"), |ui| {
        ui.set_min_width(300.0);
        hint(ui, "Type an input by hand (or use Listen above):");
        let id = egui::Id::new(("map_draft", format!("{target:?}")));
        let mut draft: phosphor_engine::ControlSource = ui.ctx().data(|d| d.get_temp(id)).unwrap_or(phosphor_engine::ControlSource::MidiCc { channel: 0, cc: 1 });
        let mut set = false;
        ui.horizontal(|ui| {
            source_editor(ui, ("map_to", format!("{target:?}")), &mut draft);
            set = ui.button("Map").clicked();
        });
        ui.ctx().data_mut(|d| d.insert_temp(id, draft.clone()));
        if set {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(MAP_REQUEST), (target.clone(), draft)));
            ui.close();
        }
    });
    let sources = mapped_sources(ui, target);
    if !sources.is_empty() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Mapped: {}", sources.join(", "))).small().color(Color32::from_rgb(255, 196, 70)));
            if ui.small_button(label(icon::X, "Forget")).clicked() {
                request(ui, UNLEARN_REQUEST, target.clone());
                ui.close();
            }
        });
    }
}

/// Small badge on a control that has a MIDI/OSC/key mapping.
pub(crate) fn mapped_badge(ui: &egui::Ui, resp: &egui::Response, target: &ControlTarget) {
    lock_badge(ui, resp, target);
    mapped_dot(ui, resp, target);
}

/// The mapped dot and Map mode overlay, without the lock badge (sliders draw their own lock).
pub(crate) fn mapped_dot(ui: &egui::Ui, resp: &egui::Response, target: &ControlTarget) {
    let mapped = is_mapped(ui, target);
    if mapped {
        let p = resp.rect.left_top() + egui::vec2(1.0, 1.0);
        ui.painter().circle_filled(p, 3.0, Color32::from_rgb(255, 196, 70));
    }
    if ui.ctx().data(|d| d.get_temp::<bool>(egui::Id::new(MAP_MODE))).unwrap_or(false) {
        map_overlay(ui, resp, target, mapped);
    }
}

pub const MAP_MODE: &str = "phosphor_map_mode";

pub const LEARNING: &str = "phosphor_learning";

/// Map mode: a clickable layer over each mappable control. Click it, then move a knob, send OSC
/// or press a key, and the two are linked. The control itself doesn't change while mapping.
pub(crate) fn map_overlay(ui: &egui::Ui, resp: &egui::Response, target: &ControlTarget, mapped: bool) {
    let learning = ui.ctx().data(|d| d.get_temp::<Option<ControlTarget>>(egui::Id::new(LEARNING))).flatten();
    let active = learning.as_ref() == Some(target);
    let rect = resp.rect.expand(1.5);
    // Click *and* drag, so sliders underneath get neither while mapping.
    let r = ui.interact(rect, resp.id.with("map_overlay"), Sense::click_and_drag());
    let gold = Color32::from_rgb(255, 196, 70);
    let (fill, stroke) = if active {
        let t = ui.ctx().input(|i| i.time) as f32;
        let a = 0.5 + 0.5 * (t * 6.0).sin();
        ui.ctx().request_repaint();
        (gold.gamma_multiply(0.25 + 0.2 * a), Stroke::new(2.0, gold))
    } else if mapped {
        (gold.gamma_multiply(0.12), Stroke::new(1.5, gold))
    } else if r.hovered() {
        (accent().gamma_multiply(0.25), Stroke::new(1.5, accent()))
    } else {
        (accent().gamma_multiply(0.08), Stroke::new(1.0, accent().gamma_multiply(0.7)))
    };
    let p = ui.painter();
    p.rect_filled(rect, 3.0, fill);
    p.rect_stroke(rect, 3.0, stroke, StrokeKind::Outside);
    if r.clicked() || r.drag_started() {
        request(ui, LEARN_REQUEST, target.clone());
    }
    let sources = mapped_sources(ui, target);
    let hint = if active {
        "Now move a knob, send OSC, or press a key".to_string()
    } else if sources.is_empty() {
        "Click to listen, then move a knob / send OSC / press a key".to_string()
    } else {
        format!("Mapped to {} — click to map again, right-click to forget", sources.join(", "))
    };
    r.on_hover_text(hint).right_menu(|ui| learn_items(ui, target));
}

/// The standard right-click menu for a slider: Learn, Modulate, Reset. Returns true on Reset (which also takes
/// off any modulation, so the value really stays at its default).
/// `extra` items come first (`ui::Level` puts a field to type the exact value there).
pub(crate) fn control_menu_with(resp: &egui::Response, target: Option<&ControlTarget>, m: Option<&mut Option<Modulation>>, extra: Option<&mut dyn FnMut(&mut egui::Ui)>) -> bool {
    let mut reset = false;
    resp.right_menu(|ui| {
        if let Some(extra) = extra {
            extra(ui);
            ui.separator();
        }
        if let Some(t) = target {
            learn_items(ui, t);
        }
        let moving = m.as_ref().is_some_and(|m| m.is_some());
        if let Some(m) = m {
            ui.menu_button(label(icon::WAVE_SINE, "Modulate…"), |ui| modulation_editor(ui, &mut *m));
            if moving && ui.button(label(icon::PAUSE, "Stop the motion")).on_hover_text("Keeps the value where it is set, without the modulation").clicked() {
                *m = None;
                ui.close();
            }
            if ui.button(label(icon::ARROW_COUNTER_CLOCKWISE, "Reset to default")).on_hover_text("The default value, with no modulation").clicked() {
                *m = None;
                reset = true;
                ui.close();
            }
        } else if ui.button(label(icon::ARROW_COUNTER_CLOCKWISE, "Reset to default")).clicked() {
            reset = true;
            ui.close();
        }
    });
    reset
}

/// Right-click → Map (Learn / OSC / MIDI / key) for a button or value, with the mapped badge.
pub(crate) fn map_menu(ui: &egui::Ui, resp: &egui::Response, target: ControlTarget) {
    mapped_badge(ui, resp, &target);
    resp.right_menu(|ui| learn_items(ui, &target));
}
