//! Mixer: one fader strip per layer, one per group (after its members) and MASTER.

use super::*;

pub(crate) fn fader(ui: &mut egui::Ui, value: &mut f32, m: &mut Option<Modulation>, target: ControlTarget, height: f32) {
    ui.spacing_mut().slider_width = height;
    let r = guarded!(ui, *value, Slider::new(value, 0.0..=1.0).vertical().show_value(false));
    value_bubble(ui, &r, format!("{:.0}%", *value * 100.0));
    if let Some(mm) = m.as_ref() {
        let live = phosphor_engine::control::modulate(*value, 0.0, 1.0, Some(mm), &mod_inputs(ui));
        let rr = r.rect;
        let pad = rr.width() * 0.5;
        let y = rr.bottom() - pad - live * (height - 2.0 * pad).max(1.0);
        ui.painter().line_segment([egui::pos2(rr.left() - 2.0, y), egui::pos2(rr.right() + 2.0, y)], Stroke::new(2.0, ACCENT));
    }
    mapped_badge(ui, &r, &target);
    if control_menu(&r, Some(&target), Some(m)) {
        *value = 1.0;
    }
}

/// One Mixer strip: card, thumbnail (click selects) and name; `body` adds the controls. Returns true when clicked.
fn strip(ui: &mut egui::Ui, id: u64, is_sel: bool, thumb: Option<egui::TextureId>, aspect: f32, lit: bool, name: RichText, body: impl FnOnce(&mut egui::Ui)) -> bool {
    let card_id = egui::Id::new(("mixer_strip", id));
    let bg = card_background(ui, card_id);
    let mut clicked = false;
    let frame = card_frame(ui, is_sel).inner_margin(egui::Margin::same(5)).show(ui, |ui| {
        ui.set_width(78.0);
        ui.vertical_centered(|ui| {
            clicked = thumbnail(ui, thumb, egui::vec2(72.0, 72.0 / aspect), lit).clicked();
            ui.label(name);
            body(ui);
        });
    });
    remember_card(ui, card_id, frame.response.rect);
    clicked || bg.is_some_and(|b| b.clicked())
}

fn short(name: &str) -> String {
    if name.chars().count() > 11 { format!("{}…", name.chars().take(10).collect::<String>()) } else { name.to_string() }
}

/// ON toggle with Learn on right-click (layers and groups).
fn on_toggle(ui: &mut egui::Ui, enabled: &mut bool, target: ControlTarget) {
    let on = ui.toggle_value(enabled, RichText::new("ON").small());
    mapped_badge(ui, &on, &target);
    on.right_menu(|ui| learn_items(ui, &target));
}

pub(crate) fn mixer(ui: &mut egui::Ui, st: &mut State) {
    let aspect = st.aspect();
    let height = (ui.available_height() - 210.0).clamp(80.0, 600.0);
    let State { comp, selected, layer_textures, texture, .. } = st;
    crossfader(ui, comp);
    let any_solo = comp.layers.iter().any(|l| l.enabled && l.solo);
    let tex = |id: u64| layer_textures.get(&id).map(|(t, _)| *t);
    let phosphor_engine::Composition { layers, groups, master_fade, master_fade_mod, .. } = comp;
    egui::ScrollArea::horizontal().auto_shrink(false).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            let count = layers.len();
            for i in 0..count {
                let layer = &mut layers[i];
                let muted = any_solo && !layer.solo;
                let name = RichText::new(short(&layer.name)).small().strong();
                let lit = layer.enabled && !muted;
                if strip(ui, layer.id, *selected == Some(layer.id), tex(layer.id), aspect, lit, name, |ui| {
                    fader(ui, &mut layer.opacity, &mut layer.opacity_mod, ControlTarget::LayerOpacity(layer.id), height);
                    percent_field(ui, &mut layer.opacity);
                    ui.horizontal(|ui| {
                        on_toggle(ui, &mut layer.enabled, ControlTarget::LayerEnabled(layer.id));
                        ui.toggle_value(&mut layer.solo, RichText::new("S").small());
                    });
                    ui.horizontal(|ui| {
                        for g in [phosphor_engine::CrossGroup::A, phosphor_engine::CrossGroup::B] {
                            let on = layer.group == g;
                            if ui.selectable_label(on, RichText::new(g.name()).small().strong()).on_hover_text("Crossfader side (click again for none)").clicked() {
                                layer.group = if on { phosphor_engine::CrossGroup::None } else { g };
                            }
                        }
                    });
                    choice_combo(ui, ("mix_blend", layer.id), &mut layer.blend, 70.0, true);
                }) {
                    *selected = Some(layer.id);
                }
                // A group's strip follows its last member, like a bus.
                let parent = layers[i].parent;
                if let Some(gid) = parent
                    && layers.get(i + 1).and_then(|l| l.parent) != Some(gid)
                    && let Some(g) = groups.iter_mut().find(|g| g.id == gid)
                {
                    let name = RichText::new(format!("{} {}", icon::STACK, short(&g.name))).small().strong().color(ACCENT);
                    if strip(ui, g.id, *selected == Some(g.id), tex(g.id), aspect, g.enabled, name, |ui| {
                        fader(ui, &mut g.opacity, &mut g.opacity_mod, ControlTarget::LayerOpacity(g.id), height);
                        percent_field(ui, &mut g.opacity);
                        on_toggle(ui, &mut g.enabled, ControlTarget::LayerEnabled(g.id));
                        choice_combo(ui, ("mix_group_blend", g.id), &mut g.blend, 70.0, true);
                    }) {
                        *selected = Some(g.id);
                    }
                }
            }
            // Master strip.
            let name = RichText::new("MASTER").small().strong().color(ACCENT);
            if strip(ui, MASTER_ID, *selected == Some(MASTER_ID), *texture, aspect, true, name, |ui| {
                fader(ui, master_fade, master_fade_mod, ControlTarget::MasterFade, height);
                percent_field(ui, master_fade);
            }) {
                *selected = Some(MASTER_ID);
            }
        });
    });
}

/// The A ↔ B crossfader across the top of the Mixer; double-click centres it.
fn crossfader(ui: &mut egui::Ui, comp: &mut phosphor_engine::Composition) {
    let target = ControlTarget::App(phosphor_engine::AppControl::Crossfader);
    let used = comp.layers.iter().any(|l| l.group != phosphor_engine::CrossGroup::None);
    ui.horizontal(|ui| {
        ui.label(RichText::new("A").strong());
        ui.spacing_mut().slider_width = (ui.available_width() - 40.0).clamp(120.0, 520.0);
        let r = guarded!(ui, comp.crossfader, Slider::new(&mut comp.crossfader, 0.0..=1.0).show_value(false));
        let r = r.on_hover_text(if used { "Crossfader: left = A layers, right = B layers, centre = both · double-click to centre" } else { "Crossfader: set layers to A or B below" });
        if r.double_clicked() {
            comp.crossfader = 0.5;
        }
        value_bubble(ui, &r, match comp.crossfader {
            x if x < 0.01 => "A".to_string(),
            x if x > 0.99 => "B".to_string(),
            x => format!("A {:.0}% · B {:.0}%", phosphor_engine::CrossGroup::A.gain(x) * 100.0, phosphor_engine::CrossGroup::B.gain(x) * 100.0),
        });
        mapped_badge(ui, &r, &target);
        map_menu(ui, &r, target);
        ui.label(RichText::new("B").strong());
    });
}
