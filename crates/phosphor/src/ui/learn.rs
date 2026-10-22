//! Controller mapping for any control: Learn, built-in OSC addresses, typed OSC/MIDI/key
//! mappings, the mapped badge and Map mode.

use super::*;

pub const LEARN_REQUEST: &str = "phosphor_learn_request";

pub const UNLEARN_REQUEST: &str = "phosphor_unlearn_request";

pub const MAP_REQUEST: &str = "phosphor_map_request";

pub const MAPPED_TARGETS: &str = "phosphor_mapped_targets";

/// Copy this control's mappings / paste the copied ones onto this control (the app keeps the copy).
pub const COPY_MAPS_REQUEST: &str = "phosphor_copy_maps_request";
pub const PASTE_MAPS_REQUEST: &str = "phosphor_paste_maps_request";
/// How many mappings are copied (set by the app each frame), so menus can offer Paste.
pub const MAPS_COPIED: &str = "phosphor_maps_copied";

/// The project's sends (`Composition::control_sends`), published each frame; and add / remove requests.
pub const SENDS: &str = "phosphor_sends";
pub const SEND_REQUEST: &str = "phosphor_send_request";

/// A copied motion (modulation), for pasting onto another value.
const MOTION_CLIPBOARD: &str = "phosphor_motion_clipboard";

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
    ui.ctx().data(|d| d.get_temp::<MappedList>(egui::Id::new(MAPPED_TARGETS))).is_some_and(|v| v.iter().any(|(t, _)| t == target))
}

/// The colour that says "mapped to a controller" (outline and tint on the control, Map mode, the menu).
pub(crate) const MAPPED: Color32 = Color32::from_rgb(255, 196, 70);

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
    let copied = ui.ctx().data(|d| d.get_temp::<usize>(egui::Id::new(MAPS_COPIED))).unwrap_or(0);
    if !sources.is_empty() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Mapped: {}", sources.join(", "))).small().color(MAPPED));
            if ui.small_button(label(icon::X, "Forget")).clicked() {
                request(ui, UNLEARN_REQUEST, target.clone());
                ui.close();
            }
            if ui.small_button(label(icon::COPY, "Copy")).on_hover_text("Copy these mappings (input, range, value effects) to paste onto another control").clicked() {
                request(ui, COPY_MAPS_REQUEST, target.clone());
                ui.close();
            }
        });
    }
    if copied > 0 && ui.button(label(icon::CLIPBOARD_TEXT, &format!("Paste mapping{} here", if copied == 1 { "" } else { "s" }))).on_hover_text("The copied inputs drive this control too").clicked() {
        request(ui, PASTE_MAPS_REQUEST, target.clone());
        ui.close();
    }
    send_items(ui, target);
}

/// "Send out…": this control's value goes out as OSC or MIDI whenever it changes (lights, another app, a synth).
fn send_items(ui: &mut egui::Ui, target: &ControlTarget) {
    use phosphor_engine::SendTo;
    let all: std::sync::Arc<Vec<phosphor_engine::ControlSend>> = ui.ctx().data(|d| d.get_temp(egui::Id::new(SENDS))).unwrap_or_default();
    let mine: Vec<SendTo> = all.iter().filter(|s| s.target == *target).map(|s| s.to.clone()).collect();
    let title = if mine.is_empty() { "Send out…".to_string() } else { format!("Send out ({})", mine.len()) };
    ui.menu_button(label(icon::BROADCAST, &title), |ui| {
        ui.set_min_width(280.0);
        hint(ui, "Whenever this changes, its value goes out too. OSC goes to the host and port set in the Control Surface (OSC out).");
        for to in &mine {
            ui.horizontal(|ui| {
                ui.label(to.label());
                if ui.small_button(icon::X).on_hover_text("Stop sending").clicked() {
                    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(SEND_REQUEST), (target.clone(), to.clone(), false)));
                }
            });
        }
        let id = egui::Id::new(("send_draft", format!("{target:?}")));
        let mut draft: SendTo = ui.ctx().data(|d| d.get_temp(id)).unwrap_or(SendTo::Osc { address: "/phosphor/out".into() });
        ui.horizontal(|ui| {
            for (name, make) in [("OSC", SendTo::Osc { address: "/phosphor/out".into() }), ("MIDI CC", SendTo::MidiCc { channel: 0, cc: 20 }), ("Note", SendTo::MidiNote { channel: 0, note: 60 })] {
                if ui.selectable_label(std::mem::discriminant(&draft) == std::mem::discriminant(&make), name).clicked() {
                    draft = make;
                }
            }
        });
        ui.horizontal(|ui| {
            match &mut draft {
                SendTo::Osc { address } => {
                    text_field(ui, address, "/address", Some(180.0));
                }
                SendTo::MidiCc { channel, cc } => {
                    let mut ch = *channel + 1;
                    guarded!(ui, ch, egui::DragValue::new(&mut ch).range(1..=16).prefix("ch "));
                    *channel = ch - 1;
                    guarded!(ui, *cc, egui::DragValue::new(cc).range(0..=127).prefix("CC "));
                }
                SendTo::MidiNote { channel, note } => {
                    let mut ch = *channel + 1;
                    guarded!(ui, ch, egui::DragValue::new(&mut ch).range(1..=16).prefix("ch "));
                    *channel = ch - 1;
                    guarded!(ui, *note, egui::DragValue::new(note).range(0..=127).prefix("note "));
                }
            }
            if ui.button("Add").clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(SEND_REQUEST), (target.clone(), draft.clone(), true)));
                ui.close();
            }
        });
        ui.ctx().data_mut(|d| d.insert_temp(id, draft));
    });
}

/// Small badge on a control that has a MIDI/OSC/key mapping.
pub(crate) fn mapped_badge(ui: &egui::Ui, resp: &egui::Response, target: &ControlTarget) {
    lock_badge(ui, resp, target);
    mapped_look(ui, resp, target, ui.visuals().widgets.inactive.corner_radius);
}

/// The mapped look and Map mode overlay, without the lock badge (sliders draw their own lock). A mapped control
/// is tinted gold with a gold outline and a dot in its corner, so it reads as mapped at a glance.
pub(crate) fn mapped_dot(ui: &egui::Ui, resp: &egui::Response, target: &ControlTarget) {
    mapped_look(ui, resp, target, egui::CornerRadius::ZERO);
}

/// `mapped_dot` with the control's own corners (square sliders, rounded buttons).
fn mapped_look(ui: &egui::Ui, resp: &egui::Response, target: &ControlTarget, corners: egui::CornerRadius) {
    let mapped = is_mapped(ui, target);
    if mapped {
        let painter = ui.painter();
        let rect = resp.rect.shrink(0.5);
        painter.rect_filled(rect, corners, MAPPED.gamma_multiply(0.16));
        painter.rect_stroke(rect, corners, Stroke::new(1.5, MAPPED), egui::StrokeKind::Inside);
        painter.circle_filled(resp.rect.left_top() + egui::vec2(2.5, 2.5), 3.0, MAPPED);
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
    let gold = MAPPED;
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
            let clip = egui::Id::new(MOTION_CLIPBOARD);
            if let Some(motion) = m.as_ref().filter(|_| moving)
                && ui.button(label(icon::COPY, "Copy the motion")).on_hover_text("To paste onto another value").clicked()
            {
                ui.ctx().data_mut(|d| d.insert_temp(clip, Some(motion.clone())));
                ui.close();
            }
            if let Some(motion) = ui.ctx().data(|d| d.get_temp::<Option<Modulation>>(clip)).flatten()
                && ui.button(label(icon::CLIPBOARD_TEXT, "Paste the motion")).on_hover_text("The copied modulation, on this value").clicked()
            {
                *m = Some(motion);
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
