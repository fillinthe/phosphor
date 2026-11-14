//! The Studio's list of everything on an output (MadMapper's layers): layouts on top (whole arrangements to switch
//! or fade between), then groups, slices and masks. Groups fold open and shut, sit inside each other, and have their
//! own on/off and opacity (mappable); slices and masks go in and out of them. Click a group to select everything in
//! it; right-click any row to group the selection, move, rename, duplicate or delete.

use phosphor_engine::mapping::Mapping;

use super::*;

/// What a row asked for (applied after the list is drawn, by the Studio: these need new ids or the app).
pub(super) enum SliceAct {
    /// Group these slices / masks (the Studio gives the new group its id).
    Group(Vec<u64>),
    Duplicate(u64),
    /// Delete slices or masks by id.
    Delete(Vec<u64>),
    /// A new layout on this output.
    AddLayout,
    /// Fade to this layout (the others fade out).
    SwitchLayout(u64),
    /// Copy a group or layout with everything in it.
    DuplicateGroup(u64),
}

enum Edit {
    Ungroup(u64),
    Rename(u64, String),
    Fold(u64),
    SlicesTo(Vec<u64>, Option<u64>),
    /// Rows dropped on another slice / mask row: into its layout / group, just before it.
    NextTo(Vec<u64>, u64),
    GroupTo(u64, Option<u64>),
    /// Delete a group / layout and everything in it.
    DeleteGroup(u64),
}

/// Every row for `mapping`: the layouts bar, then groups first at each level, then the slices, then the masks.
pub(super) fn slice_rows(ui: &mut egui::Ui, mapping: &mut Mapping, selected: &mut Option<u64>, acts: &mut Vec<SliceAct>) {
    layouts_bar(ui, mapping, acts);
    let mut edits = Vec::new();
    level(ui, mapping, None, 0, selected, acts, &mut edits);
    for e in edits {
        match e {
            Edit::Ungroup(g) => mapping.ungroup(g),
            Edit::Rename(g, name) => {
                if let Some(g) = mapping.groups.iter_mut().find(|x| x.id == g) {
                    g.name = name;
                }
            }
            Edit::Fold(g) => {
                if let Some(g) = mapping.groups.iter_mut().find(|x| x.id == g) {
                    g.collapsed = !g.collapsed;
                }
            }
            Edit::SlicesTo(ids, to) => mapping.move_to_group(&ids, to),
            Edit::NextTo(ids, target) => mapping.move_next_to(&ids, target),
            Edit::GroupTo(g, to) => {
                mapping.move_group(g, to);
            }
            Edit::DeleteGroup(g) => {
                let inside = |m: &Mapping, x: Option<u64>| x.is_some_and(|x| x == g || m.inside(x, g));
                let slices: Vec<u64> = mapping.surfaces.iter().filter(|s| inside(mapping, s.group)).map(|s| s.id).collect();
                let masks: Vec<u64> = mapping.masks.iter().filter(|m| inside(mapping, m.group)).map(|m| m.id).collect();
                acts.push(SliceAct::Delete(slices.into_iter().chain(masks).collect()));
                let gone: Vec<u64> = mapping.groups.iter().filter(|x| x.id == g || mapping.inside(x.id, g)).map(|x| x.id).collect();
                mapping.groups.retain(|x| !gone.contains(&x.id));
            }
        }
    }
}

/// Slice and mask rows being dragged in the list (the whole selection when the row is part of it).
struct RowDrag(Vec<u64>);

/// Rows dropped on `r` (a row, a group, a layout button); it's outlined while rows hover over it.
fn drop_rows(ui: &egui::Ui, r: &egui::Response) -> Option<Vec<u64>> {
    if r.dnd_hover_payload::<RowDrag>().is_some() {
        ui.painter().rect_stroke(r.rect, 0.0, Stroke::new(2.0, accent()), StrokeKind::Outside);
    }
    crate::ui::drop_payload::<RowDrag>(r).map(|p| p.0.clone())
}

/// The output's layouts as buttons: click one to fade to it (auto-fade time), right-click to map it to a pad / key
/// / OSC, rename, copy or delete it.
fn layouts_bar(ui: &mut egui::Ui, mapping: &mut Mapping, acts: &mut Vec<SliceAct>) {
    let layouts: Vec<(u64, String, bool)> = mapping.layouts().map(|g| (g.id, g.name.clone(), g.enabled && g.opacity > 0.5)).collect();
    crate::ui::toolbar(ui, |ui| {
        ui.label(RichText::new(format!("{} Layouts", icon::STACK)).small().color(Color32::GRAY));
        for (id, name, showing) in &layouts {
            let target = ControlTarget::Layout(*id);
            let r = crate::ui::pick_item(ui, *showing, name.as_str()).on_hover_text("Click: fade to this layout (the others fade out; auto-fade time) · right-click: map it, rename, copy, delete");
            crate::ui::mapped_badge(ui, &r, &target);
            if r.clicked() {
                acts.push(SliceAct::SwitchLayout(*id));
            }
            // Rows dragged from the list and let go on a layout's button move into that layout.
            if let Some(rows) = drop_rows(ui, &r) {
                mapping.move_to_group(&rows, Some(*id));
            }
            if let Some(new) = crate::ui::renamed(ui.ctx(), ("layout_name", *id))
                && let Some(g) = mapping.groups.iter_mut().find(|g| g.id == *id)
            {
                g.name = new;
            }
            r.right_menu(|ui| {
                crate::ui::rename_item(ui, ("layout_name", *id), name);
                if crate::ui::command(ui, label(icon::COPY, "Duplicate")).on_hover_text("A copy of every slice and mask in it, to change into another arrangement").clicked() {
                    acts.push(SliceAct::DuplicateGroup(*id));
                    ui.close();
                }
                if danger_button(ui, label(icon::TRASH, "Delete layout and what's in it")).clicked() {
                    let inside = |x: Option<u64>| x.is_some_and(|x| x == *id || mapping.inside(x, *id));
                    let ids: Vec<u64> = mapping.surfaces.iter().filter(|s| inside(s.group)).map(|s| s.id).chain(mapping.masks.iter().filter(|m| inside(m.group)).map(|m| m.id)).collect();
                    acts.push(SliceAct::Delete(ids));
                    let gone: Vec<u64> = mapping.groups.iter().filter(|g| g.id == *id || mapping.inside(g.id, *id)).map(|g| g.id).collect();
                    mapping.groups.retain(|g| !gone.contains(&g.id));
                    ui.close();
                }
                ui.separator();
                crate::ui::learn_items(ui, &target);
            });
        }
        let tip = if layouts.is_empty() { "Make what's on this output Layout 1, and a new empty Layout 2 to build another arrangement in" } else { "Another arrangement of this output (starts empty and faded out)" };
        if crate::ui::small_command(ui, label(icon::PLUS, "Layout")).on_hover_text(tip).clicked() {
            acts.push(SliceAct::AddLayout);
        }
    });
}

fn level(ui: &mut egui::Ui, mapping: &mut Mapping, parent: Option<u64>, depth: usize, selected: &mut Option<u64>, acts: &mut Vec<SliceAct>, edits: &mut Vec<Edit>) {
    let indent = 14.0 + 14.0 * depth as f32;
    let groups: Vec<u64> = mapping.groups.iter().filter(|g| g.parent == parent).map(|g| g.id).collect();
    for gid in groups {
        let members: Vec<u64> = {
            let inside = |x: Option<u64>| x.is_some_and(|x| x == gid || mapping.inside(x, gid));
            mapping.surfaces.iter().filter(|s| inside(s.group)).map(|s| s.id).chain(mapping.masks.iter().filter(|m| inside(m.group)).map(|m| m.id)).collect()
        };
        let picked = super::mapping_points::selected_slices(ui.ctx(), *selected);
        // Where this group may move: not into itself or a group inside it.
        let movable: Vec<(u64, String, usize)> = mapping.groups.iter().filter(|x| x.id != gid && !mapping.inside(x.id, gid)).map(|x| (x.id, x.name.clone(), mapping.group_depth(x.id))).collect();
        let Some(g) = mapping.groups.iter_mut().find(|g| g.id == gid) else { continue };
        let (collapsed, group_parent, name, is_layout) = (g.collapsed, g.parent, g.name.clone(), g.layout);
        // (One line: the fade sits at the right and a long name is cut short, instead of the fade wrapping under it.)
        ui.horizontal(|ui| {
            ui.add_space(indent);
            let caret = if collapsed { icon::CARET_RIGHT } else { icon::CARET_DOWN };
            if crate::ui::small_command(ui, caret).on_hover_text(if collapsed { "Open the group" } else { "Fold the group" }).clicked() {
                edits.push(Edit::Fold(gid));
            }
            let target = ControlTarget::SliceEnabled(gid);
            let all_picked = !members.is_empty() && members.iter().all(|m| picked.contains(m));
            let glyph = if is_layout { icon::STACK } else { icon::FOLDER };
            let text = format!("{glyph} {name}  ({})", members.len());
            // The name gets the row less a fixed switch and fade (cut short past that, whole on hover); the fade is
            // pushed to the right edge, so every group's switch and fade line up.
            const FADE_W: f32 = 52.0;
            let gap = ui.spacing().item_spacing.x;
            let tail = crate::ui::CONTROL_H + FADE_W + 2.0 * gap;
            let edge = ui.clip_rect().right().min(ui.max_rect().right()) - 6.0;
            let r = crate::ui::pick_item_cut(ui, all_picked, &text, edge - ui.cursor().left() - tail).on_hover_text("Click to select everything in it");
            let fade_w = FADE_W;
            ui.add_space((edge - ui.cursor().left() - tail + gap).max(0.0));
            crate::ui::mapped_badge(ui, &r, &target);
            if r.clicked() {
                super::mapping_points::select_slices(ui.ctx(), selected, members.clone());
            }
            if let Some(rows) = drop_rows(ui, &r) {
                edits.push(Edit::SlicesTo(rows, Some(gid)));
            }
            if let Some(name) = crate::ui::renamed(ui.ctx(), ("slice_group", gid)) {
                edits.push(Edit::Rename(gid, name));
            }
            // Its own fade, on top of each slice's and mask's (mappable; layouts crossfade with it).
            // On / off without moving the fader (mappable).
            Level::percent(&mut g.opacity, "").default(1.0).width(fade_w).target(ControlTarget::SliceOpacity(gid)).switch(&mut g.enabled, Some(target.clone())).show(ui);
            r.right_menu(|ui| {
                crate::ui::rename_item(ui, ("slice_group", gid), &name);
                if !is_layout {
                    move_menu(ui, movable.clone(), Some(gid), group_parent, &mut |to| edits.push(Edit::GroupTo(gid, to)));
                    if crate::ui::command(ui, label(icon::SELECTION_SLASH, "Ungroup")).on_hover_text("What's in it stays, one level up").clicked() {
                        edits.push(Edit::Ungroup(gid));
                        ui.close();
                    }
                }
                if crate::ui::command(ui, label(icon::COPY, "Duplicate")).clicked() {
                    acts.push(SliceAct::DuplicateGroup(gid));
                    ui.close();
                }
                if danger_button(ui, label(icon::TRASH, "Delete it and what's in it")).clicked() {
                    edits.push(Edit::DeleteGroup(gid));
                    ui.close();
                }
                ui.separator();
                crate::ui::learn_items(ui, &target);
            });
        });
        if !collapsed {
            level(ui, mapping, Some(gid), depth + 1, selected, acts, edits);
        }
    }
    let picked = super::mapping_points::selected_slices(ui.ctx(), *selected);
    let groups_now: Vec<(u64, String, usize)> = mapping.groups.iter().map(|g| (g.id, g.name.clone(), mapping.group_depth(g.id))).collect();
    let ids: Vec<u64> = mapping.surfaces.iter().filter(|s| s.group == parent).map(|s| s.id).collect();
    for id in ids {
        let Some(i) = mapping.surfaces.iter().position(|s| s.id == id) else { continue };
        let mut solo = mapping.solo.contains(&id);
        let was_solo = solo;
        let s = &mut mapping.surfaces[i];
        if let Some(name) = crate::ui::renamed(ui.ctx(), ("slice_name", id)) {
            s.name = name;
        }
        let name = s.name.clone();
        let is_picked = picked.contains(&id);
        let (enabled, opacity, locked) = (&mut s.enabled, &mut s.opacity, &mut s.locked);
        // A narrow list keeps the name readable: the fader stays, solo and lock show once there's room (a locked
        // or soloed row always shows its own, so you can see why it won't move / why the rest is dark).
        let room = ui.available_width() - indent;
        let r = layer_row(ui, indent, is_picked, icon::POLYGON, &name, None, enabled, |ui| {
            Level::percent(opacity, "").default(1.0).width(FADE_W).target(ControlTarget::SliceOpacity(id)).tip("This slice's opacity").show(ui);
            if room > 300.0 || *locked {
                let glyph = if *locked { icon::LOCK } else { icon::LOCK_OPEN };
                crate::ui::icon_toggle(ui, locked, glyph).on_hover_text("Locked: its points can't be dragged or nudged");
            }
            if room > 300.0 || solo {
                crate::ui::icon_toggle(ui, &mut solo, "S").on_hover_text("Solo: the output shows only the soloed slices (to see which is which on the real object)");
            }
        });
        row_clicks(ui, &r, id, &name, &picked, selected, edits);
        row_menu(&r, id, &name, s.group, &picked, &groups_now, Some(&mut s.blend), acts, edits);
        if solo != was_solo {
            if solo {
                mapping.solo.push(id);
            } else {
                mapping.solo.retain(|x| *x != id);
            }
        }
    }
    let masks: Vec<u64> = mapping.masks.iter().filter(|m| m.group == parent).map(|m| m.id).collect();
    for id in masks {
        let Some(m) = mapping.masks.iter_mut().find(|m| m.id == id) else { continue };
        if let Some(name) = crate::ui::renamed(ui.ctx(), ("slice_name", id)) {
            m.name = name;
        }
        let name = m.name.clone();
        let is_picked = picked.contains(&id) || *selected == Some(id);
        let (enabled, opacity, invert) = (&mut m.enabled, &mut m.opacity, &mut m.invert);
        let r = layer_row(ui, indent, is_picked, icon::SELECTION, &name, Some(MASK_COLOR), enabled, |ui| {
            Level::percent(opacity, "").default(1.0).width(FADE_W).tip("How strongly this mask hides").show(ui);
            if ui.available_width() > 150.0 || *invert {
                crate::ui::icon_toggle(ui, invert, icon::SELECTION_INVERSE).on_hover_text("Inverted: only the inside of the mask shows");
            }
        });
        row_clicks(ui, &r, id, &name, &picked, selected, edits);
        row_menu(&r, id, &name, m.group, &picked, &groups_now, None, acts, edits);
    }
}

/// A row's opacity fader (slices, masks) is this wide, so they line up down the list.
const FADE_W: f32 = 52.0;

/// One slice or mask as a layer row across the whole list: a grip, its eye, its kind and name, and `tail` (its own
/// controls, added from the right edge). The whole row is the thing you click, drag and right-click (owner,
/// 2026-11-14: "more like layers and not chips").
#[allow(clippy::too_many_arguments)]
fn layer_row(ui: &mut egui::Ui, indent: f32, picked: bool, glyph: &str, name: &str, colour: Option<Color32>, shown: &mut bool, tail: impl FnOnce(&mut egui::Ui)) -> egui::Response {
    let h = crate::ui::CONTROL_H + 6.0;
    let (outer, row) = ui.allocate_exact_size(egui::vec2(ui.available_width(), h), Sense::click_and_drag());
    let rect = egui::Rect::from_min_max(outer.min + egui::vec2(indent, 0.0), outer.max).shrink2(egui::vec2(0.0, 1.0));
    let fill = if picked { ui.visuals().selection.bg_fill } else if row.hovered() { ui.visuals().widgets.hovered.weak_bg_fill } else { ui.visuals().faint_bg_color };
    ui.painter().rect_filled(rect, 0.0, fill);
    // Its kind down the left edge (a mask is the mask colour).
    let edge = colour.unwrap_or_else(accent);
    ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())), 0.0, if *shown { edge } else { edge.gamma_multiply(0.35) });
    let words = if !*shown { ui.visuals().weak_text_color() } else { colour.unwrap_or_else(|| ui.visuals().strong_text_color()) };
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(6.0, 0.0))).layout(egui::Layout::right_to_left(egui::Align::Center)), |ui| {
        tail(ui);
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add(egui::Label::new(RichText::new(icon::DOTS_SIX_VERTICAL).color(ui.visuals().weak_text_color())).selectable(false));
            crate::ui::eye_toggle(ui, shown);
            ui.add(egui::Label::new(RichText::new(format!("{glyph} {name}")).color(words)).truncate().selectable(false));
        });
    });
    row
}

/// A layer row's clicks: select (Shift adds), double-click renames, drag moves it, rows dropped on it land before it.
fn row_clicks(ui: &egui::Ui, r: &egui::Response, id: u64, name: &str, picked: &[u64], selected: &mut Option<u64>, edits: &mut Vec<Edit>) {
    if r.double_clicked() {
        crate::ui::ask_rename(ui.ctx(), ("slice_name", id), name);
    } else if r.clicked() {
        let shift = ui.input(|i| i.modifiers.shift);
        super::mapping_points::click_slice(ui.ctx(), selected, id, shift);
    }
    if r.drag_started() {
        r.dnd_set_drag_payload(RowDrag(if picked.contains(&id) { picked.to_vec() } else { vec![id] }));
    }
    if let Some(rows) = drop_rows(ui, r) {
        edits.push(Edit::NextTo(rows, id));
    }
    r.clone().on_hover_text("Click to select (Shift adds) · double-click to rename · drag onto a layout, a group or another row · right-click for more");
}

/// The right-click menu of a slice or mask row (on the whole selection when the row is part of it).
#[allow(clippy::too_many_arguments)]
fn row_menu(r: &egui::Response, id: u64, name: &str, group: Option<u64>, picked: &[u64], groups: &[(u64, String, usize)], mut blend: Option<&mut phosphor_engine::mapping::SliceBlend>, acts: &mut Vec<SliceAct>, edits: &mut Vec<Edit>) {
    let these: Vec<u64> = if picked.contains(&id) { picked.to_vec() } else { vec![id] };
    let slice = blend.is_some();
    r.right_menu(|ui| {
        crate::ui::rename_item(ui, ("slice_name", id), name);
        // How a slice lands on the ones under it.
        if let Some(blend) = blend.as_deref_mut() {
            ui.menu_button(label(icon::CIRCLES_THREE, &format!("Blend: {}", blend.name())), |ui| {
                for b in phosphor_engine::mapping::SliceBlend::ALL {
                    if crate::ui::pick_item(ui, *blend == b, b.name()).clicked() {
                        *blend = b;
                        ui.close();
                    }
                }
            });
        }
        ui.separator();
        let n = these.len();
        let text = if n > 1 { format!("Group these {n}") } else { "Put in a new group".into() };
        if crate::ui::command(ui, label(icon::FOLDER_PLUS, &text)).clicked() {
            acts.push(SliceAct::Group(these.clone()));
            ui.close();
        }
        move_menu(ui, groups.to_vec(), None, group, &mut |to| edits.push(Edit::SlicesTo(these.clone(), to)));
        ui.separator();
        if slice && crate::ui::command(ui, label(icon::COPY, "Duplicate")).clicked() {
            acts.push(SliceAct::Duplicate(id));
            ui.close();
        }
        if crate::ui::command(ui, label(icon::TRASH, "Delete")).clicked() {
            acts.push(SliceAct::Delete(these.clone()));
            ui.close();
        }
    });
}

/// "Move to group ▸": every group from `groups` (id, name, depth) that `moving` (a group, or None for slices and
/// masks) may go into, plus the top level.
fn move_menu(ui: &mut egui::Ui, groups: Vec<(u64, String, usize)>, moving: Option<u64>, now: Option<u64>, pick: &mut dyn FnMut(Option<u64>)) {
    let targets: Vec<(u64, String, usize)> = groups.into_iter().filter(|(g, ..)| Some(*g) != now && moving.is_none_or(|m| *g != m)).collect();
    if targets.is_empty() && now.is_none() {
        return;
    }
    ui.menu_button(label(icon::ARROW_SQUARE_IN, "Move to layout / group"), |ui| {
        if now.is_some() && crate::ui::command(ui, "Out of every layout and group").clicked() {
            pick(None);
            ui.close();
        }
        for (id, name, depth) in targets {
            if crate::ui::command(ui, format!("{}{} {name}", "   ".repeat(depth), icon::FOLDER)).clicked() {
                pick(Some(id));
                ui.close();
            }
        }
    });
}
