//! The node canvas: boxes wired together, for any graph (Patches in the Shader Editor, Designer networks). Right-click
//! the canvas to add a box, drag from a box's output dot (right) to another's input dot (left) to wire them, drag a
//! wire's end away to unplug it, drag a box by its title, middle-drag or two-finger scroll to move around.
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
    /// How tall the settings under the inputs are.
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
}

/// A wire being dragged out of box `from`'s output.
#[derive(Clone, Copy)]
struct WireDrag {
    from: u32,
    kind: PortKind,
}

/// Dots and wires: pictures in white, numbers in the controllers' colour.
fn port_colour(kind: PortKind, lit: bool) -> Color32 {
    match (kind, lit) {
        (_, true) => accent(),
        (PortKind::Picture, false) => Color32::from_gray(200),
        (PortKind::Number, false) => crate::theme::Hue::Control.color(),
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
    let mut drag: Option<WireDrag> = ui.ctx().data(|d| d.get_temp(id.with("wire"))).flatten();
    let mut changed = false;

    let area = ui.available_rect_before_wrap();
    let bg = ui.interact(area, id.with("bg"), Sense::click_and_drag());
    let painter = ui.painter_at(area);
    painter.rect_filled(area, 4.0, Color32::from_gray(16));
    // Dots every 24 points, moving with the view.
    let step = 24.0;
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
    if bg.hovered() {
        pan += ui.input(|i| i.smooth_scroll_delta);
    }
    let origin = area.min + pan;
    let at = |p: [f32; 2]| origin + egui::vec2(p[0], p[1]);

    // Where each box and its ports are this frame.
    let ids = graph.ids();
    let mut rects: Vec<(u32, egui::Rect, usize)> = Vec::new();
    let mut outputs: Vec<(u32, egui::Pos2, PortKind)> = Vec::new();
    let mut inputs: Vec<(u32, usize, egui::Pos2, PortKind)> = Vec::new();
    for &n in &ids {
        let kinds = graph.inputs(n);
        let ins = kinds.len();
        let size = egui::vec2(graph.width(n), TITLE_H + ins as f32 * INPUT_H + graph.body_height(n) + 6.0);
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
    let pointer = ui.input(|i| i.pointer.interact_pos());
    for (n, r, ins) in &rects {
        let (n, r, ins) = (*n, *r, *ins);
        if !r.intersects(area) {
            continue;
        }
        let title = egui::Rect::from_min_size(r.min, egui::vec2(r.width(), TITLE_H));
        painter.rect_filled(r, 5.0, Color32::from_gray(30));
        painter.rect_filled(title, egui::CornerRadius { nw: 5, ne: 5, sw: 0, se: 0 }, graph.colour(n).gamma_multiply(0.55));
        painter.rect_stroke(r, 5.0, Stroke::new(1.0, Color32::from_gray(60)), StrokeKind::Inside);
        painter.text(title.left_center() + egui::vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, graph.title(n), egui::FontId::proportional(13.0), Color32::WHITE);
        let t = ui.interact(title, id.with(("title", n)), Sense::click_and_drag()).on_hover_text(graph.about(n));
        if t.dragged_by(egui::PointerButton::Primary) {
            let d = t.drag_delta();
            let p = graph.pos(n);
            graph.set_pos(n, [p[0] + d.x, p[1] + d.y]);
        }
        // Saved once, when the box is let go.
        changed |= t.drag_stopped();
        let removable = graph.removable(n);
        t.context_menu(|ui| {
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
        // The settings: drawn by the graph (they paint with their own clip: only when the whole body is on the canvas).
        let body_h = graph.body_height(n);
        if body_h > 0.0 {
            let body = egui::Rect::from_min_size(egui::pos2(r.min.x + 6.0, r.min.y + TITLE_H + ins as f32 * INPUT_H + 2.0), egui::vec2(r.width() - 12.0, body_h));
            if area.contains_rect(body) {
                let inner = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(body).id_salt(("body", n)), |ui| {
                        ui.set_clip_rect(area);
                        ui.spacing_mut().item_spacing.y = 3.0;
                        graph.body(ui, n)
                    })
                    .inner;
                changed |= inner;
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
        let p = pointer.unwrap_or(area.center()) - origin;
        ui.ctx().data_mut(|d| d.insert_temp(id.with("add_at"), [p.x, p.y]));
    }
    bg.context_menu(|ui| {
        super::heading(ui, "Add a box");
        let at: [f32; 2] = ui.ctx().data(|d| d.get_temp(id.with("add_at"))).unwrap_or([40.0, 40.0]);
        if graph.add_menu(ui, at) {
            changed = true;
            ui.close();
        }
    });

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
        d.insert_temp(id.with("wire"), drag);
    });
    changed
}
