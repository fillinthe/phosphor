//! Modulation (LFOs, audio, time) for any value, and the standard parameter slider.

use super::*;

/// The project's layer names, in order (published by the app each frame) for settings that pick a layer.
pub(crate) const LAYER_NAMES: &str = "phosphor_layer_names";

pub(crate) fn param_slider(ui: &mut egui::Ui, values: &mut Vec<f32>, mods: &mut Vec<ParamMod>, i: usize, p: &shaders::Param, target: Option<ControlTarget>) {
    if values.len() <= i {
        values.resize(i + 1, p.default);
    }
    // A setting that picks a layer by number (a mask layer): the layers' names.
    if shaders::is_layer_pick(p) {
        let names = ui.ctx().data(|d| d.get_temp::<std::sync::Arc<Vec<String>>>(egui::Id::new(LAYER_NAMES))).unwrap_or_default();
        let n = values[i].round().max(0.0) as usize;
        // (0 = the effect's own layer where the setting reads from a layer, "Displace from layer #"; else nothing.)
        let none = if p.name.starts_with("Displace from") { "(this layer)" } else { "(none)" };
        let name_of = |k: usize| if k == 0 { none.to_string() } else { names.get(k - 1).cloned().unwrap_or_else(|| format!("Layer {k} (gone)")) };
        crate::ui::field(ui, p.name.trim_end_matches(" #"), |ui| {
            let r = fitted_combo(ui, ("param_layer", p.name, i)).selected_text(name_of(n)).show_ui(ui, |ui| {
                for k in 0..=names.len().min(p.max as usize) {
                    if crate::ui::pick_item(ui, n == k, name_of(k)).clicked() {
                        values[i] = k as f32;
                    }
                }
            });
            if let Some(t) = target {
                map_menu(ui, &r.response, t);
            }
        });
        return;
    }
    // A menu (ISF "long" inputs, the LUT mixer's LUTs): pick a name instead of sliding a number.
    if !p.labels.is_empty() {
        crate::ui::field(ui, p.name, |ui| {
            let current = p.label_of(values[i]).unwrap_or("?");
            let r = fitted_combo(ui, ("param_menu", p.name, i)).selected_text(current).show_ui(ui, |ui| {
                for (k, name) in p.labels.iter().enumerate() {
                    let v = p.min + k as f32;
                    if crate::ui::pick_item(ui, (values[i] - v).abs() < 0.5, *name).clicked() {
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
    // A color (an ISF color input: "Name R", "G", "B"…): a color button and swatches above its sliders.
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

/// A setting's menu exactly as wide as the room left in its row, its text cut short (whole in the list). A menu
/// as wide as its longest name made the row wider than the panel, and egui then lays every row under it out that
/// wide: the Inspector's buttons on the right (remove, up / down) went out of sight (bug report 2026-11-14, Windows).
fn fitted_combo(ui: &egui::Ui, id: impl std::hash::Hash + std::fmt::Debug) -> egui::ComboBox {
    egui::ComboBox::from_id_salt(id).width((ui.available_width() - ui.spacing().item_spacing.x).max(40.0)).truncate()
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

/// Temp-data key: the running position (0..1 of a cycle) of the shared modulator whose signal is drawn next.
pub(crate) const MOD_CYCLE: &str = "phosphor_mod_cycle";

/// One cycle of a signal's shape, with a playhead at `at` (0..1 of the cycle) and a dot at its value there.
pub(crate) fn wave_view(ui: &mut egui::Ui, m: &Modulation, at: Option<f32>, height: f32) -> egui::Response {
    const POINTS: usize = 96;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(360.0), height), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let p = ui.painter();
    p.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
    let inner = rect.shrink2(egui::vec2(0.0, 3.0));
    let y = |v: f32| inner.bottom() - v.clamp(0.0, 1.0) * inner.height();
    let pts: Vec<egui::Pos2> = (0..=POINTS)
        .map(|i| {
            // (Just short of the end, so the last point is the end of this cycle, not the start of the next.)
            let x = (i as f32 / POINTS as f32).min(0.9999);
            egui::pos2(inner.left() + inner.width() * i as f32 / POINTS as f32, y(phosphor_engine::wave_at(m, x)))
        })
        .collect();
    p.add(egui::Shape::line(pts, Stroke::new(1.5, accent())));
    if let Some(at) = at {
        let x = inner.left() + inner.width() * at.rem_euclid(1.0);
        p.vline(x, rect.y_range(), Stroke::new(1.0, ui.visuals().strong_text_color()));
        p.circle_filled(egui::pos2(x, y(phosphor_engine::wave_at(m, at.rem_euclid(1.0)))), 3.0, ui.visuals().strong_text_color());
        ui.ctx().request_repaint();
    }
    resp
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
        crate::ui::heading(ui, if signal_only { "Its signal" } else { "Drive this control with" });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::ui::pick_item(ui, current.is_none(), "Off").clicked() {
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
                if crate::ui::pick_item(ui, linked == *id, format!("{} {name}", icon::LINK)).on_hover_text("Follow this modulator (set it up in the Modulators panel)").clicked() {
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
                if crate::ui::pick_item(ui, current == Some(src), src.name()).clicked() {
                    *m = Some(Modulation { source: src, ..m.unwrap_or_default() });
                }
            }
        });
    };
    if linked == 0 {
        chips(ui, "AUDIO", |s| s.is_audio());
        chips(ui, "TIME & TEMPO", |s| !s.is_audio());
    } else if crate::ui::small_command(ui, "Use its own signal instead").clicked()
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
                crate::ui::dropdown(ui, "sync_rate", 100.0).selected_text(format!("every {current}")).show_ui(ui, |ui| {
                    for (v, name) in SYNC_CHOICES {
                        crate::ui::pick_value(ui, &mut mm.rate, *v, *name);
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    Level::range(&mut mm.rate, 0.01, 10.0, "Hz").default(1.0).log().show(ui);
                });
            }
        });
    }
    if mm.link == 0 && mm.source.is_lfo() {
        ui.horizontal(|ui| {
            Level::percent(&mut mm.phase, "Phase").default(0.0).show(ui).on_hover_text("Where in its cycle it starts: two LFOs at the same speed, one at 50%, move against each other");
        });
        if matches!(mm.source, ModSource::LfoSquare | ModSource::Exp) {
            ui.horizontal(|ui| {
                Level::percent(&mut mm.width, if mm.source == ModSource::LfoSquare { "Pulse width" } else { "Sharpness" }).default(0.5).show(ui).on_hover_text(if mm.source == ModSource::LfoSquare { "How much of each cycle it's on (10% = a short flash)" } else { "Lower = it waits longer, then rushes up harder" });
            });
        }
        // The wave itself, one cycle, with where it is now. (A shared modulator hands its own running position in.)
        let cycles = if mm.sync { inputs.beats / mm.rate.max(1.0 / 16.0) } else { inputs.time * mm.rate };
        let at = ui.ctx().data_mut(|d| d.remove_temp::<f32>(egui::Id::new(MOD_CYCLE))).unwrap_or(cycles.rem_euclid(1.0));
        wave_view(ui, mm, Some(at), if signal_only { 64.0 } else { 40.0 });
    }
    if mm.link == 0 && matches!(mm.source, ModSource::Steps | ModSource::Drawn) {
        let what = if mm.source == ModSource::Drawn { "Wave (drag to draw; a smooth loop through the points)" } else { "Steps (drag to set)" };
        ui.label(RichText::new(what).small().color(Color32::GRAY));
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
