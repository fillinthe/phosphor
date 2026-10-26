//! Layer Windows (VDMX-style): every layer as its own column, side by side — picture, source, opacity and blend,
//! then everything the Inspector shows (generator / text settings, playback, transform, effect chain). Top layer
//! on the left.

use std::collections::HashMap;

use super::*;

pub(crate) fn layer_windows(ui: &mut egui::Ui, st: &mut State) {
    let width_id = egui::Id::new("layer_windows_width");
    let mut col_w: f32 = ui.ctx().data(|d| d.get_temp(width_id)).unwrap_or(300.0);
    ui.horizontal(|ui| {
        if crate::ui::command(ui, label(icon::PLUS, "Add Layer")).clicked() {
            st.add_layer();
        }
        Level::range(&mut col_w, 220.0, 520.0, "Width").default(300.0).width(180.0).show(ui);
        help(ui, "Each layer in its own window, top layer on the left. Click a picture to select it for the Inspector and controllers.");
    });
    ui.ctx().data_mut(|d| d.insert_temp(width_id, col_w));
    let aspect = st.aspect();
    let feeds = feed_choices(&st.comp);
    let clipboard_len = st.fx_clipboard.len();
    // What each layer's movie is doing (read before the layers are borrowed for editing).
    let media: HashMap<u64, (Option<String>, Option<(f64, f64)>, bool)> =
        st.comp.layers.iter().map(|l| (l.id, (st.media.video_info(l.id), st.media.position(l.id), st.media.has_sound(l.id)))).collect();
    let comp_inherited: HashMap<u64, Vec<(String, String, bool)>> = st.comp.layers.iter().map(|l| (l.id, st.comp.inherited_effects(l.id))).collect();
    let any_solo = st.comp.layers.iter().any(|l| l.enabled && l.solo);
    let mut layer_action: Option<(u64, LayerAction)> = None;
    let mut fx_actions: Vec<(u64, FxAction)> = Vec::new();
    let mut beat_requests: Vec<u64> = Vec::new();
    let mut text_file: Option<u64> = None;
    let mut seeks: Vec<(u64, f64)> = Vec::new();
    let mut quick: Vec<(u64, crate::app::Quick)> = Vec::new();
    let height = ui.available_height();
    let State { comp, selected, layer_textures, media: manager, .. } = st;
    let bpm = comp.bpm;
    egui::ScrollArea::horizontal().auto_shrink(false).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            for layer in comp.layers.iter_mut().rev() {
                let id = layer.id;
                let is_sel = *selected == Some(id);
                let muted = any_solo && !layer.solo;
                let thumb = layer_textures.get(&id).map(|(t, _)| *t);
                let err = manager.errors.get(&id).cloned();
                ui.allocate_ui_with_layout(egui::vec2(col_w, height - 8.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    card_frame(ui, is_sel).show(ui, |ui| {
                        ui.set_width(col_w - 14.0);
                        ui.set_min_height(height - 24.0);
                        ui.horizontal(|ui| {
                            super::layers::show_button(ui, layer);
                            super::layers::solo_button(ui, &mut layer.solo);
                            ui.add(egui::TextEdit::singleline(&mut layer.name).desired_width(f32::INFINITY));
                        });
                        let w = ui.available_width();
                        let r = thumbnail(ui, thumb, egui::vec2(w, w / aspect), layer.enabled && !muted).on_hover_text("Click to select · double-click to toggle");
                        if r.clicked() {
                            *selected = Some(id);
                        }
                        if r.double_clicked() {
                            layer.enabled = !layer.enabled;
                        }
                        if let Some(e) = &err {
                            ui.colored_label(WARN, RichText::new(e).small());
                        }
                        if let Some(a) = source_picker(ui, layer, &feeds) {
                            layer_action = Some((id, a));
                        }
                        opacity_row(ui, layer);
                        if let Some(q) = crate::app::quick_row(ui) {
                            quick.push((id, q));
                        }
                        crate::ui::toolbar(ui, |ui| {
                            choice_combo(ui, ("lw_blend", id), &mut layer.blend, 120.0, false);
                            choice_combo(ui, ("lw_fit", id), &mut layer.fit, 74.0, false);
                        });
                        ui.separator();
                        crate::ui::column_scroll(ui).id_salt(("lw_scroll", id)).auto_shrink(false).show(ui, |ui| {
                            ui.spacing_mut().slider_width = (ui.available_width() - 150.0).max(70.0);
                            let (info, position, has_sound) = media.get(&id).cloned().unwrap_or((None, None, false));
                            let m = LayerMedia { info: info.as_deref(), position, has_sound, beat_pending: false, beat: None, bpm, chapters: Vec::new(), reload: Default::default(), text_file: Default::default(), match_beat: Default::default(), inherited: comp_inherited.get(&id).cloned().unwrap_or_default(), speed_above: false };
                            let (fx, seek) = layer_editor(ui, layer, &m, clipboard_len);
                            if m.match_beat.get() {
                                beat_requests.push(id);
                            }
                            if m.text_file.get() {
                                text_file = Some(id);
                            }
                            if let Some(chain_and_action) = fx {
                                fx_actions.push(chain_and_action);
                            }
                            if let Some(t) = seek {
                                seeks.push((id, t));
                            }
                        });
                    });
                });
            }
            if comp.layers.is_empty() {
                hint(ui, "No layers yet: Add Layer.");
            }
        });
    });
    let now = ui.ctx().input(|i| i.time);
    for (id, t) in seeks {
        st.media.seek(id, t);
    }
    for (id, q) in quick {
        st.quick_on(q, Some(id), now);
    }
    for (id, a) in fx_actions {
        apply_fx_action(st, id, a);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(id) = text_file {
        st.dialogs.pick_file(crate::dialogs::Purpose::TextFile(id), Some(("Text", &["txt", "md", "srt", "lrc", "csv"])));
    }
    #[cfg(target_arch = "wasm32")]
    let _ = text_file;
    #[cfg(not(target_arch = "wasm32"))]
    for id in beat_requests {
        st.match_beat(crate::media::rhythm::MatchTarget::Layer(id));
    }
    #[cfg(target_arch = "wasm32")]
    let _ = beat_requests;
    if let Some((id, a)) = layer_action {
        apply_layer_action(st, id, a, now);
    }
}
