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
            r.right_menu(|ui| {
                crate::ui::field_menu(ui, label(icon::PENCIL_SIMPLE, "Rename"), |ui| {
                    if let Some(new) = rename_menu(ui, ("layout_name", *id), name) {
                        if let Some(g) = mapping.groups.iter_mut().find(|g| g.id == *id) {
                            g.name = new;
                        }
                        ui.close();
                    }
                });
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
        crate::ui::toolbar(ui, |ui| {
            ui.add_space(indent);
            let caret = if collapsed { icon::CARET_RIGHT } else { icon::CARET_DOWN };
            if crate::ui::small_command(ui, caret).on_hover_text(if collapsed { "Open the group" } else { "Fold the group" }).clicked() {
                edits.push(Edit::Fold(gid));
            }
            let target = ControlTarget::SliceEnabled(gid);
            let eye = crate::ui::eye_toggle(ui, &mut g.enabled).on_hover_text("Show or hide everything in it (right-click to map)");
            crate::ui::map_menu(ui, &eye, target.clone());
            let all_picked = !members.is_empty() && members.iter().all(|m| picked.contains(m));
            let glyph = if is_layout { icon::STACK } else { icon::FOLDER };
            let text = RichText::new(format!("{glyph} {name}  ({})", members.len())).strong();
            let r = crate::ui::pick_item(ui, all_picked, text).on_hover_text("Click to select everything in it");
            crate::ui::mapped_badge(ui, &r, &target);
            if r.clicked() {
                super::mapping_points::select_slices(ui.ctx(), selected, members.clone());
            }
            // Its own fade, on top of each slice's and mask's (mappable; layouts crossfade with it).
            Level::percent(&mut g.opacity, "").default(1.0).width(64.0).target(ControlTarget::SliceOpacity(gid)).show(ui);
            r.right_menu(|ui| {
                crate::ui::field_menu(ui, label(icon::PENCIL_SIMPLE, "Rename"), |ui| {
                    if let Some(name) = rename_menu(ui, ("slice_group", gid), &name) {
                        edits.push(Edit::Rename(gid, name));
                        ui.close();
                    }
                });
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
        crate::ui::toolbar(ui, |ui| {
            ui.add_space(indent);
            let s = &mut mapping.surfaces[i];
            crate::ui::eye_toggle(ui, &mut s.enabled);
            let r = crate::ui::pick_item(ui, picked.contains(&s.id), format!("{} {}", icon::POLYGON, s.name)).on_hover_text("Shift-click to select several");
            if r.clicked() {
                let shift = ui.input(|i| i.modifiers.shift);
                super::mapping_points::click_slice(ui.ctx(), selected, id, shift);
            }
            row_menu(&r, id, s.group, &picked, &groups_now, true, acts, edits);
        });
    }
    let masks: Vec<u64> = mapping.masks.iter().filter(|m| m.group == parent).map(|m| m.id).collect();
    for id in masks {
        let Some(m) = mapping.masks.iter_mut().find(|m| m.id == id) else { continue };
        crate::ui::toolbar(ui, |ui| {
            ui.add_space(indent);
            let eye = crate::ui::eye_toggle(ui, &mut m.enabled);
            crate::ui::map_menu(ui, &eye, ControlTarget::MaskEnabled(id));
            let r = crate::ui::pick_item(ui, picked.contains(&id) || *selected == Some(id), RichText::new(format!("{} {}", icon::SELECTION, m.name)).color(MASK_COLOR)).on_hover_text("Shift-click to select several");
            if r.clicked() {
                let shift = ui.input(|i| i.modifiers.shift);
                super::mapping_points::click_slice(ui.ctx(), selected, id, shift);
            }
            row_menu(&r, id, m.group, &picked, &groups_now, false, acts, edits);
        });
    }
}

/// The right-click menu of a slice or mask row (on the whole selection when the row is part of it).
#[allow(clippy::too_many_arguments)]
fn row_menu(r: &egui::Response, id: u64, group: Option<u64>, picked: &[u64], groups: &[(u64, String, usize)], slice: bool, acts: &mut Vec<SliceAct>, edits: &mut Vec<Edit>) {
    let these: Vec<u64> = if picked.contains(&id) { picked.to_vec() } else { vec![id] };
    r.right_menu(|ui| {
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
    ui.menu_button(label(icon::ARROW_SQUARE_IN, "Move to group"), |ui| {
        if now.is_some() && crate::ui::command(ui, "Out of every group").clicked() {
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
