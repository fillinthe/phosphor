//! MIDI/OSC settings and the mapping list.

use super::*;

pub(crate) fn control(ui: &mut egui::Ui, st: &mut State) {
    let control = &mut st.control;
    ui.label(RichText::new("MIDI").strong());
    if control.midi_ports.is_empty() {
        hint(ui, "No MIDI inputs found. Plug in a controller and press Reconnect.");
    } else {
        hint(ui, format!("Listening on: {}", control.midi_ports.join(", ")));
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new("OSC").strong());
        guarded!(ui, control.osc_port, egui::DragValue::new(&mut control.osc_port).range(1024..=65535).prefix("port "));
        if ui.button(label(icon::ARROWS_CLOCKWISE, "Reconnect")).clicked() {
            control.reconnect();
        }
    });
    hint(ui, &control.osc_status);
    ui.horizontal_wrapped(|ui| {
        hint(ui, "TouchOSC: add a UDP connection to this address and port; Phosphor also appears in its Zeroconf list.");
        #[cfg(not(target_arch = "wasm32"))]
        if ui
            .button(label(icon::EXPORT, "Export TouchOSC layout…"))
            .on_hover_text("A ready-made TouchOSC layout from your Control Surface (or master, layers and columns if you haven't built one), already sending Phosphor's addresses")
            .clicked()
        {
            st.dialogs.save_file(crate::dialogs::Purpose::TouchOsc, ("TouchOSC layout", &["tosc"]), "Phosphor.tosc");
        }
    });
    let control = &mut st.control;
    ui.horizontal(|ui| {
        ui.checkbox(&mut control.feedback, "Send feedback")
            .on_hover_text("Controllers follow Phosphor: values changed here (mouse, phone, other controllers) go back to TouchOSC faders and to MIDI controllers with motor faders or LED rings");
        ui.add_enabled_ui(control.feedback, |ui| {
            guarded!(ui, control.feedback_port, egui::DragValue::new(&mut control.feedback_port).range(1024..=65535).prefix("to port "))
                .on_hover_text("The port your OSC controller listens on (TouchOSC: the connection's receive port, 9000 by default)");
        });
    });
    match &control.last {
        Some(m) => ui.monospace(format!("last input: {}  {:.3}", m.source.label(), m.value)),
        None => hint(ui, "No controller messages received yet."),
    };
    ui.separator();

    mappings_editor(ui, st);
}

/// Every controller mapping, editable: Listen for a new input, retype or re-listen an input, pick
/// the target, set the range, switch on/off, delete. A live meter shows each input as it moves.
fn mappings_editor(ui: &mut egui::Ui, st: &mut State) {
    use crate::app::ListenFor;
    let State { control, comp, listen_for, map_draft, map_search, .. } = st;
    // A heard input lands where the Listen was for.
    if let Some(src) = control.heard.take() {
        match listen_for.take() {
            Some(ListenFor::Replace(i)) => {
                if let Some(m) = comp.control_maps.get_mut(i) {
                    m.source = src;
                }
            }
            _ => *map_draft = Some((src, None)),
        }
    }
    let listening = control.listen_source;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Mappings").strong());
        hint(ui, format!("{}", comp.control_maps.len()));
        if listening {
            ui.label(RichText::new(format!("{} Listening… touch a knob, pad or key, or send OSC", icon::EAR)).color(Color32::from_rgb(255, 205, 90)));
            ui.ctx().request_repaint();
            if ui.button("Cancel").clicked() {
                control.listen_source = false;
                *listen_for = None;
            }
        } else if ui.button(label(icon::EAR, "Listen")).on_hover_text("Waits for the next input (MIDI knob/pad, OSC message or key), then lets you pick what it controls").clicked() {
            control.listen_source = true;
            *listen_for = Some(ListenFor::NewMapping);
        }
        if ui.button(label(icon::PLUS, "Add by hand")).clicked() {
            *map_draft = Some((phosphor_engine::ControlSource::MidiCc { channel: 0, cc: 1 }, None));
        }
        ui.add(egui::TextEdit::singleline(map_search).hint_text("Search mappings").desired_width(140.0));
    });
    let targets = crate::control::all_targets(comp);
    // The mapping being made: input heard (or typed) → pick a target → Add.
    let mut finish: Option<bool> = None;
    if let Some((src, target)) = map_draft.as_mut() {
        card_frame(ui, true).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("New").strong().color(ACCENT));
                source_editor(ui, "new_map_source", src);
                ui.label("→");
                if let Some(t) = target_picker(ui, "new_map_target", &targets, target.as_ref(), 220.0) {
                    *target = Some(t);
                }
                if ui.add_enabled(target.is_some(), egui::Button::new("Add")).clicked() {
                    finish = Some(true);
                }
                if ui.small_button(icon::X).clicked() {
                    finish = Some(false);
                }
            });
        });
    }
    if let Some(add) = finish {
        if add && let Some((src, Some(t))) = map_draft.take() {
            comp.control_maps.push(phosphor_engine::ControlMap::new(src, t));
        }
        *map_draft = None;
    }

    let last = control.last.as_ref().map(|m| (m.source.clone(), m.value));
    let q = map_search.to_lowercase();
    let mut remove = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        if comp.control_maps.is_empty() && map_draft.is_none() {
            hint(ui, "None yet. Press Listen and move a control, or right-click any slider or button → Listen, or use MAP in the top bar.");
        }
        egui::Grid::new("mappings").num_columns(6).striped(true).spacing([6.0, 3.0]).show(ui, |ui| {
            for (i, m) in comp.control_maps.iter_mut().enumerate() {
                let name = targets.iter().find(|(t, _)| *t == m.target).map_or("(removed)", |(_, n)| n.as_str());
                if !q.is_empty() && !name.to_lowercase().contains(&q) && !m.source.label().to_lowercase().contains(&q) {
                    continue;
                }
                ui.checkbox(&mut m.enabled, "").on_hover_text("On / off");
                ui.horizontal(|ui| {
                    source_editor(ui, ("map_source", i), &mut m.source);
                    let relisten = *listen_for == Some(ListenFor::Replace(i)) && control.listen_source;
                    if ui.selectable_label(relisten, icon::EAR).on_hover_text("Listen again for this mapping's input").clicked() {
                        control.listen_source = true;
                        *listen_for = Some(ListenFor::Replace(i));
                    }
                });
                // Live meter: lights up and shows the value while this input moves.
                let live = last.as_ref().filter(|(s, _)| *s == m.source).map(|(_, v)| *v);
                let (r, _) = ui.allocate_exact_size(egui::vec2(36.0, 8.0), Sense::hover());
                ui.painter().rect_filled(r, 2.0, Color32::from_gray(35));
                if let Some(v) = live {
                    let mut fill = r;
                    fill.set_width(r.width() * v.clamp(0.0, 1.0));
                    ui.painter().rect_filled(fill, 2.0, ACCENT);
                }
                if let Some(t) = target_picker(ui, ("map_target", i), &targets, Some(&m.target), 220.0) {
                    m.target = t;
                }
                ui.horizontal(|ui| range_editor(ui, &mut m.range));
                if ui.small_button(icon::TRASH).on_hover_text("Delete").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    });
    if let Some(i) = remove {
        comp.control_maps.remove(i);
        *listen_for = None;
    }
}
