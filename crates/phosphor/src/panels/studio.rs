//! Mapping Studio: a full workspace for projection mapping. Top: every output, live (the output strip). Left: the
//! selected output's slices and masks. Middle:
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
    /// Drag on the canvas to draw a mask by hand.
    drawing: bool,
    /// Centre the view on this output point next frame (Zoom to slice; the pan needs the canvas size).
    zoom_to: Option<[f32; 2]>,
}

/// Temp-data key: when the Studio was last drawn ("Only selected" ends when it's off screen).
pub(crate) const STUDIO_SHOWN: &str = "phosphor_studio_shown";

enum TreeAction {
    AddSlice(u64),
    /// A slice centred where the canvas was right-clicked (output space 0..1).
    AddSliceAt(u64, [f32; 2]),
    AddMask(u64, MaskShape),
    /// A mask drawn by hand on output `.0` (output space points).
    DrawnMask(u64, Vec<[f32; 2]>),
    Duplicate(u64),
    Delete(u64, u64),
    DeleteMany(u64, Vec<u64>),
    /// Put these slices of output `.0` into a new group.
    Group(u64, Vec<u64>),
}

pub(crate) fn studio(ui: &mut egui::Ui, st: &mut State) {
    st.comp.ensure_outputs();
    if !st.comp.outputs.iter().any(|o| o.id == st.mapping_output) {
        st.mapping_output = st.primary_output();
    }
    output_strip(ui, st, 96.0);
    ui.separator();
    let layout = egui::Layout::top_down(egui::Align::Min);
    // Each part in its own clipped area, so nothing spills into its neighbour.
    let part = |ui: &mut egui::Ui, rect: egui::Rect| {
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(layout));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        child
    };
    // Too narrow for three columns (a side tab): one part at a time.
    if ui.available_width() < 760.0 {
        let key = egui::Id::new("studio_narrow_part");
        let mut pick: u8 = ui.ctx().data(|d| d.get_temp(key)).unwrap_or(1);
        segmented(ui, &mut pick, &[(0, "Slices".to_string()), (1, "Canvas".to_string()), (2, "Properties".to_string())], |_, _, _| {});
        ui.ctx().data_mut(|d| d.insert_temp(key, pick));
        let full = ui.available_rect_before_wrap();
        match pick {
            0 => tree(&mut part(ui, full), st),
            2 => properties(&mut part(ui, full), st),
            _ => canvases(&mut part(ui, full), st),
        }
        ui.allocate_rect(full, Sense::hover());
        return;
    }
    let full = ui.available_rect_before_wrap();
    let left_w = 230.0f32.min(full.width() * 0.22);
    let right_w = 340.0f32.min(full.width() * 0.3);
    let left = egui::Rect::from_min_size(full.min, egui::vec2(left_w, full.height()));
    let right = egui::Rect::from_min_max(egui::pos2(full.right() - right_w, full.top()), full.max);
    let center = egui::Rect::from_min_max(egui::pos2(left.right() + 8.0, full.top()), egui::pos2(right.left() - 8.0, full.bottom()));
    tree(&mut part(ui, left), st);
    canvases(&mut part(ui, center), st);
    properties(&mut part(ui, right), st);
    let p = ui.painter();
    p.vline(left.right() + 4.0, full.y_range(), Stroke::new(1.0, Color32::from_gray(40)));
    p.vline(right.left() - 4.0, full.y_range(), Stroke::new(1.0, Color32::from_gray(40)));
    ui.allocate_rect(full, Sense::hover());
}

/// The selected output's slices and masks (outputs themselves are in the strip above).
fn tree(ui: &mut egui::Ui, st: &mut State) {
    let mut action = None;
    ui.horizontal_wrapped(|ui| {
        venue_menu(ui, st);
        outputs_actions(ui, st);
    });
    ui.separator();
    let State { comp, selected_surface, mapping_output, .. } = st;
    egui::ScrollArea::vertical().id_salt("studio_tree").auto_shrink(false).show(ui, |ui| {
        for o in comp.outputs.iter_mut() {
            if *mapping_output != o.id {
                continue;
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} {}", icon::PROJECTOR_SCREEN, o.name)).strong());
                toggle(ui, &mut o.mapping.enabled, "MAPPING").on_hover_text("Mapping on for this output (off = the whole picture, unmapped)");
            });
            let mut acts = Vec::new();
            super::slice_tree::slice_rows(ui, &mut o.mapping, selected_surface, &mut acts);
            for a in acts {
                use super::slice_tree::SliceAct;
                match a {
                    SliceAct::Group(ids) => action = Some(TreeAction::Group(o.id, ids)),
                    SliceAct::Duplicate(id) => action = Some(TreeAction::Duplicate(id)),
                    SliceAct::Delete(ids) => action = Some(TreeAction::DeleteMany(o.id, ids)),
                }
            }
            for m in &mut o.mapping.masks {
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    crate::ui::eye_toggle(ui, &mut m.enabled);
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
    apply_action(st, action);
}

/// Adds, duplicates and deletes slices and masks (tree and canvas menus).
fn apply_action(st: &mut State, action: Option<TreeAction>) {
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
        Some(TreeAction::AddSliceAt(oid, at)) => {
            let id = st.comp.new_id();
            if let Some(o) = st.comp.output_mut(oid) {
                let s = o.mapping.add_surface(id);
                let c = s.center();
                s.translate(at[0] - c[0], at[1] - c[1]);
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
        Some(TreeAction::DrawnMask(oid, stroke)) => {
            let id = st.comp.new_id();
            if let Some(o) = st.comp.output_mut(oid)
                && o.mapping.add_mask_stroke(id, &stroke).is_some()
            {
                o.mapping.enabled = true;
                st.selected_surface = Some(id);
            }
        }
        Some(TreeAction::Duplicate(sid)) => {
            if let Some(id) = st.comp.duplicate_slice(sid) {
                st.selected_surface = Some(id);
            }
        }
        Some(TreeAction::Delete(oid, id)) => apply_action(st, Some(TreeAction::DeleteMany(oid, vec![id]))),
        Some(TreeAction::DeleteMany(oid, ids)) => {
            if let Some(o) = st.comp.output_mut(oid) {
                o.mapping.remove_slices(|s| ids.contains(&s.id));
                o.mapping.masks.retain(|m| !ids.contains(&m.id));
            }
            if st.selected_surface.is_some_and(|s| ids.contains(&s)) {
                st.selected_surface = None;
            }
        }
        Some(TreeAction::Group(oid, ids)) => {
            let id = st.comp.new_id();
            if let Some(o) = st.comp.output_mut(oid) {
                o.mapping.group_slices(id, &ids);
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
    let mut action: Option<TreeAction> = None;
    let State { comp, selected_surface, layer_textures, output, thumbs, .. } = st;
    let Some(o) = comp.outputs.iter_mut().find(|o| o.id == oid) else { return };
    ui.horizontal_wrapped(|ui| {
        segmented(ui, &mut vs.view, &[(View::Split, "Output + input".to_string()), (View::Output, "Output".to_string()), (View::Input, "Input".to_string())], |_, _, _| {});
        ui.separator();
        if ui.button(label(icon::CORNERS_OUT, "Fit")).on_hover_text("Show the whole output").clicked() {
            vs.zoom = 1.0;
            vs.pan = egui::Vec2::ZERO;
        }
        // Zoom so the selected slices fill the view (their outline with a margin).
        let many = super::mapping_points::selected_slices(ui.ctx(), *selected_surface);
        let pts: Vec<[f32; 2]> = o.mapping.surfaces.iter_mut().filter(|s| many.contains(&s.id)).flat_map(|s| {
            let mut v: Vec<[f32; 2]> = super::mapping_points::points_mut(s).into_iter().map(|p| *p).collect();
            v.extend(s.corners);
            v
        }).collect();
        if ui.add_enabled(!pts.is_empty(), egui::Button::new(label(icon::MAGNIFYING_GLASS_PLUS, "Zoom to slice"))).on_hover_text("Fill the view with the selected slice(s)").clicked() {
            let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
            for p in &pts {
                for k in 0..2 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            let (w, h) = ((hi[0] - lo[0]).max(0.02), (hi[1] - lo[1]).max(0.02));
            vs.zoom = (0.8 / w.max(h)).clamp(0.25, 12.0);
            vs.zoom_to = Some([(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5]);
        }
        hint(ui, format!("{:.0}%", vs.zoom * 100.0));
        // Show only the selected slices on the output (and projector) while this is on.
        let mut solo = !o.mapping.solo.is_empty();
        toggle(ui, &mut solo, label(icon::CROSSHAIR, "Only selected")).on_hover_text("The output shows only the selected slice(s), so you can see which is which on the real object");
        let slices: Vec<u64> = many.iter().copied().filter(|id| o.mapping.surfaces.iter().any(|s| s.id == *id)).collect();
        o.mapping.solo = if solo { if slices.is_empty() { o.mapping.solo.clone() } else { slices } } else { Vec::new() };
        let now = ui.input(|i| i.time);
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(STUDIO_SHOWN), now));
        toggle(ui, &mut vs.snap, label(icon::MAGNET, "Snap")).on_hover_text("Dragged points land on other slices' points nearby, otherwise on a 1/64 grid");
        super::output::grid_toggle(ui, &mut o.grid);
        toggle(ui, &mut o.mapping.test_pattern, "Slice test pattern").on_hover_text("A test pattern inside every slice instead of its picture (checks each slice's own alignment)");
        toggle(ui, &mut vs.drawing, label(icon::PENCIL_SIMPLE, "Draw mask")).on_hover_text("Drag on the canvas to draw a mask by hand (it becomes a smooth outline you can edit)");
        stage_photo_menu(ui, o);
        if crate::ui::toggle(ui, &mut edit_on_output, "Edit on output (E)").changed() {
            output.lock().unwrap().entry(oid).or_default().edit_mapping = edit_on_output;
        }
    });
    hint(ui, "Pinch or ⌘-scroll to zoom, two-finger scroll or middle-drag to pan · click a point or press Tab, then arrows nudge it (Shift = 10 px) · Snap: points land on the grid or on other slices' points");
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
        // Drawing a mask: the pointer's path while the primary button is down, a mask when it's let go.
        let stroke_id = key.with("stroke");
        if bg.clicked() {
            *selected_surface = None;
        }
        let base = fit_rect(area.shrink(12.0), aspect);
        if let Some(c) = vs.zoom_to.take() {
            vs.pan = egui::vec2((0.5 - c[0]) * base.width(), (0.5 - c[1]) * base.height()) * vs.zoom;
        }
        let frame = egui::Rect::from_center_size(base.center() + vs.pan, base.size() * vs.zoom);
        let mut child = ui.new_child(UiBuilder::new().max_rect(area));
        child.set_clip_rect(area.intersect(ui.clip_rect()));
        let p = child.painter().clone();
        p.rect_filled(area, 4.0, Color32::from_gray(14));
        p.rect_filled(frame, 0.0, Color32::BLACK);
        if let Some(t) = tex {
            paint_texture(&p, t, frame);
        }
        // The stage photo over the output, to line slices up with the real stage (editor only).
        if !o.stage_photo.is_empty()
            && let Some(t) = thumbs.get_sized(&o.stage_photo, 2048)
        {
            p.image(t, frame, FULL_UV, Color32::from_white_alpha((o.stage_photo_opacity.clamp(0.0, 1.0) * 255.0) as u8));
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
        if vs.drawing {
            let to_out = |p: egui::Pos2| [(p.x - frame.left()) / frame.width(), (p.y - frame.top()) / frame.height()];
            let mut stroke: Vec<[f32; 2]> = ui.ctx().data(|d| d.get_temp(stroke_id)).unwrap_or_default();
            if bg.drag_started_by(egui::PointerButton::Primary) {
                stroke.clear();
            }
            if bg.dragged_by(egui::PointerButton::Primary)
                && let Some(p) = bg.interact_pointer_pos()
            {
                stroke.push(to_out(p));
            }
            if stroke.len() > 1 {
                let pts: Vec<egui::Pos2> = stroke.iter().map(|q| egui::pos2(frame.left() + q[0] * frame.width(), frame.top() + q[1] * frame.height())).collect();
                p.add(egui::Shape::line(pts, Stroke::new(2.0, MASK_COLOR)));
            }
            if bg.drag_stopped_by(egui::PointerButton::Primary) {
                action = Some(TreeAction::DrawnMask(o.id, std::mem::take(&mut stroke)));
                vs.drawing = false;
            }
            ui.ctx().data_mut(|d| d.insert_temp(stroke_id, stroke));
            if bg.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            }
        }
        let touched = mapping_handles(&mut child, frame, &mut o.mapping, selected_surface, true);
        if touched && vs.snap {
            // Dragging one point: onto another slice's point within 8 screen pixels, else onto the grid.
            let dragging = ui.input(|i| i.pointer.primary_down());
            match super::mapping_points::selected_point(ui.ctx()).filter(|(sid, _)| Some(*sid) == *selected_surface && dragging) {
                Some((sid, i)) => {
                    let radius = [8.0 / frame.width(), 8.0 / frame.height()];
                    if !super::mapping_points::snap_to_others(&mut o.mapping, sid, i, radius)
                        && let Some(p) = o.mapping.surfaces.iter_mut().find(|s| s.id == sid).and_then(|s| super::mapping_points::points_mut(s).into_iter().nth(i))
                    {
                        p[0] = (p[0] * 64.0).round() / 64.0;
                        p[1] = (p[1] * 64.0).round() / 64.0;
                    }
                }
                None => snap_selected(&mut o.mapping, *selected_surface),
            }
        }
        // Right-click: what can be done here. The slice under the pointer becomes the selected one.
        let menu_key = key.with("menu_at");
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("studio_canvas_rect"), frame));
        if bg.secondary_clicked()
            && let Some(p) = bg.interact_pointer_pos()
        {
            let at = [(p.x - frame.left()) / frame.width(), (p.y - frame.top()) / frame.height()];
            let near = |s: &phosphor_engine::mapping::Surface| {
                let c = s.center();
                ((c[0] - at[0]) * aspect).hypot(c[1] - at[1]) < 0.12
            };
            if let Some(id) = o.mapping.surfaces.iter().rev().find(|s| if s.model.is_some() || s.line.is_some() { near(s) } else { s.contains(at) }).map(|s| s.id) {
                *selected_surface = Some(id);
            }
            ui.ctx().data_mut(|d| d.insert_temp(menu_key, at));
        }
        bg.context_menu(|ui| {
            let at: [f32; 2] = ui.ctx().data(|d| d.get_temp(menu_key)).unwrap_or([0.5, 0.5]);
            canvas_menu(ui, o, *selected_surface, at, &mut action, &mut vs);
        });
        // Keys: Tab through points, arrows nudge the selected point (or the whole slice), one output pixel a press.
        if bg.hovered() || child.ui_contains_pointer() {
            super::mapping_points::point_keys(ui, &mut o.mapping, *selected_surface, (ow, oh));
        }
    }
    if let Some(area) = in_area {
        input_view(ui, area, o, *selected_surface, program, layer_textures, canvas);
    }
    ui.ctx().data_mut(|d| d.insert_temp(key, vs));
    apply_action(st, action);
}

/// "Stage photo": pick a photo of the stage taken from the projector, how strongly it shows, or remove it.
fn stage_photo_menu(ui: &mut egui::Ui, o: &mut phosphor_engine::OutputConfig) {
    let text = if o.stage_photo.is_empty() { label(icon::IMAGE, "Stage photo…") } else { label(icon::IMAGE, "Stage photo") };
    ui.menu_button(text, |ui| {
        ui.set_min_width(260.0);
        hint(ui, "A photo of the stage taken from where the projector stands: line the slices up with it. Only shows here, never on the output.");
        if ui.button(label(icon::FOLDER_OPEN, if o.stage_photo.is_empty() { "Choose a photo…" } else { "Another photo…" })).clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(super::mapping::PICK_STAGE_PHOTO), o.id));
            ui.close();
        }
        if !o.stage_photo.is_empty() {
            ui.horizontal(|ui| {
                Level::percent(&mut o.stage_photo_opacity, "Shows").default(0.5).show(ui);
            });
            if danger_button(ui, label(icon::X, "Remove the photo")).clicked() {
                o.stage_photo.clear();
                ui.close();
            }
        }
    })
    .response
    .on_hover_text("A photo of the real stage behind the slices, for lining them up");
}

/// The Studio canvas's right-click menu: the slice under the pointer (warp, shape, line, 3D model, fill, duplicate,
/// delete), then adding slices and masks and the view options.
fn canvas_menu(ui: &mut egui::Ui, o: &mut phosphor_engine::OutputConfig, selected: Option<u64>, at: [f32; 2], action: &mut Option<TreeAction>, vs: &mut ViewState) {
    use phosphor_engine::mapping::{Mesh, SliceShape};
    let oid = o.id;
    // Other slices on this output, for "Copy its look to" (applied at the end, outside the borrow of this slice).
    let others: Vec<(u64, String)> = o.mapping.surfaces.iter().filter(|s| Some(s.id) != selected).map(|s| (s.id, s.name.clone())).collect();
    let mut copy_to: Vec<u64> = Vec::new();
    if let Some(s) = selected.and_then(|id| o.mapping.surfaces.iter_mut().find(|s| s.id == id)) {
        ui.label(RichText::new(&s.name).strong());
        if !others.is_empty() {
            ui.menu_button(label(icon::COPY, "Copy its look to"), |ui| {
                hint(ui, "Opacity, soft edges, colour, blend and shape; not where it sits or what it shows");
                if ui.button("Every other slice here").clicked() {
                    copy_to = others.iter().map(|(id, _)| *id).collect();
                    ui.close();
                }
                ui.separator();
                for (id, name) in &others {
                    if ui.button(name).clicked() {
                        copy_to = vec![*id];
                        ui.close();
                    }
                }
            });
        }
        // A selected mesh point: add or remove the grid line through it (MadMapper's mesh point editing).
        let point = super::mapping_points::selected_point(ui.ctx()).filter(|(sid, _)| *sid == s.id).map(|(_, k)| k);
        if let (Some(k), Some(m)) = (point, s.mesh.clone()) {
            let (i, j) = m.grid_pos(k);
            let mut changed = None;
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Point {} (column {}, row {})", k + 1, i + 1, j + 1)).small());
            });
            let col_right = if i < m.cols { i } else { i.saturating_sub(1) };
            let row_below = if j < m.rows { j } else { j.saturating_sub(1) };
            for (text, next) in [
                ("Add a column beside it", m.with_column_after(col_right)),
                ("Add a row beside it", m.with_row_after(row_below)),
                ("Remove its column", m.without_column(i)),
                ("Remove its row", m.without_row(j)),
            ] {
                let can = next.is_some();
                if ui.add_enabled(can, egui::Button::new(text)).on_disabled_hover_text("Not here: the outer edges stay, and a grid has at most 16 cells a side").clicked() {
                    changed = next;
                    ui.close();
                }
            }
            if let Some(next) = changed {
                s.mesh = Some(next);
                super::mapping_points::select_point(ui.ctx(), None);
            }
            ui.separator();
        }
        let flat = s.model.is_none() && s.line.is_none();
        ui.add_enabled_ui(flat, |ui| {
            ui.menu_button(label(icon::GRID_NINE, "Mesh warp"), |ui| {
                if ui.selectable_label(s.mesh.is_none(), "Off: four corners").clicked() {
                    s.set_mesh(false, 3, 3);
                    ui.close();
                }
                for n in [2u32, 3, 4, 6, 8] {
                    let on = s.mesh.as_ref().is_some_and(|m| m.cols == n && m.rows == n);
                    if ui.selectable_label(on, format!("{n} × {n} grid")).clicked() {
                        s.set_mesh(true, n, n);
                        ui.close();
                    }
                }
                if let Some(m) = &s.mesh {
                    let (c, r) = (m.cols, m.rows);
                    ui.separator();
                    if ui.button("Flatten: straighten the grid").clicked() {
                        s.mesh = Some(Mesh::from_corners(&s.corners, c, r));
                        ui.close();
                    }
                }
            });
            ui.menu_button(label(icon::SHAPES, "Shape"), |ui| {
                for shape in SliceShape::ALL {
                    if ui.selectable_label(s.shape == shape, shape.name()).clicked() {
                        s.shape = shape;
                        ui.close();
                    }
                }
            });
            if ui.button(label(icon::CORNERS_OUT, "Fill the output")).on_hover_text("Corners back to the edges of the output").clicked() {
                s.mesh = None;
                s.corners = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
                ui.close();
            }
        });
        let mut line = s.line.is_some();
        if crate::ui::toggle(ui, &mut line, "Line slice").on_hover_text("The picture runs along a line with a width").changed() {
            s.set_line(line);
            if line {
                s.model = None;
            }
            ui.close();
        }
        if let Some(m) = &mut s.model {
            if crate::ui::toggle(ui, &mut m.calibrating, "Calibrate (pin points)").on_hover_text("Click model points, drag their pins onto the real object; 4+ pins solve the projector").changed() {
                ui.close();
            }
            if !m.pins.is_empty() && ui.button("Clear calibration pins").clicked() {
                m.pins.clear();
                ui.close();
            }
            if ui.button("Choose another model…").clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(crate::panels::PICK_MODEL), s.id));
                ui.close();
            }
        }
        let mut model = s.model.is_some();
        if crate::ui::toggle(ui, &mut model, "3D model…").on_hover_text("Map onto a real object from its OBJ model").changed() {
            if model {
                s.model = Some(phosphor_engine::mapping::Model3d::default());
                s.line = None;
                s.mesh = None;
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(crate::panels::PICK_MODEL), s.id));
            } else {
                s.model = None;
            }
            ui.close();
        }
        let id = s.id;
        ui.separator();
        let picked = super::mapping_points::selected_slices(ui.ctx(), selected);
        if picked.len() > 1 && ui.button(label(icon::FOLDER_PLUS, &format!("Group the {} selected slices", picked.len()))).clicked() {
            *action = Some(TreeAction::Group(oid, picked));
            ui.close();
        }
        if ui.button(label(icon::COPY, "Duplicate")).clicked() {
            *action = Some(TreeAction::Duplicate(id));
            ui.close();
        }
        if ui.button(label(icon::TRASH, "Delete")).clicked() {
            *action = Some(TreeAction::Delete(oid, id));
            ui.close();
        }
        ui.separator();
    }
    if ui.button(label(icon::PLUS, "Add a slice here")).clicked() {
        *action = Some(TreeAction::AddSliceAt(oid, at));
        ui.close();
    }
    ui.menu_button(label(icon::PLUS, "Add a mask"), |ui| {
        for &shape in <MaskShape as phosphor_engine::Choice>::OPTIONS {
            if ui.button(shape.name()).clicked() {
                *action = Some(TreeAction::AddMask(oid, shape));
                ui.close();
            }
        }
    });
    ui.separator();
    crate::ui::toggle(ui, &mut o.mapping.enabled, "Mapping on");
    crate::ui::toggle(ui, &mut o.grid.on, "Alignment grid").on_hover_text("Over the whole output: cells, crosshair, circle, border (settings: right-click GRID in the strip)");
    crate::ui::toggle(ui, &mut o.mapping.test_pattern, "Slice test pattern");
    crate::ui::toggle(ui, &mut vs.snap, "Snap to grid");
    if ui.button(label(icon::CORNERS_OUT, "Fit the view")).clicked() {
        vs.zoom = 1.0;
        vs.pan = egui::Vec2::ZERO;
        ui.close();
    }
    if let Some(src) = selected.and_then(|id| o.mapping.surfaces.iter().find(|s| s.id == id)).cloned() {
        for s in o.mapping.surfaces.iter_mut().filter(|s| copy_to.contains(&s.id)) {
            s.copy_look_from(&src);
        }
    }
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
        let color = if is_sel { accent() } else { Color32::from_gray(140) };
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
    let clipboard_len = st.fx_clipboard.len();
    let mut fx_action = None;
    let State {
        comp,
        selected_surface,
        #[cfg(not(target_arch = "wasm32"))]
        projectors,
        ..
    } = st;
    let others = super::mapping::picture_sources(comp, oid);
    let (layers, groups) = (&mut comp.layers, &mut comp.groups);
    let canvas = (comp.width, comp.height);
    let Some(o) = comp.outputs.iter_mut().find(|o| o.id == oid) else { return };
    let size = o.size(canvas);
    egui::ScrollArea::vertical().id_salt("studio_props").auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().slider_width = (ui.available_width() - 150.0).max(80.0);
        let many = super::mapping_points::selected_slices(ui.ctx(), *selected_surface);
        if many.len() > 1 {
            super::mapping_points::group_settings(ui, &mut o.mapping, &many);
        } else if let Some(s) = o.mapping.surfaces.iter_mut().find(|s| Some(s.id) == *selected_surface) {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon::POLYGON).color(accent()));
                toggle_mapped(ui, &mut s.enabled, "ON", ControlTarget::SliceEnabled(s.id));
                ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(f32::INFINITY));
            });
            if ui.small_button(label(icon::CORNERS_OUT, "Corners to the full output")).clicked() {
                s.corners = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
                if s.mesh.is_some() {
                    let (c, r) = s.mesh.as_ref().map_or((3, 3), |m| (m.cols, m.rows));
                    s.mesh = Some(phosphor_engine::mapping::Mesh::from_corners(&s.corners, c, r));
                }
            }
            super::mapping_points::point_fields(ui, s, size);
            ui.separator();
            surface_settings(ui, s, layers, groups, &others);
            ui.separator();
            // Surface FX: effects on this slice alone (same editor as a layer's).
            if let Some(a) = super::effects::effects_editor(ui, &mut s.effects, clipboard_len) {
                fx_action = Some((s.id, a));
            }
        } else if let Some(m) = o.mapping.masks.iter_mut().find(|m| Some(m.id) == *selected_surface) {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon::SELECTION).color(MASK_COLOR));
                toggle(ui, &mut m.enabled, "ON");
                ui.add(egui::TextEdit::singleline(&mut m.name).desired_width(f32::INFINITY));
            });
            hint(ui, format!("{} points · drag them on the canvas; the + between two points adds one; right-click a point to remove it, make it a sharp corner or give it bezier handles", m.points.len()));
            mask_settings(ui, m);
            let mid = m.id;
            super::mapping::mask_attach_row(ui, &mut o.mapping, mid);
        } else {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon::PROJECTOR_SCREEN).color(accent()));
                ui.add(egui::TextEdit::singleline(&mut o.name).desired_width(f32::INFINITY));
            });
            toggle(ui, &mut o.mapping.enabled, label(icon::PROJECTOR_SCREEN, "Mapping on")).on_hover_text("When on, this output shows only its slices");
            ui.horizontal(|ui| {
                ui.label("Size");
                guarded!(ui, o.width, egui::DragValue::new(&mut o.width).range(0..=8192).suffix(" w"));
                guarded!(ui, o.height, egui::DragValue::new(&mut o.height).range(0..=8192).suffix(" h"));
                hint(ui, "0 = canvas size");
            });
            color_grade_editor(ui, o.id, &mut o.color);
            #[cfg(not(target_arch = "wasm32"))]
            super::projector::projector_settings(ui, o, projectors);
            ui.separator();
            hint(ui, format!("{} slices, {} masks. Select one on the left or click it on the canvas.", o.mapping.surfaces.len(), o.mapping.masks.len()));
        }
    });
    if let Some((sid, a)) = fx_action {
        super::effects::apply_fx_action(st, sid, a);
    }
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
