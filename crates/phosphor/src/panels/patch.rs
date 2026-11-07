//! Patches on the node canvas (`ui::nodes`): blocks wired together into an effect or generator
//! (`phosphor_engine::patch`), shown in the Shader Editor for shaders made with + Patch.

use phosphor_engine::patch::{self, BLOCKS, Group, Patch};

use super::*;
use crate::ui::nodes::{Graph, INPUT_H, NODE_W, TITLE_H, node_canvas};

const SETTING_H: f32 = 22.0;

fn group_colour(g: Group) -> Color32 {
    match g {
        Group::Source => Color32::from_rgb(70, 110, 160),
        Group::Colour => Color32::from_rgb(150, 90, 150),
        Group::Warp => Color32::from_rgb(170, 120, 60),
        Group::Blur => Color32::from_rgb(80, 140, 130),
        Group::Mix => Color32::from_rgb(110, 110, 170),
        Group::Number => crate::theme::Hue::Control.color(),
        Group::Output => accent(),
        Group::Note => Color32::from_rgb(120, 110, 70),
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
        use phosphor_engine::PortKind;
        self.node(id).and_then(|n| patch::block(&n.block)).map_or_else(Vec::new, |b| {
            b.inputs.iter().map(|s| (s.to_string(), PortKind::Picture)).chain(b.settings.iter().map(|s| (s.name.to_string(), PortKind::Number))).collect()
        })
    }
    fn body_ports(&self, id: u32) -> usize {
        self.node(id).and_then(|n| patch::block(&n.block)).map_or(0, |b| b.settings.len())
    }
    fn output_kind(&self, id: u32) -> phosphor_engine::PortKind {
        if self.node(id).and_then(|n| patch::block(&n.block)).is_some_and(|b| b.number) { phosphor_engine::PortKind::Number } else { phosphor_engine::PortKind::Picture }
    }
    fn has_output(&self, id: u32) -> bool {
        self.node(id).is_some_and(|n| n.block != "output" && n.block != "note")
    }
    fn width(&self, id: u32) -> f32 {
        if self.node(id).is_some_and(|n| n.block == "note") { 240.0 } else { NODE_W }
    }
    fn removable(&self, id: u32) -> bool {
        self.node(id).is_some_and(|n| n.block != "output")
    }
    fn body_height(&self, id: u32) -> f32 {
        match self.node(id) {
            Some(n) if n.block == "note" => 64.0,
            Some(n) => patch::block(&n.block).map_or(0.0, |b| b.settings.len() as f32 * SETTING_H),
            None => 0.0,
        }
    }
    fn body(&mut self, ui: &mut egui::Ui, id: u32) -> bool {
        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == id && n.block == "note") {
            // Saved when the words are let be (the patch recompiles on every change).
            let r = ui.add(egui::TextEdit::multiline(&mut n.text).hint_text("Write a note…").desired_rows(3).desired_width(ui.available_width()).font(egui::FontId::proportional(12.0)));
            return r.lost_focus() && r.changed() || (r.changed() && !r.has_focus());
        }
        let Some(b) = self.node(id).and_then(|n| patch::block(&n.block)) else { return false };
        let mut changed = false;
        let width = ui.available_width();
        let wired: Vec<Option<String>> = (0..b.settings.len()).map(|i| Patch::setting_source(self, id, i).map(|from| self.title(from))).collect();
        let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) else { return false };
        for (i, s) in b.settings.iter().enumerate() {
            if n.values.len() <= i {
                n.values.resize(b.settings.len(), 0.0);
                n.values[i] = s.default;
            }
            if let Some(from) = &wired[i] {
                // Follows a number block: the row says so, as tall as the bar it replaces (its dot sits beside it).
                let h = ui.spacing().interact_size.y.max(18.0);
                let (rect, r) = ui.allocate_exact_size(egui::vec2(width, h), Sense::hover());
                ui.painter().rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
                ui.painter().text(rect.left_center() + egui::vec2(6.0, 0.0), egui::Align2::LEFT_CENTER, format!("{} {} {from}", s.name, icon::ARROW_LEFT), egui::FontId::proportional(12.0), crate::theme::Hue::Control.color());
                r.on_hover_text(format!("{} follows {from} (drag the dot away to unplug it)", s.name));
            } else if Level::range(&mut n.values[i], s.min, s.max, s.name).default(s.default).width(width).show(ui).changed() {
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
        let (picked, searching) = pick_block(ui, generator, |_| true);
        let mut added = false;
        if let Some(b) = picked {
            self.add(b, at);
            added = true;
        }
        if !self.nodes.iter().any(|n| n.block == "output") && crate::ui::command(ui, "Output").clicked() {
            self.add("output", [400.0, 40.0]);
            added = true;
        }
        if !searching && crate::ui::command(ui, label(icon::NOTE_PENCIL, "Note")).on_hover_text("Words on the canvas, wired to nothing").clicked() {
            self.add("note", at);
            added = true;
        }
        added
    }
    fn add_from_menu(&mut self, ui: &mut egui::Ui, at: [f32; 2], from: u32, kind: phosphor_engine::PortKind) -> bool {
        let generator = self.generator;
        // A picture: blocks with a picture input. A number: blocks with a setting (it goes into the first one).
        let number = kind == phosphor_engine::PortKind::Number;
        let (picked, _) = pick_block(ui, generator, |b| if number { !b.settings.is_empty() } else { !b.inputs.is_empty() });
        let Some(b) = picked else { return false };
        let id = self.add(b, at);
        let input = if number { patch::block(b).map_or(0, |b| b.inputs.len()) } else { 0 };
        self.connect(from, id, input);
        true
    }
    fn extra_menu(&mut self, ui: &mut egui::Ui, id: u32) -> bool {
        let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) else { return false };
        let Some(b) = patch::block(&n.block) else { return false };
        if b.settings.is_empty() {
            return false;
        }
        let defaults: Vec<f32> = b.settings.iter().map(|s| s.default).collect();
        if n.values != defaults && crate::ui::command(ui, label(icon::ARROW_COUNTER_CLOCKWISE, "Reset settings")).on_hover_text("Every setting of this box back to its starting value").clicked() {
            n.values = defaults;
            ui.close();
            return true;
        }
        false
    }
    fn clip(&self, ids: &[u32]) -> Option<String> {
        Patch::clip(self, ids)
    }
    fn paste(&mut self, text: &str, at: [f32; 2]) -> Vec<u32> {
        Patch::paste(self, text, at)
    }
    fn lookable(&self, id: u32) -> bool {
        self.node(id).and_then(|n| patch::block(&n.block)).is_some_and(|b| b.id != "output" && !b.number)
    }
    fn looked_at(&self, id: u32) -> bool {
        self.look_at == Some(id)
    }
    fn look_at(&mut self, id: Option<u32>) {
        Patch::set_look_at(self, id);
    }
    fn bypassable(&self, id: u32) -> bool {
        self.node(id).and_then(|n| patch::block(&n.block)).is_some_and(|b| b.id != "output" && !b.inputs.is_empty() && !b.number)
    }
    fn bypassed(&self, id: u32) -> bool {
        self.node(id).is_some_and(|n| n.bypassed)
    }
    fn set_bypassed(&mut self, id: u32, off: bool) {
        Patch::set_bypassed(self, id, off);
    }
    fn insert_menu(&mut self, ui: &mut egui::Ui, to: u32, input: usize) -> bool {
        let generator = self.generator;
        match block_menu(ui, generator, &|b| !b.inputs.is_empty() && !b.number) {
            Some(b) => self.insert(b, to, input).is_some(),
            None => false,
        }
    }
    fn tidy(&mut self) -> bool {
        Patch::tidy(self, NODE_W + 70.0, |n| {
            let b = patch::block(&n.block);
            TITLE_H + b.map_or(0, |b| b.inputs.len()) as f32 * INPUT_H + b.map_or(0, |b| b.settings.len()) as f32 * SETTING_H + 8.0
        });
        true
    }
}

/// A search box, then the blocks that match (Enter adds the first), or the groups when it's empty: the block
/// picked, if one was, and whether a search is on (the menu's other entries hide then). `keep` narrows the list.
fn pick_block(ui: &mut egui::Ui, generator: bool, keep: impl Fn(&patch::Block) -> bool) -> (Option<&'static str>, bool) {
    let key = egui::Id::new("patch_block_search");
    let mut query: String = ui.ctx().data(|d| d.get_temp(key)).unwrap_or_default();
    let field = crate::ui::text_field(ui, &mut query, "Find a block…", Some(200.0));
    if ui.memory(|m| m.focused().is_none()) {
        field.request_focus();
    }
    let q = query.trim().to_lowercase();
    let mut picked = None;
    if !q.is_empty() {
        let found: Vec<&patch::Block> = BLOCKS.iter().filter(|b| b.id != "output" && b.id != "note" && (b.id != "input" || !generator) && keep(b) && (b.name.to_lowercase().contains(&q) || b.about.to_lowercase().contains(&q))).take(12).collect();
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        for (i, b) in found.iter().enumerate() {
            if crate::ui::command(ui, format!("{} · {}", b.name, b.group.name())).on_hover_text(b.about).clicked() || (enter && i == 0) {
                picked = Some(b.id);
                query.clear();
                ui.close();
            }
        }
        if found.is_empty() {
            hint(ui, "No block matches");
        }
    } else {
        picked = block_menu(ui, generator, &keep);
    }
    let searching = !q.is_empty();
    ui.ctx().data_mut(|d| d.insert_temp(key, query));
    (picked, searching)
}

/// The block list by group (the add menu and a wire's menu): the block picked, if one was. `keep` narrows the
/// list (on a wire: blocks with an input, no sources).
fn block_menu(ui: &mut egui::Ui, generator: bool, keep: &dyn Fn(&patch::Block) -> bool) -> Option<&'static str> {
    let mut picked = None;
    for g in Group::ALL.into_iter().filter(|g| *g != Group::Output && *g != Group::Note) {
        let blocks: Vec<&patch::Block> = BLOCKS.iter().filter(|b| b.group == g && (b.id != "input" || !generator) && keep(b)).collect();
        if blocks.is_empty() {
            continue;
        }
        ui.menu_button(g.name(), |ui| {
            for b in blocks {
                if crate::ui::command(ui, b.name).on_hover_text(b.about).clicked() {
                    picked = Some(b.id);
                    ui.close();
                }
            }
        });
    }
    picked
}

/// Draws and edits `patch`; `key` keeps this patch's view (scroll position) apart from others. Returns true when
/// the patch changed (blocks, wires or settings).
pub(crate) fn patch_canvas(ui: &mut egui::Ui, patch: &mut Patch, key: &str) -> bool {
    let key = format!("patch:{key}");
    // Undo / redo: the patch as it was after each change (a drag counts once, when it's let go), kept per patch
    // while the app runs. Cmd+Z / Shift+Cmd+Z with the pointer over the canvas, or the buttons.
    let hist_id = egui::Id::new(("patch_history", &key));
    let mut hist: History = ui.ctx().data(|d| d.get_temp(hist_id)).unwrap_or_default();
    let committed = hist.committed.get_or_insert_with(|| patch.clone()).clone();
    let (mut undo, mut redo) = (false, false);
    crate::ui::toolbar(ui, |ui| {
        crate::ui::nodes::zoom_controls(ui, &key);
        undo = crate::ui::small_command_if(ui, !hist.undo.is_empty(), icon::ARROW_U_UP_LEFT).on_hover_text("Undo (Cmd+Z)").clicked();
        redo = crate::ui::small_command_if(ui, !hist.redo.is_empty(), icon::ARROW_U_UP_RIGHT).on_hover_text("Redo (Shift+Cmd+Z)").clicked();
    });
    let area = ui.available_rect_before_wrap();
    if ui.rect_contains_pointer(area) && !ui.ctx().egui_wants_keyboard_input() {
        let (z, shift) = ui.input(|i| (i.modifiers.command && i.key_pressed(egui::Key::Z), i.modifiers.shift));
        undo |= z && !shift;
        redo |= z && shift;
    }
    let mut changed = node_canvas(ui, patch, &key);
    if undo && let Some(prev) = hist.undo.pop() {
        hist.redo.push(committed.clone());
        *patch = prev;
        hist.committed = Some(patch.clone());
        changed = true;
    } else if redo && let Some(next) = hist.redo.pop() {
        hist.undo.push(committed.clone());
        *patch = next;
        hist.committed = Some(patch.clone());
        changed = true;
    } else if changed {
        hist.undo.push(committed);
        if hist.undo.len() > 50 {
            hist.undo.remove(0);
        }
        hist.redo.clear();
        hist.committed = Some(patch.clone());
    }
    ui.ctx().data_mut(|d| d.insert_temp(hist_id, hist));
    changed
}

/// A patch's undo history (`patch_canvas`): what to go back to, what to go forward to, and the patch as last saved.
#[derive(Clone, Default)]
struct History {
    undo: Vec<Patch>,
    redo: Vec<Patch>,
    committed: Option<Patch>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the canvas for one frame with these pointer events (the canvas alone, without its zoom bar, filling the
    /// screen from 0,0: the boxes are on egui's transformed layer, so this also checks pointers reach them there).
    fn frame(ctx: &egui::Context, patch: &mut Patch, events: Vec<egui::Event>) -> bool {
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 600.0))), events, ..Default::default() };
        let mut changed = false;
        let mut out = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| changed = node_canvas(ui, patch, "patch:test"));
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

    /// A band dragged over the empty canvas selects the boxes it touches; Delete then removes them (Output stays).
    #[test]
    fn a_band_selects_boxes_and_delete_removes_them() {
        let ctx = egui::Context::default();
        let mut p = Patch::new("Test", false);
        let inv = p.add("invert", [300.0, 200.0]);
        frame(&ctx, &mut p, vec![]);
        // From empty canvas above-left of every box, over Input (20,40) and Invert (300,200), but not Output (320,40).
        assert!(!drag(&ctx, &mut p, egui::pos2(5.0, 30.0), egui::pos2(310.0, 230.0)), "a selection isn't a change");
        let id = egui::Id::new(("node_canvas", "patch:test"));
        let sel: Vec<u32> = ctx.data(|d| d.get_temp(id.with("sel"))).unwrap_or_default();
        assert_eq!(sel, vec![p.nodes[0].id, inv]);
        let del = vec![egui::Event::PointerMoved(egui::pos2(500.0, 500.0)), egui::Event::Key { key: egui::Key::Delete, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }];
        assert!(frame(&ctx, &mut p, del));
        assert_eq!(p.nodes.len(), 1, "Input and Invert gone, Output kept");
        assert_eq!(p.nodes[0].block, "output");
    }

    /// Delete a block, Cmd+Z: it's back (the canvas with its zoom bar, as the Shader Editor draws it).
    #[test]
    fn undo_brings_a_deleted_block_back() {
        let ctx = egui::Context::default();
        let mut p = Patch::new("Test", false);
        let inv = p.add("invert", [300.0, 200.0]);
        let run = |ctx: &egui::Context, p: &mut Patch, events: Vec<egui::Event>| {
            // (The held modifiers come from the input state, not the key event: a Cmd+A frame holds Cmd.)
            let modifiers = events.iter().find_map(|e| if let egui::Event::Key { modifiers, .. } = e { Some(*modifiers) } else { None }).unwrap_or_default();
            let events = std::iter::once(egui::Event::ModifiersChanged(modifiers)).chain(events).collect();
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 600.0))), events, ..Default::default() };
            let mut changed = false;
            let mut out = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| changed = patch_canvas(ui, p, "undo"));
            });
            out.textures_delta.clear();
            changed
        };
        run(&ctx, &mut p, vec![]);
        // Cmd+A selects every block, Delete removes the removable ones (Output stays).
        let cmd = egui::Modifiers { command: true, ..Default::default() };
        let over = egui::Event::PointerMoved(egui::pos2(600.0, 500.0));
        run(&ctx, &mut p, vec![over.clone()]);
        let all = egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: cmd };
        run(&ctx, &mut p, vec![over.clone(), all]);
        let del = egui::Event::Key { key: egui::Key::Delete, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() };
        assert!(run(&ctx, &mut p, vec![over.clone(), del]));
        assert!(p.node(inv).is_none(), "deleted");
        assert_eq!(p.nodes.len(), 1);
        let undo = egui::Event::Key { key: egui::Key::Z, physical_key: None, pressed: true, repeat: false, modifiers: cmd };
        assert!(run(&ctx, &mut p, vec![over.clone(), undo]));
        assert!(p.node(inv).is_some(), "back after Cmd+Z");
        assert_eq!(p.nodes.len(), 3);
        let redo = egui::Event::Key { key: egui::Key::Z, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers { command: true, shift: true, ..Default::default() } };
        assert!(run(&ctx, &mut p, vec![over, redo]));
        assert!(p.node(inv).is_none(), "gone again after Shift+Cmd+Z");
    }

    #[test]
    fn a_block_moves_by_its_title() {
        let ctx = egui::Context::default();
        let mut p = Patch::new("Test", false);
        frame(&ctx, &mut p, vec![]);
        assert!(drag(&ctx, &mut p, egui::pos2(60.0, 50.0), egui::pos2(160.0, 150.0)), "saved when let go");
        assert_eq!(p.nodes[0].pos, [120.0, 140.0]);
    }

    /// Zoomed out to half: a box is drawn half size, found under the pointer there, and a drag on screen moves it
    /// twice as far on the canvas.
    #[test]
    fn zoomed_boxes_take_the_pointer() {
        let ctx = egui::Context::default();
        let mut p = Patch::new("Test", false);
        let id = egui::Id::new(("node_canvas", "patch:test"));
        ctx.data_mut(|d| d.insert_temp(id.with("zoom"), 0.5f32));
        frame(&ctx, &mut p, vec![]);
        // The first block sits at (20, 40): its title at half size is about (10..105, 20..31).
        assert!(drag(&ctx, &mut p, egui::pos2(30.0, 25.0), egui::pos2(130.0, 125.0)));
        assert_eq!(p.nodes[0].pos, [220.0, 240.0]);
    }
}
