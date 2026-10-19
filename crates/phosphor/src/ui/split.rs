//! Two regions with a draggable divider (the stages of the areas): each side gets a fixed rectangle, its
//! content clipped to it, so a long panel scrolls inside its half instead of pushing the other one away.
//! The divider position is remembered per id; the second side keeps a minimum size (layer cards need room).

use eframe::egui;

/// Splits the remaining space top / bottom. `bottom` starts at `default` (fraction of the height). `with` is
/// handed to each side in turn (usually the app state).
pub(crate) fn split_vertical<T>(ui: &mut egui::Ui, id: &str, default: f32, min_second: f32, with: &mut T, top: impl FnOnce(&mut egui::Ui, &mut T), bottom: impl FnOnce(&mut egui::Ui, &mut T)) {
    split(ui, id, default, min_second, true, with, top, bottom);
}

/// Splits the remaining space left / right. `second` (the right side) starts at `default` of the width.
pub(crate) fn split_horizontal<T>(ui: &mut egui::Ui, id: &str, default: f32, min_second: f32, with: &mut T, first: impl FnOnce(&mut egui::Ui, &mut T), second: impl FnOnce(&mut egui::Ui, &mut T)) {
    split(ui, id, default, min_second, false, with, first, second);
}

fn split<T>(ui: &mut egui::Ui, id: &str, default: f32, min_second: f32, vertical: bool, with: &mut T, first: impl FnOnce(&mut egui::Ui, &mut T), second: impl FnOnce(&mut egui::Ui, &mut T)) {
    let full = ui.available_rect_before_wrap();
    let key = egui::Id::new(("split", id));
    let mut frac: f32 = ui.ctx().data(|d| d.get_temp(key)).unwrap_or(default);
    const GAP: f32 = 8.0;
    let len = if vertical { full.height() } else { full.width() };
    // The second side never gets narrower than it needs (`min_second`, while the first keeps 80), nor the first
    // than 80.
    let lo = min_second.max(80.0).min((len - 80.0).max(len * 0.3));
    let second_len = (len * frac).clamp(lo, (len - 80.0).max(lo));
    let (a, divider, b) = if vertical {
        let cut = full.bottom() - second_len;
        (
            egui::Rect::from_min_max(full.min, egui::pos2(full.right(), cut - GAP / 2.0)),
            egui::Rect::from_min_max(egui::pos2(full.left(), cut - GAP / 2.0), egui::pos2(full.right(), cut + GAP / 2.0)),
            egui::Rect::from_min_max(egui::pos2(full.left(), cut + GAP / 2.0), full.max),
        )
    } else {
        let cut = full.right() - second_len;
        (
            egui::Rect::from_min_max(full.min, egui::pos2(cut - GAP / 2.0, full.bottom())),
            egui::Rect::from_min_max(egui::pos2(cut - GAP / 2.0, full.top()), egui::pos2(cut + GAP / 2.0, full.bottom())),
            egui::Rect::from_min_max(egui::pos2(cut + GAP / 2.0, full.top()), full.max),
        )
    };
    let handle = ui.interact(divider, key.with("divider"), egui::Sense::drag());
    if handle.hovered() || handle.dragged() {
        ui.ctx().set_cursor_icon(if vertical { egui::CursorIcon::ResizeVertical } else { egui::CursorIcon::ResizeHorizontal });
    }
    if handle.dragged() {
        let d = handle.drag_delta();
        frac = ((second_len - if vertical { d.y } else { d.x }) / len.max(1.0)).clamp(0.1, 0.9);
        ui.ctx().data_mut(|m| m.insert_temp(key, frac));
    }
    let color = if handle.hovered() || handle.dragged() { egui::Color32::from_gray(90) } else { egui::Color32::from_gray(40) };
    let line = if vertical {
        [egui::pos2(divider.left(), divider.center().y), egui::pos2(divider.right(), divider.center().y)]
    } else {
        [egui::pos2(divider.center().x, divider.top()), egui::pos2(divider.center().x, divider.bottom())]
    };
    ui.painter().line_segment(line, egui::Stroke::new(1.0, color));
    let layout = egui::Layout::top_down(egui::Align::Min);
    let clip = ui.clip_rect();
    let mut a_ui = ui.new_child(egui::UiBuilder::new().max_rect(a).layout(layout));
    a_ui.set_clip_rect(a.intersect(clip));
    first(&mut a_ui, with);
    let mut b_ui = ui.new_child(egui::UiBuilder::new().max_rect(b).layout(layout));
    b_ui.set_clip_rect(b.intersect(clip));
    second(&mut b_ui, with);
    ui.allocate_rect(full, egui::Sense::hover());
}

/// A side column's width: (default, min, max) in points.
pub(crate) type ColumnSize = (f32, f32, f32);

/// Left column · middle · right column (the areas' Library · stage · Inspector). Side widths only change when you
/// drag their inner edge (remembered per id); content is clipped inside, so a wide widget can never push a column
/// wider. `None` hides that side.
#[allow(clippy::too_many_arguments)]
pub(crate) fn columns<T>(
    ui: &mut egui::Ui,
    id: &str,
    left: Option<ColumnSize>,
    right: Option<ColumnSize>,
    with: &mut T,
    left_ui: impl FnOnce(&mut egui::Ui, &mut T),
    middle_ui: impl FnOnce(&mut egui::Ui, &mut T),
    right_ui: impl FnOnce(&mut egui::Ui, &mut T),
) {
    let full = ui.available_rect_before_wrap();
    const EDGE: f32 = 6.0;
    let fill = ui.visuals().panel_fill;
    let line = ui.visuals().widgets.noninteractive.bg_stroke.color;
    let width = |side: &str, size: Option<ColumnSize>| -> f32 {
        let Some((default, min, max)) = size else { return 0.0 };
        let key = egui::Id::new(("columns", id, side));
        let w: f32 = ui.ctx().data(|d| d.get_temp(key)).unwrap_or(default);
        w.clamp(min, max.max(min))
    };
    let lw = width("left", left);
    let rw = width("right", right);
    let l_rect = egui::Rect::from_min_size(full.min, egui::vec2(lw, full.height()));
    let r_rect = egui::Rect::from_min_max(egui::pos2(full.right() - rw, full.top()), full.max);
    let m_rect = egui::Rect::from_min_max(egui::pos2(l_rect.right(), full.top()), egui::pos2(r_rect.left(), full.bottom()));
    // Drag handles on the inner edges.
    for (side, size, edge_x, sign) in [("left", left, l_rect.right(), 1.0f32), ("right", right, r_rect.left(), -1.0)] {
        let Some((_, min, max)) = size else { continue };
        let key = egui::Id::new(("columns", id, side));
        let handle = egui::Rect::from_min_max(egui::pos2(edge_x - EDGE / 2.0, full.top()), egui::pos2(edge_x + EDGE / 2.0, full.bottom()));
        let r = ui.interact(handle, key.with("edge"), egui::Sense::drag());
        if r.hovered() || r.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if r.dragged() {
            let current = if side == "left" { lw } else { rw };
            let w = (current + sign * r.drag_delta().x).clamp(min, max.max(min));
            ui.ctx().data_mut(|d| d.insert_temp(key, w));
        }
        let c = if r.hovered() || r.dragged() { line.gamma_multiply(2.0) } else { line };
        ui.painter().vline(edge_x, full.y_range(), egui::Stroke::new(1.0, c));
    }
    let clip = ui.clip_rect();
    let layout = egui::Layout::top_down(egui::Align::Min);
    let mut region = |rect: egui::Rect, pad: f32, add: &mut dyn FnMut(&mut egui::Ui)| {
        if rect.width() < 1.0 {
            return;
        }
        ui.painter().rect_filled(rect, 0.0, fill);
        let inner = rect.shrink2(egui::vec2(pad, pad.min(6.0)));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(layout));
        child.set_clip_rect(inner.intersect(clip));
        add(&mut child);
    };
    let mut left_ui = Some(left_ui);
    let mut middle_ui = Some(middle_ui);
    let mut right_ui = Some(right_ui);
    if left.is_some() {
        region(l_rect, 10.0, &mut |ui| (left_ui.take().unwrap())(ui, with));
    }
    region(m_rect, 6.0, &mut |ui| (middle_ui.take().unwrap())(ui, with));
    if right.is_some() {
        region(r_rect, 10.0, &mut |ui| (right_ui.take().unwrap())(ui, with));
    }
    ui.allocate_rect(full, egui::Sense::hover());
}
