//! The Stack: every layer with its controls and its clips side by side, in one list (VDMX's organisation). Groups are
//! foldable bands around their layers with their own on/off, opacity, blend and effects; a comp is a group with its
//! own size (and outputs), shown the same way. Nothing hides behind tabs. The tempo bar, column headers and clip
//! cells are the Clips panel's components (`clip_grid`); the old Clips and Layers panels still exist.

use super::clip_grid::*;
use super::groups::GroupAction;
use super::layers::LayerAction;
use super::*;
use phosphor_engine::{Comp, Composition, LayerGroup};

/// The layer strip before each clip row: a share of the panel, within these.
pub(crate) const STRIP_W: f32 = 470.0;
const STRIP_MIN_W: f32 = 380.0;

/// The strip width for this panel width (narrow panels keep room for clip cells).
fn strip_width(panel_w: f32) -> f32 {
    (panel_w * 0.42).clamp(STRIP_MIN_W, STRIP_W)
}

/// What the strips and bands asked for this frame (applied after the borrows end).
#[derive(Default)]
struct StackActs {
    layer: Option<(u64, LayerAction)>,
    group: Option<(u64, GroupAction)>,
    comp: Option<(u64, CompBandAct)>,
    /// Drag-reorder: (dragged layer, dropped onto this layer).
    drop: Option<(u64, u64)>,
    /// Stop every layer in this list (the launch fade applies).
    stop: Vec<u64>,
    launch: Option<(u64, SourceDrag)>,
    media_launch: Option<(u64, usize)>,
    palette: Option<(u64, crate::explorer::PaletteDrag)>,
}

pub(crate) enum CompBandAct {
    Rename(String),
    Earlier,
    Later,
    Delete,
    BrowseEffects,
}

pub(crate) fn stack(ui: &mut egui::Ui, st: &mut State) {
    let now = ui.ctx().input(|i| i.time);
    transport_bar(ui, st);
    stack_toolbar(ui, st, now);
    ui.add_space(2.0);
    let (grid, mut out) = Grid::new(ui, st);
    let aspect = grid.aspect;
    let strip_w = strip_width(ui.available_width());
    let feeds = super::layers::feed_choices(&st.comp);
    let others = other_comps(&st.comp);
    let any_solo = st.comp.layers.iter().any(|l| l.enabled && l.solo);
    let mut acts = StackActs::default();
    let adding = ui.input(|i| i.modifiers.shift || i.modifiers.command);
    {
        let State { comp, thumbs, clip_cells, preview_wants, preview_keys, preview_textures, layer_textures, selected, also_selected, also_clips, media, .. } = st;
        let ctx = ui.ctx().clone();
        let mut cx = CellCtx { thumbs, clip_cells, preview_wants, preview_keys, preview_textures, also_clips, ctx };
        let sel_is_layer = selected.is_some_and(|s| comp.layers.iter().any(|l| l.id == s));
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            column_headers(ui, &grid, strip_w, &mut out);
            // Comps are bands, the later (on top in the master) first. No comps, or one plain one: just the layers.
            let comp_ids: Vec<Option<u64>> = if comp.comps.is_empty() { vec![None] } else { comp.comps.iter().rev().map(|c| Some(c.id)).collect() };
            let many = comp.comps.len() > 1;
            for cid in comp_ids {
                let band = cid.filter(|id| many || comp.comp(*id).is_some_and(|c| !c.is_plain()));
                let mut folded = false;
                if let Some(id) = band {
                    let members: Vec<u64> = comp.layers.iter().filter(|l| comp.comp_of(l) == Some(id)).map(|l| l.id).collect();
                    let Some(c) = comp.comps.iter_mut().find(|c| c.id == id) else { continue };
                    let thumb = layer_textures.get(&id).map(|(t, _)| *t);
                    let (select, act, stop) = comp_band(ui, c, thumb, aspect, *selected == Some(id), members.len(), many);
                    if select {
                        *selected = Some(id);
                    }
                    if let Some(a) = act {
                        acts.comp = Some((id, a));
                    }
                    if stop {
                        acts.stop.extend(members);
                    }
                    folded = c.collapsed;
                }
                if folded {
                    continue;
                }
                let n = comp.layers.len();
                for idx in (0..n).rev() {
                    if comp.comp_of(&comp.layers[idx]) != cid {
                        continue;
                    }
                    // A group's band goes above its top member; folded groups hide their members.
                    let parent = comp.layers[idx].parent;
                    if let Some(gid) = parent {
                        let top = comp.layers.get(idx + 1).is_none_or(|l| l.parent != Some(gid));
                        if top && let Some(g) = comp.groups.iter_mut().find(|g| g.id == gid) {
                            let thumb = layer_textures.get(&gid).map(|(t, _)| *t);
                            let members: Vec<u64> = comp.layers.iter().filter(|l| l.parent == Some(gid)).map(|l| l.id).collect();
                            let (select, act, stop, dropped) = group_band(ui, g, thumb, aspect, *selected == Some(gid), members.len(), band.is_some());
                            if select {
                                *selected = Some(gid);
                            }
                            if let Some(a) = act {
                                acts.group = Some((gid, a));
                            }
                            if stop {
                                acts.stop.extend(members);
                            }
                            if let Some(from) = dropped
                                && let Some(top_member) = comp.layers.get(idx)
                            {
                                acts.drop = Some((from, top_member.id));
                            }
                        }
                        if comp.group(gid).is_some_and(|g| g.collapsed) {
                            continue;
                        }
                    }
                    let layer = &mut comp.layers[idx];
                    let thumb = layer_textures.get(&layer.id).map(|(t, _)| *t);
                    let err = media.errors.get(&layer.id).cloned();
                    let is_selected = *selected == Some(layer.id) || also_selected.contains(&layer.id);
                    let dimmed = any_solo && !layer.solo;
                    let indent = 10.0 * (band.is_some() as u8 + parent.is_some() as u8) as f32;
                    ui.horizontal(|ui| {
                        let strip = Strip { thumb, aspect, is_selected, dimmed, err: err.as_deref(), feeds: &feeds, others: &others, indent, size: egui::vec2(strip_w, grid.cell.y) };
                        let o = layer_strip(ui, layer, &strip, &mut out);
                        if o.select {
                            if adding && sel_is_layer && *selected != Some(layer.id) {
                                match also_selected.iter().position(|id| *id == layer.id) {
                                    Some(k) => {
                                        also_selected.remove(k);
                                    }
                                    None => also_selected.push(layer.id),
                                }
                            } else {
                                also_selected.clear();
                                *selected = Some(layer.id);
                            }
                        }
                        if let Some(a) = o.action {
                            acts.layer = Some((layer.id, a));
                        }
                        if o.stop {
                            acts.stop.push(layer.id);
                        }
                        if let Some(from) = o.dropped {
                            acts.drop = Some((from, layer.id));
                        }
                        if let Some(m) = o.dropped_media {
                            acts.media_launch = Some((layer.id, m));
                        }
                        if let Some(d) = o.dropped_source {
                            acts.launch = Some((layer.id, d));
                        }
                        if let Some(d) = o.dropped_palette {
                            acts.palette = Some((layer.id, d));
                        }
                        super::clip_grid::clip_cells(ui, &grid, &mut cx, layer, idx, &mut out);
                    });
                }
            }
            if comp.layers.is_empty() {
                help(ui, "No layers yet: + Layer above (or drop files onto the window).");
            }
            ui.add_space(6.0);
            master_strip(ui, comp, *selected == Some(MASTER_ID), &mut acts, selected);
        });
    }
    apply_row_out(st, out, now);
    apply_stack_acts(st, acts, now);
}

/// + Layer (one, several, from a folder of clips), + Group, Stop all, Mute all.
fn stack_toolbar(ui: &mut egui::Ui, st: &mut State, now: f64) {
    let mut add = 0usize;
    let mut group_selected = false;
    let mut own_size = false;
    let mut from_folder = false;
    let mut stop_all = false;
    crate::ui::toolbar(ui, |ui| {
        ui.menu_button(label(icon::PLUS, "Layer"), |ui| {
            ui.set_min_width(260.0);
            if crate::ui::command(ui, label(icon::PLUS, "One layer (⌘ Shift N)")).clicked() {
                add = 1;
                ui.close();
            }
            for n in [2usize, 4, 8] {
                if crate::ui::command(ui, format!("Add {n} layers")).clicked() {
                    add = n;
                    ui.close();
                }
            }
            ui.separator();
            #[cfg(not(target_arch = "wasm32"))]
            if crate::ui::command(ui, label(icon::FOLDER_OPEN, "Layers from a folder of clips…")).on_hover_text("One layer per subfolder, its clips in the row (files at the top level make one more)").clicked() {
                from_folder = true;
                ui.close();
            }
            ui.separator();
            let sel = selected_layers(st);
            if crate::ui::command_if(ui, !sel.is_empty(), label(icon::STACK, "Group the selected layers (⌘G)")).on_hover_text("Mix them together first, then treat them like one layer: on/off, opacity, blend, effects").clicked() {
                group_selected = true;
                ui.close();
            }
            if crate::ui::command(ui, label(icon::FRAME_CORNERS, "Group with its own size (a comp)")).on_hover_text("A group that is its own canvas: its own size, mixed into the master or sent to outputs of its own").clicked() {
                own_size = true;
                ui.close();
            }
        })
        .response
        .on_hover_text("Add layers, or a group");
        if crate::ui::command(ui, icon::STOP).on_hover_text("Stop every layer (what's playing clears, with the launch fade)").clicked() {
            stop_all = true;
        }
        let mut muted = st.comp.sound_muted;
        if crate::ui::icon_toggle(ui, &mut muted, icon::SPEAKER_SIMPLE_X).on_hover_text("Mute all sound").changed() {
            st.comp.sound_muted = muted;
        }
        help(ui, "Top of the list is the top of the stack · drag a picture to reorder · right-click a name for more");
    });
    for _ in 0..add {
        st.add_layer();
    }
    if group_selected {
        let sel = selected_layers(st);
        if let Some(gid) = st.comp.create_group(&sel) {
            st.selected = Some(gid);
            st.also_selected.clear();
        }
    }
    if own_size {
        let n = st.comp.comps.len().max(1) + 1;
        let id = st.comp.add_comp(&format!("Comp {n}"));
        st.selected = Some(id);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if from_folder {
        st.dialogs.pick_folder(crate::dialogs::Purpose::LayersFromFolder);
    }
    #[cfg(target_arch = "wasm32")]
    let _ = from_folder;
    if stop_all {
        let ids: Vec<u64> = st.comp.layers.iter().map(|l| l.id).collect();
        stop_layers(st, &ids, now);
    }
}

/// The selected layer and the ones added with Shift / Cmd (layers only).
fn selected_layers(st: &State) -> Vec<u64> {
    let mut v: Vec<u64> = st.selected.into_iter().chain(st.also_selected.iter().copied()).filter(|id| st.comp.layers.iter().any(|l| l.id == *id)).collect();
    v.dedup();
    v
}

/// Clears what these layers play (with the launch fade, if Fade is on).
pub(crate) fn stop_layers(st: &mut State, ids: &[u64], now: f64) {
    let secs = if st.comp.launch.fade { st.comp.launch.fade_secs } else { 0.0 };
    for id in ids {
        if let Some(l) = st.comp.layer_mut(*id) {
            l.stop(now as f32, secs);
        }
    }
}

/// Everything a strip needs besides the layer.
struct Strip<'a> {
    thumb: Option<egui::TextureId>,
    aspect: f32,
    is_selected: bool,
    /// Another layer is soloed.
    dimmed: bool,
    err: Option<&'a str>,
    feeds: &'a [(u64, String, bool)],
    others: &'a [(u64, String)],
    indent: f32,
    size: egui::Vec2,
}

#[derive(Default)]
struct StripOut {
    select: bool,
    action: Option<LayerAction>,
    stop: bool,
    dropped: Option<u64>,
    dropped_media: Option<usize>,
    dropped_source: Option<SourceDrag>,
    dropped_palette: Option<crate::explorer::PaletteDrag>,
}

/// One layer's strip: picture (select, drag to reorder), eye, solo, mute, stop, name (menu), source; opacity, blend,
/// fx. The same height as its clip cells.
fn layer_strip(ui: &mut egui::Ui, layer: &mut LayerState, s: &Strip, out: &mut RowOut) -> StripOut {
    let mut o = StripOut::default();
    let frame = egui::Frame::new().inner_margin(egui::Margin { left: s.indent as i8 + 2, right: 4, top: 2, bottom: 2 }).corner_radius(4.0).fill(if s.is_selected { crate::theme::tint(ui.visuals().panel_fill, 0.08) } else { Color32::TRANSPARENT });
    let resp = frame
        .show(ui, |ui| {
            // The strip's outer width is exactly STRIP_W (margins included), so the cells line up with the column headers.
            let inner = s.size.x - s.indent - 6.0;
            ui.set_min_size(egui::vec2(inner, s.size.y - 4.0));
            ui.set_max_width(inner);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 3.0);
                let pic_h = s.size.y - 6.0;
                let pic = super::preview::thumbnail(ui, s.thumb, egui::vec2(pic_h * s.aspect, pic_h), layer.enabled && !s.dimmed).on_hover_text("Click to select (Inspector) · double-click: on/off · drag to reorder");
                pic.dnd_set_drag_payload(layer.id);
                if pic.clicked() {
                    o.select = true;
                }
                if pic.double_clicked() {
                    layer.enabled = !layer.enabled;
                }
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        super::layers::show_button(ui, layer);
                        super::layers::solo_button(ui, &mut layer.solo);
                        let has_sound = matches!(layer.source, SourceSpec::Movie { .. });
                        if has_sound {
                            super::inspector::mute_button(ui, &mut layer.muted, "this layer");
                        }
                        if crate::ui::small_command(ui, icon::STOP).on_hover_text("Stop: clear what this layer plays (launch fade)").clicked() {
                            o.stop = true;
                        }
                        let name: String = layer.name.chars().take(18).collect();
                        let name = RichText::new(name).strong();
                        let name = if s.dimmed { name.color(Color32::GRAY) } else if s.is_selected { name.color(accent()) } else { name };
                        let (mut fill, mut beat_all, mut row_midi) = (false, false, None);
                        let r = ui.menu_button(name, |ui| {
                            super::clips::row_menu(ui, layer, &mut fill, &mut beat_all, &mut row_midi);
                            ui.separator();
                            layer_menu(ui, layer, s.others, &mut o);
                        });
                        r.response.on_hover_text(format!("{} · click: fill, the beat, MIDI, autopilot, duplicate, group, delete…", layer.name));
                        if fill {
                            out.fill_row = Some(layer.id);
                        }
                        if beat_all {
                            out.beat_row = Some(layer.id);
                        }
                        if let Some(m) = row_midi {
                            out.row_midi = Some((layer.id, m));
                        }
                        // The source, at the row's end (the strip never grows past its width: the cells stay under
                        // their column headers).
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.set_max_width(ui.available_width());
                            if let Some(a) = super::layers::source_picker(ui, layer, s.feeds) {
                                o.action = Some(a);
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        // Opacity takes what the blend menu and the fx badge leave.
                        let w = (ui.available_width() - 62.0 - 44.0 - 12.0).max(60.0);
                        Level::percent(&mut layer.opacity, "Opacity").modulation(&mut layer.opacity_mod).target(ControlTarget::LayerOpacity(layer.id)).width(w).show(ui);
                        match layer.mixer.as_deref().and_then(shaders::mixer) {
                            Some(d) => {
                                hint(ui, format!("{} {}", icon::INTERSECT, d.name)).on_hover_text("Blended by this FFGL mixer (Inspector › Mixer plugin)");
                            }
                            None => {
                                choice_combo(ui, ("stack_blend", layer.id), &mut layer.blend, 62.0, true);
                            }
                        }
                        let marks = layer.marks();
                        if let Some(r) = super::groups::fx_count_badge(ui, marks.clip_fx + marks.fx, &mut layer.fx_bypass) {
                            map_menu(ui, &r, ControlTarget::LayerFxBypass(layer.id));
                        }
                        super::layers::layer_marks(ui, &marks);
                    });
                    if let Some(e) = s.err {
                        ui.label(RichText::new(e).small().color(WARN));
                    }
                });
            });
        })
        .response;
    // Drop targets: a dragged layer (reorder), a media bin item or a live input (play it), a palette (effect).
    o.dropped = crate::ui::drop_payload::<u64>(&resp).map(|p| *p);
    o.dropped_media = crate::ui::drop_payload::<MediaDrag>(&resp).map(|p| p.0);
    o.dropped_source = crate::ui::drop_payload::<SourceDrag>(&resp).map(|p| (*p).clone());
    o.dropped_palette = crate::ui::drop_payload::<crate::explorer::PaletteDrag>(&resp).map(|p| *p);
    let hovering = resp.dnd_hover_payload::<u64>().is_some_and(|p| *p != layer.id) || resp.dnd_hover_payload::<SourceDrag>().is_some() || resp.dnd_hover_payload::<MediaDrag>().is_some() || resp.dnd_hover_payload::<crate::explorer::PaletteDrag>().is_some();
    if hovering {
        ui.painter().rect_stroke(resp.rect, 4.0, Stroke::new(2.0, accent()), StrokeKind::Outside);
    }
    // What the layer plays: a stripe down the strip's left edge in that kind's colour (as on its clips).
    if let Some(h) = crate::theme::Hue::of_source(&layer.source) {
        let r = resp.rect;
        let stripe = egui::Rect::from_min_max(r.left_top() + egui::vec2(s.indent, 6.0), egui::pos2(r.left() + s.indent + 3.0, r.bottom() - 6.0));
        ui.painter().rect_filled(stripe, 1.5, h.color());
    }
    o
}

/// The rest of a layer's menu (after the row items): sources, effects, duplicate, move, group, delete.
fn layer_menu(ui: &mut egui::Ui, layer: &mut LayerState, others: &[(u64, String)], o: &mut StripOut) {
    let mut picked: Option<LayerAction> = None;
    crate::ui::field_menu(ui, label(icon::PENCIL_SIMPLE, "Rename"), |ui| {
        if let Some(n) = rename_menu(ui, ("stack_rename", layer.id), &layer.name) {
            layer.name = n;
            ui.close();
        }
    });
    let mut act = |ui: &mut egui::Ui, a: LayerAction| {
        picked = Some(a);
        ui.close();
    };
    if crate::ui::command(ui, label(icon::SPARKLE, "Browse sources…")).clicked() {
        act(ui, LayerAction::BrowseSources);
    }
    if crate::ui::command(ui, label(icon::MAGIC_WAND, "Add effect…")).clicked() {
        act(ui, LayerAction::BrowseEffects);
    }
    if crate::ui::command(ui, label(icon::MONITOR, "Preview in a window")).clicked() {
        act(ui, LayerAction::Monitor);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if crate::ui::command(ui, label(icon::CAMERA, "Save a picture…")).clicked() {
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
        act(ui, LayerAction::Duplicate);
    }
    if crate::ui::command(ui, label(icon::ARROW_UP, "Move up")).clicked() {
        act(ui, LayerAction::Up);
    }
    if crate::ui::command(ui, label(icon::ARROW_DOWN, "Move down")).clicked() {
        act(ui, LayerAction::Down);
    }
    if crate::ui::command(ui, "Clear source").clicked() {
        act(ui, LayerAction::ClearSource);
    }
    if let Some(c) = move_to_comp_menu(ui, others) {
        o.action = Some(LayerAction::MoveToComp(c));
    }
    ui.separator();
    if layer.parent.is_some() {
        if crate::ui::command(ui, label(icon::SELECTION_SLASH, "Take out of the group")).clicked() {
            act(ui, LayerAction::LeaveGroup);
        }
    } else if crate::ui::command(ui, label(icon::STACK, "Group with the layer below (⌘G)")).clicked() {
        act(ui, LayerAction::GroupWithBelow);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if crate::ui::command(ui, label(icon::PALETTE, "Match colour to a picture…")).clicked() {
        act(ui, LayerAction::MatchColour);
    }
    if crate::ui::command(ui, label(icon::SWATCHES, "Palette from this picture")).clicked() {
        act(ui, LayerAction::Palette);
    }
    ui.separator();
    if danger_button(ui, label(icon::TRASH, "Delete (Backspace)")).clicked() {
        act(ui, LayerAction::Remove);
    }
    if let Some(a) = picked {
        o.action = Some(a);
    }
}

/// A group's band: fold, on/off, name, stop, opacity, blend, fx, menu. Returns (select, action, stop its layers,
/// a layer dropped on it).
fn group_band(ui: &mut egui::Ui, g: &mut LayerGroup, thumb: Option<egui::TextureId>, aspect: f32, is_selected: bool, members: usize, in_comp: bool) -> (bool, Option<GroupAction>, bool, Option<u64>) {
    let mut select = false;
    let mut action = None;
    let mut stop = false;
    let indent = if in_comp { 10.0 } else { 0.0 };
    let resp = band_frame(ui, is_selected, indent, crate::theme::Hue::Effect.color(), |ui| {
        let caret = if g.collapsed { icon::CARET_RIGHT } else { icon::CARET_DOWN };
        if crate::ui::small_command(ui, caret).on_hover_text(if g.collapsed { "Show the group's layers" } else { "Fold the group's layers away" }).clicked() {
            g.collapsed = !g.collapsed;
        }
        let r = super::preview::thumbnail(ui, thumb, egui::vec2(CONTROL_H * aspect, CONTROL_H), g.enabled).on_hover_text("Click to select · double-click: on/off");
        if r.clicked() {
            select = true;
        }
        if r.double_clicked() {
            g.enabled = !g.enabled;
        }
        let on = toggle(ui, &mut g.enabled, "ON").on_hover_text("Group on/off · right-click to Learn");
        mapped_badge(ui, &on, &ControlTarget::LayerEnabled(g.id));
        on.right_menu(|ui| learn_items(ui, &ControlTarget::LayerEnabled(g.id)));
        let name = RichText::new(format!("{} {}", icon::STACK, g.name)).strong();
        let name = if is_selected { name.color(accent()) } else { name };
        if crate::ui::command(ui, name).on_hover_text(format!("{members} layers mixed together, then treated like one layer · click to select")).clicked() {
            select = true;
        }
        if crate::ui::small_command(ui, icon::STOP).on_hover_text("Stop every layer in the group").clicked() {
            stop = true;
        }
        Level::percent(&mut g.opacity, "Opacity").modulation(&mut g.opacity_mod).target(ControlTarget::LayerOpacity(g.id)).width(150.0).show(ui);
        choice_combo(ui, ("stack_group_blend", g.id), &mut g.blend, 62.0, true);
        if let Some(r) = super::groups::fx_badge(ui, &g.effects, &mut g.fx_bypass) {
            map_menu(ui, &r, ControlTarget::LayerFxBypass(g.id));
        }
        if !g.in_mix {
            hint(ui, "surfaces only");
        }
        ui.menu_button(icon::DOTS_THREE, |ui| {
            crate::ui::field_menu(ui, label(icon::PENCIL_SIMPLE, "Rename"), |ui| {
                if let Some(n) = rename_menu(ui, ("stack_group_rename", g.id), &g.name) {
                    g.name = n;
                    ui.close();
                }
            });
            if crate::ui::command(ui, label(icon::MAGIC_WAND, "Add effect to the group…")).clicked() {
                action = Some(GroupAction::BrowseEffects);
                ui.close();
            }
            if crate::ui::command(ui, label(icon::MONITOR, "Preview in a window")).clicked() {
                request_preview(ui, MonitorSource::Group(g.id));
                ui.close();
            }
            #[cfg(not(target_arch = "wasm32"))]
            if crate::ui::command(ui, label(icon::CAMERA, "Save a picture…")).clicked() {
                super::request_snapshot(ui, g.id);
                ui.close();
            }
            ui.separator();
            if crate::ui::command(ui, label(icon::ARROW_UP, "Move up")).clicked() {
                action = Some(GroupAction::Up);
                ui.close();
            }
            if crate::ui::command(ui, label(icon::ARROW_DOWN, "Move down")).clicked() {
                action = Some(GroupAction::Down);
                ui.close();
            }
            ui.separator();
            if crate::ui::command(ui, label(icon::SELECTION_SLASH, "Ungroup (keep the layers)")).clicked() {
                action = Some(GroupAction::Ungroup);
                ui.close();
            }
            if danger_button(ui, label(icon::TRASH, "Delete group and its layers")).clicked() {
                action = Some(GroupAction::Delete);
                ui.close();
            }
        });
    });
    let dropped = crate::ui::drop_payload::<u64>(&resp).map(|p| *p);
    if resp.dnd_hover_payload::<u64>().is_some() {
        ui.painter().rect_stroke(resp.rect, 4.0, Stroke::new(2.0, accent()), StrokeKind::Outside);
    }
    (select, action, stop, dropped)
}

/// A comp's band: a group with its own size (and outputs). Returns (select, action, stop its layers).
pub(crate) fn comp_band(ui: &mut egui::Ui, c: &mut Comp, thumb: Option<egui::TextureId>, aspect: f32, is_selected: bool, members: usize, many: bool) -> (bool, Option<CompBandAct>, bool) {
    let mut select = false;
    let mut action = None;
    let mut stop = false;
    let resp = band_frame(ui, is_selected, 0.0, crate::theme::Hue::Output.color(), |ui| {
        let caret = if c.collapsed { icon::CARET_RIGHT } else { icon::CARET_DOWN };
        if crate::ui::small_command(ui, caret).on_hover_text(if c.collapsed { "Show its layers" } else { "Fold its layers away" }).clicked() {
            c.collapsed = !c.collapsed;
        }
        let r = super::preview::thumbnail(ui, thumb, egui::vec2(CONTROL_H * aspect, CONTROL_H), c.enabled).on_hover_text("Click to select (size, outputs, effects in the Inspector) · double-click: on/off");
        if r.clicked() {
            select = true;
        }
        if r.double_clicked() {
            c.enabled = !c.enabled;
        }
        let on = toggle(ui, &mut c.enabled, "ON").on_hover_text("On/off · right-click to Learn");
        mapped_badge(ui, &on, &ControlTarget::LayerEnabled(c.id));
        on.right_menu(|ui| learn_items(ui, &ControlTarget::LayerEnabled(c.id)));
        let name = RichText::new(format!("{} {}", icon::FRAME_CORNERS, c.name)).strong();
        let name = if is_selected { name.color(accent()) } else { name };
        if crate::ui::command(ui, name).on_hover_text(format!("{members} layers on a canvas of its own · click to select")).clicked() {
            select = true;
        }
        if crate::ui::small_command(ui, icon::STOP).on_hover_text("Stop every layer in it").clicked() {
            stop = true;
        }
        Level::percent(&mut c.opacity, "Opacity").modulation(&mut c.opacity_mod).target(ControlTarget::LayerOpacity(c.id)).width(150.0).show(ui);
        choice_combo(ui, ("stack_comp_blend", c.id), &mut c.blend, 62.0, true);
        if let Some(r) = super::groups::fx_badge(ui, &c.effects, &mut c.fx_bypass) {
            map_menu(ui, &r, ControlTarget::LayerFxBypass(c.id));
        }
        let size = if c.width == 0 { "master size".to_string() } else { format!("{}×{}", c.width, c.height) };
        hint(ui, if c.in_mix { size } else { format!("{size} · outputs only") });
        ui.menu_button(icon::DOTS_THREE, |ui| {
            crate::ui::field_menu(ui, label(icon::PENCIL_SIMPLE, "Rename"), |ui| {
                if let Some(n) = rename_menu(ui, ("stack_comp_rename", c.id), &c.name) {
                    action = Some(CompBandAct::Rename(n));
                    ui.close();
                }
            });
            if crate::ui::command(ui, label(icon::SLIDERS_HORIZONTAL, "Size, outputs, effects (Inspector)")).clicked() {
                select = true;
                ui.close();
            }
            if crate::ui::command(ui, label(icon::MAGIC_WAND, "Add effect…")).clicked() {
                action = Some(CompBandAct::BrowseEffects);
                ui.close();
            }
            if crate::ui::command(ui, label(icon::MONITOR, "Preview in a window")).clicked() {
                request_preview(ui, MonitorSource::Group(c.id));
                ui.close();
            }
            if many {
                ui.separator();
                if crate::ui::command(ui, label(icon::ARROW_DOWN, "Earlier (further down in the master)")).clicked() {
                    action = Some(CompBandAct::Earlier);
                    ui.close();
                }
                if crate::ui::command(ui, label(icon::ARROW_UP, "Later (on top in the master)")).clicked() {
                    action = Some(CompBandAct::Later);
                    ui.close();
                }
                ui.separator();
                if danger_button(ui, label(icon::TRASH, "Delete (its layers move to the first)")).clicked() {
                    action = Some(CompBandAct::Delete);
                    ui.close();
                }
            }
        });
    });
    let _ = resp;
    (select, action, stop)
}

/// A band across the stack (group / comp): a coloured line down its left edge, lightly filled when selected.
fn band_frame(ui: &mut egui::Ui, is_selected: bool, indent: f32, color: Color32, add: impl FnOnce(&mut egui::Ui)) -> egui::Response {
    let fill = if is_selected { crate::theme::tint(ui.visuals().panel_fill, 0.1) } else { crate::theme::tint(ui.visuals().panel_fill, 0.04) };
    let r = egui::Frame::new().inner_margin(egui::Margin { left: indent as i8 + 8, right: 6, top: 3, bottom: 3 }).corner_radius(4.0).fill(fill).show(ui, |ui| {
        ui.set_min_width(ui.available_width().min(STRIP_W * 2.0));
        crate::ui::toolbar(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            add(ui);
        });
    });
    let rect = r.response.rect;
    ui.painter().rect_filled(egui::Rect::from_min_max(rect.left_top() + egui::vec2(indent + 2.0, 3.0), egui::pos2(rect.left() + indent + 5.0, rect.bottom() - 3.0)), 1.5, color);
    r.response
}

/// MASTER at the bottom: select (effects over everything), Fade, fx, Output.
fn master_strip(ui: &mut egui::Ui, comp: &mut Composition, is_selected: bool, acts: &mut StackActs, selected: &mut Option<u64>) {
    let _ = acts;
    crate::ui::toolbar(ui, |ui| {
        let name = RichText::new("MASTER").strong().color(accent());
        if crate::ui::command(ui, name).on_hover_text("Effects here run over all layers · click to select").clicked() {
            *selected = Some(MASTER_ID);
        }
        let _ = is_selected;
        Level::percent(&mut comp.master_fade, "Fade").modulation(&mut comp.master_fade_mod).target(ControlTarget::MasterFade).width(220.0).show(ui);
        if let Some(r) = super::groups::fx_badge(ui, &comp.master_effects, &mut comp.master_fx_bypass) {
            map_menu(ui, &r, ControlTarget::App(phosphor_engine::AppControl::MasterFxBypass));
        }
    });
}


/// Does what a comp band asked (Stack and Layers).
pub(crate) fn apply_comp_band_act(st: &mut State, id: u64, a: CompBandAct) {
    match a {
        CompBandAct::Rename(n) => {
            if let Some(c) = st.comp.comp_mut(id) {
                c.name = n;
            }
        }
        CompBandAct::Earlier => st.comp.move_comp(id, false),
        CompBandAct::Later => st.comp.move_comp(id, true),
        CompBandAct::Delete => {
            st.comp.remove_comp(id);
            if st.selected == Some(id) {
                st.selected = None;
            }
        }
        CompBandAct::BrowseEffects => st.fx_browser = Some(crate::explorer::FxBrowser::new(id, false, None)),
    }
}

fn apply_stack_acts(st: &mut State, acts: StackActs, now: f64) {
    if let Some((from, onto)) = acts.drop {
        st.comp.drop_layer(from, onto);
    }
    if let Some((id, a)) = acts.layer {
        super::layers::apply_layer_action(st, id, a, now);
    }
    if let Some((gid, a)) = acts.group {
        super::groups::apply_group_action(st, gid, a);
    }
    if let Some((id, a)) = acts.comp {
        apply_comp_band_act(st, id, a);
    }
    if !acts.stop.is_empty() {
        stop_layers(st, &acts.stop, now);
    }
    if let Some((layer, m)) = acts.media_launch
        && let Some(item) = st.comp.media_bin.get(m).cloned()
    {
        st.launch_source(layer, crate::media::spec_for_path(&item.path), Vec::new(), item.name, now);
    }
    if let Some((layer, d)) = acts.launch {
        st.launch_source(layer, d.source, Vec::new(), d.name, now);
    }
    if let Some((target, d)) = acts.palette {
        crate::explorer::apply_palette(st, d, Some(target));
    }
}

/// Layers from a folder of clips: one per subfolder (its clips in the row, in name order), and one for the files at
/// the top level. Returns how many layers were made.
#[cfg(test)]
mod tests {
    #[test]
    fn the_strip_shares_a_narrow_panel_with_the_cells() {
        assert_eq!(super::strip_width(2000.0), super::STRIP_W);
        assert_eq!(super::strip_width(700.0), super::STRIP_MIN_W);
        assert!((super::strip_width(1000.0) - 420.0).abs() < 0.01);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn layers_from_folder(st: &mut State, dir: &std::path::Path) -> usize {
    let mut made = 0;
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    let mut subs: Vec<std::path::PathBuf> = Vec::new();
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            subs.push(p);
        } else if crate::media::is_supported(&p.to_string_lossy()) {
            files.push(p);
        }
    }
    subs.sort();
    files.sort();
    let items = |paths: &[std::path::PathBuf]| -> Vec<(String, SourceSpec)> {
        paths.iter().map(|p| {
            let s = p.to_string_lossy().into_owned();
            (crate::media::clip_name(&s), crate::media::spec_for_path(&s))
        }).collect()
    };
    let folder_name = |p: &std::path::Path| p.file_name().map_or_else(|| p.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned());
    let mut rows: Vec<(String, Vec<(String, SourceSpec)>)> = Vec::new();
    if !files.is_empty() {
        rows.push((folder_name(dir), items(&files)));
    }
    for sub in &subs {
        let Ok(inner) = std::fs::read_dir(sub) else { continue };
        let mut clips: Vec<std::path::PathBuf> = inner.flatten().map(|e| e.path()).filter(|p| p.is_file() && crate::media::is_supported(&p.to_string_lossy())).collect();
        clips.sort();
        if !clips.is_empty() {
            rows.push((folder_name(sub), items(&clips)));
        }
    }
    // The first row ends up on top: add from the last.
    for (name, clips) in rows.into_iter().rev() {
        let id = st.add_layer();
        if let Some(l) = st.comp.layer_mut(id) {
            l.name = name;
        }
        st.comp.place_clips(id, 0, clips);
        made += 1;
    }
    made
}
