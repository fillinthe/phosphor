//! Designer: a comp built as boxes wired together (Picture, Effect, Mix, Output) on the node canvas. The boxes are
//! `phosphor_engine::Network` (behind each one: ordinary layers in the comp, see the engine's `designer`), so each
//! box shows its live picture and its settings are the same sliders as anywhere else (mappable, modulated).

use std::collections::HashMap;

use phosphor_engine::{Composition, DesignerNode, Network, NodeKind};

use super::*;
use crate::ui::nodes::{Graph, node_canvas};

const BOX_W: f32 = 230.0;
const ROW_H: f32 = 26.0;
const PARAM_H: f32 = 22.0;

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
    let State { comp, layer_textures, .. } = st;
    let net = comp.comp(cid).and_then(|c| c.network.clone()).unwrap_or_default();
    let name = comp.comp(cid).map_or(String::new(), |c| c.name.clone());
    crate::ui::toolbar(ui, |ui| {
        crate::ui::heading(ui, format!("{} {name}", icon::GRAPH));
        help(ui, "Right-click the canvas to add a box · drag a box's right dot to another's left dot · the box wired into Output is what shows");
    });
    let feeds = super::layers::feed_choices(comp).into_iter().filter(|(id, ..)| !net.parts().any(|p| p == *id)).collect();
    let aspect = comp.width.max(1) as f32 / comp.height.max(1) as f32;
    let mut view = View { net, comp, cid, textures: layer_textures, feeds, aspect, resync: false };
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

/// The network with what its boxes need to draw themselves: their layers (in the comp) and pictures.
struct View<'a> {
    net: Network,
    comp: &'a mut Composition,
    cid: u64,
    textures: &'a HashMap<u64, (egui::TextureId, (u32, u32))>,
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
            _ => accent(),
        }
    }
    fn inputs(&self, id: u32) -> Vec<String> {
        self.node(id).map_or_else(Vec::new, |n| n.kind.inputs().iter().map(|s| s.to_string()).collect())
    }
    fn has_output(&self, id: u32) -> bool {
        self.node(id).is_some_and(|n| n.kind != NodeKind::Output)
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
            NodeKind::Output => pic,
        }
    }
    fn body(&mut self, ui: &mut egui::Ui, id: u32) -> bool {
        let Some(n) = self.node(id).cloned() else { return false };
        let width = ui.available_width();
        match n.kind {
            NodeKind::Picture => {
                self.preview(ui, n.parts.first().copied());
                if let Some(l) = n.parts.first().and_then(|p| self.comp.layer_mut(*p)) {
                    super::layers::source_picker(ui, l, &self.feeds);
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
        let ok = self.net.connect(from, to, input);
        self.resync |= ok;
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
