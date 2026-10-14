//! Layers panel: master and layer cards, source picker.

use super::*;

pub(crate) enum LayerAction {
    BrowseSources,
    OpenFile,
    BrowseEffects,
    Monitor,
    Up,
    Down,
    Duplicate,
    Remove,
    ClearSource,
}

pub(crate) fn layers(ui: &mut egui::Ui, st: &mut State) {
    ui.horizontal(|ui| {
        if ui.button(label(icon::PLUS, "Add Layer")).clicked() {
            st.add_layer();
        }
        ui.weak("Top of the list is the top of the stack · drag a thumbnail to reorder · right-click for more");
    });
    ui.add_space(2.0);
    let aspect = st.aspect();
    let output_tex = st.texture;
    let any_solo = st.comp.layers.iter().any(|l| l.enabled && l.solo);
    let State { comp, selected, media, layer_textures, .. } = st;
    let count = comp.layers.len();
    let mut action = None;
    let mut drop: Option<(u64, u64)> = None;
    let mut media_drop: Option<(usize, usize)> = None;
    let mut palette_drop: Option<(u64, crate::explorer::PaletteDrag)> = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        let (pick, dropped) = master_card(ui, output_tex, aspect, *selected == Some(MASTER_ID), &comp.master_effects, &mut comp.master_fx_bypass);
        if pick {
            *selected = Some(MASTER_ID);
        }
        if let Some(d) = dropped {
            palette_drop = Some((MASTER_ID, d));
        }
        ui.add_space(4.0);
        for idx in (0..count).rev() {
            let layer = &mut comp.layers[idx];
            let thumb = layer_textures.get(&layer.id).map(|(t, _)| *t);
            let err = media.errors.get(&layer.id).cloned();
            let is_selected = *selected == Some(layer.id);
            let muted = any_solo && !layer.solo;
            let out = layer_card(ui, layer, thumb, aspect, is_selected, muted, err.as_deref());
            if out.select {
                *selected = Some(layer.id);
            }
            if let Some(a) = out.action {
                action = Some((idx, a));
            }
            if let Some(from) = out.dropped {
                drop = Some((from, layer.id));
            }
            if let Some(m) = out.dropped_media {
                media_drop = Some((idx, m));
            }
            if let Some(d) = out.dropped_palette {
                palette_drop = Some((layer.id, d));
            }
            ui.add_space(4.0);
        }
    });
    let media_launch = media_drop.and_then(|(idx, m)| Some((comp.layers.get(idx)?.id, comp.media_bin.get(m)?.clone())));
    if let Some((from, onto)) = drop
        && from != onto
        && let Some(a) = comp.layers.iter().position(|l| l.id == from)
        && let Some(b) = comp.layers.iter().position(|l| l.id == onto)
    {
        let moved = comp.layers.remove(a);
        comp.layers.insert(b, moved);
    }
    match action {
        Some((i, LayerAction::Up)) if i + 1 < count => comp.layers.swap(i, i + 1),
        Some((i, LayerAction::Down)) if i > 0 => comp.layers.swap(i, i - 1),
        Some((i, LayerAction::Duplicate)) => {
            let id = comp.layers[i].id;
            duplicate_layer(st, id);
        }
        Some((i, LayerAction::Remove)) => {
            let id = comp.layers[i].id;
            remove_layer(st, id);
        }
        Some((i, LayerAction::BrowseSources)) => {
            let id = comp.layers[i].id;
            st.fx_browser = Some(crate::explorer::FxBrowser::new(id, true, None));
        }
        Some((i, LayerAction::OpenFile)) => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let id = comp.layers[i].id;
                let exts = crate::media::dialog_extensions();
                st.dialogs.pick_file(crate::dialogs::Purpose::LayerSource(id), Some(("Media", &exts)));
            }
            #[cfg(target_arch = "wasm32")]
            let _ = i;
        }
        Some((i, LayerAction::BrowseEffects)) => {
            let id = comp.layers[i].id;
            st.fx_browser = Some(crate::explorer::FxBrowser::new(id, false, None));
        }
        Some((i, LayerAction::Monitor)) => {
            let id = comp.layers[i].id;
            st.new_monitor = Some(MonitorSource::Layer(id));
        }
        Some((i, LayerAction::ClearSource)) => {
            comp.layers[i].source = SourceSpec::None;
        }
        _ => {}
    }
    if let Some((target, d)) = palette_drop {
        crate::explorer::apply_palette(st, d, Some(target));
    }
    if let Some((layer, item)) = media_launch {
        let now = ui.ctx().input(|i| i.time);
        st.launch_source(layer, crate::media::spec_for_path(&item.path), Vec::new(), item.name, now);
    }
}

pub fn duplicate_layer(st: &mut State, id: u64) {
    let Some(i) = st.comp.layers.iter().position(|l| l.id == id) else { return };
    let mut copy = st.comp.layers[i].clone();
    copy.id = st.comp.new_id();
    copy.name = format!("{} copy", copy.name);
    for fx in &mut copy.effects {
        fx.id = st.comp.new_id();
    }
    st.selected = Some(copy.id);
    st.comp.layers.insert(i + 1, copy);
}

pub fn remove_layer(st: &mut State, id: u64) {
    let Some(i) = st.comp.layers.iter().position(|l| l.id == id) else { return };
    st.comp.layers.remove(i);
    if st.selected == Some(id) {
        st.selected = st.comp.layers.get(i.saturating_sub(1)).or(st.comp.layers.last()).map(|l| l.id);
    }
}

pub(crate) struct CardOut {
    select: bool,
    action: Option<LayerAction>,
    /// Id of a layer that was dropped onto this card.
    dropped: Option<u64>,
    /// Media bin index dropped onto this card.
    dropped_media: Option<usize>,
    /// Effect or generator dragged from the Effects explorer.
    dropped_palette: Option<crate::explorer::PaletteDrag>,
}

/// Drag payload from the media bin.
#[derive(Clone, Copy)]
pub struct MediaDrag(pub usize);

/// The pinned card for the whole mix: shows the output and the master effect count.
pub(crate) fn master_card(
    ui: &mut egui::Ui,
    tex: Option<egui::TextureId>,
    aspect: f32,
    is_selected: bool,
    effects: &[EffectState],
    bypass: &mut bool,
) -> (bool, Option<crate::explorer::PaletteDrag>) {
    let mut select = false;
    let card_id = egui::Id::new("master_card");
    let bg = card_background(ui, card_id);
    let frame = card_frame(ui, is_selected).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            if thumbnail(ui, tex, egui::vec2(112.0, 112.0 / aspect), true).clicked() {
                select = true;
            }
            ui.vertical(|ui| {
                if ui.selectable_label(is_selected, RichText::new("MASTER").strong().size(12.5).color(ACCENT)).clicked() {
                    select = true;
                }
                ui.weak("Effects here run over all layers");
                let fx = effects.iter().filter(|e| e.enabled).count();
                if fx > 0 {
                    let mut active = !*bypass;
                    let color = if active { ACCENT } else { Color32::GRAY };
                    let r = ui.toggle_value(&mut active, RichText::new(format!("{fx} fx")).small().color(color)).on_hover_text("Click to bypass the master effects · right-click to map");
                    map_menu(ui, &r, ControlTarget::App(phosphor_engine::AppControl::MasterFxBypass));
                    if r.changed() {
                        *bypass = !active;
                    }
                }
            });
        });
    });
    remember_card(ui, card_id, frame.response.rect);
    if bg.is_some_and(|b| b.clicked()) {
        select = true;
    }
    // Only effects make sense on MASTER.
    let dropped = frame.response.dnd_release_payload::<crate::explorer::PaletteDrag>().map(|p| *p).filter(|d| d.effect.is_some());
    if frame.response.dnd_hover_payload::<crate::explorer::PaletteDrag>().is_some_and(|d| d.effect.is_some()) {
        ui.painter().rect_stroke(frame.response.rect, 6.0, Stroke::new(2.0, ACCENT), StrokeKind::Outside);
    }
    (select, dropped)
}

pub(crate) fn layer_card(
    ui: &mut egui::Ui,
    layer: &mut LayerState,
    thumb: Option<egui::TextureId>,
    aspect: f32,
    is_selected: bool,
    muted: bool,
    err: Option<&str>,
) -> CardOut {
    let mut select = false;
    let mut action = None;
    let card_id = egui::Id::new(("layer_card", layer.id));
    let bg = card_background(ui, card_id);
    let frame = card_frame(ui, is_selected).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let r = thumbnail(ui, thumb, egui::vec2(112.0, 112.0 / aspect), layer.enabled && !muted)
                .on_hover_text("Click to select · double-click to toggle · drag to reorder");
            r.dnd_set_drag_payload(layer.id);
            if r.clicked() {
                select = true;
            }
            if r.double_clicked() {
                layer.enabled = !layer.enabled;
            }
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let on = ui.toggle_value(&mut layer.enabled, RichText::new("ON").small().strong()).on_hover_text("Layer on/off (Space) · right-click to Learn");
                    mapped_badge(ui, &on, &ControlTarget::LayerEnabled(layer.id));
                    on.right_menu(|ui| learn_items(ui, &ControlTarget::LayerEnabled(layer.id)));
                    let solo_color = if layer.solo { WARN } else { Color32::GRAY };
                    ui.toggle_value(&mut layer.solo, RichText::new("S").small().strong().color(solo_color))
                        .on_hover_text("Solo: only soloed layers reach the mix");
                    let name = RichText::new(&layer.name).strong().size(12.5);
                    let name = if muted { name.color(Color32::GRAY) } else { name };
                    if ui.selectable_label(is_selected, name).clicked() {
                        select = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(icon::TRASH).on_hover_text("Delete layer").clicked() {
                            action = Some(LayerAction::Remove);
                        }
                        if ui.small_button(icon::COPY).on_hover_text("Duplicate layer").clicked() {
                            action = Some(LayerAction::Duplicate);
                        }
                        if ui.small_button(icon::ARROW_DOWN).on_hover_text("Move down the stack").clicked() {
                            action = Some(LayerAction::Down);
                        }
                        if ui.small_button(icon::ARROW_UP).on_hover_text("Move up the stack").clicked() {
                            action = Some(LayerAction::Up);
                        }
                    });
                });
                if let Some(a) = source_picker(ui, layer) {
                    action = Some(a);
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().slider_width = (ui.available_width() - 100.0).max(60.0);
                    let r = guarded!(ui, layer.opacity, Slider::new(&mut layer.opacity, 0.0..=1.0).show_value(false)).on_hover_text("Opacity");
                    mod_marker(ui, &r, layer.opacity, 0.0, 1.0, layer.opacity_mod.as_ref());
                    mapped_badge(ui, &r, &ControlTarget::LayerOpacity(layer.id));
                    if control_menu(&r, Some(&ControlTarget::LayerOpacity(layer.id)), Some(&mut layer.opacity_mod)) {
                        layer.opacity = 1.0;
                    }
                    ui.monospace(format!("{:3.0}%", layer.opacity * 100.0));
                    mod_button(ui, &mut layer.opacity_mod);
                });
                ui.horizontal(|ui| {
                    ComboBox::from_id_salt(("blend", layer.id)).width(104.0).selected_text(layer.blend.name()).show_ui(ui, |ui| {
                        for mode in BlendMode::ALL {
                            ui.selectable_value(&mut layer.blend, mode, mode.name());
                        }
                    });
                    ComboBox::from_id_salt(("fit", layer.id)).width(74.0).selected_text(layer.fit.name()).show_ui(ui, |ui| {
                        for mode in FitMode::ALL {
                            ui.selectable_value(&mut layer.fit, mode, mode.name());
                        }
                    });
                    let fx = layer.effects.iter().filter(|e| e.enabled).count();
                    if fx > 0 {
                        let mut active = !layer.fx_bypass;
                        let color = if active { ACCENT } else { Color32::GRAY };
                        let r = ui.toggle_value(&mut active, RichText::new(format!("{fx} fx")).small().color(color)).on_hover_text("Click to bypass this layer's effects · right-click to map");
                        map_menu(ui, &r, ControlTarget::LayerFxBypass(layer.id));
                        if r.changed() {
                            layer.fx_bypass = !active;
                        }
                    }
                });
            });
        });
        if let Some(err) = err {
            ui.colored_label(WARN, err);
        }
    });
    remember_card(ui, card_id, frame.response.rect);
    let resp = bg.unwrap_or_else(|| frame.response.clone());
    if resp.clicked() {
        select = true;
    }
    resp.right_menu(|ui| {
        if ui.button(label(icon::SPARKLE, "Browse sources…")).clicked() {
            action = Some(LayerAction::BrowseSources);
            ui.close();
        }
        if ui.button(label(icon::MAGIC_WAND, "Add effect…")).clicked() {
            action = Some(LayerAction::BrowseEffects);
            ui.close();
        }
        if ui.button(label(icon::MONITOR, "Show in a new monitor")).clicked() {
            action = Some(LayerAction::Monitor);
            ui.close();
        }
        ui.separator();
        learn_items(ui, &ControlTarget::LayerEnabled(layer.id));
        if ui.button(label(icon::BROADCAST, "Learn opacity")).clicked() {
            request(ui, LEARN_REQUEST, ControlTarget::LayerOpacity(layer.id));
            ui.close();
        }
        ui.separator();
        if ui.button(label(icon::COPY, "Duplicate (⌘D)")).clicked() {
            action = Some(LayerAction::Duplicate);
            ui.close();
        }
        if ui.button(label(icon::ARROW_UP, "Move up")).clicked() {
            action = Some(LayerAction::Up);
            ui.close();
        }
        if ui.button(label(icon::ARROW_DOWN, "Move down")).clicked() {
            action = Some(LayerAction::Down);
            ui.close();
        }
        if ui.button("Clear source").clicked() {
            action = Some(LayerAction::ClearSource);
            ui.close();
        }
        ui.separator();
        if ui.button(label(icon::TRASH, "Delete (⌫)")).clicked() {
            action = Some(LayerAction::Remove);
            ui.close();
        }
    });
    // Drop target for a dragged layer or a media bin item.
    let dropped = frame.response.dnd_release_payload::<u64>().map(|p| *p);
    let dropped_media = frame.response.dnd_release_payload::<MediaDrag>().map(|p| p.0);
    let dropped_palette = frame.response.dnd_release_payload::<crate::explorer::PaletteDrag>().map(|p| *p);
    let hovering = frame.response.dnd_hover_payload::<u64>().is_some_and(|p| *p != layer.id)
        || frame.response.dnd_hover_payload::<MediaDrag>().is_some()
        || frame.response.dnd_hover_payload::<crate::explorer::PaletteDrag>().is_some();
    if hovering {
        ui.painter().rect_stroke(frame.response.rect, 6.0, Stroke::new(2.0, ACCENT), StrokeKind::Outside);
    }
    CardOut { select, action, dropped, dropped_media, dropped_palette }
}

/// Source menu for a layer card; returns a request for the browser or the Open dialog.
pub(crate) fn source_picker(ui: &mut egui::Ui, layer: &mut LayerState) -> Option<LayerAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        if ui.button(icon::SPARKLE).on_hover_text("Browse sources with previews").clicked() {
            action = Some(LayerAction::BrowseSources);
        }
        ComboBox::from_id_salt(("source", layer.id)).width(150.0).selected_text(layer.source.label()).show_ui(ui, |ui| {
            if ui.selectable_label(layer.source == SourceSpec::None, "No source").clicked() {
                layer.source = SourceSpec::None;
            }
            ui.separator();
            for g in shaders::GENERATORS {
                let active = matches!(&layer.source, SourceSpec::Generator(id) if id == g.id);
                if ui.selectable_label(active, g.name).clicked() {
                    layer.set_generator(g.id);
                }
            }
        });
        if ui.button(label(icon::FOLDER_OPEN, "Open")).on_hover_text("Image or movie file (or drag one onto the window)").clicked() {
            action = Some(LayerAction::OpenFile);
        }
    });
    action
}
