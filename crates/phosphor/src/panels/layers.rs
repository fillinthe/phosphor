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
    /// New group from this layer and the one below it.
    GroupWithBelow,
    /// Take the layer out of its group.
    LeaveGroup,
    ClearSource,
    /// Put this source on the layer (through the launch settings).
    Launch(SourceSpec),
}

pub(crate) fn layers(ui: &mut egui::Ui, st: &mut State) {
    ui.horizontal(|ui| {
        if ui.button(label(icon::PLUS, "Add Layer")).clicked() {
            st.add_layer();
        }
        hint(ui, "Top of the list is the top of the stack · drag a thumbnail to reorder · right-click for more");
    });
    ui.add_space(2.0);
    let aspect = st.aspect();
    let output_tex = st.texture;
    let any_solo = st.comp.layers.iter().any(|l| l.enabled && l.solo);
    let feeds = feed_choices(&st.comp);
    let State { comp, selected, media, layer_textures, .. } = st;
    let count = comp.layers.len();
    let mut action = None;
    let mut drop: Option<(u64, u64)> = None;
    let mut media_drop: Option<(usize, usize)> = None;
    let mut palette_drop: Option<(u64, crate::explorer::PaletteDrag)> = None;
    let mut group_action: Option<(u64, GroupAction)> = None;
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
            // A group's card goes above its top member; folded groups hide their members.
            let parent = comp.layers[idx].parent;
            if let Some(gid) = parent {
                let top = comp.layers.get(idx + 1).is_none_or(|l| l.parent != Some(gid));
                let members = comp.layers.iter().filter(|l| l.parent == Some(gid)).count();
                if top && let Some(g) = comp.groups.iter_mut().find(|g| g.id == gid) {
                    let thumb = layer_textures.get(&gid).map(|(t, _)| *t);
                    let out = group_card(ui, g, thumb, aspect, *selected == Some(gid), members);
                    if out.select {
                        *selected = Some(gid);
                    }
                    if let Some(a) = out.action {
                        group_action = Some((gid, a));
                    }
                    if let Some(from) = out.dropped
                        && let Some(top_member) = comp.layers.get(idx)
                    {
                        drop = Some((from, top_member.id));
                    }
                    ui.add_space(4.0);
                }
                if comp.group(gid).is_some_and(|g| g.collapsed) {
                    continue;
                }
            }
            let layer = &mut comp.layers[idx];
            let thumb = layer_textures.get(&layer.id).map(|(t, _)| *t);
            let err = media.errors.get(&layer.id).cloned();
            let is_selected = *selected == Some(layer.id);
            let muted = any_solo && !layer.solo;
            let out = if parent.is_some() {
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    ui.vertical(|ui| layer_card(ui, layer, thumb, aspect, is_selected, muted, err.as_deref(), &feeds)).inner
                })
                .inner
            } else {
                layer_card(ui, layer, thumb, aspect, is_selected, muted, err.as_deref(), &feeds)
            };
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
    if let Some((from, onto)) = drop {
        comp.drop_layer(from, onto);
    }
    if let Some((i, a)) = action
        && let Some(id) = comp.layers.get(i).map(|l| l.id)
    {
        let now = ui.ctx().input(|i| i.time);
        apply_layer_action(st, id, a, now);
    }
    if let Some((gid, a)) = group_action {
        apply_group_action(st, gid, a);
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
    let mut copy = st.comp.adopt_layer(st.comp.layers[i].clone());
    copy.name = format!("{} copy", copy.name);
    st.selected = Some(copy.id);
    st.comp.layers.insert(i + 1, copy);
}

pub fn remove_layer(st: &mut State, id: u64) {
    let Some(i) = st.comp.layers.iter().position(|l| l.id == id) else { return };
    st.comp.layers.remove(i);
    st.comp.normalize_groups();
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
                if ui.selectable_label(is_selected, RichText::new("MASTER").strong().size(12.5).color(accent())).clicked() {
                    select = true;
                }
                hint(ui, "Effects here run over all layers");
                let fx = effects.iter().filter(|e| e.enabled).count();
                if fx > 0 {
                    let mut active = !*bypass;
                    let color = if active { accent() } else { Color32::GRAY };
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
        ui.painter().rect_stroke(frame.response.rect, 6.0, Stroke::new(2.0, accent()), StrokeKind::Outside);
    }
    (select, dropped)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn layer_card(
    ui: &mut egui::Ui,
    layer: &mut LayerState,
    thumb: Option<egui::TextureId>,
    aspect: f32,
    is_selected: bool,
    muted: bool,
    err: Option<&str>,
    feeds: &[(u64, String)],
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
                    // The name shrinks (full name on hover) so it never runs under the buttons on the right.
                    // Narrow cards drop the up/down arrows (drag the thumbnail, or right-click › Move up/down).
                    let button = ui.spacing().interact_size.y + ui.spacing().item_spacing.x;
                    let arrows = ui.available_width() - 4.0 * button >= 90.0;
                    let reserve = if arrows { 4.0 } else { 2.0 } * button;
                    let width = (ui.available_width() - reserve).max(24.0);
                    let r = ui.scope(|ui| {
                        ui.set_max_width(width);
                        ui.add(egui::Button::selectable(is_selected, name).truncate())
                    });
                    if r.inner.clicked() {
                        select = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(icon::TRASH).on_hover_text("Delete layer").clicked() {
                            action = Some(LayerAction::Remove);
                        }
                        if ui.small_button(icon::COPY).on_hover_text("Duplicate layer").clicked() {
                            action = Some(LayerAction::Duplicate);
                        }
                        if arrows && ui.small_button(icon::ARROW_DOWN).on_hover_text("Move down the stack").clicked() {
                            action = Some(LayerAction::Down);
                        }
                        if arrows && ui.small_button(icon::ARROW_UP).on_hover_text("Move up the stack").clicked() {
                            action = Some(LayerAction::Up);
                        }
                    });
                });
                if let Some(a) = source_picker(ui, layer, feeds) {
                    action = Some(a);
                }
                opacity_row(ui, layer);
                ui.horizontal(|ui| {
                    choice_combo(ui, ("blend", layer.id), &mut layer.blend, 104.0, false);
                    choice_combo(ui, ("fit", layer.id), &mut layer.fit, 74.0, false);
                    if let Some(r) = fx_badge(ui, &layer.effects, &mut layer.fx_bypass) {
                        map_menu(ui, &r, ControlTarget::LayerFxBypass(layer.id));
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
        if layer.parent.is_some() {
            if ui.button(label(icon::SELECTION_SLASH, "Take out of the group")).clicked() {
                action = Some(LayerAction::LeaveGroup);
                ui.close();
            }
        } else if ui.button(label(icon::STACK, "Group with the layer below (⌘G)")).on_hover_text("Mix the two together first, then treat them like one layer (opacity, blend, effects)").clicked() {
            action = Some(LayerAction::GroupWithBelow);
            ui.close();
        }
        ui.separator();
        if ui.button(label(icon::TRASH, "Delete (Backspace)")).clicked() {
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
        ui.painter().rect_stroke(frame.response.rect, 6.0, Stroke::new(2.0, accent()), StrokeKind::Outside);
    }
    CardOut { select, action, dropped, dropped_media, dropped_palette }
}

/// Source menu for a layer card; returns a request for the browser or the Open dialog.
/// What a layer can take another picture from: the program (feedback), groups and layers.
pub(crate) fn feed_choices(comp: &phosphor_engine::Composition) -> Vec<(u64, String)> {
    std::iter::once((MASTER_ID, "Program (feedback)".to_string()))
        .chain(comp.groups.iter().map(|g| (g.id, g.name.clone())))
        .chain(comp.layers.iter().rev().map(|l| (l.id, l.name.clone())))
        .collect()
}

pub(crate) fn source_picker(ui: &mut egui::Ui, layer: &mut LayerState, feeds: &[(u64, String)]) -> Option<LayerAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        if ui.button(icon::SPARKLE).on_hover_text("Browse sources with previews").clicked() {
            action = Some(LayerAction::BrowseSources);
        }
        // Leave room for the Open and Live icon buttons so the row never runs off the card.
        let width = (ui.available_width() - 2.0 * (ui.spacing().interact_size.y + 2.0 * ui.spacing().item_spacing.x)).clamp(60.0, 180.0);
        ComboBox::from_id_salt(("source", layer.id)).width(width).selected_text(layer.source.label()).show_ui(ui, |ui| {
            if ui.selectable_label(layer.source == SourceSpec::None, "No source").clicked() {
                layer.source = SourceSpec::None;
            }
            if ui.selectable_label(matches!(layer.source, SourceSpec::Text(_)), label(icon::TEXT_T, "Text")).on_hover_text("Words on the layer: type them in the Inspector").clicked() && !matches!(layer.source, SourceSpec::Text(_)) {
                layer.source = SourceSpec::Text(crate::text::default_style());
            }
            ui.menu_button(label(icon::ARROW_BEND_DOWN_RIGHT, "From another layer"), |ui| {
                hint(ui, "Use another layer's finished picture (with its effects), or the program for video feedback.");
                for (id, name) in feeds.iter().filter(|(id, _)| *id != layer.id) {
                    let active = matches!(&layer.source, SourceSpec::Feed { id: cur, .. } if cur == id);
                    if ui.selectable_label(active, name).clicked() {
                        layer.source = SourceSpec::Feed { id: *id, name: name.clone() };
                        ui.close();
                    }
                }
                // A layer feeding on itself = trails / feedback of its own previous frame.
                let me = matches!(&layer.source, SourceSpec::Feed { id, .. } if *id == layer.id);
                if ui.selectable_label(me, "Itself (previous frame)").clicked() {
                    layer.source = SourceSpec::Feed { id: layer.id, name: "itself".into() };
                    ui.close();
                }
            });
            ui.separator();
            for g in shaders::GENERATORS {
                let active = matches!(&layer.source, SourceSpec::Generator(id) if id == g.id);
                if ui.selectable_label(active, g.name).clicked() {
                    layer.set_generator(g.id);
                }
            }
        });
        if ui.button(icon::FOLDER_OPEN).on_hover_text("Open an image or movie file (or drag one onto the window)").clicked() {
            action = Some(LayerAction::OpenFile);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(source) = live_menu(ui, layer.id) {
            action = Some(LayerAction::Launch(source));
        }
    });
    action
}

/// Live inputs for a layer: cameras / capture cards on this Mac, or a stream address.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn live_menu(ui: &mut egui::Ui, layer: u64) -> Option<SourceSpec> {
    let mut picked = None;
    let r = ui.menu_button(icon::VIDEO_CAMERA, |ui| {
        ui.set_min_width(260.0);
        let cams_id = egui::Id::new("live_cameras");
        let mut cams: Option<Vec<String>> = ui.ctx().data(|d| d.get_temp(cams_id));
        if cams.is_none() {
            cams = Some(crate::media::live::cameras().into_iter().map(|c| c.name).collect());
        }
        let list = cams.clone().unwrap_or_default();
        ui.label(RichText::new("Cameras & capture cards").small().color(Color32::GRAY));
        if list.is_empty() {
            hint(ui, "None found.");
        }
        for name in &list {
            if ui.button(label(icon::VIDEO_CAMERA, name)).clicked() {
                picked = Some(crate::media::live::camera_source(&crate::media::live::Camera { name: name.clone() }));
                ui.close();
            }
        }
        if ui.small_button(label(icon::ARROWS_CLOCKWISE, "Rescan")).clicked() {
            cams = None;
        }
        ui.ctx().data_mut(|d| match &cams {
            Some(c) => {
                d.insert_temp(cams_id, c.clone());
            }
            None => d.remove::<Vec<String>>(cams_id),
        });
        #[cfg(target_os = "macos")]
        {
            ui.separator();
            ui.label(RichText::new("Syphon (other apps)").small().color(Color32::GRAY));
            let servers = crate::syphon::servers();
            if servers.is_empty() {
                hint(ui, "No app is publishing Syphon right now.");
            }
            for s in servers {
                if ui.button(label(icon::SHARE_NETWORK, &s.label())).clicked() {
                    picked = Some(crate::media::live::syphon_source(&s.label()));
                    ui.close();
                }
            }
        }
        #[cfg(any(target_os = "macos", windows))]
        {
            ui.separator();
            ui.label(RichText::new("NDI (network)").small().color(Color32::GRAY));
            let sources = crate::ndi::sources();
            if sources.is_empty() {
                hint(ui, if crate::ndi::Ndi::library().is_some() { "No NDI sources found yet (they appear within a second or two)." } else { "Install the free NDI Tools (ndi.video) to receive NDI." });
            }
            for s in sources {
                if ui.button(label(icon::BROADCAST, &s)).clicked() {
                    picked = Some(crate::media::live::ndi_source(&s));
                    ui.close();
                }
            }
        }
        ui.separator();
        ui.label(RichText::new("Stream address").small().color(Color32::GRAY));
        let url_id = egui::Id::new(("live_url", layer));
        let mut url: String = ui.ctx().data(|d| d.get_temp(url_id)).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut url).hint_text("rtsp://… srt://… https://….m3u8").desired_width(190.0));
            if ui.add_enabled(url.contains("://"), egui::Button::new("Play")).clicked() {
                picked = Some(crate::media::live::stream_source(&url));
                ui.close();
            }
        });
        ui.ctx().data_mut(|d| d.insert_temp(url_id, url));
    });
    r.response.on_hover_text("Live input: camera, capture card or network stream (RTSP, SRT, HLS, RTMP)");
    picked
}

/// Carries out a layer card's action (Layers panel and Layer Windows).
pub(crate) fn apply_layer_action(st: &mut State, id: u64, action: LayerAction, now: f64) {
    match action {
        LayerAction::Up => st.comp.move_layer(id, true),
        LayerAction::Down => st.comp.move_layer(id, false),
        LayerAction::GroupWithBelow => group_with_below(&mut st.comp, &mut st.selected, id),
        LayerAction::LeaveGroup => {
            if let Some(l) = st.comp.layer_mut(id) {
                l.parent = None;
            }
            st.comp.normalize_groups();
        }
        LayerAction::Duplicate => duplicate_layer(st, id),
        LayerAction::Remove => remove_layer(st, id),
        LayerAction::BrowseSources => st.fx_browser = Some(crate::explorer::FxBrowser::new(id, true, None)),
        LayerAction::OpenFile => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let exts = crate::media::dialog_extensions();
                st.dialogs.pick_file(crate::dialogs::Purpose::LayerSource(id), Some(("Media", &exts)));
            }
        }
        LayerAction::BrowseEffects => st.fx_browser = Some(crate::explorer::FxBrowser::new(id, false, None)),
        LayerAction::Monitor => st.new_monitor = Some(MonitorSource::Layer(id)),
        LayerAction::ClearSource => {
            if let Some(l) = st.comp.layer_mut(id) {
                l.source = SourceSpec::None;
            }
        }
        LayerAction::Launch(source) => {
            let name = source.label();
            st.launch_source(id, source, Vec::new(), name, now);
        }
    }
}

/// A layer's opacity: slider (mappable, modulation marker), % field and the ~ modulation button.
pub(crate) fn opacity_row(ui: &mut egui::Ui, layer: &mut LayerState) {
    ui.horizontal(|ui| {
        show_button(ui, layer);
        Level::percent(&mut layer.opacity, "Opacity").modulation(&mut layer.opacity_mod).target(ControlTarget::LayerOpacity(layer.id)).show(ui);
    });
}

/// The eye next to a layer's opacity: show / hide the layer in one click (Learn on right-click).
pub(crate) fn show_button(ui: &mut egui::Ui, layer: &mut LayerState) {
    let (glyph, color, tip) = if layer.enabled { (icon::EYE, ui.visuals().text_color(), "Showing: click to hide this layer") } else { (icon::EYE_SLASH, WARN, "Hidden: click to show this layer") };
    let r = ui.add(egui::Button::new(RichText::new(glyph).color(color)).frame(false).min_size(egui::vec2(22.0, 18.0))).on_hover_text(tip);
    if r.clicked() {
        layer.enabled = !layer.enabled;
    }
    mapped_badge(ui, &r, &ControlTarget::LayerEnabled(layer.id));
    r.right_menu(|ui| learn_items(ui, &ControlTarget::LayerEnabled(layer.id)));
}
