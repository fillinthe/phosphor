//! MIDI/OSC settings and the mapping list.

use super::*;

pub(crate) fn control(ui: &mut egui::Ui, st: &mut State) {
    use crate::control::{all_targets, target_label};
    let State { control, comp, learn_pick, .. } = st;
    ui.label(RichText::new("MIDI").strong());
    if control.midi_ports.is_empty() {
        ui.weak("No MIDI inputs found. Plug in a controller and press Reconnect.");
    } else {
        ui.weak(format!("Listening on: {}", control.midi_ports.join(", ")));
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new("OSC").strong());
        guarded!(ui, control.osc_port, egui::DragValue::new(&mut control.osc_port).range(1024..=65535).prefix("port "));
        if ui.button(label(icon::ARROWS_CLOCKWISE, "Reconnect")).clicked() {
            control.reconnect();
        }
    });
    ui.weak(&control.osc_status);
    ui.weak("TouchOSC: add a UDP connection to this address and port; Phosphor also appears in its Zeroconf list.");
    ui.separator();

    ui.label(RichText::new("Learn").strong());
    ui.horizontal(|ui| {
        let targets = all_targets(comp);
        let current = learn_pick.as_ref().map(|t| target_label(comp, t)).unwrap_or_else(|| "Pick what to control…".into());
        ComboBox::from_id_salt("learn_target").width(260.0).selected_text(current).show_ui(ui, |ui| {
            for (t, name) in targets {
                if ui.selectable_label(learn_pick.as_ref() == Some(&t), name).clicked() {
                    *learn_pick = Some(t);
                }
            }
        });
        let can_learn = learn_pick.is_some() && control.learning.is_none();
        if ui.add_enabled(can_learn, egui::Button::new(label(icon::BROADCAST, "Learn"))).clicked() {
            control.learning = learn_pick.clone();
        }
        if control.learning.is_some() && ui.button("Cancel").clicked() {
            control.learning = None;
        }
    });
    if let Some(t) = &control.learning {
        ui.colored_label(ACCENT, format!("Move a knob or send an OSC message to bind → {}", target_label(comp, t)));
    }
    match &control.last {
        Some(m) => ui.monospace(format!("last: {}  {:.3}", m.source.label(), m.value)),
        None => ui.weak("No controller messages received yet."),
    };
    ui.separator();

    ui.label(RichText::new("Mappings").strong());
    let mut remove = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        if comp.control_maps.is_empty() {
            ui.weak("None yet. Pick a target above, press Learn, then move a control.");
        }
        for (i, m) in comp.control_maps.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.small_button(icon::TRASH).clicked() {
                    remove = Some(i);
                }
                ui.monospace(m.source.label());
                ui.label("→");
                ui.label(target_label(comp, &m.target));
            });
        }
    });
    if let Some(i) = remove {
        comp.control_maps.remove(i);
    }
}
