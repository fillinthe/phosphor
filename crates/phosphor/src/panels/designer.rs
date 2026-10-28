//! Designer: a comp built as boxes wired together (Picture, Effect, Mix, Number, Output) on the node canvas. The
//! boxes are `phosphor_engine::Network` (behind each one: ordinary layers or a shared modulator in the comp, see the
//! engine's `designer`), so each box shows its live picture or value and its settings are the same sliders as
//! anywhere else (mappable, modulated). Every setting is also a number input: wire a Number box into it.

use std::collections::HashMap;

use phosphor_engine::control::MAX_SHARED;
use phosphor_engine::{ChainMode, Composition, DesignerNode, ModSource, Network, NodeKind, PortKind};

use super::*;
use crate::ui::nodes::{Graph, node_canvas};

const BOX_W: f32 = 230.0;
const ROW_H: f32 = 26.0;
const PARAM_H: f32 = 22.0;
const METER_H: f32 = 12.0;

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
        help(ui, "Boxes wired together instead of a stack of layers: a Picture into an Effect into the Output. Each box shows its picture; drag from a box's right dot to another's left dot to wire them. The comp's layers stay as they are.");
        return;
    }
    let cid = cid.unwrap();
    let shared = st.mod_inputs.shared;
    let State { comp, layer_textures, .. } = st;
    let net = comp.comp(cid).and_then(|c| c.network.clone()).unwrap_or_default();
    let name = comp.comp(cid).map_or(String::new(), |c| c.name.clone());
    crate::ui::toolbar(ui, |ui| {
        crate::ui::heading(ui, format!("{} {name}", icon::GRAPH));
        help(ui, "Right-click the canvas to add a box · drag a box's right dot to another's left dot · the box wired into Output is what shows · a Number box wires into any setting (the blue dots)");
    });
    let feeds = super::layers::feed_choices(comp).into_iter().filter(|(id, ..)| !net.parts().any(|p| p == *id)).collect();
    let aspect = comp.width.max(1) as f32 / comp.height.max(1) as f32;
    let mut view = View { net, comp, cid, textures: layer_textures, shared, feeds, aspect, resync: false };
    let changed = node_canvas(ui, &mut view, &format!("designer:{cid}"));
    let View { net, resync, .. } = view;
    // (A network from a file made by hand, or a box added elsewhere, may still be without its layers.)
    let unmade = net.nodes.iter().any(|n| n.kind != NodeKind::Output && n.parts.is_empty());
    if changed || resync || unmade {
        if let Some(c) = comp.comp_mut(cid) {
            c.network = Some(net);
        }
        comp.sync_network(cid);
    }
}

/// The network with what its boxes need to draw themselves: their layers and modulators (in the comp), pictures
/// and this frame's values.
struct View<'a> {
    net: Network,
    comp: &'a mut Composition,
    cid: u64,
    textures: &'a HashMap<u64, (egui::TextureId, (u32, u32))>,
    shared: [(u64, f32); MAX_SHARED],
    feeds: Vec<(u64, String, bool)>,
    aspect: f32,
    /// Boxes came or went, or wires changed: the layers must follow.
    resync: bool,
}

impl View<'_> {
    fn node(&self, id: u32) -> Option<&DesignerNode> {
        self.net.node(id)
    }

    fn preview_h(&self) -> f32 {
        (BOX_W - 12.0) / self.aspect
    }

    /// The live picture of the layer / group `part` (or, for Output, of what's wired in).
    fn preview(&self, ui: &mut egui::Ui, part: Option<u64>) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), self.preview_h()), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 3.0, Color32::BLACK);
        match part.and_then(|id| self.textures.get(&id)) {
            Some((tex, _)) => paint_texture(p, *tex, fit_rect(rect, self.aspect)),
            None => {
                p.text(rect.center(), egui::Align2::CENTER_CENTER, "nothing yet", egui::FontId::proportional(11.0), Color32::from_gray(90));
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
            Level::percent(&mut m.value, "Value").default(0.5).width(width).show(ui);
        } else if m.signal.source.is_audio() {
            hint(ui, "Follows the sound Phosphor hears (Audio)");
        } else if m.signal.sync {
            Level::range(&mut m.signal.rate, 0.25, 32.0, "Beats per cycle").log().default(4.0).width(width).show(ui);
        } else {
            Level::range(&mut m.signal.rate, 0.05, 20.0, "Cycles a second").log().default(0.5).width(width).show(ui);
        }
        crate::ui::toolbar(ui, |ui| {
            if !m.signal.source.is_audio() && m.signal.source != ModSource::Hand {
                crate::ui::toggle(ui, &mut m.signal.sync, "Beat").on_hover_text("Its cycle follows the tempo");
            }
            if let Some(c) = m.chain.as_mut() {
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
        changed
    }
}

impl Graph for View<'_> {
    fn ids(&self) -> Vec<u32> {
        self.net.nodes.iter().map(|n| n.id).collect()
    }
    fn pos(&self, id: u32) -> [f32; 2] {
        self.node(id).map_or([0.0; 2], |n| n.pos)
    }
    fn set_pos(&mut self, id: u32, pos: [f32; 2]) {
        if let Some(n) = self.net.node_mut(id) {
            n.pos = pos;
        }
    }
    fn title(&self, id: u32) -> String {
        let Some(n) = self.node(id) else { return String::new() };
        match n.kind {
            NodeKind::Picture => n.parts.first().and_then(|p| self.comp.layer(*p)).map_or_else(|| "Picture".to_string(), |l| l.source.label()),
            NodeKind::Effect => n.parts.first().and_then(|p| self.comp.layer(*p)).and_then(|l| l.effects.first()).and_then(|e| shaders::effect(&e.effect)).map_or_else(|| "Effect".to_string(), |d| d.name.to_string()),
            NodeKind::Number => n.modulator().and_then(|m| self.comp.modulators.iter().find(|x| x.id == m)).map_or_else(|| "Number".to_string(), |m| m.signal.source.name().to_string()),
            other => other.name().to_string(),
        }
    }
    fn about(&self, id: u32) -> String {
        self.node(id).map_or_else(String::new, |n| n.kind.about().to_string())
    }
    fn colour(&self, id: u32) -> Color32 {
        match self.node(id).map(|n| n.kind) {
            Some(NodeKind::Picture) => crate::theme::Hue::Media.color(),
            Some(NodeKind::Effect) => crate::theme::Hue::Effect.color(),
            Some(NodeKind::Mix) => crate::theme::Hue::Live.color(),
            Some(NodeKind::Number) => crate::theme::Hue::Control.color(),
            _ => accent(),
        }
    }
    fn inputs(&self, id: u32) -> Vec<(String, PortKind)> {
        self.node(id).map_or_else(Vec::new, |n| self.comp.node_inputs(n))
    }
    fn has_output(&self, id: u32) -> bool {
        self.node(id).is_some_and(|n| n.kind != NodeKind::Output)
    }
    fn output_kind(&self, id: u32) -> PortKind {
        self.node(id).map_or(PortKind::Picture, |n| n.kind.output())
    }
    fn removable(&self, id: u32) -> bool {
        self.has_output(id)
    }
    fn width(&self, _id: u32) -> f32 {
        BOX_W
    }
    fn body_height(&self, id: u32) -> f32 {
        let Some(n) = self.node(id) else { return 0.0 };
        let pic = self.preview_h() + 3.0;
        match n.kind {
            NodeKind::Picture => pic + ROW_H,
            NodeKind::Effect => {
                let params = n.parts.first().and_then(|p| self.comp.layer(*p)).and_then(|l| l.effects.first()).and_then(|e| shaders::effect(&e.effect)).map_or(0, |d| d.params.len());
                pic + ROW_H + (params as f32 + 1.0) * PARAM_H
            }
            NodeKind::Mix => pic + ROW_H + PARAM_H,
            NodeKind::Number => METER_H + ROW_H + PARAM_H * 2.0 + 6.0,
            NodeKind::Output => pic,
        }
    }
    fn body(&mut self, ui: &mut egui::Ui, id: u32) -> bool {
        let Some(n) = self.node(id).cloned() else { return false };
        let width = ui.available_width();
        match n.kind {
            NodeKind::Picture => {
                self.preview(ui, n.parts.first().copied());
                let before = n.parts.first().and_then(|p| self.comp.layer(*p)).map(|l| l.source.clone());
                if let Some(l) = n.parts.first().and_then(|p| self.comp.layer_mut(*p)) {
                    super::layers::source_picker(ui, l, &self.feeds);
                    // A new generator has other settings: its number inputs change.
                    return before.as_ref() != Some(&l.source);
                }
            }
            NodeKind::Effect => {
                self.preview(ui, n.parts.first().copied());
                let Some(part) = n.parts.first().copied() else { return false };
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
                    Level::percent(&mut fx.mix, "Mix").default(1.0).width(width).target(ControlTarget::EffectMix(fx.id)).show(ui);
                }
            }
            NodeKind::Mix => {
                self.preview(ui, n.parts.first().copied());
                if let Some(b) = n.parts.get(2).and_then(|p| self.comp.layer_mut(*p)) {
                    crate::ui::choice_combo(ui, ("designer_blend", id), &mut b.blend, width - 20.0, false).on_hover_text("How B blends over A");
                    Level::percent(&mut b.opacity, "B opacity").modulation(&mut b.opacity_mod).width(width).target(ControlTarget::LayerOpacity(b.id)).show(ui);
                }
            }
            NodeKind::Number => return self.number_body(ui, &n, id),
            NodeKind::Output => {
                let shown = self.net.shown().and_then(|s| self.node(s)).and_then(|n| n.picture());
                self.preview(ui, shown);
            }
        }
        false
    }
    fn source_of(&self, to: u32, input: usize) -> Option<u32> {
        self.net.source_of(to, input)
    }
    fn connect(&mut self, from: u32, to: u32, input: usize) -> bool {
        // (The comp knows every box's inputs, numbers included: it does the wiring.)
        if let Some(c) = self.comp.comp_mut(self.cid) {
            c.network = Some(self.net.clone());
        }
        let ok = self.comp.connect_boxes(self.cid, from, to, input);
        if ok && let Some(net) = self.comp.comp(self.cid).and_then(|c| c.network.clone()) {
            self.net = net;
            self.resync = true;
        }
        ok
    }
    fn disconnect(&mut self, to: u32, input: usize) {
        self.net.disconnect(to, input);
        self.resync = true;
    }
    fn remove(&mut self, id: u32) {
        self.net.remove(id);
        self.resync = true;
    }
    fn duplicate(&mut self, id: u32) {
        if let Some(n) = self.node(id).cloned() {
            self.net.add(n.kind, [n.pos[0] + 24.0, n.pos[1] + 24.0]);
            self.resync = true;
        }
    }
    fn add_menu(&mut self, ui: &mut egui::Ui, at: [f32; 2]) -> bool {
        let has_output = self.net.nodes.iter().any(|n| n.kind == NodeKind::Output);
        for kind in NodeKind::ALL.into_iter().filter(|k| *k != NodeKind::Output || !has_output) {
            if crate::ui::command(ui, kind.name()).on_hover_text(kind.about()).clicked() {
                self.net.add(kind, at);
                self.resync = true;
                return true;
            }
        }
        false
    }
}
