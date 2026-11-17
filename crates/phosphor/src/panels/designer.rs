//! Designer: a comp built as boxes wired together (Picture, Effect, Mix, Number, Group, Output) on the node canvas.
//! The boxes are `phosphor_engine::Network` (behind each one: ordinary layers or a shared modulator in the comp, see
//! the engine's `designer`), so each box shows its live picture or value and its settings are the same sliders as
//! anywhere else (mappable, modulated). Every setting is also a number input: wire a Number box into it. A Group
//! (box of boxes) opens on a double-click: the panel then edits the network inside it (a breadcrumb leads back), and
//! a Group can be saved by name (`State::box_presets`) and added again from the canvas menu.

use std::collections::HashMap;

use phosphor_engine::control::MAX_SHARED;
use phosphor_engine::{ChainMode, Composition, DataFeed, DataKind, DesignerNode, ISS_READINGS, LiveText, MathMode, ModSource, Network, NodeKind, NumberOp, PortKind, QUAKE_READINGS, SWITCH_INPUTS, SavedBox, SourceSpec, Trigger, WEATHER_READINGS};
use crate::ui::guarded;

use super::*;
use crate::presets::PresetStore;
use crate::ui::nodes::{Graph, INPUT_H, LIGHT_COLOUR, POINTS_COLOUR, TITLE_H, node_canvas};

/// The Points submenu of the canvas: (name, what the box gets, true = a generator on a Picture box, false = an
/// effect on an Effect box).
const POINTS_BOXES: [(&str, &str, bool); 9] = [
    ("Shape", "pts_shape", true),
    ("Model (an OBJ file)…", "model", true),
    ("Relief (a picture as points)", "pts_relief", false),
    ("Move", "pts_move", false),
    ("Particles", "pts_particles", false),
    ("Dots", "pts_dots", false),
    ("Lines", "pts_lines", false),
    ("Mesh", "pts_mesh", false),
    ("Light (wire it into a render)", "pts_light", false),
];

/// The Data submenu: (name, preset id). "data:N" = a Number box with `DataKind::ALL[N]`, "event" = a Number box
/// with an event, "text" = a Picture box showing words (with a Value input).
const DATA_BOXES: [(&str, &str); 7] = [
    ("Live reading (weather, ISS, earthquakes)", "data:0"),
    ("Web (JSON)", "data:1"),
    ("Timer", "data:2"),
    ("Clock", "data:3"),
    ("Table file (CSV)…", "data:4"),
    ("Text (words, with a number in them)", "text"),
    ("Event (a number crossing a line sets a control)", "event"),
];

/// The Numbers submenu's preset ids: "op:N" = a Number box with the N-th operator of `NumberOp::all()`.
const OP_BOXES: [&str; 11] = ["op:0", "op:1", "op:2", "op:3", "op:4", "op:5", "op:6", "op:7", "op:8", "op:9", "op:10"];

const BOX_W: f32 = 230.0;
const ROW_H: f32 = 26.0;
const PARAM_H: f32 = 22.0;
const METER_H: f32 = 12.0;
/// A Note box's words: four lines.
const NOTE_H: f32 = 74.0;

pub(crate) fn designer_panel(ui: &mut egui::Ui, st: &mut State) {
    let cid = st.comp.open_comp();
    let has = cid.and_then(|c| st.comp.comp(c)).is_some_and(|c| c.network.is_some());
    if !has {
        crate::ui::toolbar(ui, |ui| {
            let name = cid.and_then(|c| st.comp.comp(c)).map_or("this comp".to_string(), |c| c.name.clone());
            if crate::ui::action_button(ui, crate::ui::Act::Go, label(icon::GRAPH, &format!("Build {name} as a network"))).clicked() {
                let c = cid.unwrap_or_else(|| st.comp.add_comp("Designer"));
                st.comp.start_network(c);
            }
            if crate::ui::command(ui, label(icon::PLUS, "New Designer comp")).on_hover_text("A new comp, built as a network").clicked() {
                let c = st.comp.add_comp("Designer");
                st.comp.start_network(c);
            }
        });
        crate::ui::empty_note(ui, "Boxes wired together instead of a stack of layers: a Picture into an Effect into the Output. Each box shows its picture; drag from a box's right dot to another's left dot to wire them. The comp's layers stay as they are.");
        return;
    }
    let cid = cid.unwrap();
    // Where we are: the top network, or inside a box of boxes (each step a Group's id).
    let path_id = egui::Id::new(("designer_path", cid));
    let mut path: Vec<u32> = ui.ctx().data(|d| d.get_temp(path_id)).unwrap_or_default();
    let shared = st.mod_inputs.shared;
    let now = ui.ctx().input(|i| i.time);
    // Event boxes pick a target: the list once per frame, only while one exists.
    let targets = if st.comp.modulators.iter().any(|m| m.designer && m.event.is_some()) { crate::control::all_targets(&st.comp) } else { Vec::new() };
    // Undo and Redo here are the app's own (Edit menu, Cmd-Z): one history for everything, so a deleted box comes
    // back with its picture and settings, and a changed setting is a step too. (The Designer used to keep its own
    // list of boxes and wires only: a box came back empty and settings weren't steps at all.)
    let (can_undo, can_redo) = (st.history.can_undo(&st.comp), st.history.can_redo());
    let State { comp, layer_textures, box_presets, notice, .. } = st;
    let net = comp.comp(cid).and_then(|c| c.network.clone()).unwrap_or_default();
    if net.at(&path).is_none() {
        path.clear();
    }
    let name = comp.comp(cid).map_or(String::new(), |c| c.name.clone());
    let mut step: Option<i8> = None;
    crate::ui::toolbar(ui, |ui| {
        let top = format!("{} {name}", icon::GRAPH);
        if path.is_empty() {
            crate::ui::heading(ui, top);
        } else if crate::ui::command(ui, top).on_hover_text("Back to the whole network").clicked() {
            path.clear();
        }
        for i in 0..path.len() {
            ui.label(RichText::new(icon::CARET_RIGHT).small());
            let last = i + 1 == path.len();
            let crumb = net.at(&path[..i]).and_then(|l| l.node(path[i])).map(|n| n.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("Box {}", path[i]));
            if last {
                crate::ui::heading(ui, crumb);
            } else if crate::ui::command(ui, crumb).clicked() {
                path.truncate(i + 1);
            }
        }
        if crate::ui::command_if(ui, can_undo, label(icon::ARROW_COUNTER_CLOCKWISE, "Undo")).on_hover_text("Takes back the last change: a box, a wire, a setting, anything (Cmd-Z, the same Undo as the Edit menu)").clicked() {
            step = Some(-1);
        }
        if crate::ui::command_if(ui, can_redo, label(icon::ARROW_CLOCKWISE, "Redo")).on_hover_text("Puts back what Undo took away (Shift-Cmd-Z)").clicked() {
            step = Some(1);
        }
        crate::ui::nodes::zoom_controls(ui, &format!("designer:{cid}:{path:?}"));
        help(ui, "Right-click the canvas to add a box · drag a box's right dot to another's left dot · the box wired into Output is what shows · a Number box wires into any setting (the blue dots) · Numbers in the menu work on other numbers (math, smooth, count, envelope) · a Switch box shows one of several pictures (a number picks) · a Feedback box brings a box's last frame back around · double-click a box of boxes to build inside it");
    });
    let feeds = super::layers::feed_choices(comp).into_iter().filter(|(id, ..)| !net.parts().contains(id) && *id != cid).collect();
    let aspect = comp.width.max(1) as f32 / comp.height.max(1) as f32;
    let mut view = View { net, comp, cid, path: path.clone(), textures: layer_textures, shared, feeds, aspect, resync: false, boxes: box_presets, go: None, error: None, preset: None, action: None, targets, restart: None, pick_table: None, hovered: false };
    let canvas_changed = node_canvas(ui, &mut view, &format!("designer:{cid}:{path:?}"));
    view.hovered = ui.ui_contains_pointer();
    let View { net, resync, go, error, preset, action, restart, pick_table, hovered, .. } = view;
    if let Some(step) = step {
        ui.ctx().data_mut(|d| d.insert_temp(path_id, path.clone()));
        st.undo_step(step);
        return;
    }
    let _ = hovered;
    let changed = canvas_changed;
    #[cfg(target_arch = "wasm32")]
    let _ = (restart, pick_table);
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(mid) = restart {
        st.data.restart(mid);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(mid) = pick_table {
        st.dialogs.pick_file(crate::dialogs::Purpose::TableFile(mid), Some(("Table", &["csv", "tsv", "txt"])));
    }
    let State { comp, .. } = st;
    if let Some(e) = error {
        *notice = Some(e);
    }
    if let Some(p) = go {
        path = p;
    }
    ui.ctx().data_mut(|d| d.insert_temp(path_id, path.clone()));
    // (A network from a file made by hand, or a box added elsewhere, may still be without its layers.)
    let unmade = net.at(&path).is_some_and(|l| l.nodes.iter().any(|n| !matches!(n.kind, NodeKind::Output | NodeKind::Group | NodeKind::Input | NodeKind::Note) && n.parts.is_empty()));
    if changed || resync || unmade {
        if let Some(c) = comp.comp_mut(cid) {
            c.network = Some(net);
        }
        comp.sync_network(cid);
    }
    // A box added from the Points menu: its shader, now that it has its layer (a Model box asks for its file).
    let mut action = action;
    let mut pick_table_now: Option<u64> = None;
    if let Some((id, shader, generator)) = preset {
        let part = comp.comp(cid).and_then(|c| c.network.as_ref()).and_then(|n| n.at(&path)).and_then(|l| l.node(id)).and_then(|n| n.parts.first().copied());
        let fx = (!generator).then(|| comp.new_effect(shader)).flatten();
        if let Some(l) = part.and_then(|p| comp.layer_mut(p)) {
            if shader == crate::models::FORMAT {
                action = Some((l.id, super::layers::LayerAction::OpenModel));
            } else if shader == "text" {
                l.source = SourceSpec::Text(phosphor_engine::TextStyle { text: "Hello".into(), live_source: "{raw}".into(), ..crate::text::default_style() });
            } else if generator {
                l.set_generator(shader);
            } else if let Some(fx) = fx {
                l.effects = vec![fx];
            }
        }
        // A Data or Event box: the modulator behind the new Number box.
        if let Some(m) = part.and_then(|p| comp.modulators.iter_mut().find(|m| m.id == p)) {
            if let Some(n) = shader.strip_prefix("data:").and_then(|n| n.parse::<usize>().ok()) {
                m.data = DataKind::ALL.get(n).map(|k| DataFeed { kind: k.clone(), ..Default::default() });
                m.signal.source = ModSource::Hand;
                if matches!(m.data.as_ref().map(|d| &d.kind), Some(DataKind::Table { .. })) {
                    pick_table_now = Some(m.id);
                }
            } else if shader == "event" {
                m.event = Some(Trigger::default());
            } else if let Some(n) = shader.strip_prefix("op:").and_then(|n| n.parse::<usize>().ok()) {
                m.op = NumberOp::all().into_iter().nth(n);
                m.name = m.op.as_ref().map_or(m.name.clone(), |o| o.name().to_string());
            }
        }
        comp.sync_network(cid);
    }
    // A box's source picker asked for a file or the browser.
    if let Some((layer, a)) = action {
        super::layers::apply_layer_action(st, layer, a, now);
    }
    #[cfg(target_arch = "wasm32")]
    let _ = pick_table_now;
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(mid) = pick_table_now {
        st.dialogs.pick_file(crate::dialogs::Purpose::TableFile(mid), Some(("Table", &["csv", "tsv", "txt"])));
    }
}

/// What the add menu's search finds for `q` (lower case): (the box to add, the shader / preset it starts with or "",
/// whether that's a generator), its label and tooltip. Box kinds first, then operators, data and points boxes, then
/// every effect and generator by name (their descriptions too once nothing matches by name).
fn find_boxes(q: &str, kinds: &[NodeKind]) -> Vec<((NodeKind, &'static str, bool), String, String)> {
    let hit = |name: &str| name.to_lowercase().contains(q);
    let mut v: Vec<((NodeKind, &'static str, bool), String, String)> = Vec::new();
    v.extend(kinds.iter().filter(|k| hit(k.name())).map(|k| ((*k, "", false), k.name().to_string(), k.about().to_string())));
    v.extend(NumberOp::all().iter().zip(OP_BOXES).filter(|(op, _)| hit(op.name())).map(|(op, preset)| ((NodeKind::Number, preset, false), format!("{} · Number", op.name()), op.about().to_string())));
    v.extend(DATA_BOXES.iter().filter(|(name, _)| hit(name)).map(|(name, preset)| ((if *preset == "text" { NodeKind::Picture } else { NodeKind::Number }, *preset, *preset == "text"), format!("{name} · Data"), String::new())));
    v.extend(POINTS_BOXES.iter().filter(|(name, ..)| hit(name)).map(|(name, shader, generator)| ((if *generator { NodeKind::Picture } else { NodeKind::Effect }, *shader, *generator), format!("{name} · Points"), shaders::info(shader).1.to_string())));
    let listed: Vec<&str> = POINTS_BOXES.iter().map(|(_, shader, _)| *shader).collect();
    let (effects, generators) = (shaders::all_effects(), shaders::all_generators());
    let shaders_found = |by_about: bool| -> Vec<((NodeKind, &'static str, bool), String, String)> {
        let pick = |d: &&&'static shaders::ShaderDesc| !listed.contains(&d.id) && if by_about { !hit(d.name) && shaders::info(d.id).1.to_lowercase().contains(q) } else { hit(d.name) };
        let fx = effects.iter().filter(pick).map(|d| ((NodeKind::Effect, d.id, false), format!("{} · Effect", d.name), shaders::info(d.id).1.to_string()));
        let made = generators.iter().filter(pick).map(|d| ((NodeKind::Picture, d.id, true), format!("{} · Generator", d.name), shaders::info(d.id).1.to_string()));
        fx.chain(made).collect()
    };
    v.extend(shaders_found(false));
    if v.len() < 6 {
        v.extend(shaders_found(true));
    }
    v.truncate(14);
    v
}

/// Every live reading's name.
fn reading_names() -> impl Iterator<Item = &'static str> {
    WEATHER_READINGS.into_iter().chain(ISS_READINGS).chain(QUAKE_READINGS)
}

/// How many setting rows a Data box's kind shows.
fn data_rows(kind: &DataKind) -> f32 {
    match kind {
        DataKind::Reading(_) => 2.0,
        DataKind::Json { .. } => 5.0,
        DataKind::Timer { .. } => 2.0,
        DataKind::Clock => 1.0,
        DataKind::Table { .. } => 4.0,
    }
}

/// The network with what its boxes need to draw themselves: their layers and modulators (in the comp), pictures
/// and this frame's values; `path` = which network inside it is on the canvas.
struct View<'a> {
    net: Network,
    comp: &'a mut Composition,
    cid: u64,
    path: Vec<u32>,
    textures: &'a HashMap<u64, (egui::TextureId, (u32, u32))>,
    shared: [(u64, f32); MAX_SHARED],
    feeds: Vec<(u64, String, bool)>,
    aspect: f32,
    /// Boxes came or went, or wires changed: the layers must follow.
    resync: bool,
    boxes: &'a mut PresetStore,
    /// Open this network (a box of boxes was double-clicked, or a crumb).
    go: Option<Vec<u32>>,
    error: Option<String>,
    /// A box just added from the Points menu: (box, shader id, true = a generator) to set once it has its layer.
    preset: Option<(u32, &'static str, bool)>,
    /// What a box's source picker asked for (a file, the browser), for its layer.
    action: Option<(u64, super::layers::LayerAction)>,
    /// Every control an Event box may set (empty while no box has an event).
    targets: Vec<(ControlTarget, String)>,
    /// A Timer / Table box's Restart was pressed (its modulator).
    restart: Option<u64>,
    /// A Table box wants its file picked (its modulator).
    pick_table: Option<u64>,
    /// The mouse is over the canvas (Cmd-Z undoes a box change then).
    hovered: bool,
}

impl View<'_> {
    fn level(&self) -> Option<&Network> {
        self.net.at(&self.path)
    }

    fn level_mut(&mut self) -> Option<&mut Network> {
        self.net.at_mut(&self.path)
    }

    fn node(&self, id: u32) -> Option<&DesignerNode> {
        self.level()?.node(id)
    }

    fn preview_h(&self) -> f32 {
        (BOX_W - 12.0) / self.aspect
    }

    /// The comp's copy of the network follows this one (before the comp is asked to wire or load).
    fn store(&mut self) {
        if let Some(c) = self.comp.comp_mut(self.cid) {
            c.network = Some(self.net.clone());
        }
    }

    fn reload(&mut self) {
        if let Some(net) = self.comp.comp(self.cid).and_then(|c| c.network.clone()) {
            self.net = net;
        }
    }

    /// The live picture of the layer / group `part` (or, for Output, of what's wired in).
    fn preview(&self, ui: &mut egui::Ui, part: Option<u64>) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), self.preview_h()), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 3.0, Color32::BLACK);
        match part.and_then(|id| self.textures.get(&id)) {
            Some((tex, _)) => paint_texture(p, *tex, fit_rect(rect, self.aspect)),
            None => {
                p.text(rect.center(), egui::Align2::CENTER_CENTER, "nothing yet", egui::FontId::proportional(11.0), crate::theme::faint());
            }
        }
    }

    /// The Number box's settings: its value now, its source, speed and sync, and how it chains.
    fn number_body(&mut self, ui: &mut egui::Ui, n: &DesignerNode, id: u32) -> bool {
        let width = ui.available_width();
        let Some(mid) = n.modulator() else {
            hint(ui, "The modulator list is full (16): remove one in LFOs & Modulators");
            return false;
        };
        let value = self.shared.iter().find(|(k, _)| *k == mid).map_or(0.0, |x| x.1);
        ui.add(crate::ui::meter(value).desired_width(width).desired_height(METER_H)).on_hover_text("Its value now (0 to 1)");
        let Some(m) = self.comp.modulators.iter_mut().find(|m| m.id == mid) else { return false };
        let mut changed = false;
        let is_op = m.op.is_some();
        if let Some(op) = m.op.as_mut() {
            op_rows(ui, op, id, width);
        }
        if !is_op {
        let shown = m.signal.source.name().to_string();
        crate::ui::dropdown(ui, ("designer_source", id), width - 20.0).selected_text(shown).show_ui(ui, |ui| {
            for s in ModSource::ALL.into_iter().filter(|s| *s != ModSource::Drawn) {
                let name = if s == ModSource::Hand { "Knob (a value you set or map)" } else { s.name() };
                if crate::ui::pick_item(ui, m.signal.source == s, name).clicked() {
                    m.signal.source = s;
                    changed = true;
                }
            }
        });
        if m.signal.source == ModSource::Hand {
            // (Mappable: right-click › Learn maps a MIDI / OSC knob to it.)
            Level::percent(&mut m.value, "Value").default(0.5).width(width).target(ControlTarget::Modulator(mid)).show(ui);
        } else if m.signal.source.is_audio() {
            hint(ui, "Follows the sound Phosphor hears (Audio)");
        } else if m.signal.sync {
            Level::range(&mut m.signal.rate, 0.25, 32.0, "Beats per cycle").log().default(4.0).width(width).show(ui);
        } else {
            Level::range(&mut m.signal.rate, 0.05, 20.0, "Cycles a second").log().default(0.5).width(width).show(ui);
        }
        // Data: where a Knob's value comes from.
        if m.signal.source == ModSource::Hand {
            let shown = m.data.as_ref().map_or("No data", |d| d.kind.name()).to_string();
            crate::ui::dropdown(ui, ("designer_data", id), width - 20.0).selected_text(shown).show_ui(ui, |ui| {
                if crate::ui::pick_item(ui, m.data.is_none(), "No data (the knob)").clicked() {
                    m.data = None;
                    changed = true;
                }
                for k in DataKind::ALL {
                    let on = m.data.as_ref().is_some_and(|d| d.kind.same(&k));
                    if crate::ui::pick_item(ui, on, k.name()).on_hover_text(k.about()).clicked() && !on {
                        if matches!(k, DataKind::Table { .. }) {
                            self.pick_table = Some(mid);
                        }
                        m.data = Some(DataFeed { kind: k, ..Default::default() });
                        changed = true;
                    }
                }
            });
        }
        if let Some(d) = m.data.as_mut().filter(|_| m.signal.source == ModSource::Hand) {
            match &mut d.kind {
                DataKind::Reading(name) => {
                    let shown = if name.is_empty() { "Pick a reading…".to_string() } else { name.clone() };
                    crate::ui::dropdown(ui, ("designer_reading", id), width - 20.0).selected_text(shown).show_ui(ui, |ui| {
                        for r in reading_names() {
                            if crate::ui::pick_item(ui, name == r, r).clicked() {
                                *name = r.to_string();
                            }
                        }
                    });
                }
                DataKind::Json { url, path, every } => {
                    crate::ui::text_field(ui, url, "https://… (answers JSON)", Some(width));
                    crate::ui::text_field(ui, path, "Path to the number: main.temp", Some(width));
                    Level::range(every, 2.0, 600.0, "Ask every (s)").log().default(60.0).width(width).show(ui);
                    crate::ui::toolbar(ui, |ui| {
                        guarded!(ui, d.min, egui::DragValue::new(&mut d.min).speed(0.1).prefix("0 at ")).on_hover_text("The number that means 0");
                        guarded!(ui, d.max, egui::DragValue::new(&mut d.max).speed(0.1).prefix("1 at ")).on_hover_text("The number that means 1");
                    });
                }
                DataKind::Timer { seconds } => {
                    Level::range(seconds, 0.1, 600.0, "Seconds").log().default(10.0).width(width).show(ui);
                    if crate::ui::command(ui, label(icon::ARROW_COUNTER_CLOCKWISE, "Restart")).clicked() {
                        self.restart = Some(mid);
                    }
                }
                DataKind::Clock => {}
                DataKind::Table { path, column, seconds } => {
                    let file = std::path::Path::new(path.as_str()).file_name().map_or_else(|| "Pick a file…".to_string(), |f| f.to_string_lossy().to_string());
                    if crate::ui::command(ui, label(icon::FOLDER_OPEN, &file)).on_hover_text(path.as_str()).clicked() {
                        self.pick_table = Some(mid);
                    }
                    crate::ui::toolbar(ui, |ui| {
                        guarded!(ui, *column, egui::DragValue::new(column).range(0..=99).prefix("column ")).on_hover_text("Which column (the first is 0)");
                        guarded!(ui, d.min, egui::DragValue::new(&mut d.min).speed(0.1).prefix("0 at "));
                        guarded!(ui, d.max, egui::DragValue::new(&mut d.max).speed(0.1).prefix("1 at "));
                    });
                    Level::range(seconds, 0.05, 60.0, "Seconds a row").log().default(1.0).width(width).show(ui);
                    if crate::ui::command(ui, label(icon::ARROW_COUNTER_CLOCKWISE, "Restart")).clicked() {
                        self.restart = Some(mid);
                    }
                }
            }
            let status = match &m.live.problem {
                Some(p) => p.clone(),
                None if !m.live.shown.is_empty() => format!("Now: {}", m.live.shown),
                None => "Waiting for data…".to_string(),
            };
            hint(ui, status);
        }
        }
        crate::ui::toolbar(ui, |ui| {
            if !is_op && !m.signal.source.is_audio() && m.signal.source != ModSource::Hand {
                crate::ui::toggle(ui, &mut m.signal.sync, "Beat").on_hover_text("Its cycle follows the tempo");
            }
            let mut event = m.event.is_some();
            if crate::ui::toggle(ui, &mut event, "Event").on_hover_text("When the value crosses a line, set a control").changed() {
                m.event = event.then(Trigger::default);
                changed = true;
            }
            if let Some(c) = m.chain.as_mut().filter(|_| !is_op) {
                let mut mode = c.mode;
                crate::ui::dropdown(ui, ("designer_chain", id), 110.0).selected_text(mode.name()).show_ui(ui, |ui| {
                    for k in ChainMode::ALL {
                        if crate::ui::pick_item(ui, mode == k, k.name()).clicked() {
                            mode = k;
                        }
                    }
                });
                c.mode = mode;
            }
        });
        if let Some(e) = m.event.as_mut() {
            Level::percent(&mut e.threshold, "Line").default(0.5).width(width).show(ui);
            if let Some(t) = crate::ui::target_picker(ui, ("designer_event", id), &self.targets, e.target.as_ref(), width - 20.0) {
                e.target = Some(t);
            }
            Level::percent(&mut e.value, "Set it to").default(1.0).width(width).show(ui);
            crate::ui::toolbar(ui, |ui| {
                crate::ui::toggle(ui, &mut e.both_ways, "Both ways").on_hover_text("Fire on the way back down too");
                let mut sends = e.send.is_some();
                if crate::ui::toggle(ui, &mut sends, "Sends").on_hover_text("Also sends the value out (OSC to the Control Surface's OSC out, MIDI, an Arduino pin)").changed() {
                    e.send = sends.then(|| phosphor_engine::SendTo::Osc { address: "/phosphor/event".into() });
                }
            });
            if let Some(to) = &mut e.send {
                crate::ui::toolbar(ui, |ui| crate::ui::send_to_editor(ui, to));
            }
        }
        changed
    }

    /// The effect behind an Effect box.
    fn box_effect(&self, id: u32) -> Option<&phosphor_engine::EffectState> {
        let n = self.node(id).filter(|n| n.kind == NodeKind::Effect)?;
        self.comp.layer(*n.parts.first()?)?.effects.first()
    }

    /// Adds a saved box here.
    fn add_saved(&mut self, json: &str, at: [f32; 2]) -> bool {
        let Ok(saved) = serde_json::from_str::<SavedBox>(json) else {
            self.error = Some("That saved box can't be read".into());
            return false;
        };
        self.store();
        let path = self.path.clone();
        let added = self.comp.load_box(self.cid, &path, &saved, at).is_some();
        self.reload();
        added
    }
}

impl Graph for View<'_> {
    fn ids(&self) -> Vec<u32> {
        self.level().map_or_else(Vec::new, |l| l.nodes.iter().map(|n| n.id).collect())
    }
    fn pos(&self, id: u32) -> [f32; 2] {
        self.node(id).map_or([0.0; 2], |n| n.pos)
    }
    fn set_pos(&mut self, id: u32, pos: [f32; 2]) {
        if let Some(n) = self.level_mut().and_then(|l| l.node_mut(id)) {
            n.pos = pos;
        }
    }
    fn title(&self, id: u32) -> String {
        let Some(n) = self.node(id) else { return String::new() };
        if n.kind == NodeKind::Note {
            return "Note".to_string();
        }
        if !n.name.is_empty() {
            return n.name.clone();
        }
        match n.kind {
            NodeKind::Feedback => match self.node(n.target).filter(|_| n.target != 0) {
                Some(_) => format!("Feedback of {}", self.title(n.target)),
                None => "Feedback".to_string(),
            },
            NodeKind::Layer => self.comp.layer(n.layer).map_or_else(|| "Layer".to_string(), |l| format!("Layer: {}", l.name)),
            NodeKind::Picture => n.parts.first().and_then(|p| self.comp.layer(*p)).map_or_else(|| "Picture".to_string(), |l| l.source.label()),
            NodeKind::Effect => n.parts.first().and_then(|p| self.comp.layer(*p)).and_then(|l| l.effects.first()).and_then(|e| shaders::effect(&e.effect)).map_or_else(|| "Effect".to_string(), |d| d.name.to_string()),
            NodeKind::Number => n.modulator().and_then(|m| self.comp.modulators.iter().find(|x| x.id == m)).map_or_else(
                || "Number".to_string(),
                |m| match (&m.data, &m.event) {
                    _ if m.op.is_some() => m.op.as_ref().map_or(String::new(), |o| o.name().to_string()),
                    (Some(d), _) if m.signal.source == ModSource::Hand => d.kind.name().to_string(),
                    (None, Some(_)) => "Event".to_string(),
                    _ => m.signal.source.name().to_string(),
                },
            ),
            NodeKind::Group => format!("Box {id}"),
            NodeKind::Input => format!("In {}", self.level().map_or(0, |l| l.input_boxes().iter().position(|x| *x == id).unwrap_or(0)) + 1),
            NodeKind::NumberInput => format!("Number {}", self.level().map_or(0, |l| l.number_input_boxes().iter().position(|x| *x == id).unwrap_or(0)) + 1),
            other => other.name().to_string(),
        }
    }
    fn about(&self, id: u32) -> String {
        self.node(id).map_or_else(String::new, |n| n.kind.about().to_string())
    }
    fn colour(&self, id: u32) -> Color32 {
        match self.node(id).map(|n| self.comp.node_output(n)) {
            Some(PortKind::Points) => return POINTS_COLOUR,
            Some(PortKind::Light) => return LIGHT_COLOUR,
            _ => {}
        }
        match self.node(id).map(|n| n.kind) {
            Some(NodeKind::Layer | NodeKind::Picture | NodeKind::Input | NodeKind::Feedback) => crate::theme::Hue::Media.color(),
            Some(NodeKind::Effect) => crate::theme::Hue::Effect.color(),
            Some(NodeKind::Mix | NodeKind::Switch) => crate::theme::Hue::Live.color(),
            Some(NodeKind::Number | NodeKind::NumberInput) => crate::theme::Hue::Control.color(),
            Some(NodeKind::Group) => crate::theme::Hue::Text.color(),
            Some(NodeKind::Note) => crate::theme::faint(),
            _ => accent(),
        }
    }
    fn inputs(&self, id: u32) -> Vec<(String, PortKind)> {
        self.node(id).map_or_else(Vec::new, |n| self.comp.node_inputs(n))
    }
    fn has_output(&self, id: u32) -> bool {
        self.node(id).is_some_and(|n| !matches!(n.kind, NodeKind::Output | NodeKind::Note))
    }
    fn output_kind(&self, id: u32) -> PortKind {
        self.node(id).map_or(PortKind::Picture, |n| self.comp.node_output(n))
    }
    fn removable(&self, id: u32) -> bool {
        self.node(id).is_some_and(|n| n.kind != NodeKind::Output)
    }
    fn width(&self, _id: u32) -> f32 {
        BOX_W
    }
    fn body_height(&self, id: u32) -> f32 {
        let Some(n) = self.node(id) else { return 0.0 };
        let pic = self.preview_h() + 3.0;
        match n.kind {
            NodeKind::Layer | NodeKind::Feedback => pic + ROW_H,
            NodeKind::Note => NOTE_H,
            NodeKind::Picture => {
                let text = n.parts.first().and_then(|p| self.comp.layer(*p)).is_some_and(|l| matches!(l.source, SourceSpec::Text(_)));
                pic + ROW_H + if text { PARAM_H } else { 0.0 }
            }
            NodeKind::Group => pic + ROW_H,
            NodeKind::Effect => {
                let fx = n.parts.first().and_then(|p| self.comp.layer(*p)).and_then(|l| l.effects.first());
                let params = fx.and_then(|e| shaders::effect(&e.effect)).map_or(0, |d| d.params.len());
                // (A light has no Mix.)
                let mix = if fx.is_some_and(|e| e.effect == shaders::POINTS_LIGHT) { 0.0 } else { 1.0 };
                pic + ROW_H + (params as f32 + mix) * PARAM_H
            }
            NodeKind::Mix | NodeKind::Switch => pic + ROW_H + PARAM_H,
            NodeKind::Number => {
                let m = n.modulator().and_then(|m| self.comp.modulators.iter().find(|x| x.id == m));
                let data = m.and_then(|m| m.data.as_ref().filter(|_| m.signal.source == ModSource::Hand)).map_or(0.0, |d| 1.0 + data_rows(&d.kind));
                let event = if m.is_some_and(|m| m.event.is_some()) { 4.0 } else { 0.0 };
                if let Some(op) = m.and_then(|m| m.op.as_ref()) {
                    return METER_H + ROW_H + PARAM_H * (op_row_count(op) + event) + 6.0;
                }
                METER_H + ROW_H + PARAM_H * (2.0 + data + event) + 6.0
            }
            NodeKind::Output | NodeKind::Input => pic,
            NodeKind::NumberInput => METER_H + 6.0,
        }
    }
    fn body(&mut self, ui: &mut egui::Ui, id: u32) -> bool {
        let Some(n) = self.node(id).cloned() else { return false };
        let width = ui.available_width();
        match n.kind {
            // One of your own layers, as it is: pick which. Nothing is made for it.
            NodeKind::Layer => {
                self.preview(ui, (n.layer != 0).then_some(n.layer));
                let shown = self.feeds.iter().find(|(l, _, is_layer)| *is_layer && *l == n.layer).map_or("Pick a layer…".to_string(), |(_, name, _)| name.clone());
                let mut pick: Option<u64> = None;
                crate::ui::dropdown(ui, ("designer_layer", id), width - 20.0).selected_text(shown).show_ui(ui, |ui| {
                    crate::ui::column_scroll(ui).max_height(320.0).show(ui, |ui| {
                        for (l, name, _) in self.feeds.iter().filter(|(_, _, is_layer)| *is_layer) {
                            if crate::ui::pick_item(ui, n.layer == *l, name).clicked() {
                                pick = Some(*l);
                            }
                        }
                        if !self.feeds.iter().any(|(_, _, is_layer)| *is_layer) {
                            hint(ui, "No layers yet: make one in Layers or Clips");
                        }
                    });
                });
                if let Some(l) = pick {
                    if let Some(node) = self.level_mut().and_then(|x| x.node_mut(id)) {
                        node.layer = l;
                    }
                    return true;
                }
            }
            NodeKind::Picture => {
                self.preview(ui, n.parts.first().copied());
                let before = n.parts.first().and_then(|p| self.comp.layer(*p)).map(|l| l.source.clone());
                if let Some(l) = n.parts.first().and_then(|p| self.comp.layer_mut(*p)) {
                    let id = l.id;
                    if let Some(a) = super::layers::source_picker(ui, l, &self.feeds) {
                        self.action = Some((id, a));
                    }
                    // Words: the text itself, or (with a Value wired in) a template the number fills.
                    if let SourceSpec::Text(style) = &mut l.source {
                        if matches!(style.live, LiveText::Number(_)) {
                            crate::ui::text_field(ui, &mut style.live_source, "Words with {value}, {percent} or {raw}", Some(width)).on_hover_text("{value} = 0 to 1, {percent} = 0 to 100, {raw} = the reading itself (21 °C, a table cell)");
                        } else {
                            crate::ui::text_field(ui, &mut style.text, "Words", Some(width));
                        }
                    }
                    // A new generator has other settings: its number inputs change.
                    return before.as_ref() != Some(&l.source);
                }
            }
            NodeKind::Effect => {
                let Some(part) = n.parts.first().copied() else { return false };
                match self.comp.layer(part).and_then(|l| l.effects.first()).filter(|e| e.effect == shaders::POINTS_LIGHT) {
                    Some(fx) => light_ball(ui, &fx.values, self.preview_h()),
                    None => self.preview(ui, Some(part)),
                }
                let now = self.comp.layer(part).and_then(|l| l.effects.first()).map(|e| e.effect.clone());
                let shown = now.as_deref().and_then(shaders::effect).map_or("Pick an effect…".to_string(), |d| d.name.to_string());
                let mut pick: Option<&'static str> = None;
                crate::ui::dropdown(ui, ("designer_fx", id), width - 20.0).selected_text(shown).show_ui(ui, |ui| {
                    crate::ui::column_scroll(ui).max_height(320.0).show(ui, |ui| {
                        for d in shaders::all_effects() {
                            if crate::ui::pick_item(ui, now.as_deref() == Some(d.id), d.name).clicked() {
                                pick = Some(d.id);
                            }
                        }
                    });
                });
                if let Some(fx_id) = pick
                    && let Some(fx) = self.comp.new_effect(fx_id)
                    && let Some(l) = self.comp.layer_mut(part)
                {
                    l.effects = vec![fx];
                    return true;
                }
                if let Some(l) = self.comp.layer_mut(part)
                    && let Some(fx) = l.effects.first_mut()
                    && let Some(d) = shaders::effect(&fx.effect)
                {
                    for (i, p) in d.params.iter().enumerate() {
                        crate::ui::param_slider(ui, &mut fx.values, &mut fx.mods, i, p, Some(ControlTarget::EffectParam { effect: fx.id, index: i }));
                    }
                    if d.id != shaders::POINTS_LIGHT {
                        Level::percent(&mut fx.mix, "Mix").default(1.0).width(width).target(ControlTarget::EffectMix(fx.id)).show(ui);
                    }
                }
            }
            NodeKind::Mix => {
                self.preview(ui, n.parts.first().copied());
                if let Some(b) = n.parts.get(2).and_then(|p| self.comp.layer_mut(*p)) {
                    crate::ui::choice_combo(ui, ("designer_blend", id), &mut b.blend, width - 20.0, false).on_hover_text("How B blends over A");
                    Level::percent(&mut b.opacity, "B opacity").modulation(&mut b.opacity_mod).width(width).target(ControlTarget::LayerOpacity(b.id)).show(ui);
                }
            }
            NodeKind::Note => {
                let mut words = n.name.clone();
                if crate::ui::note_field(ui, &mut words, "Write a note…", 4).changed() {
                    if let Some(node) = self.level_mut().and_then(|x| x.node_mut(id)) {
                        node.name = words;
                    }
                    return true;
                }
            }
            // One of the pictures wired in: clicked here, or picked by the number wired into Pick.
            NodeKind::Switch => {
                self.preview(ui, n.parts.first().copied());
                let wired = self.level().is_some_and(|l| l.source_of(id, SWITCH_INPUTS).is_some());
                let options: Vec<(u32, String)> = (0..SWITCH_INPUTS as u32).map(|k| (k, (k + 1).to_string())).collect();
                // A number picking: the buttons show which picture is on now.
                let live = self.level().and_then(|l| l.source_of(id, SWITCH_INPUTS).and_then(|f| l.node(f)).and_then(|f| f.modulator()).map(|m| (m, l.switch_count(id))));
                let now = live.map(|(m, count)| (self.shared.iter().find(|(k, _)| *k == m).map_or(0.0, |x| x.1) * (count - 1) as f32).round() as u32);
                let (mut pick, mut fade) = (now.unwrap_or(n.target), n.fade);
                ui.add_enabled_ui(!wired, |ui| {
                    crate::ui::segmented(ui, &mut pick, &options, |_, r, _| {
                        r.clone().on_hover_text("Show this picture").on_disabled_hover_text("The number wired into Pick chooses");
                    });
                });
                Level::percent(&mut fade, "Fade").default(0.0).width(width).show(ui).on_hover_text("How much of each step is a fade when a number picks: 0% cuts, 100% is always fading from one picture to the next");
                if !wired && pick != n.target || fade != n.fade {
                    let pick = if wired { n.target } else { pick };
                    if let Some(node) = self.level_mut().and_then(|x| x.node_mut(id)) {
                        (node.target, node.fade) = (pick, fade);
                    }
                    return true;
                }
            }
            NodeKind::Number => return self.number_body(ui, &n, id),
            // Another box's last frame: pick which (a wire would be a loop, so it's picked, not wired).
            NodeKind::Feedback => {
                self.preview(ui, n.parts.first().copied());
                let boxes: Vec<(u32, String)> = self.level().map_or_else(Vec::new, |l| {
                    l.nodes
                        .iter()
                        .filter(|x| x.id != id && !matches!(x.kind, NodeKind::Output | NodeKind::Number | NodeKind::NumberInput | NodeKind::Feedback | NodeKind::Note) && self.comp.node_output(x) == PortKind::Picture)
                        .map(|x| (x.id, format!("{} · box {}", self.title(x.id), x.id)))
                        .collect()
                });
                let shown = boxes.iter().find(|(b, _)| *b == n.target).map_or("Pick a box…".to_string(), |(_, t)| t.clone());
                let mut pick: Option<u32> = None;
                crate::ui::dropdown(ui, ("designer_feedback", id), width - 20.0).selected_text(shown).show_ui(ui, |ui| {
                    crate::ui::column_scroll(ui).max_height(320.0).show(ui, |ui| {
                        for (b, name) in &boxes {
                            if crate::ui::pick_item(ui, n.target == *b, name).clicked() {
                                pick = Some(*b);
                            }
                        }
                        if boxes.is_empty() {
                            hint(ui, "No other picture boxes yet");
                        }
                    });
                });
                if let Some(b) = pick {
                    if let Some(node) = self.level_mut().and_then(|x| x.node_mut(id)) {
                        node.target = b;
                    }
                    return true;
                }
            }
            NodeKind::Group => {
                self.preview(ui, n.picture());
                let boxes = n.inner.as_ref().map_or(0, |i| i.nodes.len());
                hint(ui, format!("{boxes} boxes inside · double-click to build inside"));
            }
            NodeKind::Input => self.preview(ui, n.picture()),
            NodeKind::NumberInput => {
                let value = n.modulator().and_then(|m| self.shared.iter().find(|(k, _)| *k == m)).map_or(0.0, |x| x.1);
                ui.add(crate::ui::meter(value).desired_width(width).desired_height(METER_H)).on_hover_text("The number wired into the box, now");
            }
            NodeKind::Output => {
                let shown = self.level().and_then(|l| l.shown()).and_then(|s| self.node(s)).and_then(|n| n.picture());
                self.preview(ui, shown);
            }
        }
        false
    }
    fn source_of(&self, to: u32, input: usize) -> Option<u32> {
        self.level()?.source_of(to, input)
    }
    fn connect(&mut self, from: u32, to: u32, input: usize) -> bool {
        // (The comp knows every box's inputs, numbers included: it does the wiring.)
        self.store();
        let path = self.path.clone();
        let ok = self.comp.connect_boxes(self.cid, &path, from, to, input);
        if ok {
            self.reload();
            self.resync = true;
        }
        ok
    }
    fn disconnect(&mut self, to: u32, input: usize) {
        if let Some(l) = self.level_mut() {
            l.disconnect(to, input);
        }
        self.resync = true;
    }
    fn remove(&mut self, id: u32) {
        if let Some(l) = self.level_mut() {
            l.remove(id);
        }
        self.resync = true;
    }
    fn tidy(&mut self) -> bool {
        let ids = self.ids();
        let heights: Vec<(u32, f32)> = ids.iter().map(|id| (*id, TITLE_H + self.inputs(*id).len() as f32 * INPUT_H + self.body_height(*id) + 6.0)).collect();
        let widest = ids.iter().map(|id| self.width(*id)).fold(0.0, f32::max);
        self.store();
        let Some(level) = self.level_mut() else { return false };
        level.tidy(widest + 70.0, |id| heights.iter().find(|(n, _)| *n == id).map_or(60.0, |(_, h)| *h));
        true
    }
    fn duplicate(&mut self, id: u32) {
        // (The comp copies the box with its settings; a box of boxes with everything inside.)
        self.store();
        let path = self.path.clone();
        if self.comp.duplicate_box(self.cid, &path, id).is_some() {
            self.reload();
        }
        self.resync = true;
    }
    fn add_menu(&mut self, ui: &mut egui::Ui, at: [f32; 2]) -> bool {
        let inside = !self.path.is_empty();
        let has_output = self.level().is_some_and(|l| l.nodes.iter().any(|n| n.kind == NodeKind::Output));
        let kinds: Vec<NodeKind> = NodeKind::ALL.into_iter().filter(|k| *k != NodeKind::Output || !has_output).chain(inside.then_some(NodeKind::Input)).chain(inside.then_some(NodeKind::NumberInput)).collect();
        // Type to find any box: a kind, a number operator, a data or points box, or any effect or generator by name.
        let (found, searching) = crate::ui::menu_search(ui, "designer_box_search", "Find a box, effect or generator…", "Nothing by that name", |q| find_boxes(q, &kinds));
        if let Some((kind, preset, generator)) = found {
            if let Some(l) = self.level_mut() {
                let id = l.add(kind, at);
                if !preset.is_empty() {
                    self.preset = Some((id, preset, generator));
                }
            }
            self.resync = true;
            return true;
        }
        if searching {
            return false;
        }
        for kind in kinds {
            if crate::ui::command(ui, kind.name()).on_hover_text(kind.about()).clicked() {
                if let Some(l) = self.level_mut() {
                    l.add(kind, at);
                }
                self.resync = true;
                return true;
            }
        }
        let mut added = false;
        ui.menu_button("Points", |ui| {
            hint(ui, "Points in space: a Shape or a Relief makes them, Move and Particles change them, Dots, Lines or Mesh draw them (camera and light inside)");
            for (name, shader, generator) in POINTS_BOXES {
                let about = shaders::info(shader).1;
                if crate::ui::command(ui, name).on_hover_text(about).clicked() {
                    let kind = if generator { NodeKind::Picture } else { NodeKind::Effect };
                    if let Some(l) = self.level_mut() {
                        let id = l.add(kind, at);
                        self.preset = Some((id, shader, generator));
                    }
                    self.resync = true;
                    added = true;
                    ui.close();
                }
            }
        });
        ui.menu_button("Numbers", |ui| {
            hint(ui, "Boxes that work on other numbers: wire a Number box in, wire the result into any setting");
            for (op, preset) in NumberOp::all().iter().zip(OP_BOXES) {
                if crate::ui::command(ui, op.name()).on_hover_text(op.about()).clicked() {
                    if let Some(l) = self.level_mut() {
                        let id = l.add(NodeKind::Number, at);
                        self.preset = Some((id, preset, false));
                    }
                    self.resync = true;
                    added = true;
                    ui.close();
                }
            }
        });
        ui.menu_button("Data", |ui| {
            hint(ui, "Numbers from the world: a live reading, a web address, a timer, the clock or a table; words that show a number; an event when a number crosses a line");
            for (name, preset) in DATA_BOXES {
                if crate::ui::command(ui, name).clicked() {
                    let kind = if preset == "text" { NodeKind::Picture } else { NodeKind::Number };
                    if let Some(l) = self.level_mut() {
                        let id = l.add(kind, at);
                        self.preset = Some((id, preset, kind == NodeKind::Picture));
                    }
                    self.resync = true;
                    added = true;
                    ui.close();
                }
            }
        });
        if added {
            return true;
        }
        self.boxes.ensure_loaded();
        let saved: Vec<(String, String)> = self.boxes.builtins().chain(self.boxes.user.iter().cloned()).map(|p| (p.name, p.json)).collect();
        if !saved.is_empty() {
            ui.separator();
            hint(ui, "Saved boxes:");
            for (name, json) in saved {
                if crate::ui::command(ui, label(icon::PACKAGE, &name)).clicked() {
                    return self.add_saved(&json, at);
                }
            }
        }
        false
    }
    /// A wire let go on empty canvas: the boxes that take what it carries; the one picked comes wired from `from`.
    fn add_from_menu(&mut self, ui: &mut egui::Ui, at: [f32; 2], from: u32, kind: PortKind) -> bool {
        let has_output = self.level().is_some_and(|l| l.nodes.iter().any(|n| n.kind == NodeKind::Output));
        let takes = |b: &(NodeKind, &'static str, bool)| match kind {
            PortKind::Picture => match b.0 {
                NodeKind::Effect => b.1 != shaders::POINTS_LIGHT && !shaders::points_in(b.1),
                NodeKind::Output => !has_output,
                k => matches!(k, NodeKind::Mix | NodeKind::Switch),
            },
            PortKind::Number => b.0 == NodeKind::Number && (b.1.is_empty() || b.1.starts_with("op:")),
            PortKind::Points => b.0 == NodeKind::Effect && shaders::points_in(b.1),
            PortKind::Light => false,
        };
        let kinds = NodeKind::ALL;
        let (found, searching) = crate::ui::menu_search(ui, "designer_wire_search", "Find a box or effect…", "Nothing by that name takes this wire", |q| find_boxes(q, &kinds).into_iter().filter(|(b, ..)| takes(b)).collect());
        let mut pick = found;
        if !searching {
            let listed: Vec<((NodeKind, &'static str, bool), String, String)> = match kind {
                PortKind::Picture => [NodeKind::Effect, NodeKind::Mix, NodeKind::Switch, NodeKind::Output].into_iter().map(|k| ((k, "", false), k.name().to_string(), k.about().to_string())).collect(),
                PortKind::Number => std::iter::once(((NodeKind::Number, "", false), "Number (chained)".to_string(), NodeKind::Number.about().to_string()))
                    .chain(NumberOp::all().iter().zip(OP_BOXES).map(|(op, preset)| ((NodeKind::Number, preset, false), op.name().to_string(), op.about().to_string())))
                    .collect(),
                PortKind::Points => POINTS_BOXES.iter().map(|(name, shader, generator)| ((if *generator { NodeKind::Picture } else { NodeKind::Effect }, *shader, *generator), name.to_string(), shaders::info(shader).1.to_string())).collect(),
                PortKind::Light => Vec::new(),
            };
            let listed: Vec<_> = listed.into_iter().filter(|(b, ..)| takes(b)).collect();
            if listed.is_empty() {
                hint(ui, "A light wires into the Light input of Points: Dots, Lines or Mesh");
            }
            for (b, name, about) in listed {
                if crate::ui::command(ui, name).on_hover_text(about).clicked() {
                    pick = Some(b);
                }
            }
        }
        let Some((box_kind, preset, generator)) = pick else { return false };
        if let Some(l) = self.level_mut() {
            let id = l.add(box_kind, at);
            l.connect(from, id, 0);
            if !preset.is_empty() {
                self.preset = Some((id, preset, generator));
            }
        }
        self.resync = true;
        true
    }
    fn can_wrap_many(&self, ids: &[u32]) -> bool {
        self.level().is_none_or(|l| l.can_wrap_many(ids))
    }
    fn wrap_many(&mut self, ids: &[u32]) {
        let kinds: HashMap<u32, Vec<PortKind>> = ids.iter().filter_map(|id| self.node(*id).map(|n| (*id, self.comp.node_inputs(n).iter().map(|(_, k)| *k).collect()))).collect();
        if let Some(l) = self.level_mut() {
            l.wrap_many(ids, &|id| kinds.get(&id).cloned().unwrap_or_default());
        }
        self.resync = true;
    }
    fn extra_menu(&mut self, ui: &mut egui::Ui, id: u32) -> bool {
        let Some(n) = self.node(id).cloned() else { return false };
        // (A note's words are its name: nothing to rename.)
        if n.kind == NodeKind::Note {
            return false;
        }
        let mut named: Option<String> = None;
        crate::ui::field_menu(ui, label(icon::PENCIL_SIMPLE, "Rename…"), |ui| {
            if let Some(name) = crate::ui::new_name_menu(ui, ("rename_box", id), "A name for this box", "Rename") {
                named = Some(name);
            }
            if !n.name.is_empty() && crate::ui::command(ui, "Back to its own name").clicked() {
                named = Some(String::new());
                ui.close();
            }
        });
        if let Some(name) = named {
            if let Some(node) = self.level_mut().and_then(|l| l.node_mut(id)) {
                node.name = name;
            }
            return true;
        }
        if !matches!(n.kind, NodeKind::Output | NodeKind::Input | NodeKind::NumberInput)
            && crate::ui::command(ui, label(icon::PACKAGE, "Wrap in a box")).on_hover_text("Puts this box inside a new box of boxes, its wires kept (then build around it inside)").clicked()
        {
            let inputs: Vec<PortKind> = self.comp.node_inputs(&n).iter().map(|(_, k)| *k).collect();
            if let Some(l) = self.level_mut() {
                l.wrap(id, &inputs);
            }
            self.resync = true;
            ui.close();
            return true;
        }
        if n.kind != NodeKind::Group {
            return false;
        }
        if crate::ui::command(ui, label(icon::ARROW_SQUARE_IN, "Open")).on_hover_text("Build inside it (or double-click its title)").clicked() {
            self.open(id);
            ui.close();
        }
        let mut saved = false;
        crate::ui::field_menu(ui, label(icon::FLOPPY_DISK, "Save as a box…"), |ui| {
            if let Some(name) = crate::ui::new_name_menu(ui, ("save_box", id), "Name this box", "Save") {
                self.store();
                let path = self.path.clone();
                match self.comp.save_box(self.cid, &path, id).and_then(|b| serde_json::to_string(&b).ok()) {
                    Some(json) => match self.boxes.save(&name, &json) {
                        Ok(()) => saved = true,
                        Err(e) => self.error = Some(e),
                    },
                    None => self.error = Some("Nothing to save in that box".into()),
                }
                ui.close();
            }
        });
        ui.separator();
        saved
    }
    // An Effect box switched off passes its picture straight through (the effect's own on / off).
    fn bypassable(&self, id: u32) -> bool {
        self.box_effect(id).is_some_and(|e| e.effect != shaders::POINTS_LIGHT && !shaders::points_in(&e.effect) && !shaders::points_out(&e.effect))
    }
    fn bypassed(&self, id: u32) -> bool {
        self.bypassable(id) && self.box_effect(id).is_some_and(|e| !e.enabled)
    }
    fn set_bypassed(&mut self, id: u32, off: bool) {
        let part = self.node(id).filter(|n| n.kind == NodeKind::Effect).and_then(|n| n.parts.first().copied());
        if let Some(fx) = part.and_then(|p| self.comp.layer_mut(p)).and_then(|l| l.effects.first_mut()) {
            fx.enabled = !off;
        }
    }
    fn open(&mut self, id: u32) {
        if self.node(id).is_some_and(|n| n.kind == NodeKind::Group) {
            let mut p = self.path.clone();
            p.push(id);
            self.go = Some(p);
        }
    }
}

/// How many setting rows an operator shows.
fn op_row_count(op: &NumberOp) -> f32 {
    match op {
        NumberOp::Range { .. } => 4.0,
        NumberOp::Math { .. } | NumberOp::Smooth { .. } | NumberOp::Envelope { .. } => 2.0,
        _ => 1.0,
    }
}

/// An operator box's settings.
fn op_rows(ui: &mut egui::Ui, op: &mut NumberOp, id: u32, width: f32) {
    let whole = |ui: &mut egui::Ui, n: &mut u32, name: &str| {
        let mut v = *n as f32;
        Level::range(&mut v, 2.0, 16.0, name).default(4.0).width(width).show(ui);
        *n = v.round().clamp(2.0, 64.0) as u32;
    };
    match op {
        NumberOp::Math { mode, b } => {
            crate::ui::dropdown(ui, ("designer_math", id), width - 20.0).selected_text(mode.name()).show_ui(ui, |ui| {
                for k in MathMode::ALL {
                    if crate::ui::pick_item(ui, *mode == k, k.name()).clicked() {
                        *mode = k;
                    }
                }
            });
            Level::percent(b, "B").default(0.5).width(width).show(ui).on_hover_text("B while nothing is wired into it");
        }
        NumberOp::Smooth { rise, fall } => {
            Level::range(rise, 0.0, 5.0, "Rise (s)").default(0.3).width(width).show(ui);
            Level::range(fall, 0.0, 5.0, "Fall (s)").default(0.3).width(width).show(ui);
        }
        NumberOp::Range { from, to } => {
            Level::percent(&mut from[0], "From").default(0.0).width(width).show(ui).on_hover_text("The part of the number's travel that's used: from here…");
            Level::percent(&mut from[1], "up to").default(1.0).width(width).show(ui);
            Level::percent(&mut to[0], "Becomes").default(0.0).width(width).show(ui).on_hover_text("…stretched onto this range (swap the two to turn it upside down)");
            Level::percent(&mut to[1], "up to").default(1.0).width(width).show(ui);
        }
        NumberOp::Gate { line } => {
            Level::percent(line, "Line").default(0.5).width(width).show(ui).on_hover_text("1 while the number is at or above it");
        }
        NumberOp::Steps { count } | NumberOp::Count { steps: count } => whole(ui, count, "Steps"),
        NumberOp::Hold => {
            hint(ui, "Takes In each time When rises");
        }
        NumberOp::Envelope { attack, release } => {
            Level::range(attack, 0.0, 2.0, "Rise (s)").default(0.05).width(width).show(ui);
            Level::range(release, 0.0, 5.0, "Fall (s)").default(0.5).width(width).show(ui);
        }
        NumberOp::Speed { rate } => {
            Level::range(rate, 0.05, 20.0, "Cycles a second at 1").log().default(1.0).width(width).show(ui);
        }
        NumberOp::Delay { seconds } => {
            Level::range(seconds, 0.0, phosphor_engine::MAX_DELAY, "Seconds ago").default(0.5).width(width).show(ui);
        }
        NumberOp::Formula(f) => {
            crate::ui::text_field(ui, f, "a * b, sin(t * 0.2), a > b", Some(width)).on_hover_text("a and b = the numbers wired in, t = seconds, beat = the beat count. + - * / % ( ), a > b, min max abs floor fract clamp mix sin cos");
        }
    }
}

/// A Points: Light's look on its box: a ball lit from where the light comes (left / right, below / above), in its
/// color, with its ambient on the dark side. Drawn as rings stepping toward the bright spot (no texture).
fn light_ball(ui: &mut egui::Ui, values: &[f32], h: f32) {
    let v = |i: usize, d: f32| values.get(i).copied().unwrap_or(d);
    let (across, up, hue, amount, strength, ambient) = (v(0, -0.5), v(1, 0.6), v(2, 0.1), v(3, 0.0), v(4, 1.0), v(5, 0.15));
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), h), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 3.0, Color32::BLACK);
    let r = rect.height() * 0.42;
    let centre = rect.center();
    let tint = egui::Rgba::from(egui::ecolor::Hsva::new(hue, 1.0, 1.0, 1.0));
    let light = |k: f32| {
        let w = |c: f32| ((ambient + 0.95 * strength * k * (1.0 + amount * (c - 1.0))).clamp(0.0, 1.0) * 255.0) as u8;
        Color32::from_rgb(w(tint.r()), w(tint.g()), w(tint.b()))
    };
    let toward = egui::vec2(across.clamp(-1.0, 1.0), -up.clamp(-1.0, 1.0)) * r * 0.55;
    const RINGS: usize = 14;
    for i in 0..RINGS {
        let t = i as f32 / (RINGS - 1) as f32;
        p.circle_filled(centre + toward * t, r * (1.0 - 0.85 * t), light(0.15 + 0.85 * t));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_add_menu_search_finds_boxes_operators_effects_and_generators() {
        let kinds = NodeKind::ALL;
        let found = |q: &str| find_boxes(q, &kinds);
        // A box kind by name, first.
        assert_eq!(found("switch").first().map(|x| x.0), Some((NodeKind::Switch, "", false)));
        // A number operator comes as a Number box with its preset.
        assert!(found("smooth").iter().any(|x| x.0 == (NodeKind::Number, "op:1", false)));
        // An effect by name is an Effect box that starts with it; a generator a Picture box.
        assert!(found("kaleid").iter().any(|x| x.0.0 == NodeKind::Effect && x.1.ends_with("· Effect")));
        assert!(found("plasma").iter().any(|x| x.0 == (NodeKind::Picture, "plasma", true)));
        // Never a long list, and nothing for nonsense.
        assert!(found("e").len() <= 14);
        assert!(found("zzzqqq").is_empty());
    }
}
