//! What's selected on the mapping canvas: several slices (Shift-click adds or removes one; they move, nudge and
//! change together) and one point. The selected point (MadMapper's fine-tuning):  (MadMapper's fine-tuning): click a corner, mesh or line point to select
//! it, Tab / Shift-Tab to the next / previous point, arrows nudge it (Shift = 10 px), type its X / Y in output pixels,
//! and with Snap on a dragged point lands on another slice's point when it comes close.

use phosphor_engine::mapping::{Mapping, Surface};

use super::*;

const KEY: &str = "phosphor_mapping_point";
const MULTI: &str = "phosphor_mapping_multi";

/// The slices selected on the mapping canvas (`ui::Selection` under one key).
fn slices() -> crate::ui::Selection {
    crate::ui::Selection::new(MULTI)
}

/// Every selected slice: the set, or just `primary` when nothing else is selected.
pub(crate) fn selected_slices(ctx: &egui::Context, primary: Option<u64>) -> Vec<u64> {
    slices().all(ctx, primary)
}

/// A click on slice `id`: with Shift it joins (or leaves) the selection, else it's the only one selected.
pub(crate) fn click_slice(ctx: &egui::Context, primary: &mut Option<u64>, id: u64, shift: bool) {
    slices().click(ctx, primary, id, shift);
}

/// Selects exactly `ids` (a slice group): the last one becomes the primary.
pub(crate) fn select_slices(ctx: &egui::Context, primary: &mut Option<u64>, ids: Vec<u64>) {
    slices().set(ctx, primary, ids);
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
        help(ui, "Click a point on the canvas (or press Tab) to place it by number.");
        return;
    };
    let locked = s.locked;
    let Some(p) = points_mut(s).into_iter().nth(i) else { return };
    crate::ui::toolbar(ui, |ui| {
        crate::ui::heading(ui, format!("Point {}", i + 1));
        for (axis, (name, len)) in [("X", size.0), ("Y", size.1)].into_iter().enumerate() {
            let mut v = p[axis] * len as f32;
            let r = ui.add_enabled(!locked, egui::DragValue::new(&mut v).speed(0.25).fixed_decimals(1).prefix(format!("{name} ")).suffix(" px"));
            if r.changed() {
                p[axis] = v / len.max(1) as f32;
            }
        }
    });
    help(ui, "Tab / Shift-Tab: next / previous point · arrows nudge it (Shift = 10 px) · Esc: the whole slice again");
}

/// A drag with Snap on, as it goes: where each point of the thing dragged would be without snapping (`raw`, which
/// follows the mouse exactly) and where it was put last frame (`shown`).
#[derive(Clone, Default)]
pub(crate) struct SnapDrag {
    id: u64,
    raw: Vec<[f32; 2]>,
    shown: Vec<[f32; 2]>,
}

/// One frame of a drag with Snap on. `cur` = the points of the slice or mask after this frame's drag was applied
/// (to where they were shown last frame); returns where they should be shown now.
///
/// The mouse's whole travel is kept in `raw`, and only what's shown is snapped. (Snapping the stored point every
/// frame threw away each small move: the point stuck until the mouse had gone half a grid cell in one frame, then
/// jumped, which felt like lag and like it snapped where it wanted. Owner, 2026-11-16.)
/// - One point dragged: it lands on another slice's point within `radius`, else on the grid.
/// - The whole thing dragged: it moves as one, its first point on the grid (its shape is never bent).
/// - Anything else (a scale, a rotate), or `free` (Alt held): no snapping.
pub(crate) fn snap_step(state: &mut Option<SnapDrag>, id: u64, cur: &[[f32; 2]], others: &[[f32; 2]], radius: [f32; 2], grid: f32, free: bool) -> Vec<[f32; 2]> {
    let on_grid = |p: [f32; 2]| [(p[0] * grid).round() / grid, (p[1] * grid).round() / grid];
    let st = match state {
        Some(st) if st.id == id && st.shown.len() == cur.len() => st,
        _ => {
            *state = Some(SnapDrag { id, raw: cur.to_vec(), shown: cur.to_vec() });
            return cur.to_vec();
        }
    };
    let moved: Vec<usize> = (0..cur.len()).filter(|&k| (cur[k][0] - st.shown[k][0]).abs() > 1e-7 || (cur[k][1] - st.shown[k][1]).abs() > 1e-7).collect();
    for &k in &moved {
        st.raw[k][0] += cur[k][0] - st.shown[k][0];
        st.raw[k][1] += cur[k][1] - st.shown[k][1];
    }
    let mut out = st.raw.clone();
    if !free {
        if let [k] = moved[..] {
            let p = st.raw[k];
            let near = others.iter().filter(|q| (q[0] - p[0]).abs() <= radius[0] && (q[1] - p[1]).abs() <= radius[1]).min_by(|a, b| (a[0] - p[0]).hypot(a[1] - p[1]).total_cmp(&(b[0] - p[0]).hypot(b[1] - p[1])));
            out[k] = near.copied().unwrap_or_else(|| on_grid(p));
        } else if moved.len() == cur.len() && cur.len() > 1 {
            let to = on_grid(st.raw[0]);
            let shift = [to[0] - st.raw[0][0], to[1] - st.raw[0][1]];
            for p in &mut out {
                *p = [p[0] + shift[0], p[1] + shift[1]];
            }
        } else if moved.is_empty() {
            // Nothing moved this frame: what's shown stays as it is.
            out.clone_from(&st.shown);
        }
    }
    st.shown.clone_from(&out);
    out
}

/// The points of slice or mask `id` (a slice's corners / mesh / line points; a mask's outline).
pub(crate) fn shape_points(mapping: &mut Mapping, id: u64) -> Vec<&mut [f32; 2]> {
    if let Some(i) = mapping.surfaces.iter().position(|s| s.id == id) {
        return points_mut(&mut mapping.surfaces[i]);
    }
    mapping.masks.iter_mut().find(|m| m.id == id).map_or_else(Vec::new, |m| m.points.iter_mut().collect())
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

    /// A slow drag with Snap on follows the mouse: after 60 small moves the point is within half a grid cell of
    /// where the mouse took it (it used to stay where it began), a whole slice keeps its shape, and Alt is free.
    #[test]
    fn snapping_follows_a_slow_drag() {
        let grid = 64.0;
        let mut state = None;
        let mut shown = vec![[0.25, 0.25], [0.75, 0.25], [0.75, 0.75], [0.25, 0.75]];
        let first = snap_step(&mut state, 1, &shown, &[], [0.004, 0.004], grid, false);
        assert_eq!(first, shown, "nothing moves when the drag starts");
        // One corner, 0.001 a frame (a fifteenth of a grid cell).
        for _ in 0..60 {
            let mut cur = shown.clone();
            cur[0][0] += 0.001;
            shown = snap_step(&mut state, 1, &cur, &[], [0.004, 0.004], grid, false);
        }
        assert!((shown[0][0] - 0.31).abs() <= 0.5 / grid + 1e-5, "followed the mouse: {}", shown[0][0]);
        assert!(((shown[0][0] * grid).round() / grid - shown[0][0]).abs() < 1e-6, "on the grid");
        assert_eq!(shown[1], [0.75, 0.25], "the other corners stay");
        // Close to another slice's point, it lands on it.
        let mut cur = shown.clone();
        cur[0][0] += 0.02;
        // (The mouse has taken it to 0.33; a neighbor's point sits 0.002 from there, inside the snap distance.)
        let other = [[0.332, 0.2502]];
        shown = snap_step(&mut state, 1, &cur, &other, [0.004, 0.004], grid, false);
        assert_eq!(shown[0], other[0]);
        // The whole slice dragged slowly: it moves as one, its shape kept.
        let mut state = None;
        let mut shown = vec![[0.2, 0.2], [0.53, 0.21], [0.5, 0.6], [0.22, 0.58]];
        let shape: Vec<[f32; 2]> = shown.iter().map(|p| [p[0] - shown[0][0], p[1] - shown[0][1]]).collect();
        snap_step(&mut state, 2, &shown, &[], [0.004, 0.004], grid, false);
        for _ in 0..50 {
            let cur: Vec<[f32; 2]> = shown.iter().map(|p| [p[0] + 0.0013, p[1]]).collect();
            shown = snap_step(&mut state, 2, &cur, &[], [0.004, 0.004], grid, false);
        }
        assert!((shown[0][0] - 0.265).abs() <= 0.5 / grid + 1e-5, "{}", shown[0][0]);
        for (p, d) in shown.iter().zip(&shape) {
            assert!((p[0] - shown[0][0] - d[0]).abs() < 1e-5 && (p[1] - shown[0][1] - d[1]).abs() < 1e-5, "shape kept");
        }
        // Alt: exactly where the mouse is.
        let mut cur = shown.clone();
        cur[1][0] += 0.0007;
        let want = cur[1][0];
        // (The free point is the raw one: the mouse's travel so far, not the snapped place plus the move.)
        let free = snap_step(&mut state, 2, &cur, &[], [0.004, 0.004], grid, true);
        assert!((free[1][0] - want).abs() < 0.02);
    }
}

/// Properties for several selected slices at once: opacity, blend, on/off and lock for all of them (the first
/// one's values are shown).
pub(crate) fn group_settings(ui: &mut egui::Ui, mapping: &mut Mapping, ids: &[u64]) {
    let Some(first) = mapping.surfaces.iter().find(|s| ids.contains(&s.id)).cloned() else { return };
    crate::ui::heading_help(ui, format!("{} {} slices selected", icon::SELECTION_ALL, ids.len()), "Changes here apply to all of them · drag one to move them all · arrows nudge them · Shift-click to add or remove one");
    let all = |mapping: &mut Mapping, f: &dyn Fn(&mut Surface)| mapping.surfaces.iter_mut().filter(|s| ids.contains(&s.id)).for_each(f);
    let mut on = first.enabled;
    if toggle(ui, &mut on, "ON").changed() {
        all(mapping, &|s| s.enabled = on);
    }
    let mut locked = first.locked;
    if toggle(ui, &mut locked, label(icon::LOCK, "Lock")).changed() {
        all(mapping, &|s| s.locked = locked);
    }
    let mut opacity = first.opacity;
    if ui.horizontal(|ui| Level::percent(&mut opacity, "Opacity").default(1.0).show(ui)).inner.changed() {
        all(mapping, &|s| s.opacity = opacity);
    }
    let mut blend = first.blend;
    crate::ui::field(ui, "Blend", |ui| {
        choice_buttons(ui, &mut blend, |_, _| {});
    });
    if blend != first.blend {
        all(mapping, &|s| s.blend = blend);
    }
}
