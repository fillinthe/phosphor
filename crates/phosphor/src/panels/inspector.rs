//! Inspector for the selected layer.

use super::*;

pub(crate) fn inspector(ui: &mut egui::Ui, st: &mut State) {
    let aspect = st.aspect();
    let Some(sel) = st.selected.filter(|id| *id == MASTER_ID || st.comp.layers.iter().any(|l| l.id == *id) || st.comp.group(*id).is_some()) else {
        hint(ui, "Select a layer (or MASTER) to edit its source and effects.");
        return;
    };
    let clipboard_len = st.fx_clipboard.len();
    let mut fx_action: Option<FxAction> = None;
    if let Some(g) = st.comp.groups.iter_mut().find(|g| g.id == sel) {
        let thumb = st.layer_textures.get(&sel).map(|(t, _)| *t);
        fx_action = group_inspector(ui, g, thumb, aspect, clipboard_len);
    } else if sel == MASTER_ID {
        let tex = st.texture;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 210.0).max(80.0);
            let w = ui.available_width().min(360.0);
            thumbnail(ui, tex, egui::vec2(w, w / aspect), true);
            ui.label(RichText::new("MASTER").strong().color(ACCENT));
            hint(ui, "These effects run over the whole mix, after every layer has been blended.");
            ui.separator();
            fx_action = effects_editor(ui, &mut st.comp.master_effects, clipboard_len);
        });
    } else {
        let thumb = st.layer_textures.get(&sel).map(|(t, _)| *t);
        let info = st.media.video_info(sel);
        let position = st.media.position(sel);
        let has_sound = st.media.has_sound(sel);
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
            if let SourceSpec::Text(style) = &mut layer.source {
                ui.label(RichText::new("Text").strong());
                text_editor(ui, style, layer.id);
                ui.separator();
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new("Fade style").strong());
                choice_combo(ui, ("trans", layer.id), &mut layer.transition_kind, 120.0, false);
                hint(ui, "time and Cut/Fade: Launch settings (Clips panel)");
            });
            ui.separator();
            if matches!(layer.source, SourceSpec::Movie { .. }) {
                ui.label(RichText::new("Playback").strong());
                if let Some(info) = &info {
                    hint(ui, info);
                }
                if let Some((pos, dur)) = position {
                    let (mut lo, mut hi) = (layer.loop_in, layer.loop_out);
                    seek_to = scrub_bar(ui, pos, dur, (layer.loop_beats <= 0.0).then_some((&mut lo, &mut hi)));
                    if (lo, hi) != (layer.loop_in, layer.loop_out) {
                        layer.loop_in = lo.min(hi - 0.01).max(0.0);
                        layer.loop_out = hi.max(lo + 0.01).min(1.0);
                    }
                    let at = (pos / dur.max(1e-6)) as f32;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Section").small().color(Color32::GRAY));
                        let r = ui.button("In").on_hover_text("Loop starts here (drag the yellow marks on the bar, or right-click to map)");
                        map_menu(ui, &r, ControlTarget::LayerLoopIn(layer.id));
                        if r.clicked() {
                            layer.loop_in = at.min(layer.loop_out - 0.01).max(0.0);
                        }
                        let r = ui.button("Out").on_hover_text("Loop ends here");
                        map_menu(ui, &r, ControlTarget::LayerLoopOut(layer.id));
                        if r.clicked() {
                            layer.loop_out = at.max(layer.loop_in + 0.01).min(1.0);
                            layer.loop_beats = 0.0;
                        }
                        let beats = if layer.loop_beats > 0.0 { format!("{} beats", layer.loop_beats) } else { "Beats: off".into() };
                        ComboBox::from_id_salt(("loop_beats", layer.id)).width(84.0).selected_text(beats).show_ui(ui, |ui| {
                            ui.selectable_value(&mut layer.loop_beats, 0.0, "Off (use Out)");
                            for b in [0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0] {
                                ui.selectable_value(&mut layer.loop_beats, b, format!("{b} beats"));
                            }
                        }).response.on_hover_text("Loop a number of beats from In, at the current tempo");
                        if (layer.loop_in, layer.loop_out, layer.loop_beats) != (0.0, 1.0, 0.0) && ui.small_button("Clear").on_hover_text("Play the whole clip").clicked() {
                            (layer.loop_in, layer.loop_out, layer.loop_beats) = (0.0, 1.0, 0.0);
                        }
                    });
                }
                ui.horizontal(|ui| {
                    let text = if layer.playing { label(icon::PAUSE, "Pause") } else { label(icon::PLAY, "Play") };
                    ui.toggle_value(&mut layer.playing, text);
                    if ui.button(icon::SKIP_BACK).on_hover_text("Back to the start").clicked() {
                        seek_to = Some(0.0);
                    }
                    ui.checkbox(&mut layer.looping, "Loop");
                    ui.separator();
                    let target = ControlTarget::LayerDirection(layer.id);
                    choice_buttons(ui, &mut layer.direction, |ui, r| map_menu(ui, r, target.clone()));
                });
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(layer.sync_beats <= 0.0, |ui| {
                        let speed = guarded!(ui, layer.rate, Slider::new(&mut layer.rate, 0.1..=4.0).text("Speed").logarithmic(true));
                        map_menu(ui, &speed, ControlTarget::LayerSpeed(layer.id));
                    });
                });
                ui.horizontal(|ui| {
                    let mut sync = layer.sync_beats > 0.0;
                    if ui.toggle_value(&mut sync, label(icon::METRONOME, "BPM sync")).on_hover_text("Stretch the clip (or its loop) to a number of beats; it speeds up and slows down with the tempo").changed() {
                        layer.sync_beats = if sync { 16.0 } else { 0.0 };
                    }
                    if sync {
                        guarded!(ui, layer.sync_beats, egui::DragValue::new(&mut layer.sync_beats).range(1.0..=256.0).speed(0.1).suffix(" beats"));
                        for b in [4.0, 8.0, 16.0, 32.0] {
                            if ui.selectable_label(layer.sync_beats == b, format!("{b:.0}")).clicked() {
                                layer.sync_beats = b;
                            }
                        }
                        if layer.loop_beats > 0.0 {
                            hint(ui, "off while a beat loop is set");
                        }
                    }
                });
                if has_sound {
                    sound_row(ui, layer);
                }
                ui.separator();
            }
            transform_editor(ui, layer);
            ui.separator();
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

/// Position, scale, rotation and flips of a layer (all mappable; scale and rotation can be modulated).
fn transform_editor(ui: &mut egui::Ui, layer: &mut LayerState) {
    let id = layer.id;
    let t = &mut layer.transform;
    let title = if t.is_identity() { label(icon::ARROWS_OUT_CARDINAL, "Transform") } else { label(icon::ARROWS_OUT_CARDINAL, "Transform (moved)") };
    egui::CollapsingHeader::new(title).id_salt(("transform", id)).show(ui, |ui| {
        let row = |ui: &mut egui::Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, text: &str, target: ControlTarget, m: Option<&mut Option<Modulation>>, log: bool, reset: f32| {
            ui.horizontal(|ui| {
                let r = guarded!(ui, *value, Slider::new(value, range).text(text).logarithmic(log).fixed_decimals(2));
                mapped_badge(ui, &r, &target);
                if let Some(m) = m {
                    if control_menu(&r, Some(&target), Some(&mut *m)) {
                        *value = reset;
                    }
                    mod_button(ui, m);
                } else if control_menu(&r, Some(&target), None) {
                    *value = reset;
                }
            });
        };
        row(ui, &mut t.x, -1.0..=1.0, "X", ControlTarget::LayerX(id), None, false, 0.0);
        row(ui, &mut t.y, -1.0..=1.0, "Y", ControlTarget::LayerY(id), None, false, 0.0);
        row(ui, &mut t.scale, 0.05..=4.0, "Scale", ControlTarget::LayerScale(id), Some(&mut t.scale_mod), true, 1.0);
        row(ui, &mut t.rotation, -180.0..=180.0, "Rotation °", ControlTarget::LayerRotation(id), Some(&mut t.rotation_mod), false, 0.0);
        ui.horizontal(|ui| {
            guarded!(ui, t.stretch_x, egui::DragValue::new(&mut t.stretch_x).range(0.05..=8.0).speed(0.01).prefix("width ×"));
            guarded!(ui, t.stretch_y, egui::DragValue::new(&mut t.stretch_y).range(0.05..=8.0).speed(0.01).prefix("height ×"));
            ui.toggle_value(&mut t.flip_x, "Flip ↔");
            ui.toggle_value(&mut t.flip_y, "Flip ↕");
            if ui.add_enabled(!t.is_identity(), egui::Button::new(label(icon::ARROW_COUNTER_CLOCKWISE, "Reset"))).clicked() {
                *t = Default::default();
            }
        });
        hint(ui, "Tip: put an LFO on Rotation (~ button) to spin a layer, or the audio level on Scale to pulse it.");
    });
}

/// Movie sound: volume (mappable) and whether it fades with the layer's opacity.
fn sound_row(ui: &mut egui::Ui, layer: &mut LayerState) {
    ui.horizontal(|ui| {
        let icon = if layer.volume <= 0.001 { icon::SPEAKER_SIMPLE_X } else { icon::SPEAKER_SIMPLE_HIGH };
        if ui.small_button(icon).on_hover_text("Mute / unmute").clicked() {
            layer.volume = if layer.volume <= 0.001 { 1.0 } else { 0.0 };
        }
        let r = guarded!(ui, layer.volume, Slider::new(&mut layer.volume, 0.0..=2.0).text("Volume").show_value(false));
        value_bubble(ui, &r, format!("{:.0}%", layer.volume * 100.0));
        map_menu(ui, &r, ControlTarget::LayerVolume(layer.id));
    });
    ui.checkbox(&mut layer.volume_follows_opacity, "Sound follows opacity").on_hover_text("Fading the layer (or its group, or the crossfader) fades its sound too");
}

/// Edits a text layer: the words, font (searchable list of built-in and installed fonts), size, colour,
/// alignment, outline and line spacing. Changes redraw the layer in the background.
pub(crate) fn text_editor(ui: &mut egui::Ui, style: &mut phosphor_engine::TextStyle, id: u64) {
    use phosphor_engine::TextAlign;
    ui.add(egui::TextEdit::multiline(&mut style.text).desired_rows(3).desired_width(f32::INFINITY).hint_text("Type your text (Enter for a new line)"));
    ui.horizontal(|ui| {
        let search_id = egui::Id::new(("font_search", id));
        let mut search: String = ui.ctx().data(|d| d.get_temp(search_id)).unwrap_or_default();
        ComboBox::from_id_salt(("font", id)).width(200.0).selected_text(crate::text::font_name(&style.font)).height(320.0).show_ui(ui, |ui| {
            ui.add(egui::TextEdit::singleline(&mut search).hint_text("Search fonts").desired_width(180.0)).request_focus();
            let q = search.to_lowercase();
            for f in crate::text::fonts().iter().filter(|f| q.is_empty() || f.name.to_lowercase().contains(&q)).take(400) {
                ui.selectable_value(&mut style.font, f.id.clone(), &f.name);
            }
        });
        ui.ctx().data_mut(|d| d.insert_temp(search_id, search));
        let mut c = egui::Color32::from_rgba_unmultiplied(style.color[0], style.color[1], style.color[2], style.color[3]);
        if ui.color_edit_button_srgba(&mut c).changed() {
            style.color = c.to_srgba_unmultiplied();
        }
        for (a, glyph, tip) in [(TextAlign::Left, icon::TEXT_ALIGN_LEFT, "Left"), (TextAlign::Center, icon::TEXT_ALIGN_CENTER, "Centre"), (TextAlign::Right, icon::TEXT_ALIGN_RIGHT, "Right")] {
            ui.selectable_value(&mut style.align, a, glyph).on_hover_text(tip);
        }
    });
    let size = guarded!(ui, style.size, Slider::new(&mut style.size, 0.02..=0.8).text("Size").show_value(false));
    value_bubble(ui, &size, format!("{:.0}% of the height", style.size * 100.0));
    let outline = guarded!(ui, style.outline, Slider::new(&mut style.outline, 0.0..=0.15).text("Outline").show_value(false));
    value_bubble(ui, &outline, if style.outline > 0.0 { format!("{:.0}%", style.outline * 100.0) } else { "off".into() });
    guarded!(ui, style.line_spacing, Slider::new(&mut style.line_spacing, 0.6..=2.0).text("Line spacing").fixed_decimals(2));
    hint(ui, "Move, scale or animate it with effects (Zoom/Rotate, Tile, Wave…) and the layer's blend mode.");
}
