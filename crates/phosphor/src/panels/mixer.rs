//! Mixer: one fader strip per layer.

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

pub(crate) fn mixer(ui: &mut egui::Ui, st: &mut State) {
    let aspect = st.aspect();
    let height = (ui.available_height() - 150.0).clamp(80.0, 600.0);
    let State { comp, selected, layer_textures, texture, .. } = st;
    let any_solo = comp.layers.iter().any(|l| l.enabled && l.solo);
    egui::ScrollArea::horizontal().auto_shrink(false).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            for layer in comp.layers.iter_mut() {
                let is_sel = *selected == Some(layer.id);
                let card_id = egui::Id::new(("mixer_strip", layer.id));
                let bg = card_background(ui, card_id);
                let frame = card_frame(ui, is_sel).inner_margin(egui::Margin::same(5)).show(ui, |ui| {
                    ui.set_width(78.0);
                    ui.vertical_centered(|ui| {
                        let tex = layer_textures.get(&layer.id).map(|(t, _)| *t);
                        let muted = any_solo && !layer.solo;
                        if thumbnail(ui, tex, egui::vec2(72.0, 72.0 / aspect), layer.enabled && !muted).clicked() {
                            *selected = Some(layer.id);
                        }
                        let mut name = layer.name.clone();
                        if name.chars().count() > 11 {
                            name = format!("{}…", name.chars().take(10).collect::<String>());
                        }
                        ui.label(RichText::new(name).small().strong());
                        fader(ui, &mut layer.opacity, &mut layer.opacity_mod, ControlTarget::LayerOpacity(layer.id), height);
                        percent_field(ui, &mut layer.opacity);
                        ui.horizontal(|ui| {
                            let on = ui.toggle_value(&mut layer.enabled, RichText::new("ON").small());
                            mapped_badge(ui, &on, &ControlTarget::LayerEnabled(layer.id));
                            on.right_menu(|ui| learn_items(ui, &ControlTarget::LayerEnabled(layer.id)));
                            ui.toggle_value(&mut layer.solo, RichText::new("S").small());
                        });
                        ComboBox::from_id_salt(("mix_blend", layer.id)).width(70.0).selected_text(RichText::new(layer.blend.name()).small()).show_ui(ui, |ui| {
                            for mode in BlendMode::ALL {
                                ui.selectable_value(&mut layer.blend, mode, mode.name());
                            }
                        });
                    });
                });
                remember_card(ui, card_id, frame.response.rect);
                if bg.is_some_and(|b| b.clicked()) {
                    *selected = Some(layer.id);
                }
            }
            // Master strip.
            card_frame(ui, *selected == Some(MASTER_ID)).inner_margin(egui::Margin::same(5)).show(ui, |ui| {
                ui.set_width(78.0);
                ui.vertical_centered(|ui| {
                    if thumbnail(ui, *texture, egui::vec2(72.0, 72.0 / aspect), true).clicked() {
                        *selected = Some(MASTER_ID);
                    }
                    ui.label(RichText::new("MASTER").small().strong().color(ACCENT));
                    fader(ui, &mut comp.master_fade, &mut comp.master_fade_mod, ControlTarget::MasterFade, height);
                    percent_field(ui, &mut comp.master_fade);
                });
            });
        });
    });
}
