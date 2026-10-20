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
    // "Shows on": the outputs, and whether the selection already shows on each.
    let outs: Vec<(u64, String, bool)> = {
        let showing = st.comp.outputs_showing(sel);
        st.comp.outputs.iter().map(|o| (o.id, o.name.clone(), showing.contains(&o.id))).collect()
    };
    let mut send: Option<(u64, bool)> = None;
    if let Some(g) = st.comp.groups.iter_mut().find(|g| g.id == sel) {
        let thumb = st.layer_textures.get(&sel).map(|(t, _)| *t);
        (fx_action, send) = group_inspector(ui, g, thumb, aspect, clipboard_len, &outs);
    } else if sel == MASTER_ID {
        let tex = st.texture;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 210.0).max(80.0);
            let w = ui.available_width().min(360.0);
            thumbnail(ui, tex, egui::vec2(w, w / aspect), true);
            ui.label(RichText::new("MASTER").strong().color(accent()));
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
        let mut quick = None;
        let mut match_beat: Option<u64> = None;
        #[cfg(not(target_arch = "wasm32"))]
        let (beat_pending, beat) = {
            let path = st.comp.layers.iter().find(|l| l.id == sel).and_then(|l| crate::app::beat_match::movie_path(&l.source));
            let pending = path.as_deref().is_some_and(|p| st.rhythm.is_pending(p));
            let found = path.and_then(|p| st.rhythm.results.get(&p).cloned()).and_then(|r| r.ok()).map(|r| (r.bpm, r.confidence));
            (pending, found)
        };
        #[cfg(target_arch = "wasm32")]
        let (beat_pending, beat): (bool, Option<(f32, f32)>) = (false, None);
        let layer = st.comp.layer_mut(sel).unwrap();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 210.0).max(80.0);
            let w = ui.available_width().min(360.0);
            thumbnail(ui, thumb, egui::vec2(w, w / aspect), layer.enabled);
            ui.horizontal(|ui| {
                super::layers::show_button(ui, layer);
                send = outputs_menu(ui, &outs).or(send);
                ui.add(egui::TextEdit::singleline(&mut layer.name).desired_width(f32::INFINITY));
            });
            quick = crate::app::quick_row(ui);
            ui.separator();
            let media = LayerMedia { info: info.as_deref(), position, has_sound, beat_pending, beat, match_beat: Default::default() };
            let (fx, seek) = layer_editor(ui, layer, &media, clipboard_len);
            fx_action = fx;
            seek_to = seek;
            if media.match_beat.get() {
                match_beat = Some(layer.id);
            }
        });
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(id) = match_beat {
            st.match_beat(crate::media::rhythm::MatchTarget::Layer(id));
        }
        #[cfg(target_arch = "wasm32")]
        let _ = match_beat;
        if let Some(t) = seek_to {
            st.media.seek(sel, t);
        }
        if let Some(q) = quick {
            st.quick_on(q, Some(sel), ui.ctx().input(|i| i.time));
        }
    }
    if let Some((o, on)) = send {
        if on {
            st.comp.send_to_output(sel, o);
        } else {
            st.comp.remove_from_output(sel, o);
        }
        let oname = st.comp.outputs.iter().find(|x| x.id == o).map_or(String::new(), |x| x.name.clone());
        st.notice = Some(if on { format!("Now on \"{oname}\" as a slice (Outputs area to shape it)") } else { format!("No longer on \"{oname}\" on its own") });
    }
    if let Some(action) = fx_action {
        apply_fx_action(st, sel, action);
    }
}

/// What a layer's movie is doing, for `layer_editor` (read before the layer is borrowed).
pub(crate) struct LayerMedia<'a> {
    pub info: Option<&'a str>,
    pub position: Option<(f64, f64)>,
    pub has_sound: bool,
    /// Match the beat: a scan of this movie is running.
    pub beat_pending: bool,
    /// Match the beat: what the last scan of this movie found (BPM, how steady 0..1).
    pub beat: Option<(f32, f32)>,
    /// Set by the editor when the Match the beat button is pressed (the caller starts the scan).
    pub match_beat: std::cell::Cell<bool>,
}

/// Everything about one layer below its name: generator / text settings, fade style, movie playback, transform
/// and effect chain (Inspector, and each column of the Layer Windows panel). Returns an effect-chain action and a
/// seek the caller carries out.
pub(crate) fn layer_editor(ui: &mut egui::Ui, layer: &mut LayerState, media: &LayerMedia, clipboard_len: usize) -> (Option<FxAction>, Option<f64>) {
    let (info, position, has_sound) = (media.info, media.position, media.has_sound);
    let mut seek_to: Option<f64> = None;

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
        if let Some(name) = layer.collection.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{} Playing collection · {name}", icon::BOOKMARKS_SIMPLE)).strong());
                if ui.small_button(label(icon::STOP, "Stop")).on_hover_text("Keep what's showing; stop stepping through the collection").clicked() {
                    layer.collection = None;
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Order");
                for a in [phosphor_engine::Autopilot::Next, phosphor_engine::Autopilot::Previous, phosphor_engine::Autopilot::Random] {
                    ui.selectable_value(&mut layer.autopilot, a, a.name());
                }
                let mut at_end = layer.autopilot_beats <= 0.0;
                if ui.selectable_label(at_end, "when the clip ends").clicked() {
                    at_end = true;
                    layer.autopilot_beats = 0.0;
                }
                if ui.selectable_label(!at_end, "after").clicked() && at_end {
                    layer.autopilot_beats = 16.0;
                }
                if layer.autopilot_beats > 0.0 {
                    guarded!(ui, layer.autopilot_beats, egui::DragValue::new(&mut layer.autopilot_beats).range(1.0..=256.0).speed(0.2).suffix(" beats"));
                }
            });
            hint(ui, "New files in the collection's folders join as they arrive. Put any clip or file on this layer yourself and it stops following the collection.");
            ui.separator();
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("Fade style").strong());
            choice_combo(ui, ("trans", layer.id), &mut layer.transition_kind, 120.0, false);
            if transition_effect_combo(ui, ("trans_fx", layer.id), layer.transition_kind, &mut layer.transition_effect) {
                layer.transition_values.clear();
            }
            hint(ui, "time and Cut/Fade: Launch settings (Clips panel)");
        });
        transition_settings(ui, layer);
        ui.separator();
        if matches!(layer.source, SourceSpec::Movie { .. }) {
            ui.label(RichText::new("Playback").strong());
            if let Some(info) = &info {
                hint(ui, *info);
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
                // Play / pause makes something happen: an action button (not a toggle pill).
                let text = if layer.playing { label(icon::PAUSE, "Pause") } else { label(icon::PLAY, "Play") };
                if action_button(ui, Act::Go, text).clicked() {
                    layer.playing = !layer.playing;
                }
                if ui.button(icon::SKIP_BACK).on_hover_text("Back to the start").clicked() {
                    seek_to = Some(0.0);
                }
                toggle(ui, &mut layer.looping, "Loop");
                ui.separator();
                let target = ControlTarget::LayerDirection(layer.id);
                choice_mapped(ui, &mut layer.direction, target.clone());
            });
            ui.horizontal(|ui| {
                ui.add_enabled_ui(layer.sync_beats <= 0.0, |ui| {
                    Level::range(&mut layer.rate, 0.1, 4.0, "Speed").log().default(1.0).target(ControlTarget::LayerSpeed(layer.id)).show(ui);
                });
            });
            ui.horizontal(|ui| {
                let mut sync = layer.sync_beats > 0.0;
                if toggle(ui, &mut sync, label(icon::METRONOME, "BPM sync")).on_hover_text("Stretch the clip (or its loop) to a number of beats; it speeds up and slows down with the tempo").changed() {
                    layer.sync_beats = if sync { 16.0 } else { 0.0 };
                }
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(path) = crate::app::beat_match::movie_path(&layer.source) {
                    let _ = path;
                    let pending = media.beat_pending;
                    let text = if pending { format!("{} Listening to the picture…", icon::HOURGLASS) } else { format!("{} Match the beat", icon::MAGIC_WAND) };
                    if ui.add_enabled(!pending, egui::Button::new(text)).on_hover_text("Scans the movie's motion for its own rhythm, then starts the loop on its first hit and stretches it so every hit lands on a beat").clicked() {
                        media.match_beat.set(true);
                    }
                    if let Some((bpm, steady)) = media.beat {
                        if steady >= crate::media::rhythm::STEADY {
                            hint(ui, format!("visual beat {bpm:.0} BPM · {:.0}% steady", steady * 100.0));
                        } else {
                            hint(ui, format!("no steady visual beat ({:.0}%): not locked", steady * 100.0));
                        }
                        if sync {
                            if ui.small_button("÷2").on_hover_text("Hits every second beat (half as many beats)").clicked() {
                                layer.sync_beats = (layer.sync_beats / 2.0).round().max(1.0);
                            }
                            if ui.small_button("×2").on_hover_text("Two hits per beat (twice the beats)").clicked() {
                                layer.sync_beats = (layer.sync_beats * 2.0).round().min(256.0);
                            }
                        }
                    }
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
    (effects_editor(ui, &mut layer.effects, clipboard_len), seek_to)
}

/// Position, scale, rotation and flips of a layer (all mappable; scale and rotation can be modulated).
fn transform_editor(ui: &mut egui::Ui, layer: &mut LayerState) {
    let id = layer.id;
    let t = &mut layer.transform;
    let title = if t.is_identity() { label(icon::ARROWS_OUT_CARDINAL, "Transform") } else { label(icon::ARROWS_OUT_CARDINAL, "Transform (moved)") };
    egui::CollapsingHeader::new(title).id_salt(("transform", id)).show(ui, |ui| {
        let row = |ui: &mut egui::Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, text: &str, target: ControlTarget, m: Option<&mut Option<Modulation>>, log: bool, reset: f32| {
            ui.horizontal(|ui| {
                let mut level = Level::range(value, *range.start(), *range.end(), text).default(reset).target(target);
                if log {
                    level = level.log();
                }
                if let Some(m) = m {
                    level = level.modulation(m);
                }
                level.show(ui);
            });
        };
        row(ui, &mut t.x, -1.0..=1.0, "X", ControlTarget::LayerX(id), None, false, 0.0);
        row(ui, &mut t.y, -1.0..=1.0, "Y", ControlTarget::LayerY(id), None, false, 0.0);
        row(ui, &mut t.scale, 0.05..=4.0, "Scale", ControlTarget::LayerScale(id), Some(&mut t.scale_mod), true, 1.0);
        row(ui, &mut t.rotation, -180.0..=180.0, "Rotation °", ControlTarget::LayerRotation(id), Some(&mut t.rotation_mod), false, 0.0);
        ui.horizontal(|ui| {
            guarded!(ui, t.stretch_x, egui::DragValue::new(&mut t.stretch_x).range(0.05..=8.0).speed(0.01).prefix("width ×"));
            guarded!(ui, t.stretch_y, egui::DragValue::new(&mut t.stretch_y).range(0.05..=8.0).speed(0.01).prefix("height ×"));
            toggle(ui, &mut t.flip_x, format!("Flip {}", icon::ARROWS_LEFT_RIGHT));
            toggle(ui, &mut t.flip_y, format!("Flip {}", icon::ARROWS_DOWN_UP));
            if ui.add_enabled(!t.is_identity(), egui::Button::new(label(icon::ARROW_COUNTER_CLOCKWISE, "Reset"))).clicked() {
                *t = Default::default();
            }
        });
        hint(ui, "Tip: put an LFO on Rotation (~ button) to spin a layer, or the audio level on Scale to pulse it.");
    });
}

/// Speaker toggle: sound on / off for a layer or a clip (keeps the volume). Shared by the Inspector and clip rows.
pub(crate) fn mute_button(ui: &mut egui::Ui, muted: &mut bool, what: &str) -> egui::Response {
    let (glyph, color) = if *muted { (icon::SPEAKER_SIMPLE_X, crate::theme::WARN) } else { (icon::SPEAKER_SIMPLE_HIGH, ui.visuals().text_color()) };
    let r = ui.small_button(RichText::new(glyph).color(color)).on_hover_text(if *muted { format!("Sound off for {what}: click to turn it on") } else { format!("Mute {what}") });
    if r.clicked() {
        *muted = !*muted;
    }
    r
}

/// Movie sound: volume (mappable) and whether it fades with the layer's opacity.
fn sound_row(ui: &mut egui::Ui, layer: &mut LayerState) {
    ui.horizontal(|ui| {
        mute_button(ui, &mut layer.muted, "this layer");
        let w = ui.available_width();
        volume_bar(ui, layer, w);
    });
    ui.checkbox(&mut layer.volume_follows_opacity, "Sound follows opacity").on_hover_text("Fading the layer (or its group, or the crossfader) fades its sound too");
}

/// A layer's volume (to 200%; the mark is 100%).
pub(crate) fn volume_bar(ui: &mut egui::Ui, layer: &mut LayerState, width: f32) {
    let tip = if layer.muted { "Volume (muted: click the speaker to hear it)" } else { "Volume" };
    Level::percent(&mut layer.volume, "Volume").max(2.0).target(ControlTarget::LayerVolume(layer.id)).width(width).tip(tip).show(ui);
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
    ui.horizontal(|ui| {
        Level::range(&mut style.size, 0.02, 0.8, "Size").default(0.18).tip("Text height, as a share of the picture's height").show(ui);
    });
    ui.horizontal(|ui| {
        Level::range(&mut style.outline, 0.0, 0.15, "Outline").default(0.0).show(ui);
    });
    ui.horizontal(|ui| {
        Level::range(&mut style.line_spacing, 0.6, 2.0, "Line spacing").default(1.0).show(ui);
    });
    hint(ui, "Move, scale or animate it with effects (Zoom/Rotate, Tile, Wave…) and the layer's blend mode.");
}

/// A shader transition's own settings (all but its progress), as bars; "Reset" goes back to its defaults.
fn transition_settings(ui: &mut egui::Ui, layer: &mut LayerState) {
    use phosphor_engine::{TransitionKind, shaders};
    if layer.transition_kind != TransitionKind::Shader {
        return;
    }
    let Some(desc) = shaders::transition(&layer.transition_effect) else { return };
    let progress = shaders::progress_index(desc);
    if desc.params.len() <= usize::from(progress.is_some()) {
        return;
    }
    if layer.transition_values.len() != desc.params.len() {
        layer.transition_values = desc.defaults();
    }
    egui::CollapsingHeader::new(RichText::new(format!("{} settings", desc.name)).small()).id_salt(("trans_settings", layer.id)).show(ui, |ui| {
        for (i, p) in desc.params.iter().enumerate() {
            if Some(i) == progress {
                continue;
            }
            ui.horizontal(|ui| {
                Level::range(&mut layer.transition_values[i], p.min, p.max, p.name).default(p.default).show(ui);
            });
        }
    });
}

/// Picks what a fade uses: the effect a "Through an effect" fade runs both pictures through, or the shader of a
/// "Shader transition" (cube, doorway, morph…). Returns true when it changed. Other styles: nothing to pick.
pub(crate) fn transition_effect_combo(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, kind: phosphor_engine::TransitionKind, effect: &mut String) -> bool {
    use phosphor_engine::{TransitionKind, shaders};
    let (list, current, empty) = match kind {
        TransitionKind::Effect => (shaders::all_effects(), shaders::effect(effect).map(|d| d.name), "Pick an effect"),
        TransitionKind::Shader => (shaders::all_transitions(), shaders::transition(effect).map(|d| d.name), "Pick a transition"),
        _ => return false,
    };
    let mut changed = false;
    egui::ComboBox::from_id_salt(id).selected_text(current.unwrap_or(empty)).width(150.0).height(360.0).show_ui(ui, |ui| {
        if list.is_empty() {
            hint(ui, "No shader transitions found (they come with the ISF collection).");
        }
        for d in list {
            if ui.selectable_label(d.id == effect.as_str(), d.name).clicked() {
                *effect = d.id.to_string();
                changed = true;
            }
        }
    });
    changed
}
