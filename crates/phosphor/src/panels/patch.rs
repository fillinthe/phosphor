//! Patches on the node canvas (`ui::nodes`): blocks wired together into an effect or generator
//! (`phosphor_engine::patch`), shown in the Shader Editor for shaders made with + Patch.

use phosphor_engine::patch::{self, BLOCKS, Group, Patch};

use super::*;
use crate::ui::nodes::{Graph, node_canvas};
#[cfg(test)]
use crate::ui::nodes::{INPUT_H, NODE_W, TITLE_H};

const SETTING_H: f32 = 22.0;

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

impl Graph for Patch {
    fn ids(&self) -> Vec<u32> {
        self.nodes.iter().map(|n| n.id).collect()
    }
    fn pos(&self, id: u32) -> [f32; 2] {
        self.node(id).map_or([0.0; 2], |n| n.pos)
    }
    fn set_pos(&mut self, id: u32, pos: [f32; 2]) {
        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) {
            n.pos = pos;
        }
    }
    fn title(&self, id: u32) -> String {
        self.node(id).and_then(|n| patch::block(&n.block)).map_or_else(String::new, |b| b.name.to_string())
    }
    fn about(&self, id: u32) -> String {
        self.node(id).and_then(|n| patch::block(&n.block)).map_or_else(String::new, |b| b.about.to_string())
    }
    fn colour(&self, id: u32) -> Color32 {
        self.node(id).and_then(|n| patch::block(&n.block)).map_or(Color32::GRAY, |b| group_colour(b.group))
    }
    fn inputs(&self, id: u32) -> Vec<(String, phosphor_engine::PortKind)> {
        self.node(id).and_then(|n| patch::block(&n.block)).map_or_else(Vec::new, |b| b.inputs.iter().map(|s| (s.to_string(), phosphor_engine::PortKind::Picture)).collect())
    }
    fn has_output(&self, id: u32) -> bool {
        self.node(id).is_some_and(|n| n.block != "output")
    }
    fn removable(&self, id: u32) -> bool {
        self.has_output(id)
    }
    fn body_height(&self, id: u32) -> f32 {
        self.node(id).and_then(|n| patch::block(&n.block)).map_or(0.0, |b| b.settings.len() as f32 * SETTING_H)
    }
    fn body(&mut self, ui: &mut egui::Ui, id: u32) -> bool {
        let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) else { return false };
        let Some(b) = patch::block(&n.block) else { return false };
        let mut changed = false;
        let width = ui.available_width();
        for (i, s) in b.settings.iter().enumerate() {
            if n.values.len() <= i {
                n.values.resize(b.settings.len(), 0.0);
                n.values[i] = s.default;
            }
            if Level::range(&mut n.values[i], s.min, s.max, s.name).default(s.default).width(width).show(ui).changed() {
                changed = true;
            }
        }
        changed
    }
    fn source_of(&self, to: u32, input: usize) -> Option<u32> {
        Patch::source_of(self, to, input)
    }
    fn connect(&mut self, from: u32, to: u32, input: usize) -> bool {
        Patch::connect(self, from, to, input)
    }
    fn disconnect(&mut self, to: u32, input: usize) {
        Patch::disconnect(self, to, input);
    }
    fn remove(&mut self, id: u32) {
        Patch::remove(self, id);
    }
    fn duplicate(&mut self, id: u32) {
        if let Some(n) = self.node(id).cloned() {
            let copy = self.add(&n.block, [n.pos[0] + 24.0, n.pos[1] + 24.0]);
            if let Some(c) = self.nodes.iter_mut().find(|m| m.id == copy) {
                c.values = n.values;
            }
        }
    }
    fn add_menu(&mut self, ui: &mut egui::Ui, at: [f32; 2]) -> bool {
        let generator = self.generator;
        let mut added = false;
        for g in Group::ALL.into_iter().filter(|g| *g != Group::Output) {
            ui.menu_button(g.name(), |ui| {
                for b in BLOCKS.iter().filter(|b| b.group == g && (b.id != "input" || !generator)) {
                    if crate::ui::command(ui, b.name).on_hover_text(b.about).clicked() {
                        self.add(b.id, at);
                        added = true;
                        ui.close();
                    }
                }
            });
        }
        if !self.nodes.iter().any(|n| n.block == "output") && crate::ui::command(ui, "Output").clicked() {
            self.add("output", [400.0, 40.0]);
            added = true;
        }
        added
    }
}

/// Draws and edits `patch`; `key` keeps this patch's view (scroll position) apart from others. Returns true when
/// the patch changed (blocks, wires or settings).
pub(crate) fn patch_canvas(ui: &mut egui::Ui, patch: &mut Patch, key: &str) -> bool {
    node_canvas(ui, patch, &format!("patch:{key}"))
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
