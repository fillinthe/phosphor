//! Modulation (LFOs, audio, time) for any value, and the standard parameter slider.

use super::*;

pub(crate) fn param_slider(ui: &mut egui::Ui, values: &mut Vec<f32>, mods: &mut Vec<ParamMod>, i: usize, p: &shaders::Param, target: Option<ControlTarget>) {
    if values.len() <= i {
        values.resize(i + 1, p.default);
    }
    // A menu (ISF "long" inputs, the LUT mixer's LUTs): pick a name instead of sliding a number.
    if !p.labels.is_empty() {
        ui.horizontal(|ui| {
            ui.label(p.name);
            let current = p.label_of(values[i]).unwrap_or("?");
            let r = egui::ComboBox::from_id_salt(("param_menu", p.name, i)).selected_text(current).show_ui(ui, |ui| {
                for (k, name) in p.labels.iter().enumerate() {
                    let v = p.min + k as f32;
                    if ui.selectable_label((values[i] - v).abs() < 0.5, *name).clicked() {
                        values[i] = v;
                    }
                }
            });
            if let Some(t) = target {
                map_menu(ui, &r.response, t);
            }
        });
        return;
    }
    // A colour (an ISF colour input: "Name R", "G", "B"…): a colour button and swatches above its sliders.
    if let Some(name) = p.name.strip_suffix(" R")
        && p.min == 0.0
        && p.max == 1.0
        && values.len() >= i + 3
    {
        ui.horizontal(|ui| {
            crate::ui::colour_button(ui, &mut values[i..i + 3]);
            ui.label(name);
        });
    }
    ui.horizontal(|ui| {
        let slot = mods.iter().position(|m| m.index == i);
        let mut m = slot.map(|k| mods[k].m);
        let mut level = Level::range(&mut values[i], p.min, p.max, p.name).default(p.default).modulation(&mut m);
        if let Some(t) = target {
            level = level.target(t);
        }
        level.show(ui);
        match (slot, m) {
            (Some(k), Some(nm)) => mods[k].m = nm,
            (Some(k), None) => {
                mods.remove(k);
            }
            (None, Some(nm)) => mods.push(ParamMod { index: i, m: nm }),
            (None, None) => {}
        }
    });
}

/// The `~` button next to a control: opens the modulation editor; lit when a signal is attached.
pub(crate) fn mod_button(ui: &mut egui::Ui, m: &mut Option<Modulation>) {
    let color = if m.is_some() { accent() } else { ui.visuals().weak_text_color() };
    ui.menu_button(RichText::new(icon::WAVE_SINE).color(color), |ui| modulation_editor(ui, m))
        .response
        .on_hover_text("Drive this with audio or an LFO");
}

/// Temp-data key: the project's shared modulators (id, name), for the Follow chips.
pub(crate) const MODULATORS: &str = "phosphor_modulators";

pub(crate) fn mod_inputs(ui: &egui::Ui) -> phosphor_engine::ModInputs {
    ui.ctx().data(|d| d.get_temp(egui::Id::new("phosphor_mod_inputs"))).unwrap_or_default()
}

pub(crate) const SYNC_CHOICES: &[(f32, &str)] =
    &[(0.25, "1/4 beat"), (0.5, "1/2 beat"), (1.0, "1 beat"), (2.0, "2 beats"), (4.0, "1 bar"), (8.0, "2 bars"), (16.0, "4 bars"), (32.0, "8 bars")];

/// A shared modulator's own signal (Modulators panel): the source, rate and steps; not the amount or range
/// (each control following it sets those) and no "follow another" chips (chaining does that).
pub(crate) fn modulation_editor_signal(ui: &mut egui::Ui, m: &mut Option<Modulation>) {
    signal_editor(ui, m, true);
}

pub(crate) fn modulation_editor(ui: &mut egui::Ui, m: &mut Option<Modulation>) {
    signal_editor(ui, m, false);
}

fn signal_editor(ui: &mut egui::Ui, m: &mut Option<Modulation>, signal_only: bool) {
    ui.set_min_width(320.0);
    let inputs = mod_inputs(ui);
    let current = m.map(|m| m.source);
    ui.horizontal(|ui| {
        ui.label(RichText::new(if signal_only { "Its signal" } else { "Drive this control with" }).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.selectable_label(current.is_none(), "Off").clicked() {
                *m = None;
            }
        });
    });
    // Follow a shared modulator (Modulators panel): one source driving many controls.
    let shared: Vec<(u64, String)> = if signal_only { Vec::new() } else { ui.ctx().data(|d| d.get_temp(egui::Id::new(MODULATORS))).unwrap_or_default() };
    let linked = m.map_or(0, |m| m.link);
    if !shared.is_empty() {
        ui.label(RichText::new("SHARED MODULATORS").small().color(Color32::GRAY));
        ui.horizontal_wrapped(|ui| {
            for (id, name) in &shared {
                if ui.selectable_label(linked == *id, format!("{} {name}", icon::LINK)).on_hover_text("Follow this modulator (set it up in the Modulators panel)").clicked() {
                    *m = Some(Modulation { link: *id, ..m.unwrap_or_default() });
                }
            }
        });
    }
    let mut chips = |ui: &mut egui::Ui, title: &str, filter: fn(ModSource) -> bool| {
        ui.label(RichText::new(title).small().color(Color32::GRAY));
        ui.horizontal_wrapped(|ui| {
            // "Hand / MIDI knob" only makes sense on a shared modulator (its value is the knob).
            for src in ModSource::ALL.into_iter().filter(|s| filter(*s) && (signal_only || *s != ModSource::Hand)) {
                if ui.selectable_label(current == Some(src), src.name()).clicked() {
                    *m = Some(Modulation { source: src, ..m.unwrap_or_default() });
                }
            }
        });
    };
    if linked == 0 {
        chips(ui, "AUDIO", |s| s.is_audio());
        chips(ui, "TIME & TEMPO", |s| !s.is_audio());
    } else if ui.small_button("Use its own signal instead").clicked()
        && let Some(mm) = m.as_mut()
    {
        mm.link = 0;
    }
    let Some(mm) = m else { return };
    ui.separator();
    if mm.link == 0 && mm.source.is_lfo() {
        ui.horizontal(|ui| {
            crate::ui::toggle(ui, &mut mm.sync, "Sync to tempo");
            if mm.sync {
                let current = SYNC_CHOICES.iter().find(|(v, _)| (*v - mm.rate).abs() < 1e-3).map_or("custom", |(_, n)| *n);
                ComboBox::from_id_salt("sync_rate").width(100.0).selected_text(format!("every {current}")).show_ui(ui, |ui| {
                    for (v, name) in SYNC_CHOICES {
                        ui.selectable_value(&mut mm.rate, *v, *name);
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    Level::range(&mut mm.rate, 0.01, 10.0, "Hz").default(1.0).log().show(ui);
                });
            }
        });
    }
    if mm.link == 0 && mm.source == ModSource::Steps {
        ui.label(RichText::new("Steps (drag to set)").small().color(Color32::GRAY));
        let cycles = if mm.sync { inputs.beats / mm.rate.max(1.0 / 16.0) } else { inputs.time * mm.rate };
        let playing = ((cycles.rem_euclid(1.0)) * 8.0) as usize;
        step_bars(ui, &mut mm.steps, Some(playing), false, 56.0);
    }
    if signal_only {
        let v = phosphor_engine::control::source_value(mm, &inputs);
        ui.add(crate::ui::meter(v).text(format!("live {v:.2}")).fill(accent().gamma_multiply(0.6)));
        return;
    }
    ui.horizontal(|ui| {
        segmented(ui, &mut mm.range, &[(false, "Add to slider".to_string()), (true, "Sweep a range".to_string())], |_, r, sweep| {
            r.clone().on_hover_text(if *sweep { "The signal moves the control between From and To; the slider is ignored" } else { "The signal pushes the slider up (or down) by Amount" });
        });
        crate::ui::toggle(ui, &mut mm.invert, "Invert");
    });
    if mm.range {
        ui.horizontal(|ui| {
            Level::percent(&mut mm.min, "From").default(0.0).show(ui);
        });
        ui.horizontal(|ui| {
            Level::percent(&mut mm.max, "To").default(1.0).show(ui);
        });
    } else {
        ui.horizontal(|ui| {
            Level::range(&mut mm.amount, -1.0, 1.0, "Amount").default(0.5).show(ui);
        });
    }
    let v = phosphor_engine::control::source_value(mm, &inputs);
    ui.add(crate::ui::meter(v).text(format!("live {v:.2}")).fill(accent().gamma_multiply(0.6)));
}
