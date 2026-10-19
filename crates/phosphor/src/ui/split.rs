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
