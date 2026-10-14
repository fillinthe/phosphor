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
#[derive(Clone, Default)]
pub(crate) struct MapDraft {
    osc: String,
    channel: u8,
    cc: u8,
    key: String,
}

/// The "Map" section of any control's right-click menu: Learn, the built-in OSC address, typed
/// OSC / MIDI CC / key mappings, and Forget.
pub(crate) fn learn_items(ui: &mut egui::Ui, target: &ControlTarget) {
    ui.set_min_width(300.0);
    if ui.button(label(icon::BROADCAST, "Learn: move a knob, send OSC, or press a key")).clicked() {
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
        ui.set_min_width(280.0);
        let id = egui::Id::new(("map_draft", format!("{target:?}")));
        let mut draft: MapDraft = ui.ctx().data(|d| d.get_temp(id)).unwrap_or(MapDraft { channel: 1, ..Default::default() });
        let mut chosen: Option<phosphor_engine::ControlSource> = None;
        ui.label(RichText::new("OSC address").small().color(Color32::GRAY));
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut draft.osc).hint_text("/touchosc/fader1").desired_width(190.0));
            if ui.add_enabled(draft.osc.starts_with('/'), egui::Button::new("Set")).clicked() {
                chosen = Some(phosphor_engine::ControlSource::Osc { address: draft.osc.trim().to_string() });
            }
        });
        ui.label(RichText::new("MIDI control change").small().color(Color32::GRAY));
        ui.horizontal(|ui| {
            guarded!(ui, draft.channel, egui::DragValue::new(&mut draft.channel).range(1..=16).prefix("ch "));
            guarded!(ui, draft.cc, egui::DragValue::new(&mut draft.cc).range(0..=127).prefix("CC "));
            if ui.button("Set").clicked() {
                chosen = Some(phosphor_engine::ControlSource::MidiCc { channel: draft.channel - 1, cc: draft.cc });
            }
            if ui.button("Note").on_hover_text("Use this number as a MIDI note (pads, keys) instead of a CC").clicked() {
                chosen = Some(phosphor_engine::ControlSource::MidiNote { channel: draft.channel - 1, note: draft.cc });
            }
        });
        ui.label(RichText::new("Keyboard key").small().color(Color32::GRAY));
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut draft.key).hint_text("Q").char_limit(12).desired_width(60.0));
            if ui.add_enabled(!draft.key.trim().is_empty(), egui::Button::new("Set")).clicked() {
                chosen = Some(phosphor_engine::ControlSource::Key { key: draft.key.trim().to_uppercase() });
            }
        });
        ui.ctx().data_mut(|d| d.insert_temp(id, draft));
        if let Some(source) = chosen {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(MAP_REQUEST), (target.clone(), source)));
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
        (ACCENT.gamma_multiply(0.25), Stroke::new(1.5, ACCENT))
    } else {
        (ACCENT.gamma_multiply(0.08), Stroke::new(1.0, ACCENT.gamma_multiply(0.7)))
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
        "Click, then move a knob / send OSC / press a key to map".to_string()
    } else {
        format!("Mapped to {} — click to map again, right-click to forget", sources.join(", "))
    };
    r.on_hover_text(hint).right_menu(|ui| learn_items(ui, target));
}

/// The standard right-click menu for a slider: Learn, Modulate, Reset. Returns true on Reset.
pub(crate) fn control_menu(resp: &egui::Response, target: Option<&ControlTarget>, m: Option<&mut Option<Modulation>>) -> bool {
    let mut reset = false;
    resp.right_menu(|ui| {
        if let Some(t) = target {
            learn_items(ui, t);
        }
        if let Some(m) = m {
            ui.menu_button(label(icon::WAVE_SINE, "Modulate…"), |ui| modulation_editor(ui, m));
        }
        if ui.button(label(icon::ARROW_COUNTER_CLOCKWISE, "Reset to default")).clicked() {
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
