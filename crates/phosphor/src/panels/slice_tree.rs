//! The Studio's slice list with slice groups (MadMapper's groups): groups fold open and shut and can sit inside
//! each other; click a group to select all its slices; right-click to group the selection, move slices or groups
//! in and out, rename, ungroup. Groups are saved with the project; their on/off and opacity are mappable controls.

use phosphor_engine::mapping::Mapping;

use super::*;

/// What a row asked for (applied after the list is drawn).
pub(super) enum SliceAct {
    /// Group these slices (the Studio gives the new group its id).
    Group(Vec<u64>),
    Duplicate(u64),
    /// Delete slices or masks by id.
    Delete(Vec<u64>),
}

enum Edit {
    Ungroup(u64),
    Rename(u64, String),
    Fold(u64),
    SlicesTo(Vec<u64>, Option<u64>),
    GroupTo(u64, Option<u64>),
    Show(Vec<u64>, bool),
}

/// The rows of every slice on `mapping`: groups first at each level, then the slices in drawing order.
pub(super) fn slice_rows(ui: &mut egui::Ui, mapping: &mut Mapping, selected: &mut Option<u64>, acts: &mut Vec<SliceAct>) {
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
            Edit::Show(ids, on) => mapping.surfaces.iter_mut().filter(|s| ids.contains(&s.id)).for_each(|s| s.enabled = on),
        }
    }
}

fn level(ui: &mut egui::Ui, mapping: &mut Mapping, parent: Option<u64>, depth: usize, selected: &mut Option<u64>, acts: &mut Vec<SliceAct>, edits: &mut Vec<Edit>) {
    let indent = 14.0 + 14.0 * depth as f32;
    let groups: Vec<_> = mapping.groups.iter().filter(|g| g.parent == parent).cloned().collect();
    for g in groups {
        let members = mapping.group_members(g.id);
        let picked = super::mapping_points::selected_slices(ui.ctx(), *selected);
        ui.horizontal(|ui| {
            ui.add_space(indent);
            let caret = if g.collapsed { icon::CARET_RIGHT } else { icon::CARET_DOWN };
            if crate::ui::small_command(ui, caret).on_hover_text(if g.collapsed { "Open the group" } else { "Fold the group" }).clicked() {
                edits.push(Edit::Fold(g.id));
            }
            let mut on = members.iter().any(|id| mapping.surfaces.iter().any(|s| s.id == *id && s.enabled));
            if crate::ui::eye_toggle(ui, &mut on).on_hover_text("Show or hide every slice in the group").changed() {
                edits.push(Edit::Show(members.clone(), on));
            }
            let all_picked = !members.is_empty() && members.iter().all(|m| picked.contains(m));
            let text = RichText::new(format!("{} {}  ({})", icon::FOLDER, g.name, members.len())).strong();
            let r = crate::ui::pick_item(ui, all_picked, text).on_hover_text("Click to select all its slices");
            let target = ControlTarget::SliceEnabled(g.id);
            crate::ui::mapped_badge(ui, &r, &target);
            if r.clicked() {
                super::mapping_points::select_slices(ui.ctx(), selected, members.clone());
            }
            r.right_menu(|ui| {
                ui.menu_button(label(icon::PENCIL_SIMPLE, "Rename"), |ui| {
                    if let Some(name) = rename_menu(ui, ("slice_group", g.id), &g.name) {
                        edits.push(Edit::Rename(g.id, name));
                        ui.close();
                    }
                });
                move_menu(ui, mapping, Some(g.id), g.parent, &mut |to| edits.push(Edit::GroupTo(g.id, to)));
                if crate::ui::command(ui, label(icon::SELECTION_SLASH, "Ungroup")).on_hover_text("Its slices stay, one level up").clicked() {
                    edits.push(Edit::Ungroup(g.id));
                    ui.close();
                }
                if danger_button(ui, label(icon::TRASH, "Delete group and its slices")).clicked() {
                    acts.push(SliceAct::Delete(members.clone()));
                    ui.close();
                }
                ui.separator();
                crate::ui::learn_items(ui, &target);
            });
        });
        if !g.collapsed {
            level(ui, mapping, Some(g.id), depth + 1, selected, acts, edits);
        }
    }
    let picked = super::mapping_points::selected_slices(ui.ctx(), *selected);
    let ids: Vec<u64> = mapping.surfaces.iter().filter(|s| s.group == parent).map(|s| s.id).collect();
    for id in ids {
        let Some(i) = mapping.surfaces.iter().position(|s| s.id == id) else { continue };
        ui.horizontal(|ui| {
            ui.add_space(indent);
            let s = &mut mapping.surfaces[i];
            crate::ui::eye_toggle(ui, &mut s.enabled);
            let r = crate::ui::pick_item(ui, picked.contains(&s.id), format!("{} {}", icon::POLYGON, s.name)).on_hover_text("Shift-click to select several");
            if r.clicked() {
                let shift = ui.input(|i| i.modifiers.shift);
                super::mapping_points::click_slice(ui.ctx(), selected, id, shift);
            }
            let group = s.group;
            // The menu works on the whole selection when this slice is part of it.
            let these = if picked.contains(&id) { picked.clone() } else { vec![id] };
            r.right_menu(|ui| {
                let n = these.len();
                let text = if n > 1 { format!("Group these {n} slices") } else { "Put in a new group".into() };
                if crate::ui::command(ui, label(icon::FOLDER_PLUS, &text)).clicked() {
                    acts.push(SliceAct::Group(these.clone()));
                    ui.close();
                }
                move_menu(ui, mapping, None, group, &mut |to| edits.push(Edit::SlicesTo(these.clone(), to)));
                ui.separator();
                if crate::ui::command(ui, label(icon::COPY, "Duplicate")).clicked() {
                    acts.push(SliceAct::Duplicate(id));
                    ui.close();
                }
                if crate::ui::command(ui, label(icon::TRASH, "Delete")).clicked() {
                    acts.push(SliceAct::Delete(vec![id]));
                    ui.close();
                }
            });
        });
    }
}

/// "Move to group ▸": every group `moving` (a group, or None for slices) may go into, plus the top level.
fn move_menu(ui: &mut egui::Ui, mapping: &Mapping, moving: Option<u64>, now: Option<u64>, pick: &mut dyn FnMut(Option<u64>)) {
    let targets: Vec<(u64, String, usize)> = mapping
        .groups
        .iter()
        .filter(|g| Some(g.id) != now && moving.is_none_or(|m| g.id != m && !mapping.inside(g.id, m)))
        .map(|g| (g.id, g.name.clone(), mapping.group_depth(g.id)))
        .collect();
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
