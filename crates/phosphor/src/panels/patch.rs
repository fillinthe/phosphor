//! The patch canvas: blocks wired together into an effect or generator (`phosphor_engine::patch`), shown in the
//! Shader Editor for shaders made with + Patch. Right-click the canvas to add a block, drag from a block's output
//! dot (right) to another's input dot (left) to wire them, drag a wire's end away to unplug it, drag a block by
//! its title, middle-drag or two-finger scroll to move around.

use phosphor_engine::patch::{self, BLOCKS, Group, Patch};

use super::*;

const NODE_W: f32 = 190.0;
const TITLE_H: f32 = 22.0;
const INPUT_H: f32 = 18.0;
const SETTING_H: f32 = 22.0;
const PORT_R: f32 = 5.0;

/// A wire being dragged out of block `from`'s output.
#[derive(Clone, Copy)]
struct WireDrag {
    from: u32,
}

fn node_size(b: &patch::Block) -> egui::Vec2 {
    egui::vec2(NODE_W, TITLE_H + b.inputs.len() as f32 * INPUT_H + b.settings.len() as f32 * SETTING_H + 6.0)
}

fn group_colour(g: Group) -> Color32 {
    match g {
        Group::Source => Color32::from_rgb(70, 110, 160),
        Group::Colour => Color32::from_rgb(150, 90, 150),
        Group::Warp => Color32::from_rgb(170, 120, 60),
        Group::Blur => Color32::from_rgb(80, 140, 130),
        Group::Mix => Color32::from_rgb(110, 110, 170),
        Group::Output => accent(),
    }
}

fn wire_shape(a: egui::Pos2, b: egui::Pos2, colour: Color32) -> egui::Shape {
    let bend = ((b.x - a.x).abs() * 0.5).max(40.0);
    egui::epaint::CubicBezierShape::from_points_stroke(
        [a, a + egui::vec2(bend, 0.0), b - egui::vec2(bend, 0.0), b],
        false,
        Color32::TRANSPARENT,
        Stroke::new(2.0, colour),
    )
    .into()
}

/// Draws and edits `patch`; `key` keeps this patch's view (scroll position) apart from others. Returns true when
/// the patch changed (blocks, wires or settings).
pub(crate) fn patch_canvas(ui: &mut egui::Ui, patch: &mut Patch, key: &str) -> bool {
    let id = egui::Id::new(("patch_canvas", key));
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

    // Where each block's ports are this frame.
    let mut outputs: Vec<(u32, egui::Pos2)> = Vec::new();
    let mut inputs: Vec<(u32, usize, egui::Pos2)> = Vec::new();
    for n in &patch.nodes {
        let Some(b) = patch::block(&n.block) else { continue };
        let r = egui::Rect::from_min_size(at(n.pos), node_size(b));
        if b.id != "output" {
            outputs.push((n.id, egui::pos2(r.max.x, r.min.y + TITLE_H * 0.5)));
        }
        for i in 0..b.inputs.len() {
            inputs.push((n.id, i, egui::pos2(r.min.x, r.min.y + TITLE_H + (i as f32 + 0.5) * INPUT_H)));
        }
    }
    let out_pos = |id: u32| outputs.iter().find(|(n, _)| *n == id).map(|(_, p)| *p);
    for w in &patch.wires {
        if let (Some(a), Some((_, _, b))) = (out_pos(w.from), inputs.iter().find(|(n, i, _)| *n == w.to && *i == w.input)) {
            painter.add(wire_shape(a, *b, Color32::from_gray(150)));
        }
    }

    let mut remove: Option<u32> = None;
    let mut duplicate: Option<u32> = None;
    let pointer = ui.input(|i| i.pointer.interact_pos());
    for n in patch.nodes.iter_mut() {
        let Some(b) = patch::block(&n.block) else { continue };
        let r = egui::Rect::from_min_size(at(n.pos), node_size(b));
        if !r.intersects(area) {
            continue;
        }
        let title = egui::Rect::from_min_size(r.min, egui::vec2(r.width(), TITLE_H));
        painter.rect_filled(r, 5.0, Color32::from_gray(30));
        painter.rect_filled(title, egui::CornerRadius { nw: 5, ne: 5, sw: 0, se: 0 }, group_colour(b.group).gamma_multiply(0.55));
        painter.rect_stroke(r, 5.0, Stroke::new(1.0, Color32::from_gray(60)), StrokeKind::Inside);
        painter.text(title.left_center() + egui::vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, b.name, egui::FontId::proportional(13.0), Color32::WHITE);
        let t = ui.interact(title, id.with(("title", n.id)), Sense::click_and_drag()).on_hover_text(b.about);
        if t.dragged_by(egui::PointerButton::Primary) {
            let d = t.drag_delta();
            n.pos = [n.pos[0] + d.x, n.pos[1] + d.y];
        }
        // Saved (and recompiled) once, when the block is let go.
        changed |= t.drag_stopped();
        t.context_menu(|ui| {
            if crate::ui::command(ui, label(icon::COPY, "Duplicate")).clicked() {
                duplicate = Some(n.id);
                ui.close();
            }
            if b.id != "output" && danger_button(ui, label(icon::TRASH, "Delete")).clicked() {
                remove = Some(n.id);
                ui.close();
            }
        });
        // Input names, and their dots (drag a wired one away to unplug it).
        for (i, name) in b.inputs.iter().enumerate() {
            let y = r.min.y + TITLE_H + (i as f32 + 0.5) * INPUT_H;
            painter.text(egui::pos2(r.min.x + 12.0, y), egui::Align2::LEFT_CENTER, *name, egui::FontId::proportional(12.0), Color32::from_gray(170));
        }
        // Settings: the usual slider each.
        for (i, s) in b.settings.iter().enumerate() {
            let y = r.min.y + TITLE_H + b.inputs.len() as f32 * INPUT_H + i as f32 * SETTING_H + 2.0;
            let row = egui::Rect::from_min_size(egui::pos2(r.min.x + 6.0, y), egui::vec2(r.width() - 12.0, SETTING_H - 3.0));
            if n.values.len() <= i {
                n.values.resize(b.settings.len(), 0.0);
                n.values[i] = s.default;
            }
            // (The slider paints with its own clip: only draw it when its whole row is on the canvas.)
            if !area.contains_rect(row) {
                continue;
            }
            let mut v = n.values[i];
            let resp = ui
                .scope_builder(egui::UiBuilder::new().max_rect(row).id_salt(("setting", n.id, i)), |ui| {
                    ui.set_clip_rect(area);
                    Level::range(&mut v, s.min, s.max, s.name).default(s.default).width(row.width()).show(ui)
                })
                .inner;
            if resp.changed() {
                n.values[i] = v;
                changed = true;
            }
        }
    }

    // Ports go last, on top of the blocks, so they win the pointer.
    let hover_input = pointer.and_then(|p| inputs.iter().find(|(_, _, q)| q.distance(p) < PORT_R * 2.5).copied());
    for (nid, pos) in &outputs {
        let r = ui.interact(egui::Rect::from_center_size(*pos, egui::vec2(PORT_R * 3.0, PORT_R * 3.0)), id.with(("out", *nid)), Sense::drag());
        if r.drag_started() {
            drag = Some(WireDrag { from: *nid });
        }
        let lit = r.hovered() || drag.is_some_and(|d| d.from == *nid);
        painter.circle(*pos, PORT_R, if lit { accent() } else { Color32::from_gray(200) }, Stroke::new(1.0, Color32::BLACK));
    }
    for (nid, i, pos) in &inputs {
        let r = ui.interact(egui::Rect::from_center_size(*pos, egui::vec2(PORT_R * 3.0, PORT_R * 3.0)), id.with(("in", *nid, *i)), Sense::drag());
        let wired = patch.source_of(*nid, *i);
        if r.drag_started()
            && let Some(from) = wired
        {
            patch.disconnect(*nid, *i);
            drag = Some(WireDrag { from });
            changed = true;
        }
        let lit = drag.is_some() && hover_input.is_some_and(|(n, k, _)| n == *nid && k == *i);
        let fill = if lit { accent() } else if wired.is_some() { Color32::from_gray(200) } else { Color32::from_gray(70) };
        painter.circle(*pos, PORT_R, fill, Stroke::new(1.0, Color32::BLACK));
    }
    if let Some(d) = drag {
        let (down, released) = ui.input(|i| (i.pointer.primary_down(), i.pointer.any_released()));
        if let (Some(a), Some(p)) = (out_pos(d.from), pointer) {
            painter.add(wire_shape(a, hover_input.map_or(p, |(_, _, q)| q), accent()));
        }
        if released || !down {
            if let Some((to, i, _)) = hover_input
                && patch.connect(d.from, to, i)
            {
                changed = true;
            }
            drag = None;
        }
    }

    // Right-click the empty canvas: add a block where you clicked.
    if bg.secondary_clicked() {
        let p = pointer.unwrap_or(area.center()) - origin;
        ui.ctx().data_mut(|d| d.insert_temp(id.with("add_at"), [p.x, p.y]));
    }
    let generator = patch.generator;
    bg.context_menu(|ui| {
        crate::ui::heading(ui, "Add a block");
        for g in Group::ALL.into_iter().filter(|g| *g != Group::Output) {
            ui.menu_button(g.name(), |ui| {
                for b in BLOCKS.iter().filter(|b| b.group == g && (b.id != "input" || !generator)) {
                    if crate::ui::command(ui, b.name).on_hover_text(b.about).clicked() {
                        let at: [f32; 2] = ui.ctx().data(|d| d.get_temp(id.with("add_at"))).unwrap_or([40.0, 40.0]);
                        patch.add(b.id, at);
                        changed = true;
                        ui.close();
                    }
                }
            });
        }
        if !patch.nodes.iter().any(|n| n.block == "output") && crate::ui::command(ui, "Output").clicked() {
            patch.add("output", [400.0, 40.0]);
            changed = true;
            ui.close();
        }
    });

    if let Some(n) = duplicate.and_then(|d| patch.node(d).cloned()) {
        let copy = patch.add(&n.block, [n.pos[0] + 24.0, n.pos[1] + 24.0]);
        if let Some(c) = patch.nodes.iter_mut().find(|m| m.id == copy) {
            c.values = n.values;
        }
        changed = true;
    }
    if let Some(r) = remove {
        patch.remove(r);
        changed = true;
    }
    ui.ctx().data_mut(|d| {
        d.insert_temp(id.with("pan"), pan);
        d.insert_temp(id.with("wire"), drag);
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the canvas for one frame with these pointer events (the canvas fills the screen from 0,0).
    fn frame(ctx: &egui::Context, patch: &mut Patch, events: Vec<egui::Event>) -> bool {
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 600.0))), events, ..Default::default() };
        let mut changed = false;
        let mut out = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| changed = patch_canvas(ui, patch, "test"));
        });
        out.textures_delta.clear();
        changed
    }

    fn mouse(pos: egui::Pos2, pressed: Option<bool>) -> Vec<egui::Event> {
        let mut e = vec![egui::Event::PointerMoved(pos)];
        if let Some(pressed) = pressed {
            e.push(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() });
        }
        e
    }

    /// Presses at `a`, drags to `b` in a few steps and lets go. Returns whether anything changed.
    fn drag(ctx: &egui::Context, patch: &mut Patch, a: egui::Pos2, b: egui::Pos2) -> bool {
        let mut changed = frame(ctx, patch, mouse(a, None));
        changed |= frame(ctx, patch, mouse(a, Some(true)));
        for k in 1..=4 {
            changed |= frame(ctx, patch, mouse(a + (b - a) * (k as f32 / 4.0), None));
        }
        changed |= frame(ctx, patch, mouse(b, Some(false)));
        changed |= frame(ctx, patch, vec![]);
        changed
    }

    #[test]
    fn wires_are_dragged_from_an_output_dot_to_an_input_dot_and_unplugged_by_dragging_away() {
        let ctx = egui::Context::default();
        let mut p = Patch::new("Test", false);
        let (input, out) = (p.nodes[0].id, p.nodes[1].id);
        p.disconnect(out, 0);
        let inv = p.add("invert", [300.0, 200.0]);
        frame(&ctx, &mut p, vec![]);
        // Input's output dot: right edge of its title bar; Invert's input dot: left edge of its first input row.
        let from = egui::pos2(20.0 + NODE_W, 40.0 + TITLE_H * 0.5);
        let to = egui::pos2(300.0, 200.0 + TITLE_H + INPUT_H * 0.5);
        assert!(drag(&ctx, &mut p, from, to));
        assert_eq!(p.source_of(inv, 0), Some(input));
        // Dragging the wired end off into empty space unplugs it.
        assert!(drag(&ctx, &mut p, to, egui::pos2(600.0, 500.0)));
        assert_eq!(p.source_of(inv, 0), None);
        // A drop that would loop (Output has no output dot; Invert into itself) does nothing.
        let inv_out = egui::pos2(300.0 + NODE_W, 200.0 + TITLE_H * 0.5);
        drag(&ctx, &mut p, inv_out, to);
        assert_eq!(p.source_of(inv, 0), None);
    }

    #[test]
    fn a_block_moves_by_its_title() {
        let ctx = egui::Context::default();
        let mut p = Patch::new("Test", false);
        frame(&ctx, &mut p, vec![]);
        assert!(drag(&ctx, &mut p, egui::pos2(60.0, 50.0), egui::pos2(160.0, 150.0)), "saved when let go");
        assert_eq!(p.nodes[0].pos, [120.0, 140.0]);
    }
}
