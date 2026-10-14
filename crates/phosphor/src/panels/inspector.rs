//! Inspector for the selected layer.

use super::*;

pub(crate) fn inspector(ui: &mut egui::Ui, st: &mut State) {
    let aspect = st.aspect();
    let Some(sel) = st.selected.filter(|id| *id == MASTER_ID || st.comp.layers.iter().any(|l| l.id == *id)) else {
        ui.weak("Select a layer (or MASTER) to edit its source and effects.");
        return;
    };
    let clipboard_len = st.fx_clipboard.len();
    let mut fx_action: Option<FxAction> = None;
    if sel == MASTER_ID {
        let tex = st.texture;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 210.0).max(80.0);
            let w = ui.available_width().min(360.0);
            thumbnail(ui, tex, egui::vec2(w, w / aspect), true);
            ui.label(RichText::new("MASTER").strong().color(ACCENT));
            ui.weak("These effects run over the whole mix, after every layer has been blended.");
            ui.separator();
            fx_action = effects_editor(ui, &mut st.comp.master_effects, clipboard_len);
        });
    } else {
        let thumb = st.layer_textures.get(&sel).map(|(t, _)| *t);
        let info = st.media.video_info(sel);
        let position = st.media.position(sel);
        let mut seek_to: Option<f64> = None;
        let layer = st.comp.layer_mut(sel).unwrap();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 210.0).max(80.0);
            let w = ui.available_width().min(360.0);
            thumbnail(ui, thumb, egui::vec2(w, w / aspect), layer.enabled);
            ui.horizontal(|ui| {
                ui.toggle_value(&mut layer.enabled, RichText::new("ON").small().strong());
                ui.add(egui::TextEdit::singleline(&mut layer.name).desired_width(f32::INFINITY));
            });
            ui.separator();

            if let SourceSpec::Generator(id) = &layer.source
                && let Some(desc) = shaders::generator(id)
            {
                ui.label(RichText::new(format!("Generator · {}", desc.name)).strong());
                for (i, p) in desc.params.iter().enumerate() {
                    param_slider(ui, &mut layer.generator_values, &mut layer.generator_mods, i, p, Some(ControlTarget::GeneratorParam { layer: layer.id, index: i }));
                }
                ui.separator();
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new("Fade style").strong());
                ComboBox::from_id_salt(("trans", layer.id)).width(120.0).selected_text(layer.transition_kind.name()).show_ui(ui, |ui| {
                    for k in TransitionKind::ALL {
                        ui.selectable_value(&mut layer.transition_kind, k, k.name());
                    }
                });
                ui.weak("time and Cut/Fade: Launch settings (Clips panel)");
            });
            ui.separator();
            if matches!(layer.source, SourceSpec::Movie { .. }) {
                ui.label(RichText::new("Playback").strong());
                if let Some(info) = &info {
                    ui.weak(info);
                }
                if let Some((pos, dur)) = position {
                    seek_to = scrub_bar(ui, pos, dur);
                }
                ui.horizontal(|ui| {
                    let text = if layer.playing { label(icon::PAUSE, "Pause") } else { label(icon::PLAY, "Play") };
                    ui.toggle_value(&mut layer.playing, text);
                    if ui.button(icon::SKIP_BACK).on_hover_text("Back to the start").clicked() {
                        seek_to = Some(0.0);
                    }
                    ui.checkbox(&mut layer.looping, "Loop");
                });
                let speed = guarded!(ui, layer.rate, Slider::new(&mut layer.rate, 0.1..=4.0).text("Speed").logarithmic(true));
                map_menu(ui, &speed, ControlTarget::LayerSpeed(layer.id));
                ui.separator();
            }
            fx_action = effects_editor(ui, &mut layer.effects, clipboard_len);
        });
        if let Some(t) = seek_to {
            st.media.seek(sel, t);
        }
    }
    if let Some(action) = fx_action {
        apply_fx_action(st, sel, action);
    }
}
