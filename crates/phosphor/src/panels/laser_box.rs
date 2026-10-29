//! The Laser Designer's transform box: a frame round the picked shape (turned with it), corner handles that size it
//! evenly from its centre, a round handle above that turns it, and its middle dragged to move it. Laser shapes are a
//! place, a size and a turn (not points), so this sets those; `panels::mapping` has the point-shape box.

use phosphor_engine::LaserShape;
use phosphor_engine::laser::Path;

use super::*;

/// What a drag on the box changed.
pub(crate) enum BoxChange {
    Size(f32),
    Turn(f32),
    Move(f32, f32),
}

/// The box round `s` as drawn now (`paths`: its lines at this moment), on a canvas where `to_screen` / `from_screen`
/// convert scan-area points (-1..1, up = +y). `turn`: its angle now (radians, with its spin).
pub(crate) fn shape_box(ui: &mut egui::Ui, frame: egui::Rect, s: &LaserShape, paths: &[Path], turn: f32, to_screen: impl Fn(f32, f32) -> egui::Pos2, from_screen: impl Fn(egui::Pos2) -> (f32, f32)) -> Option<BoxChange> {
    let (lo, hi) = local_bounds(s, paths, turn)?;
    let (sin, cos) = turn.sin_cos();
    let world = |l: [f32; 2]| to_screen(s.x + l[0] * cos - l[1] * sin, s.y + l[0] * sin + l[1] * cos);
    let corners = [[lo[0], lo[1]], [hi[0], lo[1]], [hi[0], hi[1]], [lo[0], hi[1]]].map(world);
    let colour = egui::Color32::from_rgb(90, 220, 255).gamma_multiply(0.7);
    let painter = ui.painter_at(frame.expand(30.0));
    painter.add(egui::Shape::closed_line(corners.to_vec(), egui::Stroke::new(1.0, colour)));
    let centre = to_screen(s.x, s.y);
    let id = egui::Id::new(("laser_box", s.id));
    let mut change = None;
    // Inside: drag to move.
    let body = egui::Rect::from_points(&corners).shrink(8.0);
    if body.is_positive() {
        let r = ui.interact(body, id.with("move"), egui::Sense::drag()).on_hover_cursor(egui::CursorIcon::Grab);
        if r.dragged()
            && let Some(now) = r.interact_pointer_pos()
        {
            let (x, y) = from_screen(now);
            let (bx, by) = from_screen(now - r.drag_delta());
            change = Some(BoxChange::Move((s.x + x - bx).clamp(-1.0, 1.0), (s.y + y - by).clamp(-1.0, 1.0)));
        }
    }
    // Corners: size evenly from the centre (how much farther from it the pointer got).
    for (k, at) in corners.iter().enumerate() {
        let mut d = [0.0; 2];
        let r = crate::panels::mapping::point_handle(ui, id.with(("corner", k)), *at, frame, colour, true, &mut d);
        if r.dragged()
            && let Some(now) = r.interact_pointer_pos()
        {
            let before = (now - r.drag_delta() - centre).length();
            if before > 2.0 {
                change = Some(BoxChange::Size((s.size * (now - centre).length() / before).clamp(0.01, 1.0)));
            }
        }
        r.on_hover_text("Drag to size it (from its centre)");
    }
    // Turn: a round handle above its top edge.
    let top = corners[2].lerp(corners[3], 0.5);
    let out = (top - centre).normalized() * 26.0;
    let knob = top + if out.is_finite() { out } else { egui::vec2(0.0, -26.0) };
    painter.line_segment([top, knob], egui::Stroke::new(1.0, colour));
    let r = ui.interact(egui::Rect::from_center_size(knob, egui::vec2(18.0, 18.0)), id.with("turn"), egui::Sense::drag());
    let lit = r.hovered() || r.dragged();
    painter.circle(knob, if lit { 7.0 } else { 5.5 }, if lit { egui::Color32::WHITE } else { egui::Color32::BLACK }, egui::Stroke::new(1.5, colour));
    if r.dragged()
        && let Some(now) = r.interact_pointer_pos()
    {
        let (before, after) = (now - r.drag_delta() - centre, now - centre);
        // (Screen y runs down: a clockwise drag turns it clockwise.)
        let delta = -(after.y.atan2(after.x) - before.y.atan2(before.x)).to_degrees();
        let turned = (s.rotate + delta + 180.0).rem_euclid(360.0) - 180.0;
        change = Some(BoxChange::Turn(turned));
    }
    r.on_hover_text("Drag to turn it");
    change
}

/// The shape's lines measured from its centre, unturned: (lowest corner, highest corner). None when it draws nothing.
pub(crate) fn local_bounds(s: &LaserShape, paths: &[Path], turn: f32) -> Option<([f32; 2], [f32; 2])> {
    let (sin, cos) = turn.sin_cos();
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for (p, _) in paths.iter().flat_map(|p| &p.points) {
        let (dx, dy) = (p[0] - s.x, p[1] - s.y);
        let l = [dx * cos + dy * sin, -dx * sin + dy * cos];
        lo = [lo[0].min(l[0]), lo[1].min(l[1])];
        hi = [hi[0].max(l[0]), hi[1].max(l[1])];
    }
    // A dot or a flat line still gets a box to hold.
    let pad = |lo: &mut [f32; 2], hi: &mut [f32; 2], i: usize| {
        if hi[i] - lo[i] < 0.04 {
            let m = (lo[i] + hi[i]) * 0.5;
            (lo[i], hi[i]) = (m - 0.02, m + 0.02);
        }
    };
    if lo[0] > hi[0] {
        return None;
    }
    pad(&mut lo, &mut hi, 0);
    pad(&mut lo, &mut hi, 1);
    Some((lo, hi))
}
