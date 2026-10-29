//! The node canvas: boxes wired together, for any graph (Patches in the Shader Editor, Designer networks). Right-click
//! the canvas to add a box, drag from a box's output dot (right) to another's input dot (left) to wire them, drag a
//! wire's end away to unplug it, drag a box by its title, middle-drag or two-finger scroll to move around, pinch or
//! Cmd-scroll to zoom (`zoom_controls`: −, the zoom, +, Fit, for the canvas's toolbar). The boxes are drawn on a
//! layer egui scales (`set_transform_layer`), so everything in them, settings included, zooms together.
//! The graph says what its boxes are (`Graph`); the canvas draws and edits them.

use eframe::egui::{self, Color32, Sense, Stroke, StrokeKind};
use egui_phosphor::regular as icon;
use phosphor_engine::PortKind;

use super::{accent, command, danger_button, label};

pub(crate) const NODE_W: f32 = 190.0;
pub(crate) const TITLE_H: f32 = 22.0;
pub(crate) const INPUT_H: f32 = 18.0;
const PORT_R: f32 = 5.0;

/// What a canvas needs to know about a graph's boxes.
pub(crate) trait Graph {
    fn ids(&self) -> Vec<u32>;
    fn pos(&self, id: u32) -> [f32; 2];
    fn set_pos(&mut self, id: u32, pos: [f32; 2]);
    fn title(&self, id: u32) -> String;
    fn about(&self, id: u32) -> String;
    fn colour(&self, id: u32) -> Color32;
    /// Its inputs: a name and what each takes (a picture or a number).
    fn inputs(&self, id: u32) -> Vec<(String, PortKind)>;
    fn has_output(&self, id: u32) -> bool;
    /// What its output carries.
    fn output_kind(&self, _id: u32) -> PortKind {
        PortKind::Picture
    }
    fn removable(&self, id: u32) -> bool;
    fn width(&self, _id: u32) -> f32 {
        NODE_W
    }
    /// How tall the settings under the inputs are (a first guess: the canvas then sizes each box to what its
    /// settings really took, so a box never cuts its last row off).
    fn body_height(&self, id: u32) -> f32;
    /// Draws the settings into `ui` (sized to `body_height`). True when something changed.
    fn body(&mut self, ui: &mut egui::Ui, id: u32) -> bool;
    fn source_of(&self, to: u32, input: usize) -> Option<u32>;
    fn connect(&mut self, from: u32, to: u32, input: usize) -> bool;
    fn disconnect(&mut self, to: u32, input: usize);
    fn remove(&mut self, id: u32);
    fn duplicate(&mut self, id: u32);
    /// The right-click menu's entries for adding a box at `at`. True when one was added.
    fn add_menu(&mut self, ui: &mut egui::Ui, at: [f32; 2]) -> bool;
    /// A box's own right-click entries (before Duplicate / Delete). True when something changed.
    /// Puts several boxes in one box of boxes (Shift-click selects; the menu offers it).
    fn wrap_many(&mut self, _ids: &[u32]) {}
    fn extra_menu(&mut self, _ui: &mut egui::Ui, _id: u32) -> bool {
        false
    }
    /// The box's title was double-clicked (a box of boxes opens).
    fn open(&mut self, _id: u32) {}
}

/// A wire being dragged out of box `from`'s output.
#[derive(Clone, Copy)]
struct WireDrag {
    from: u32,
    kind: PortKind,
}

/// Points (point pictures) on the canvas: their dots, wires and boxes.
pub const POINTS_COLOUR: Color32 = Color32::from_rgb(110, 205, 170);
/// A light (Points: Light) on the canvas: its box and wire.
pub const LIGHT_COLOUR: Color32 = Color32::from_rgb(235, 200, 90);

/// Dots and wires: pictures in white, numbers in the controllers' colour, points in their own.
fn port_colour(kind: PortKind, lit: bool) -> Color32 {
    match (kind, lit) {
        (_, true) => accent(),
        (PortKind::Picture, false) => Color32::from_gray(200),
        (PortKind::Number, false) => crate::theme::Hue::Control.color(),
        (PortKind::Points, false) => POINTS_COLOUR,
        (PortKind::Light, false) => LIGHT_COLOUR,
    }
}

fn wire_shape(a: egui::Pos2, b: egui::Pos2, colour: Color32) -> egui::Shape {
    let bend = ((b.x - a.x).abs() * 0.5).max(40.0);
    egui::epaint::CubicBezierShape::from_points_stroke([a, a + egui::vec2(bend, 0.0), b - egui::vec2(bend, 0.0), b], false, Color32::TRANSPARENT, Stroke::new(2.0, colour)).into()
}

/// Draws and edits `graph`; `key` keeps this canvas's view (scroll position) apart from others. Returns true when
/// the graph changed (boxes, wires or settings).
pub(crate) fn node_canvas(ui: &mut egui::Ui, graph: &mut dyn Graph, key: &str) -> bool {
    let id = egui::Id::new(("node_canvas", key));
    let mut pan: egui::Vec2 = ui.ctx().data(|d| d.get_temp(id.with("pan"))).unwrap_or_default();
    let mut zoom: f32 = ui.ctx().data(|d| d.get_temp(id.with("zoom"))).unwrap_or(1.0);
    let mut drag: Option<WireDrag> = ui.ctx().data(|d| d.get_temp(id.with("wire"))).flatten();
    let mut changed = false;

    let area = ui.available_rect_before_wrap();
    let bg = ui.interact(area, id.with("bg"), Sense::click_and_drag());
    // Zoom: pinch or Cmd-scroll over the canvas, round the pointer; the toolbar's −, + round the middle.
    let mut zoom_by = ui.input(|i| i.zoom_delta());
    let mut zoom_at = ui.input(|i| i.pointer.latest_pos()).filter(|p| area.contains(*p));
    if zoom_at.is_none() {
        zoom_by = 1.0;
    }
    if let Some(k) = ui.ctx().data_mut(|d| d.remove_temp::<f32>(id.with("zoom_by"))) {
        (zoom_by, zoom_at) = (k, Some(area.center()));
    }
    if zoom_by != 1.0
        && let Some(p) = zoom_at
    {
        let to = (zoom * zoom_by).clamp(ZOOM_MIN, ZOOM_MAX);
        let local = p - area.min;
        pan = local - (local - pan) * (to / zoom);
        zoom = to;
    }
    let painter = ui.painter_at(area);
    painter.rect_filled(area, 4.0, Color32::from_gray(16));
    // Dots every 24 points (at this zoom), moving with the view.
    let step = 24.0 * zoom;
    let start = area.min + egui::vec2(pan.x.rem_euclid(step), pan.y.rem_euclid(step));
    let mut y = start.y;
    while y < area.max.y {
        let mut x = start.x;
        while x < area.max.x {
            painter.circle_filled(egui::pos2(x, y), 0.8, Color32::from_gray(40));
            x += step;
        }
        y += step;
    }

    // Moving around: middle-drag, or drag the empty canvas, or scroll.
    if bg.dragged_by(egui::PointerButton::Middle) || bg.dragged_by(egui::PointerButton::Primary) {
        pan += bg.drag_delta();
    }
    // Several boxes at once: Shift-click their titles (a click on the canvas lets them all go).
    let mut selected: Vec<u32> = ui.ctx().data(|d| d.get_temp(id.with("sel"))).unwrap_or_default();
    let shift = ui.input(|i| i.modifiers.shift);
    if bg.clicked() {
        selected.clear();
    }
    let mut wrap_all = false;
    if bg.hovered() {
        pan += ui.input(|i| i.smooth_scroll_delta);
    }
    // The boxes: on their own layer over the canvas, scaled by the zoom. From here on, positions are the graph's own
    // (a box at its `pos`), and `area` is the part of them in view.
    let to_screen = egui::emath::TSTransform::new(area.min.to_vec2() + pan, zoom);
    let layer = egui::LayerId::new(ui.layer_id().order, id.with("boxes"));
    ui.ctx().set_sublayer(ui.layer_id(), layer);
    ui.ctx().set_transform_layer(layer, to_screen);
    let screen_area = area;
    let area = to_screen.inverse() * screen_area;
    let mut boxes = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(area));
    boxes.set_clip_rect(area);
    let ui = &mut boxes;
    let painter = ui.painter_at(area);
    let at = |p: [f32; 2]| egui::pos2(p[0], p[1]);

    // Where each box and its ports are this frame. Settings are as tall as they were last drawn (the guess before).
    let measured_id = id.with("body_h");
    let mut measured: std::collections::HashMap<u32, f32> = ui.ctx().data(|d| d.get_temp(measured_id)).unwrap_or_default();
    let ids = graph.ids();
    let mut rects: Vec<(u32, egui::Rect, usize)> = Vec::new();
    let mut outputs: Vec<(u32, egui::Pos2, PortKind)> = Vec::new();
    let mut inputs: Vec<(u32, usize, egui::Pos2, PortKind)> = Vec::new();
    for &n in &ids {
        let kinds = graph.inputs(n);
        let ins = kinds.len();
        let size = egui::vec2(graph.width(n), TITLE_H + ins as f32 * INPUT_H + shown_body(&measured, &*graph, n) + 6.0);
        let r = egui::Rect::from_min_size(at(graph.pos(n)), size);
        rects.push((n, r, ins));
        if graph.has_output(n) {
            outputs.push((n, egui::pos2(r.max.x, r.min.y + TITLE_H * 0.5), graph.output_kind(n)));
        }
        for (i, (_, kind)) in kinds.iter().enumerate() {
            inputs.push((n, i, egui::pos2(r.min.x, r.min.y + TITLE_H + (i as f32 + 0.5) * INPUT_H), *kind));
        }
    }
    let out_pos = |id: u32| outputs.iter().find(|(n, ..)| *n == id).map(|(_, p, _)| *p);
    for (to, i, p, kind) in &inputs {
        if let Some(a) = graph.source_of(*to, *i).and_then(out_pos) {
            painter.add(wire_shape(a, *p, port_colour(*kind, false).gamma_multiply(0.75)));
        }
    }

    let mut remove: Option<u32> = None;
    let mut duplicate: Option<u32> = None;
    let pointer = ui.input(|i| i.pointer.interact_pos()).map(|p| to_screen.inverse() * p);
    for (n, r, ins) in &rects {
        let (n, r, ins) = (*n, *r, *ins);
        if !r.intersects(area) {
            continue;
        }
        let title = egui::Rect::from_min_size(r.min, egui::vec2(r.width(), TITLE_H));
        painter.rect_filled(r, 5.0, Color32::from_gray(30));
        painter.rect_filled(title, egui::CornerRadius { nw: 5, ne: 5, sw: 0, se: 0 }, graph.colour(n).gamma_multiply(0.55));
        let picked = selected.contains(&n);
        painter.rect_stroke(r, 5.0, Stroke::new(if picked { 2.0 } else { 1.0 }, if picked { accent() } else { Color32::from_gray(60) }), StrokeKind::Inside);
        painter.text(title.left_center() + egui::vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, graph.title(n), egui::FontId::proportional(13.0), Color32::WHITE);
        let t = ui.interact(title, id.with(("title", n)), Sense::click_and_drag()).on_hover_text(graph.about(n));
        if t.clicked() && shift {
            if picked {
                selected.retain(|s| *s != n);
            } else {
                selected.push(n);
            }
        }
        if t.dragged_by(egui::PointerButton::Primary) {
            let d = t.drag_delta();
            // A selected box drags the whole selection along.
            let moving: Vec<u32> = if picked { selected.clone() } else { vec![n] };
            for m in moving {
                let p = graph.pos(m);
                graph.set_pos(m, [p[0] + d.x, p[1] + d.y]);
            }
        }
        // Saved once, when the box is let go.
        changed |= t.drag_stopped();
        if t.double_clicked() {
            graph.open(n);
        }
        let removable = graph.removable(n);
        t.context_menu(|ui| {
            if picked && selected.len() > 1 && command(ui, label(icon::PACKAGE, &format!("Wrap {} boxes in a box", selected.len()))).on_hover_text("Puts the selected boxes inside one new box of boxes, their wires kept").clicked() {
                wrap_all = true;
                ui.close();
            }
            if graph.extra_menu(ui, n) {
                changed = true;
            }
            if command(ui, label(icon::COPY, "Duplicate")).clicked() {
                duplicate = Some(n);
                ui.close();
            }
            if removable && danger_button(ui, label(icon::TRASH, "Delete")).clicked() {
                remove = Some(n);
                ui.close();
            }
        });
        // Input names, and their dots (drag a wired one away to unplug it).
        for (i, (name, kind)) in graph.inputs(n).iter().enumerate() {
            let y = r.min.y + TITLE_H + (i as f32 + 0.5) * INPUT_H;
            let colour = if *kind == PortKind::Number { port_colour(*kind, false).gamma_multiply(0.8) } else { Color32::from_gray(170) };
            painter.text(egui::pos2(r.min.x + 12.0, y), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(12.0), colour);
        }
        // The settings: drawn by the graph, clipped to the canvas (a box half off the canvas shows what fits).
        let guess = shown_body(&measured, &*graph, n);
        if guess > 0.0 || graph.body_height(n) > 0.0 {
            let body = egui::Rect::from_min_size(egui::pos2(r.min.x + 6.0, r.min.y + TITLE_H + ins as f32 * INPUT_H + 2.0), egui::vec2(r.width() - 12.0, guess.max(1.0)));
            if area.intersects(body) {
                let inner = ui.scope_builder(egui::UiBuilder::new().max_rect(body).id_salt(("body", n)), |ui| {
                    ui.set_clip_rect(area);
                    ui.spacing_mut().item_spacing.y = 3.0;
                    graph.body(ui, n)
                });
                changed |= inner.inner;
                // What the settings really took: the box fits it from the next frame.
                let took = inner.response.rect.height();
                if (took - guess).abs() > 0.5 {
                    measured.insert(n, took);
                    ui.ctx().request_repaint();
                }
            }
        }
    }

    // Ports go last, on top of the boxes, so they win the pointer.
    // (A wire only lands on an input of its own kind.)
    let hover_input = pointer.and_then(|p| inputs.iter().find(|(_, _, q, kind)| q.distance(p) < PORT_R * 2.5 && drag.is_none_or(|d| d.kind == *kind)).copied());
    for (nid, pos, kind) in &outputs {
        let r = ui.interact(egui::Rect::from_center_size(*pos, egui::vec2(PORT_R * 3.0, PORT_R * 3.0)), id.with(("out", *nid)), Sense::drag());
        if r.drag_started() {
            drag = Some(WireDrag { from: *nid, kind: *kind });
        }
        let lit = r.hovered() || drag.is_some_and(|d| d.from == *nid);
        painter.circle(*pos, PORT_R, port_colour(*kind, lit), Stroke::new(1.0, Color32::BLACK));
    }
    for (nid, i, pos, kind) in &inputs {
        let r = ui.interact(egui::Rect::from_center_size(*pos, egui::vec2(PORT_R * 3.0, PORT_R * 3.0)), id.with(("in", *nid, *i)), Sense::drag());
        let wired = graph.source_of(*nid, *i);
        if r.drag_started()
            && let Some(from) = wired
        {
            graph.disconnect(*nid, *i);
            drag = Some(WireDrag { from, kind: *kind });
            changed = true;
        }
        let lit = drag.is_some() && hover_input.is_some_and(|(n, k, ..)| n == *nid && k == *i);
        let fill = if lit || wired.is_some() { port_colour(*kind, lit) } else { Color32::from_gray(70) };
        painter.circle(*pos, if *kind == PortKind::Number { PORT_R - 1.0 } else { PORT_R }, fill, Stroke::new(1.0, Color32::BLACK));
    }
    if let Some(d) = drag {
        let (down, released) = ui.input(|i| (i.pointer.primary_down(), i.pointer.any_released()));
        if let (Some(a), Some(p)) = (out_pos(d.from), pointer) {
            painter.add(wire_shape(a, hover_input.map_or(p, |(_, _, q, _)| q), port_colour(d.kind, true)));
        }
        if released || !down {
            if let Some((to, i, ..)) = hover_input
                && graph.connect(d.from, to, i)
            {
                changed = true;
            }
            drag = None;
        }
    }

    // Right-click the empty canvas: add a box where you clicked.
    if bg.secondary_clicked() {
        let p = pointer.unwrap_or(area.center());
        ui.ctx().data_mut(|d| d.insert_temp(id.with("add_at"), [p.x, p.y]));
    }
    // Fit (the toolbar asked, or a bench with PHOSPHOR_CANVAS_FIT): every box in view, no bigger than full size.
    let bench_fit = std::env::var_os("PHOSPHOR_CANVAS_FIT").is_some() && ui.ctx().cumulative_pass_nr() < 120;
    if (ui.ctx().data_mut(|d| d.remove_temp::<bool>(id.with("fit"))).is_some() || bench_fit)
        && let Some(all) = rects.iter().map(|(_, r, _)| *r).reduce(|a, b| a.union(b))
    {
        let room = screen_area.shrink(16.0);
        zoom = (room.width() / all.width()).min(room.height() / all.height()).clamp(ZOOM_MIN, 1.0);
        pan = room.center() - screen_area.min - all.center().to_vec2() * zoom;
        ui.ctx().request_repaint();
    }
    bg.context_menu(|ui| {
        super::heading(ui, "Add a box");
        let at: [f32; 2] = ui.ctx().data(|d| d.get_temp(id.with("add_at"))).unwrap_or([40.0, 40.0]);
        if graph.add_menu(ui, at) {
            changed = true;
            ui.close();
        }
    });

    if wrap_all {
        graph.wrap_many(&selected);
        selected.clear();
        changed = true;
    }
    if let Some(d) = duplicate {
        graph.duplicate(d);
        changed = true;
    }
    if let Some(r) = remove {
        graph.remove(r);
        changed = true;
    }
    ui.ctx().data_mut(|d| {
        d.insert_temp(id.with("pan"), pan);
        d.insert_temp(id.with("zoom"), zoom);
        d.insert_temp(id.with("wire"), drag);
        d.insert_temp(id.with("sel"), selected);
        measured.retain(|n, _| ids.contains(n));
        d.insert_temp(measured_id, measured);
    });
    changed
}

const ZOOM_MIN: f32 = 0.25;
const ZOOM_MAX: f32 = 2.0;

/// The zoom for canvas `key`, for its toolbar: −, how big (click: full size), +, Fit (every box in view).
pub(crate) fn zoom_controls(ui: &mut egui::Ui, key: &str) {
    let id = egui::Id::new(("node_canvas", key));
    let zoom: f32 = ui.ctx().data(|d| d.get_temp(id.with("zoom"))).unwrap_or(1.0);
    let ask = |ui: &egui::Ui, by: f32| ui.ctx().data_mut(|d| d.insert_temp(id.with("zoom_by"), by));
    if super::small_command(ui, icon::MINUS).on_hover_text("Zoom out (or pinch, or Cmd-scroll over the canvas)").clicked() {
        ask(ui, 0.8);
    }
    if super::small_command(ui, format!("{:.0}%", zoom * 100.0)).on_hover_text("Full size").clicked() {
        ask(ui, 1.0 / zoom);
    }
    if super::small_command(ui, icon::PLUS).on_hover_text("Zoom in").clicked() {
        ask(ui, 1.25);
    }
    if super::small_command(ui, label(icon::CORNERS_OUT, "Fit")).on_hover_text("Every box in view").clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(id.with("fit"), true));
    }
}

/// A box's settings height: as tall as they were last drawn, else the graph's guess.
fn shown_body(measured: &std::collections::HashMap<u32, f32>, graph: &dyn Graph, n: u32) -> f32 {
    measured.get(&n).copied().unwrap_or_else(|| graph.body_height(n))
}
