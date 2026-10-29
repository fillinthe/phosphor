//! Layers panel: master and layer cards, source picker.

use super::*;

pub(crate) enum LayerAction {
    BrowseSources,
    OpenFile,
    /// Pick a 3D model (OBJ) to show as points.
    OpenModel,
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
    /// Into another comp.
    MoveToComp(u64),
    /// Colours like a reference picture (a Colour Match effect).
    MatchColour,
    /// A Palette effect with this layer's own main colours.
    Palette,
}

pub(crate) fn layers(ui: &mut egui::Ui, st: &mut State) {
    // Every comp is in this one list (a band over its layers, VDMX-style), not behind tabs.
    crate::ui::toolbar(ui, |ui| {
        if crate::ui::command(ui, label(icon::PLUS, "Add Layer")).clicked() {
            st.add_layer();
        }
        if crate::ui::command(ui, label(icon::FRAME_CORNERS, "Comp")).on_hover_text("A group of layers with a canvas of its own: its own size, mixed into the master or sent to outputs of its own").clicked() {
            let n = st.comp.comps.len().max(1) + 1;
            let id = st.comp.add_comp(&format!("Comp {n}"));
            st.selected = Some(id);
        }
        help(ui, "Top of the list is the top of the stack · drag a thumbnail to reorder · right-click for more");
    });
    ui.add_space(2.0);
    let aspect = st.aspect();
    let output_tex = st.texture;
    let any_solo = st.comp.layers.iter().any(|l| l.enabled && l.solo);
    let feeds = feed_choices(&st.comp);
    let others = other_comps(&st.comp);
    let State { comp, selected, also_selected, media, layer_textures, .. } = st;
    // Shift or Cmd adds a layer to the selection (edits then go to all of them).
    let adding = ui.input(|i| i.modifiers.shift || i.modifiers.command);
    let sel_is_layer = selected.is_some_and(|s| comp.layers.iter().any(|l| l.id == s));
    let count = comp.layers.len();
    let mut action = None;
    let mut drop: Option<(u64, u64)> = None;
    let mut media_drop: Option<(usize, usize)> = None;
    let mut palette_drop: Option<(u64, crate::explorer::PaletteDrag)> = None;
    let mut source_drop: Option<(u64, SourceDrag)> = None;
    let mut group_action: Option<(u64, GroupAction)> = None;
    let mut comp_action: Option<(u64, super::stack::CompBandAct)> = None;
    let mut comp_stop: Vec<u64> = Vec::new();
    crate::ui::column_scroll(ui).auto_shrink(false).show(ui, |ui| {
        let (pick, dropped) = master_card(ui, output_tex, aspect, *selected == Some(MASTER_ID), &comp.master_effects, &mut comp.master_fx_bypass);
        if pick {
            *selected = Some(MASTER_ID);
        }
        if let Some(d) = dropped {
            palette_drop = Some((MASTER_ID, d));
        }
        ui.add_space(4.0);
        // Comps: a band each, the later (on top in the master) first. No comps, or one plain one: just the layers.
        let comp_ids: Vec<Option<u64>> = if comp.comps.is_empty() { vec![None] } else { comp.comps.iter().rev().map(|c| Some(c.id)).collect() };
        let many = comp.comps.len() > 1;
        for cid in comp_ids {
        let band = cid.filter(|id| many || comp.comp(*id).is_some_and(|c| !c.is_plain()));
        if let Some(id) = band {
            let members: Vec<u64> = comp.layers.iter().filter(|l| comp.comp_of(l) == Some(id)).map(|l| l.id).collect();
            let Some(c) = comp.comps.iter_mut().find(|c| c.id == id) else { continue };
            let thumb = layer_textures.get(&id).map(|(t, _)| *t);
            let (pick, act, stop) = super::stack::comp_band(ui, c, thumb, aspect, *selected == Some(id), members.len(), many);
            if stop {
                comp_stop.extend(members);
            }
            if pick {
                *selected = Some(id);
            }
            if let Some(a) = act {
                comp_action = Some((id, a));
            }
            ui.add_space(4.0);
            if comp.comp(id).is_some_and(|c| c.collapsed) {
                continue;
            }
        }
        for idx in (0..count).rev() {
            if comp.comp_of(&comp.layers[idx]) != cid {
                continue;
            }
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
            let is_selected = *selected == Some(layer.id) || also_selected.contains(&layer.id);
            let muted = any_solo && !layer.solo;
            let out = if parent.is_some() {
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    ui.vertical(|ui| layer_card(ui, layer, thumb, aspect, is_selected, muted, err.as_deref(), &feeds, &others)).inner
                })
                .inner
            } else {
                layer_card(ui, layer, thumb, aspect, is_selected, muted, err.as_deref(), &feeds, &others)
            };
            if out.select {
                if adding && sel_is_layer && *selected != Some(layer.id) {
                    if let Some(k) = also_selected.iter().position(|id| *id == layer.id) {
                        also_selected.remove(k);
                    } else {
                        also_selected.push(layer.id);
                    }
                } else {
                    also_selected.clear();
                    *selected = Some(layer.id);
                }
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
            if let Some(d) = out.dropped_source {
                source_drop = Some((layer.id, d));
            }
            ui.add_space(4.0);
        }
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
    if let Some((id, a)) = comp_action {
        super::stack::apply_comp_band_act(st, id, a);
    }
    if !comp_stop.is_empty() {
        let now = ui.ctx().input(|i| i.time);
        super::stack::stop_layers(st, &comp_stop, now);
    }
    if let Some((target, d)) = palette_drop {
        crate::explorer::apply_palette(st, d, Some(target));
    }
    if let Some((layer, item)) = media_launch {
        let now = ui.ctx().input(|i| i.time);
        st.launch_source(layer, crate::media::spec_for_path(&item.path), Vec::new(), item.name, now);
    }
    if let Some((layer, d)) = source_drop {
        let now = ui.ctx().input(|i| i.time);
        st.launch_source(layer, d.source, Vec::new(), d.name, now);
    }
}

pub fn duplicate_layer(st: &mut State, id: u64) {
    let Some(i) = st.comp.layers.iter().position(|l| l.id == id) else { return };
    log::info!(target: "phosphor::action", "layer duplicated: {}", st.comp.layers[i].name);
    let mut copy = st.comp.adopt_layer(st.comp.layers[i].clone());
    copy.name = format!("{} copy", copy.name);
    st.selected = Some(copy.id);
    st.comp.layers.insert(i + 1, copy);
}

pub fn remove_layer(st: &mut State, id: u64) {
    let Some(i) = st.comp.layers.iter().position(|l| l.id == id) else { return };
    log::info!(target: "phosphor::action", "layer removed: {}", st.comp.layers[i].name);
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
    /// A live input (or any ready-made source) dropped onto this card.
    dropped_source: Option<SourceDrag>,
}

/// What's changed on a layer as (badge, what it means): the clip's effects, speed / direction, trim, moved,
/// animated. The layer's own effects are the "N fx" toggle.
pub(crate) fn mark_list(m: &phosphor_engine::LayerMarks) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    if m.clip_fx > 0 {
        out.push((format!("{} {}", icon::MAGIC_WAND, m.clip_fx), "Effects that came with the clip or file playing (Inspector › Clip effects)"));
    }
    if m.playback {
        out.push((icon::SLIDERS_HORIZONTAL.to_string(), "Speed, direction or BPM sync changed"));
    }
    if m.cut {
        out.push((icon::SCISSORS.to_string(), "Trimmed: In / Out points or a beat loop"));
    }
    if m.moved {
        out.push((icon::ARROWS_OUT_CARDINAL.to_string(), "Moved, scaled, rotated, cropped or pinned (Inspector › Transform)"));
    }
    if m.animated > 0 {
        out.push((format!("{} {}", icon::WAVE_SINE, m.animated), "Values moving by themselves (LFO, sound, beat…)"));
    }
    out
}

/// The badges in a row (hover each for what it means).
pub(crate) fn layer_marks(ui: &mut egui::Ui, m: &phosphor_engine::LayerMarks) {
    for (text, tip) in mark_list(m) {
        ui.label(RichText::new(text).small().color(accent())).on_hover_text(tip);
    }
}

/// Badges painted as one chip in a picture's bottom-right corner (media tiles, clip rows); returns the words for its tooltip.
pub(crate) fn badge_chip(p: &egui::Painter, pic: egui::Rect, badges: &[(String, impl AsRef<str>)]) -> String {
    if badges.is_empty() {
        return String::new();
    }
    let text = badges.iter().map(|(b, _)| b.as_str()).collect::<Vec<_>>().join(" ");
    let g = p.layout_no_wrap(text, egui::FontId::proportional(12.0), Color32::BLACK);
    let at = pic.right_bottom() - egui::vec2(g.size().x + 10.0, g.size().y + 6.0);
    p.rect_filled(egui::Rect::from_min_size(at, g.size() + egui::vec2(6.0, 2.0)), 3.0, accent());
    p.galley(at + egui::vec2(3.0, 1.0), g, Color32::BLACK);
    badges.iter().map(|(b, w)| format!("\n{b} {}", w.as_ref())).collect()
}

/// Drag payload from the media bin.
#[derive(Clone, Copy)]
pub struct MediaDrag(pub usize);

/// Drag payload: a ready-made source (a live input from the Inputs panel) for a layer or a clip cell.
#[derive(Clone)]
pub struct SourceDrag {
    pub name: String,
    pub source: SourceSpec,
}

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
                if crate::ui::pick_item(ui, is_selected, RichText::new("MASTER").strong().size(12.5).color(accent())).clicked() {
                    select = true;
                }
                help(ui, "Effects here run over all layers");
                if let Some(r) = super::groups::fx_badge(ui, effects, bypass) {
                    map_menu(ui, &r.on_hover_text("Click to bypass the master effects · right-click to map"), ControlTarget::App(phosphor_engine::AppControl::MasterFxBypass));
                }
            });
        });
    });
    remember_card(ui, card_id, frame.response.rect);
    if bg.is_some_and(|b| b.clicked()) {
        select = true;
    }
    // Only effects make sense on MASTER.
    let dropped = crate::ui::drop_payload::<crate::explorer::PaletteDrag>(&frame.response).map(|p| *p).filter(|d| d.effect.is_some());
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
    feeds: &[(u64, String, bool)],
    other_comps: &[(u64, String)],
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
                    let on = toggle(ui, &mut layer.enabled, "ON").on_hover_text("Layer on/off (Space) · right-click to Learn");
                    mapped_badge(ui, &on, &ControlTarget::LayerEnabled(layer.id));
                    on.right_menu(|ui| learn_items(ui, &ControlTarget::LayerEnabled(layer.id)));
                    solo_button(ui, &mut layer.solo);
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
                        if crate::ui::small_command(ui, icon::TRASH).on_hover_text("Delete layer").clicked() {
                            action = Some(LayerAction::Remove);
                        }
                        if crate::ui::small_command(ui, icon::COPY).on_hover_text("Duplicate layer").clicked() {
                            action = Some(LayerAction::Duplicate);
                        }
                        if arrows && crate::ui::small_command(ui, icon::ARROW_DOWN).on_hover_text("Move down the stack").clicked() {
                            action = Some(LayerAction::Down);
                        }
                        if arrows && crate::ui::small_command(ui, icon::ARROW_UP).on_hover_text("Move up the stack").clicked() {
                            action = Some(LayerAction::Up);
                        }
                    });
                });
                if let Some(a) = source_picker(ui, layer, feeds) {
                    action = Some(a);
                }
                opacity_row(ui, layer);
                if matches!(layer.source, SourceSpec::Movie { .. }) {
                    let w = ui.available_width();
                    super::clip_bar::card_bar(ui, layer, w);
                    super::clip_bar::speed_level(ui, layer, w);
                }
                crate::ui::toolbar(ui, |ui| {
                    // A mixer plugin blends it instead of its blend mode (Inspector › Mixer plugin).
                    match layer.mixer.as_deref().and_then(shaders::mixer) {
                        Some(d) => {
                            hint(ui, format!("{} {}", icon::INTERSECT, d.name)).on_hover_text("Blended by this FFGL mixer (Inspector › Mixer plugin)");
                        }
                        None => {
                            choice_combo(ui, ("blend", layer.id), &mut layer.blend, 104.0, false);
                        }
                    }
                    choice_combo(ui, ("fit", layer.id), &mut layer.fit, 74.0, false);
                    let marks = layer.marks();
                    if let Some(r) = super::groups::fx_count_badge(ui, marks.clip_fx + marks.fx, &mut layer.fx_bypass) {
                        map_menu(ui, &r, ControlTarget::LayerFxBypass(layer.id));
                    }
                    layer_marks(ui, &marks);
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
        if crate::ui::command(ui, label(icon::SPARKLE, "Browse sources…")).clicked() {
            action = Some(LayerAction::BrowseSources);
            ui.close();
        }
        if crate::ui::command(ui, label(icon::MAGIC_WAND, "Add effect…")).clicked() {
            action = Some(LayerAction::BrowseEffects);
            ui.close();
        }
        if crate::ui::command(ui, label(icon::MONITOR, "Preview in a window")).clicked() {
            action = Some(LayerAction::Monitor);
            ui.close();
        }
        #[cfg(not(target_arch = "wasm32"))]
        if crate::ui::command(ui, label(icon::CAMERA, "Save a picture…")).on_hover_text("This layer's picture now, at the canvas size, as a PNG").clicked() {
            super::request_snapshot(ui, layer.id);
            ui.close();
        }
        ui.separator();
        learn_items(ui, &ControlTarget::LayerEnabled(layer.id));
        if crate::ui::command(ui, label(icon::BROADCAST, "Learn opacity")).clicked() {
            request(ui, LEARN_REQUEST, ControlTarget::LayerOpacity(layer.id));
            ui.close();
        }
        ui.separator();
        if crate::ui::command(ui, label(icon::COPY, "Duplicate (⌘D)")).clicked() {
            action = Some(LayerAction::Duplicate);
            ui.close();
        }
        if crate::ui::command(ui, label(icon::ARROW_UP, "Move up")).clicked() {
            action = Some(LayerAction::Up);
            ui.close();
        }
        if crate::ui::command(ui, label(icon::ARROW_DOWN, "Move down")).clicked() {
            action = Some(LayerAction::Down);
            ui.close();
        }
        if crate::ui::command(ui, "Clear source").clicked() {
            action = Some(LayerAction::ClearSource);
            ui.close();
        }
        if let Some(c) = move_to_comp_menu(ui, other_comps) {
            action = Some(LayerAction::MoveToComp(c));
        }
        ui.separator();
        if layer.parent.is_some() {
            if crate::ui::command(ui, label(icon::SELECTION_SLASH, "Take out of the group")).clicked() {
                action = Some(LayerAction::LeaveGroup);
                ui.close();
            }
        } else if crate::ui::command(ui, label(icon::STACK, "Group with the layer below (⌘G)")).on_hover_text("Mix the two together first, then treat them like one layer (opacity, blend, effects)").clicked() {
            action = Some(LayerAction::GroupWithBelow);
            ui.close();
        }
        #[cfg(not(target_arch = "wasm32"))]
        if crate::ui::command(ui, label(icon::PALETTE, "Match colour to a picture…")).on_hover_text("Adds a Colour Match effect so this layer's colours look like a picture you choose (another camera, a grade, a still)").clicked() {
            action = Some(LayerAction::MatchColour);
            ui.close();
        }
        if crate::ui::command(ui, label(icon::SWATCHES, "Palette from this picture")).on_hover_text("Adds a Palette effect: every pixel becomes one of this layer's six main colours (change them in the effect)").clicked() {
            action = Some(LayerAction::Palette);
            ui.close();
        }
        ui.separator();
        if crate::ui::command(ui, label(icon::TRASH, "Delete (Backspace)")).clicked() {
            action = Some(LayerAction::Remove);
            ui.close();
        }
    });
    // Drop target for a dragged layer or a media bin item.
    let dropped = crate::ui::drop_payload::<u64>(&frame.response).map(|p| *p);
    let dropped_media = crate::ui::drop_payload::<MediaDrag>(&frame.response).map(|p| p.0);
    let dropped_palette = crate::ui::drop_payload::<crate::explorer::PaletteDrag>(&frame.response).map(|p| *p);
    let dropped_source = crate::ui::drop_payload::<SourceDrag>(&frame.response).map(|p| (*p).clone());
    let hovering = frame.response.dnd_hover_payload::<u64>().is_some_and(|p| *p != layer.id)
        || frame.response.dnd_hover_payload::<SourceDrag>().is_some()
        || frame.response.dnd_hover_payload::<MediaDrag>().is_some()
        || frame.response.dnd_hover_payload::<crate::explorer::PaletteDrag>().is_some();
    if hovering {
        ui.painter().rect_stroke(frame.response.rect, 6.0, Stroke::new(2.0, accent()), StrokeKind::Outside);
    }
    // What the layer plays: a stripe down the card's left edge in that kind's colour (as on its clips).
    if let Some(h) = crate::theme::Hue::of_source(&layer.source) {
        let r = frame.response.rect;
        let stripe = egui::Rect::from_min_max(r.left_top() + egui::vec2(1.0, 8.0), egui::pos2(r.left() + 4.0, r.bottom() - 8.0));
        ui.painter().rect_filled(stripe, 1.5, h.color());
    }
    CardOut { select, action, dropped, dropped_media, dropped_palette, dropped_source }
}

/// Source menu for a layer card; returns a request for the browser or the Open dialog.
/// What a layer can take another picture from: the program (feedback), groups and layers.
/// What a layer can show another's picture from: (id, name, is a layer (has a "before its effects" picture too)).
pub(crate) fn feed_choices(comp: &phosphor_engine::Composition) -> Vec<(u64, String, bool)> {
    std::iter::once((MASTER_ID, "Program (feedback)".to_string(), false))
        .chain(comp.comps.iter().filter(|_| comp.comps.len() > 1).map(|c| (c.id, format!("Comp: {}", c.name), false)))
        .chain(comp.groups.iter().map(|g| (g.id, g.name.clone(), false)))
        .chain(comp.layers.iter().rev().map(|l| (l.id, l.name.clone(), true)))
        .collect()
}

pub(crate) fn source_picker(ui: &mut egui::Ui, layer: &mut LayerState, feeds: &[(u64, String, bool)]) -> Option<LayerAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        if crate::ui::command(ui, icon::SPARKLE).on_hover_text("Browse sources with previews").clicked() {
            action = Some(LayerAction::BrowseSources);
        }
        // Leave room for the Open and Live icon buttons so the row never runs off the card.
        let width = (ui.available_width() - 2.0 * (ui.spacing().interact_size.y + 2.0 * ui.spacing().item_spacing.x)).clamp(60.0, 180.0);
        ComboBox::from_id_salt(("source", layer.id)).width(width).selected_text(layer.source.label()).show_ui(ui, |ui| {
            // Search: every generator (built in and imported) by name; typing goes straight here.
            let id = ui.id().with(("source_search", layer.id));
            let mut q = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_default();
            let r = crate::ui::text_field(ui, &mut q, "Search generators…", Some(220.0));
            if ui.memory(|m| m.focused().is_none()) {
                r.request_focus();
            }
            ui.data_mut(|d| d.insert_temp(id, q.clone()));
            let q = q.trim().to_lowercase();
            if !q.is_empty() {
                let found: Vec<_> = shaders::all_generators().into_iter().filter(|g| g.name.to_lowercase().contains(&q) || g.id.to_lowercase().contains(&q)).collect();
                if found.is_empty() {
                    hint(ui, "No generator by that name");
                }
                crate::ui::column_scroll(ui).max_height(320.0).show(ui, |ui| {
                    for g in found {
                        let active = matches!(&layer.source, SourceSpec::Generator(id) if id == g.id);
                        if crate::ui::pick_item(ui, active, g.name).clicked() {
                            layer.set_generator(g.id);
                        }
                    }
                });
                return;
            }
            if crate::ui::pick_item(ui, layer.source == SourceSpec::None, "No source").clicked() {
                layer.source = SourceSpec::None;
            }
            if crate::ui::pick_item(ui, matches!(layer.source, SourceSpec::Text(_)), label(icon::TEXT_T, "Text")).on_hover_text("Words on the layer: type them in the Inspector").clicked() && !matches!(layer.source, SourceSpec::Text(_)) {
                layer.source = SourceSpec::Text(crate::text::default_style());
            }
            ui.menu_button(label(icon::ARROW_BEND_DOWN_RIGHT, "From another layer"), |ui| {
                help(ui, "Use another layer's finished picture (with its effects), or the program for video feedback.");
                for (id, name, _) in feeds.iter().filter(|(id, ..)| *id != layer.id) {
                    let active = matches!(&layer.source, SourceSpec::Feed { id: cur, before_fx: false, .. } if cur == id);
                    if crate::ui::pick_item(ui, active, name).clicked() {
                        layer.source = SourceSpec::Feed { id: *id, name: name.clone(), before_fx: false };
                        ui.close();
                    }
                }
                // Layers only (groups and the program have no "before"): the picture before its effects.
                let plain: Vec<&(u64, String, bool)> = feeds.iter().filter(|(id, _, is_layer)| *id != layer.id && *is_layer).collect();
                if !plain.is_empty() {
                    ui.separator();
                    hint(ui, "Before its effects:");
                    for (id, name, _) in plain {
                        let active = matches!(&layer.source, SourceSpec::Feed { id: cur, before_fx: true, .. } if cur == id);
                        if crate::ui::pick_item(ui, active, format!("{name} (before effects)")).clicked() {
                            layer.source = SourceSpec::Feed { id: *id, name: format!("{name} (before effects)"), before_fx: true };
                            ui.close();
                        }
                    }
                }
                // A layer feeding on itself = trails / feedback of its own previous frame.
                let me = matches!(&layer.source, SourceSpec::Feed { id, .. } if *id == layer.id);
                if crate::ui::pick_item(ui, me, "Itself (previous frame)").clicked() {
                    layer.source = SourceSpec::Feed { id: layer.id, name: "itself".into(), before_fx: false };
                    ui.close();
                }
            });
            #[cfg(not(target_arch = "wasm32"))]
            {
                let model = matches!(&layer.source, SourceSpec::Live { format, .. } if format == crate::models::FORMAT);
                if crate::ui::pick_item(ui, model, label(icon::CUBE, "3D model as points (OBJ)…")).on_hover_text("The model's vertices as points: draw them with a Points: Dots, Lines or Mesh effect (Read: In order)").clicked() {
                    action = Some(LayerAction::OpenModel);
                    ui.close();
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            ui.menu_button(label(icon::WAVEFORM, "G-Drop visualizers"), |ui| {
                help(ui, "Gamp's MilkDrop-style visualizers: they move with the sound Phosphor hears.");
                let now = match &layer.source {
                    SourceSpec::Live { format, url, .. } if format == crate::gdrop::FORMAT => Some(url.clone()),
                    _ => None,
                };
                if crate::ui::pick_item(ui, now.as_deref() == Some(""), "Changing (a new one every 20-40 s)").clicked() {
                    layer.source = crate::gdrop::source(None);
                    ui.close();
                }
                for p in crate::gdrop::presets() {
                    if crate::ui::pick_item(ui, now.as_deref() == Some(p), p).clicked() {
                        layer.source = crate::gdrop::source(Some(p));
                        ui.close();
                    }
                }
            });
            ui.separator();
            for g in shaders::GENERATORS {
                let active = matches!(&layer.source, SourceSpec::Generator(id) if id == g.id);
                if crate::ui::pick_item(ui, active, g.name).clicked() {
                    layer.set_generator(g.id);
                }
            }
        });
        if crate::ui::command(ui, icon::FOLDER_OPEN).on_hover_text("Open an image or movie file (or drag one onto the window)").clicked() {
            action = Some(LayerAction::OpenFile);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(source) = live_menu(ui, layer.id) {
            action = Some(LayerAction::Launch(source));
        }
    });
    action
}

/// A long input name cut to fit a menu (the full name shows on hover).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn short(name: &str) -> String {
    if name.chars().count() > 60 { format!("{}…", name.chars().take(59).collect::<String>()) } else { name.to_string() }
}

/// The icon for a kind of live input.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn input_icon(kind: crate::media::live::InputKind) -> &'static str {
    use crate::media::live::InputKind;
    match kind {
        InputKind::Camera => icon::VIDEO_CAMERA,
        InputKind::Syphon => icon::SHARE_NETWORK,
        InputKind::Screen => icon::MONITOR,
        InputKind::Window => icon::APP_WINDOW,
        InputKind::Ndi => icon::BROADCAST,
    }
}

/// Live inputs for a layer: cameras / capture cards on this Mac, or a stream address.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn live_menu(ui: &mut egui::Ui, layer: u64) -> Option<SourceSpec> {
    let mut picked = None;
    let r = ui.menu_button(icon::VIDEO_CAMERA, |ui| {
        ui.set_min_width(260.0);
        let refresh = ui.ctx().data(|d| d.get_temp::<bool>(egui::Id::new("live_rescan"))).unwrap_or(false);
        let cams = crate::media::live::cameras_cached(ui.ctx(), refresh);
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("live_rescan"), false));
        let (inputs, notes) = crate::media::live::inputs(&cams);
        for kind in crate::media::live::InputKind::ALL {
            let mine: Vec<_> = inputs.iter().filter(|i| i.kind == kind).collect();
            let note = notes.iter().find(|(k, _)| *k == kind);
            if mine.is_empty() && note.is_none() {
                continue;
            }
            if kind != crate::media::live::InputKind::Camera {
                ui.separator();
            }
            ui.label(RichText::new(kind.title()).small().color(Color32::GRAY));
            if let Some((_, n)) = note {
                hint(ui, n.as_str());
            }
            // Many windows: their own submenu.
            if kind == crate::media::live::InputKind::Window && mine.len() > 4 {
                ui.menu_button(label(icon::APP_WINDOW, &format!("A window ({})", mine.len())), |ui| {
                    crate::ui::column_scroll(ui).max_height(360.0).show(ui, |ui| {
                        for i in &mine {
                            if crate::ui::command(ui, short(&i.label)).on_hover_text(&i.label).clicked() {
                                picked = Some(i.source.clone());
                                ui.close();
                            }
                        }
                    });
                });
                continue;
            }
            for i in mine {
                if crate::ui::command(ui, label(input_icon(kind), &short(&i.label))).on_hover_text(&i.label).clicked() {
                    picked = Some(i.source.clone());
                    ui.close();
                }
            }
            if kind == crate::media::live::InputKind::Camera && crate::ui::small_command(ui, label(icon::ARROWS_CLOCKWISE, "Rescan")).clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("live_rescan"), true));
            }
        }
        ui.separator();
        ui.label(RichText::new("Stream address").small().color(Color32::GRAY));
        let url_id = egui::Id::new(("live_url", layer));
        let mut url: String = ui.ctx().data(|d| d.get_temp(url_id)).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut url).hint_text("rtsp://… srt://… https://….m3u8").desired_width(190.0));
            if crate::ui::command_if(ui, url.contains("://"), "Play").clicked() {
                picked = Some(crate::media::live::stream_source(&url));
                ui.close();
            }
        });
        ui.ctx().data_mut(|d| d.insert_temp(url_id, url));
    });
    r.response.on_hover_text("Live input: camera, capture card, a screen or window, Syphon, NDI or a network stream (RTSP, SRT, HLS, RTMP)");
    picked
}

/// Carries out a layer card's action (Layers panel and Layer Windows).
pub(crate) fn apply_layer_action(st: &mut State, id: u64, action: LayerAction, now: f64) {
    match action {
        LayerAction::Up => st.comp.move_layer(id, true),
        LayerAction::Down => st.comp.move_layer(id, false),
        LayerAction::GroupWithBelow => group_with_below(&mut st.comp, &mut st.selected, id),
        LayerAction::Palette => {
            // The layer's small thumbnail now (a one-off readback).
            let px = st.engine.as_ref().and_then(|e| e.read_layer_thumbnail(id));
            st.notice = Some(match px.map(|(_, _, px)| phosphor_engine::palette_of(&px, 6)) {
                Some(pal) if !pal.is_empty() && st.comp.add_palette(id, &pal).is_some() => "Palette added with this layer's six main colours".into(),
                _ => "The layer has no picture to take colours from (is it on?)".into(),
            });
        }
        LayerAction::MatchColour => {
            #[cfg(not(target_arch = "wasm32"))]
            st.dialogs.pick_file(crate::dialogs::Purpose::MatchColour(id), Some(("Picture", &["png", "jpg", "jpeg", "webp", "tif", "tiff", "bmp", "heic", "psd", "exr"])));
        }
        LayerAction::LeaveGroup => {
            if let Some(l) = st.comp.layer_mut(id) {
                l.parent = None;
            }
            st.comp.normalize_groups();
        }
        LayerAction::Duplicate => duplicate_layer(st, id),
        LayerAction::Remove => remove_layer(st, id),
        LayerAction::MoveToComp(c) => st.comp.move_to_comp(id, c),
        LayerAction::BrowseSources => st.fx_browser = Some(crate::explorer::FxBrowser::new(id, true, None)),
        LayerAction::OpenFile => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let exts = crate::media::dialog_extensions();
                st.dialogs.pick_file(crate::dialogs::Purpose::LayerSource(id), Some(("Media", &exts)));
            }
        }
        LayerAction::OpenModel => {
            #[cfg(not(target_arch = "wasm32"))]
            st.dialogs.pick_file(crate::dialogs::Purpose::ModelPoints(id), Some(("3D model (OBJ)", &["obj"])));
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
    let r = crate::ui::eye_toggle(ui, &mut layer.enabled);
    mapped_badge(ui, &r, &ControlTarget::LayerEnabled(layer.id));
    r.right_menu(|ui| learn_items(ui, &ControlTarget::LayerEnabled(layer.id)));
}

/// Solo: only soloed layers reach the mix (an orange S).
pub(crate) fn solo_button(ui: &mut egui::Ui, solo: &mut bool) -> egui::Response {
    let color = if *solo { WARN } else { Color32::GRAY };
    crate::ui::icon_toggle(ui, solo, RichText::new("S").small().strong().color(color)).on_hover_text("Solo: only soloed layers reach the mix")
}
