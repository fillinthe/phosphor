//! Mapping Studio: a full workspace for projection mapping. Left: every output with its slices and masks. Middle:
//! the output canvas (pinch / ⌘-scroll to zoom, two-finger scroll or middle-drag to pan, snap grid, arrow keys
//! nudge) and the input view (drag each slice's source rectangle on the picture). Right: the selected item.

use std::collections::HashMap;

use egui::UiBuilder;
use phosphor_engine::mapping::MaskShape;

use super::*;

/// How the middle of the studio shows the selected output.
#[derive(Clone, Copy, PartialEq, Default)]
enum View {
    #[default]
    Split,
    Output,
    Input,
}

#[derive(Clone, Copy, Default)]
struct ViewState {
    view: View,
    zoom: f32,
    pan: egui::Vec2,
    snap: bool,
}

enum TreeAction {
    AddSlice(u64),
    AddMask(u64, MaskShape),
    Duplicate(u64, u64),
    Delete(u64, u64),
}

pub(crate) fn studio(ui: &mut egui::Ui, st: &mut State) {
    st.comp.ensure_outputs();
    if !st.comp.outputs.iter().any(|o| o.id == st.mapping_output) {
        st.mapping_output = st.primary_output();
    }
    let full = ui.available_rect_before_wrap();
    let left_w = 230.0f32.min(full.width() * 0.22);
    let right_w = 340.0f32.min(full.width() * 0.3);
    let left = egui::Rect::from_min_size(full.min, egui::vec2(left_w, full.height()));
    let right = egui::Rect::from_min_max(egui::pos2(full.right() - right_w, full.top()), full.max);
    let center = egui::Rect::from_min_max(egui::pos2(left.right() + 8.0, full.top()), egui::pos2(right.left() - 8.0, full.bottom()));
    let layout = egui::Layout::top_down(egui::Align::Min);
    tree(&mut ui.new_child(UiBuilder::new().max_rect(left).layout(layout)), st);
    canvases(&mut ui.new_child(UiBuilder::new().max_rect(center).layout(layout)), st);
    properties(&mut ui.new_child(UiBuilder::new().max_rect(right).layout(layout)), st);
    let p = ui.painter();
    p.vline(left.right() + 4.0, full.y_range(), Stroke::new(1.0, Color32::from_gray(40)));
    p.vline(right.left() - 4.0, full.y_range(), Stroke::new(1.0, Color32::from_gray(40)));
    ui.allocate_rect(full, Sense::hover());
}

/// Outputs, each with its slices and masks.
fn tree(ui: &mut egui::Ui, st: &mut State) {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Outputs").strong());
        if ui.small_button(label(icon::PLUS, "Output")).on_hover_text("Another projector or screen").clicked() {
            st.mapping_output = st.comp.add_output();
        }
    });
    venue_menu(ui, st);
    ui.separator();
    let State { comp, selected_surface, mapping_output, .. } = st;
    egui::ScrollArea::vertical().id_salt("studio_tree").auto_shrink(false).show(ui, |ui| {
        for o in comp.outputs.iter_mut() {
            let current = *mapping_output == o.id;
            ui.horizontal(|ui| {
                ui.toggle_value(&mut o.mapping.enabled, RichText::new("ON").small()).on_hover_text("Mapping on for this output");
                if ui.selectable_label(current, RichText::new(format!("{} {}", icon::PROJECTOR_SCREEN, o.name)).strong()).clicked() {
                    *mapping_output = o.id;
                }
            });
            if !current {
                continue;
            }
            for s in &mut o.mapping.surfaces {
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    let glyph = if s.enabled { icon::EYE } else { icon::EYE_SLASH };
                    ui.toggle_value(&mut s.enabled, RichText::new(glyph).small());
                    let r = ui.selectable_label(*selected_surface == Some(s.id), format!("{} {}", icon::POLYGON, s.name));
                    if r.clicked() {
                        *selected_surface = Some(s.id);
                    }
                    r.right_menu(|ui| {
                        if ui.button(label(icon::COPY, "Duplicate")).clicked() {
                            action = Some(TreeAction::Duplicate(o.id, s.id));
                            ui.close();
                        }
                        if ui.button(label(icon::TRASH, "Delete")).clicked() {
                            action = Some(TreeAction::Delete(o.id, s.id));
                            ui.close();
                        }
                    });
                });
            }
            for m in &mut o.mapping.masks {
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    let glyph = if m.enabled { icon::EYE } else { icon::EYE_SLASH };
                    ui.toggle_value(&mut m.enabled, RichText::new(glyph).small());
                    let r = ui.selectable_label(*selected_surface == Some(m.id), RichText::new(format!("{} {}", icon::SELECTION, m.name)).color(MASK_COLOR));
                    if r.clicked() {
                        *selected_surface = Some(m.id);
                    }
                    r.right_menu(|ui| {
                        if ui.button(label(icon::TRASH, "Delete")).clicked() {
                            action = Some(TreeAction::Delete(o.id, m.id));
                            ui.close();
                        }
                    });
                });
            }
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                if ui.small_button(label(icon::PLUS, "Slice")).clicked() {
                    action = Some(TreeAction::AddSlice(o.id));
                }
                ui.menu_button(label(icon::PLUS, "Mask"), |ui| {
                    for &shape in <MaskShape as phosphor_engine::Choice>::OPTIONS {
                        if ui.button(shape.name()).clicked() {
                            action = Some(TreeAction::AddMask(o.id, shape));
                            ui.close();
                        }
                    }
                });
            });
            ui.add_space(6.0);
        }
    });
    let canvas = (st.comp.width, st.comp.height);
    match action {
        Some(TreeAction::AddSlice(oid)) => {
            let id = st.comp.new_id();
            if let Some(o) = st.comp.output_mut(oid) {
                o.mapping.add_surface(id);
                o.mapping.enabled = true;
            }
            st.selected_surface = Some(id);
        }
        Some(TreeAction::AddMask(oid, shape)) => {
            let id = st.comp.new_id();
            if let Some(o) = st.comp.output_mut(oid) {
                let (w, h) = o.size(canvas);
                o.mapping.add_mask_shape(id, shape, w as f32 / h.max(1) as f32);
                o.mapping.enabled = true;
            }
            st.selected_surface = Some(id);
        }
        Some(TreeAction::Duplicate(oid, sid)) => {
            let id = st.comp.new_id();
            if let Some(o) = st.comp.output_mut(oid)
                && let Some(i) = o.mapping.surfaces.iter().position(|s| s.id == sid)
            {
                let mut copy = o.mapping.surfaces[i].clone();
                copy.id = id;
                copy.name = format!("{} copy", copy.name);
                copy.translate(0.03, 0.03);
                o.mapping.surfaces.insert(i + 1, copy);
                st.selected_surface = Some(id);
            }
        }
        Some(TreeAction::Delete(oid, id)) => {
            if let Some(o) = st.comp.output_mut(oid) {
                o.mapping.surfaces.retain(|s| s.id != id);
                o.mapping.masks.retain(|m| m.id != id);
            }
            if st.selected_surface == Some(id) {
                st.selected_surface = None;
            }
        }
        None => {}
    }
}

/// The output canvas and/or the input view, with the view toolbar.
fn canvases(ui: &mut egui::Ui, st: &mut State) {
    let oid = st.mapping_output;
    let key = egui::Id::new(("studio_view", oid));
    let mut vs: ViewState = ui.ctx().data(|d| d.get_temp(key)).unwrap_or(ViewState { zoom: 1.0, snap: false, ..Default::default() });
    let tex = st.output_tex(oid);
    let program = st.texture;
    let canvas = (st.comp.width, st.comp.height);
    let mut edit_on_output = st.output.lock().unwrap().get(&oid).is_some_and(|o| o.edit_mapping);
    let State { comp, selected_surface, layer_textures, output, .. } = st;
    let Some(o) = comp.outputs.iter_mut().find(|o| o.id == oid) else { return };
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut vs.view, View::Split, "Output + input");
        ui.selectable_value(&mut vs.view, View::Output, "Output");
        ui.selectable_value(&mut vs.view, View::Input, "Input");
        ui.separator();
        if ui.button(label(icon::CORNERS_OUT, "Fit")).on_hover_text("Show the whole output").clicked() {
            vs.zoom = 1.0;
            vs.pan = egui::Vec2::ZERO;
        }
        hint(ui, format!("{:.0}%", vs.zoom * 100.0));
        ui.toggle_value(&mut vs.snap, label(icon::GRID_FOUR, "Snap")).on_hover_text("Corners and points snap to a 1/64 grid");
        ui.toggle_value(&mut o.mapping.test_pattern, "Alignment grid");
        if ui.checkbox(&mut edit_on_output, "Edit on output (E)").changed() {
            output.lock().unwrap().entry(oid).or_default().edit_mapping = edit_on_output;
        }
    });
    hint(ui, "Pinch or ⌘-scroll to zoom, two-finger scroll or middle-drag to pan · arrows nudge the selected slice (Shift = 10 px) · double-click a mask edge to add a point");
    let area = ui.available_rect_before_wrap();
    let (out_area, in_area) = match vs.view {
        View::Split => {
            let split = area.top() + area.height() * 0.62;
            (Some(egui::Rect::from_min_max(area.min, egui::pos2(area.right(), split - 4.0))), Some(egui::Rect::from_min_max(egui::pos2(area.left(), split + 4.0), area.max)))
        }
        View::Output => (Some(area), None),
        View::Input => (None, Some(area)),
    };
    if let Some(area) = out_area {
        let (ow, oh) = o.size(canvas);
        let aspect = ow as f32 / oh.max(1) as f32;
        let bg = ui.interact(area, key.with("bg"), Sense::click_and_drag());
        if bg.hovered() {
            let (zoom, scroll, pointer) = ui.ctx().input(|i| (i.zoom_delta(), i.smooth_scroll_delta, i.pointer.hover_pos()));
            if zoom != 1.0 {
                // Zoom about the pointer.
                let base = fit_rect(area.shrink(12.0), aspect);
                let new_zoom = (vs.zoom * zoom).clamp(0.25, 12.0);
                if let Some(p) = pointer {
                    let c = base.center() + vs.pan;
                    vs.pan += (p - c) * (1.0 - new_zoom / vs.zoom);
                }
                vs.zoom = new_zoom;
            } else {
                vs.pan += scroll;
            }
        }
        if bg.dragged_by(egui::PointerButton::Middle) {
            vs.pan += bg.drag_delta();
        }
        if bg.clicked() {
            *selected_surface = None;
        }
        let base = fit_rect(area.shrink(12.0), aspect);
        let frame = egui::Rect::from_center_size(base.center() + vs.pan, base.size() * vs.zoom);
        let mut child = ui.new_child(UiBuilder::new().max_rect(area));
        child.set_clip_rect(area.intersect(ui.clip_rect()));
        let p = child.painter().clone();
        p.rect_filled(area, 4.0, Color32::from_gray(14));
        p.rect_filled(frame, 0.0, Color32::BLACK);
        if let Some(t) = tex {
            paint_texture(&p, t, frame);
        }
        if vs.snap {
            for k in 1..16 {
                let f = k as f32 / 16.0;
                let stroke = Stroke::new(1.0, Color32::from_white_alpha(if k % 4 == 0 { 40 } else { 18 }));
                p.vline(frame.left() + f * frame.width(), frame.y_range(), stroke);
                p.hline(frame.x_range(), frame.top() + f * frame.height(), stroke);
            }
        }
        p.rect_stroke(frame, 0.0, Stroke::new(1.0, Color32::from_gray(80)), StrokeKind::Outside);
        let touched = mapping_handles(&mut child, frame, &mut o.mapping, selected_surface, true);
        if touched && vs.snap {
            snap_selected(&mut o.mapping, *selected_surface);
        }
        // Arrow keys nudge the selected slice by one output pixel (ten with Shift).
        if bg.hovered() && !ui.ctx().egui_wants_keyboard_input() {
            let (dx, dy) = ui.ctx().input(|i| {
                let step = if i.modifiers.shift { 10.0 } else { 1.0 };
                let k = |key| i.key_pressed(key) as i32 as f32;
                ((k(egui::Key::ArrowRight) - k(egui::Key::ArrowLeft)) * step, (k(egui::Key::ArrowDown) - k(egui::Key::ArrowUp)) * step)
            });
            if (dx, dy) != (0.0, 0.0)
                && let Some(s) = o.mapping.surfaces.iter_mut().find(|s| Some(s.id) == *selected_surface)
            {
                s.translate(dx / ow.max(1) as f32, dy / oh.max(1) as f32);
            }
        }
    }
    if let Some(area) = in_area {
        input_view(ui, area, o, *selected_surface, program, layer_textures, canvas);
    }
    ui.ctx().data_mut(|d| d.insert_temp(key, vs));
}

/// Rounds the selected slice's corners / mesh points (or mask points) to a 1/64 grid.
fn snap_selected(mapping: &mut phosphor_engine::Mapping, selected: Option<u64>) {
    let snap = |p: &mut [f32; 2]| {
        p[0] = (p[0] * 64.0).round() / 64.0;
        p[1] = (p[1] * 64.0).round() / 64.0;
    };
    for s in mapping.surfaces.iter_mut().filter(|s| Some(s.id) == selected) {
        s.corners.iter_mut().for_each(snap);
        if let Some(m) = &mut s.mesh {
            m.points.iter_mut().for_each(snap);
        }
    }
    for m in mapping.masks.iter_mut().filter(|m| Some(m.id) == selected) {
        m.points.iter_mut().for_each(snap);
    }
}

/// The picture a slice takes its part from, with every slice's input rectangle; the selected one is draggable.
fn input_view(
    ui: &mut egui::Ui,
    area: egui::Rect,
    o: &mut phosphor_engine::OutputConfig,
    selected: Option<u64>,
    program: Option<egui::TextureId>,
    layer_textures: &HashMap<u64, (egui::TextureId, (u32, u32))>,
    canvas: (u32, u32),
) {
    let aspect = canvas.0 as f32 / canvas.1.max(1) as f32;
    let sel_source = o.mapping.surfaces.iter().find(|s| Some(s.id) == selected).and_then(|s| s.layer);
    let tex = sel_source.and_then(|id| layer_textures.get(&id).map(|(t, _)| *t)).or(program);
    let frame = fit_rect(area.shrink(12.0), aspect);
    let p = ui.painter_at(area);
    p.rect_filled(area, 4.0, Color32::from_gray(14));
    p.rect_filled(frame, 0.0, Color32::BLACK);
    if let Some(t) = tex {
        paint_texture(&p, t, frame);
    }
    p.text(area.left_top() + egui::vec2(8.0, 6.0), egui::Align2::LEFT_TOP, "Input: the picture slices take their part from", egui::FontId::proportional(11.0), Color32::from_gray(150));
    let to_screen = |x: f32, y: f32| egui::pos2(frame.left() + x * frame.width(), frame.top() + y * frame.height());
    for s in o.mapping.surfaces.iter_mut().filter(|s| s.layer == sel_source) {
        let is_sel = Some(s.id) == selected;
        let r = egui::Rect::from_min_max(to_screen(s.input[0], s.input[1]), to_screen(s.input[0] + s.input[2], s.input[1] + s.input[3]));
        let color = if is_sel { ACCENT } else { Color32::from_gray(140) };
        p.rect_stroke(r, 0.0, Stroke::new(if is_sel { 2.0 } else { 1.0 }, color), StrokeKind::Middle);
        p.text(r.left_top() + egui::vec2(4.0, 2.0), egui::Align2::LEFT_TOP, &s.name, egui::FontId::proportional(11.0), color);
        if !is_sel {
            continue;
        }
        // Corners resize (keeping the opposite corner), the centre moves.
        let [x0, y0, w, h] = s.input;
        let corners = [(x0, y0), (x0 + w, y0), (x0 + w, y0 + h), (x0, y0 + h)];
        for (i, (cx, cy)) in corners.iter().enumerate() {
            let mut d = [0.0, 0.0];
            point_handle(ui, ui.id().with(("in_corner", s.id, i)), to_screen(*cx, *cy), frame, color, false, &mut d);
            if d != [0.0, 0.0] {
                let (mut l, mut t, mut rr, mut b) = (x0, y0, x0 + w, y0 + h);
                match i {
                    0 => (l, t) = (l + d[0], t + d[1]),
                    1 => (rr, t) = (rr + d[0], t + d[1]),
                    2 => (rr, b) = (rr + d[0], b + d[1]),
                    _ => (l, b) = (l + d[0], b + d[1]),
                }
                let (l, rr) = (l.clamp(0.0, 1.0), rr.clamp(0.0, 1.0));
                let (t, b) = (t.clamp(0.0, 1.0), b.clamp(0.0, 1.0));
                s.input = [l.min(rr), t.min(b), (rr - l).abs().max(0.02), (b - t).abs().max(0.02)];
            }
        }
        let mut d = [0.0, 0.0];
        point_handle(ui, ui.id().with(("in_move", s.id)), r.center(), frame, color, true, &mut d);
        if d != [0.0, 0.0] {
            s.input[0] = (s.input[0] + d[0]).clamp(0.0, 1.0 - s.input[2]);
            s.input[1] = (s.input[1] + d[1]).clamp(0.0, 1.0 - s.input[3]);
        }
    }
}

/// The selected slice or mask, or the output's own settings.
fn properties(ui: &mut egui::Ui, st: &mut State) {
    let oid = st.mapping_output;
    let State { comp, selected_surface, .. } = st;
    let others: Vec<(u64, String)> = comp.outputs.iter().filter(|o| o.id != oid).map(|o| (o.id, o.name.clone())).collect();
    let (layers, groups) = (&mut comp.layers, &mut comp.groups);
    let Some(o) = comp.outputs.iter_mut().find(|o| o.id == oid) else { return };
    egui::ScrollArea::vertical().id_salt("studio_props").auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().slider_width = (ui.available_width() - 150.0).max(80.0);
        if let Some(s) = o.mapping.surfaces.iter_mut().find(|s| Some(s.id) == *selected_surface) {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon::POLYGON).color(ACCENT));
                ui.toggle_value(&mut s.enabled, RichText::new("ON").small().strong());
                ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(f32::INFINITY));
            });
            if ui.small_button(label(icon::CORNERS_OUT, "Corners to the full output")).clicked() {
                s.corners = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
                if s.mesh.is_some() {
                    let (c, r) = s.mesh.as_ref().map_or((3, 3), |m| (m.cols, m.rows));
                    s.mesh = Some(phosphor_engine::mapping::Mesh::from_corners(&s.corners, c, r));
                }
            }
            ui.separator();
            surface_settings(ui, s, layers, groups, &others);
        } else if let Some(m) = o.mapping.masks.iter_mut().find(|m| Some(m.id) == *selected_surface) {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon::SELECTION).color(MASK_COLOR));
                ui.toggle_value(&mut m.enabled, RichText::new("ON").small().strong());
                ui.add(egui::TextEdit::singleline(&mut m.name).desired_width(f32::INFINITY));
            });
            hint(ui, format!("{} points · drag them on the canvas; double-click an edge to add one, right-click a point to remove it", m.points.len()));
            mask_settings(ui, m);
        } else {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon::PROJECTOR_SCREEN).color(ACCENT));
                ui.add(egui::TextEdit::singleline(&mut o.name).desired_width(f32::INFINITY));
            });
            ui.toggle_value(&mut o.mapping.enabled, label(icon::PROJECTOR_SCREEN, "Mapping on")).on_hover_text("When on, this output shows only its slices");
            ui.horizontal(|ui| {
                ui.label("Size");
                guarded!(ui, o.width, egui::DragValue::new(&mut o.width).range(0..=8192).suffix(" w"));
                guarded!(ui, o.height, egui::DragValue::new(&mut o.height).range(0..=8192).suffix(" h"));
                hint(ui, "0 = canvas size");
            });
            color_grade_editor(ui, o.id, &mut o.color);
            ui.separator();
            hint(ui, format!("{} slices, {} masks. Select one on the left or click it on the canvas.", o.mapping.surfaces.len(), o.mapping.masks.len()));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapping_only_moves_the_selected_slice() {
        let mut m = phosphor_engine::Mapping::default();
        m.add_surface(1).corners[0] = [0.101, 0.199];
        m.add_surface(2).corners[0] = [0.101, 0.199];
        snap_selected(&mut m, Some(1));
        assert_eq!(m.surfaces[0].corners[0], [6.0 / 64.0, 13.0 / 64.0]);
        assert_eq!(m.surfaces[1].corners[0], [0.101, 0.199]);
    }
}
