//! What's selected on the mapping canvas: several slices (Shift-click adds or removes one; they move, nudge and
//! change together) and one point. The selected point (MadMapper's fine-tuning):  (MadMapper's fine-tuning): click a corner, mesh or line point to select
//! it, Tab / Shift-Tab to the next / previous point, arrows nudge it (Shift = 10 px), type its X / Y in output pixels,
//! and with Snap on a dragged point lands on another slice's point when it comes close.

use phosphor_engine::mapping::{Mapping, Surface};

use super::*;

const KEY: &str = "phosphor_mapping_point";
const MULTI: &str = "phosphor_mapping_multi";

/// Every selected slice: the set, or just `primary` when nothing else is selected.
pub(crate) fn selected_slices(ctx: &egui::Context, primary: Option<u64>) -> Vec<u64> {
    let set: Vec<u64> = ctx.data(|d| d.get_temp(egui::Id::new(MULTI))).unwrap_or_default();
    match primary {
        Some(p) if set.contains(&p) => set,
        Some(p) => vec![p],
        None => Vec::new(),
    }
}

/// A click on slice `id`: with Shift it joins (or leaves) the selection, else it's the only one selected.
pub(crate) fn click_slice(ctx: &egui::Context, primary: &mut Option<u64>, id: u64, shift: bool) {
    let mut set = selected_slices(ctx, *primary);
    if shift {
        if let Some(i) = set.iter().position(|s| *s == id) {
            set.remove(i);
            *primary = set.last().copied();
        } else {
            set.push(id);
            *primary = Some(id);
        }
    } else {
        set = vec![id];
        *primary = Some(id);
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(MULTI), set));
}

/// Selects exactly `ids` (a slice group): the last one becomes the primary.
pub(crate) fn select_slices(ctx: &egui::Context, primary: &mut Option<u64>, ids: Vec<u64>) {
    *primary = ids.last().copied();
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(MULTI), ids));
}

/// Moves every selected slice (but locked ones) by `d`.
pub(crate) fn move_slices(mapping: &mut Mapping, ids: &[u64], d: [f32; 2]) {
    for s in mapping.surfaces.iter_mut().filter(|s| ids.contains(&s.id) && !s.locked) {
        s.translate(d[0], d[1]);
    }
}

/// (slice id, point index) of the selected point, if any.
pub(crate) fn selected_point(ctx: &egui::Context) -> Option<(u64, usize)> {
    ctx.data(|d| d.get_temp::<Option<(u64, usize)>>(egui::Id::new(KEY))).flatten()
}

pub(crate) fn select_point(ctx: &egui::Context, p: Option<(u64, usize)>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(KEY), p));
}

/// The points a slice's handles move: its line, its mesh or its four corners (none for 3D models).
pub(crate) fn points_mut(s: &mut Surface) -> Vec<&mut [f32; 2]> {
    if s.model.is_some() {
        return Vec::new();
    }
    match (&mut s.line, &mut s.mesh) {
        (Some(l), _) => l.points.iter_mut().collect(),
        (None, Some(m)) => m.points.iter_mut().collect(),
        (None, None) => s.corners.iter_mut().collect(),
    }
}

fn point_count(s: &Surface) -> usize {
    match (&s.line, &s.mesh, &s.model) {
        (_, _, Some(_)) => 0,
        (Some(l), _, _) => l.points.len(),
        (None, Some(m), _) => m.points.len(),
        _ => 4,
    }
}

/// Keys on the canvas (while it's hovered and nothing is being typed): Tab / Shift-Tab pick the next / previous point
/// of the selected slice, Escape lets go of it, arrows nudge the selected point, or the whole slice when no point is
/// selected. `size` = the output in pixels (a nudge is one pixel).
pub(crate) fn point_keys(ui: &egui::Ui, mapping: &mut Mapping, selected: Option<u64>, size: (u32, u32)) {
    if ui.ctx().egui_wants_keyboard_input() {
        return;
    }
    let many = selected_slices(ui.ctx(), selected);
    let Some(s) = mapping.surfaces.iter_mut().find(|s| Some(s.id) == selected) else { return };
    let n = point_count(s);
    let mut point = selected_point(ui.ctx()).filter(|(sid, i)| *sid == s.id && *i < n).map(|(_, i)| i);
    let (tab, back, esc, dx, dy) = ui.ctx().input_mut(|i| {
        let step = if i.modifiers.shift { 10.0 } else { 1.0 };
        let k = |i: &egui::InputState, key| i.key_pressed(key) as i32 as f32;
        let back = i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab);
        let tab = i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
        let esc = i.key_pressed(egui::Key::Escape);
        let dx = (k(i, egui::Key::ArrowRight) - k(i, egui::Key::ArrowLeft)) * step;
        let dy = (k(i, egui::Key::ArrowDown) - k(i, egui::Key::ArrowUp)) * step;
        (tab, back, esc, dx, dy)
    });
    if n > 0 && (tab || back) {
        point = Some(match (point, back) {
            (None, false) => 0,
            (None, true) => n - 1,
            (Some(p), false) => (p + 1) % n,
            (Some(p), true) => (p + n - 1) % n,
        });
    }
    if esc {
        point = None;
    }
    select_point(ui.ctx(), point.map(|p| (s.id, p)));
    if (dx, dy) == (0.0, 0.0) {
        return;
    }
    let (ddx, ddy) = (dx / size.0.max(1) as f32, dy / size.1.max(1) as f32);
    // Several slices selected and no point: they all move.
    if point.is_none() && many.len() > 1 {
        move_slices(mapping, &many, [ddx, ddy]);
        return;
    }
    let Some(s) = mapping.surfaces.iter_mut().find(|s| Some(s.id) == selected && !s.locked) else { return };
    match point {
        Some(p) => {
            if let Some(q) = points_mut(s).into_iter().nth(p) {
                q[0] += ddx;
                q[1] += ddy;
            }
        }
        None => s.translate(ddx, ddy),
    }
}

/// The selected point's X / Y in output pixels (Studio properties). Nothing when no point of `s` is selected.
pub(crate) fn point_fields(ui: &mut egui::Ui, s: &mut Surface, size: (u32, u32)) {
    let Some((_, i)) = selected_point(ui.ctx()).filter(|(sid, _)| *sid == s.id) else {
        hint(ui, "Click a point on the canvas (or press Tab) to place it by number.");
        return;
    };
    let locked = s.locked;
    let Some(p) = points_mut(s).into_iter().nth(i) else { return };
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("Point {}", i + 1)).strong());
        for (axis, (name, len)) in [("X", size.0), ("Y", size.1)].into_iter().enumerate() {
            let mut v = p[axis] * len as f32;
            let r = ui.add_enabled(!locked, egui::DragValue::new(&mut v).speed(0.25).fixed_decimals(1).prefix(format!("{name} ")).suffix(" px"));
            if r.changed() {
                p[axis] = v / len.max(1) as f32;
            }
        }
    });
    hint(ui, "Tab / Shift-Tab: next / previous point · arrows nudge it (Shift = 10 px) · Esc: the whole slice again");
}

/// With Snap on: the selected point of slice `sid` jumps onto another slice's point within `radius` (output units,
/// per axis). Returns true if it snapped.
pub(crate) fn snap_to_others(mapping: &mut Mapping, sid: u64, index: usize, radius: [f32; 2]) -> bool {
    let others: Vec<[f32; 2]> = mapping.surfaces.iter_mut().filter(|s| s.id != sid).flat_map(|s| points_mut(s).into_iter().map(|p| *p).collect::<Vec<_>>()).collect();
    let Some(s) = mapping.surfaces.iter_mut().find(|s| s.id == sid) else { return false };
    let Some(p) = points_mut(s).into_iter().nth(index) else { return false };
    let near = others.iter().filter(|q| (q[0] - p[0]).abs() <= radius[0] && (q[1] - p[1]).abs() <= radius[1]).min_by(|a, b| {
        let d = |q: &[f32; 2]| (q[0] - p[0]).hypot(q[1] - p[1]);
        d(a).total_cmp(&d(b))
    });
    match near {
        Some(q) => {
            *p = *q;
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_click_selects_several_and_they_move_together() {
        let ctx = egui::Context::default();
        let mut primary = None;
        click_slice(&ctx, &mut primary, 1, false);
        click_slice(&ctx, &mut primary, 2, true);
        click_slice(&ctx, &mut primary, 3, true);
        assert_eq!(selected_slices(&ctx, primary), vec![1, 2, 3]);
        click_slice(&ctx, &mut primary, 2, true);
        assert_eq!(selected_slices(&ctx, primary), vec![1, 3], "Shift-click again takes one out");
        let mut m = Mapping::default();
        for id in 1..=3 {
            m.add_surface(id);
        }
        m.surfaces[2].locked = true;
        let before: Vec<[f32; 2]> = m.surfaces.iter().map(|s| s.corners[0]).collect();
        move_slices(&mut m, &selected_slices(&ctx, primary), [0.1, 0.0]);
        assert!((m.surfaces[0].corners[0][0] - before[0][0] - 0.1).abs() < 1e-6);
        assert_eq!(m.surfaces[1].corners[0], before[1], "not selected");
        assert_eq!(m.surfaces[2].corners[0], before[2], "locked");
        click_slice(&ctx, &mut primary, 2, false);
        assert_eq!(selected_slices(&ctx, primary), vec![2], "a plain click selects only that one");
    }

    #[test]
    fn a_point_snaps_onto_a_neighbour_corner() {
        let mut m = Mapping::default();
        let a = m.add_surface(1).id;
        m.add_surface(2);
        m.surfaces[0].corners[1] = [0.501, 0.2];
        m.surfaces[1].corners[0] = [0.5, 0.2];
        assert!(snap_to_others(&mut m, a, 1, [0.01, 0.01]));
        assert_eq!(m.surfaces[0].corners[1], [0.5, 0.2]);
        m.surfaces[0].corners[2] = [0.9, 0.9];
        assert!(!snap_to_others(&mut m, a, 2, [0.001, 0.001]), "nothing close");
    }
}

/// Properties for several selected slices at once: opacity, blend, on/off and lock for all of them (the first
/// one's values are shown).
pub(crate) fn group_settings(ui: &mut egui::Ui, mapping: &mut Mapping, ids: &[u64]) {
    let Some(first) = mapping.surfaces.iter().find(|s| ids.contains(&s.id)).cloned() else { return };
    ui.label(RichText::new(format!("{} {} slices selected", icon::SELECTION_ALL, ids.len())).strong());
    hint(ui, "Changes here apply to all of them · drag one to move them all · arrows nudge them · Shift-click to add or remove one");
    let all = |mapping: &mut Mapping, f: &dyn Fn(&mut Surface)| mapping.surfaces.iter_mut().filter(|s| ids.contains(&s.id)).for_each(f);
    let mut on = first.enabled;
    if toggle(ui, &mut on, "ON").changed() {
        all(mapping, &|s| s.enabled = on);
    }
    let mut locked = first.locked;
    let text = if locked { label(icon::LOCK, "Locked") } else { label(icon::LOCK_OPEN, "Lock") };
    if toggle(ui, &mut locked, text).changed() {
        all(mapping, &|s| s.locked = locked);
    }
    let mut opacity = first.opacity;
    if ui.horizontal(|ui| Level::percent(&mut opacity, "Opacity").default(1.0).show(ui)).inner.changed() {
        all(mapping, &|s| s.opacity = opacity);
    }
    let mut blend = first.blend;
    ui.horizontal(|ui| {
        ui.label("Blend");
        choice_buttons(ui, &mut blend, |_, _| {});
    });
    if blend != first.blend {
        all(mapping, &|s| s.blend = blend);
    }
}
