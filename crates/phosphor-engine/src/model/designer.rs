//! Designer: a comp built as a network of boxes wired together (TouchDesigner-style) instead of a stack. Every box
//! is backed by ordinary layers in that comp, so the engine, previews, effects, modulation, mapping, cues and saving
//! all work on it unchanged:
//!
//! - **Picture**: one layer (any source: a movie, generator, camera, text, G-Drop…).
//! - **Effect**: one layer whose source is the box wired in (`SourceSpec::Feed`) and whose chain holds the effect.
//! - **Mix**: a group of two layers, each fed by an input; the second one's blend and opacity do the mixing.
//! - **Output**: no layer; the box wired into it is the one that mixes into the comp (`in_mix`). Everything else is
//!   kept out of the mix and only drawn when something downstream needs it.
//! - **Number**: a shared modulator (LFO, sound, beat, random, steps, knob). Its output wires into any setting of
//!   another box (one number input per setting) as a linked modulation, and into another Number's "Chain" input
//!   (scaled by / plus / min / max…: the maths).
//!
//! `Composition::sync_network` makes the layers match the boxes and wires; call it after the network changes.

use serde::{Deserialize, Serialize};

use super::*;
use crate::graph::{self, Wire};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    Picture,
    Effect,
    Mix,
    Output,
    Number,
}

/// What a wire carries: a picture, or a number (0..1, changing over time).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortKind {
    Picture,
    Number,
}

impl NodeKind {
    pub const ALL: [NodeKind; 5] = [NodeKind::Picture, NodeKind::Effect, NodeKind::Mix, NodeKind::Number, NodeKind::Output];

    /// What its output carries.
    pub fn output(self) -> PortKind {
        if self == NodeKind::Number { PortKind::Number } else { PortKind::Picture }
    }

    pub fn name(self) -> &'static str {
        match self {
            NodeKind::Picture => "Picture",
            NodeKind::Effect => "Effect",
            NodeKind::Mix => "Mix",
            NodeKind::Output => "Output",
            NodeKind::Number => "Number",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            NodeKind::Picture => "A movie, picture, generator, camera, text or visualizer",
            NodeKind::Effect => "Runs an effect on the picture wired in",
            NodeKind::Mix => "Blends B over A (blend mode and opacity)",
            NodeKind::Output => "What the comp shows: wire the finished picture in here",
            NodeKind::Number => "A moving number: an LFO, the sound, the beat, random, steps or a knob. Wire it into any setting of another box, or into another Number's Chain to combine them",
        }
    }

    /// The names of its inputs.
    pub fn inputs(self) -> &'static [&'static str] {
        match self {
            NodeKind::Picture => &[],
            NodeKind::Effect => &["In"],
            NodeKind::Mix => &["A", "B"],
            NodeKind::Output => &["In"],
            NodeKind::Number => &["Chain"],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Node {
    pub id: u32,
    pub kind: NodeKind,
    /// Top-left corner on the canvas (points).
    pub pos: [f32; 2],
    /// What's behind it: Picture / Effect = [layer]; Mix = [group, layer A, layer B]; Number = [modulator];
    /// Output = []. Empty until `sync_network` makes them.
    pub parts: Vec<u64>,
}

impl Default for Node {
    fn default() -> Self {
        Self { id: 0, kind: NodeKind::Picture, pos: [0.0, 0.0], parts: Vec::new() }
    }
}

impl Node {
    /// The layer or group whose finished picture is this box's output (None for Output and Number).
    pub fn picture(&self) -> Option<u64> {
        match self.kind {
            NodeKind::Output | NodeKind::Number => None,
            _ => self.parts.first().copied(),
        }
    }

    /// The shared modulator behind a Number box.
    pub fn modulator(&self) -> Option<u64> {
        (self.kind == NodeKind::Number).then(|| self.parts.first().copied()).flatten()
    }
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Network {
    pub nodes: Vec<Node>,
    pub wires: Vec<Wire>,
}

impl Network {
    /// A new network: a Picture wired into Output.
    pub fn new() -> Network {
        let mut n = Network::default();
        let from = n.add(NodeKind::Picture, [40.0, 60.0]);
        let to = n.add(NodeKind::Output, [420.0, 60.0]);
        n.connect(from, to, 0);
        n
    }

    pub fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: u32) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// Adds a box and returns its id (its layers come with the next `sync_network`). One Output only.
    pub fn add(&mut self, kind: NodeKind, pos: [f32; 2]) -> u32 {
        if kind == NodeKind::Output && let Some(out) = self.nodes.iter().find(|n| n.kind == NodeKind::Output) {
            return out.id;
        }
        let id = self.nodes.iter().map(|n| n.id + 1).max().unwrap_or(1);
        self.nodes.push(Node { id, kind, pos, parts: Vec::new() });
        id
    }

    /// Takes a box out of the network (its layers go with the next `sync_network`). Returns its parts.
    pub fn remove(&mut self, id: u32) -> Vec<u64> {
        let parts = self.node(id).map(|n| n.parts.clone()).unwrap_or_default();
        self.nodes.retain(|n| n.id != id);
        graph::remove(&mut self.wires, id);
        parts
    }

    /// Wires two boxes' picture ports (number inputs depend on the boxes' settings: `Composition::connect_boxes`).
    pub fn connect(&mut self, from: u32, to: u32, input: usize) -> bool {
        let inputs = self.node(to).map_or(0, |n| n.kind.inputs().len());
        self.node(from).is_some_and(|n| n.kind.output() == PortKind::Picture) && graph::connect(&mut self.wires, from, to, input, inputs)
    }

    pub fn disconnect(&mut self, to: u32, input: usize) {
        graph::disconnect(&mut self.wires, to, input);
    }

    pub fn source_of(&self, to: u32, input: usize) -> Option<u32> {
        graph::source_of(&self.wires, to, input)
    }

    /// The box wired into Output, if any.
    pub fn shown(&self) -> Option<u32> {
        let out = self.nodes.iter().find(|n| n.kind == NodeKind::Output)?;
        self.source_of(out.id, 0)
    }

    /// Every layer and group the boxes own.
    pub fn parts(&self) -> impl Iterator<Item = u64> + '_ {
        self.nodes.iter().flat_map(|n| n.parts.iter().copied())
    }
}

impl Composition {
    /// Every input of box `node` (in comp `cid`): its picture inputs, then one number input per setting it has (an
    /// effect's or generator's parameters, a Mix's opacity, a Number's chain).
    pub fn node_inputs(&self, node: &Node) -> Vec<(String, PortKind)> {
        let own = if node.kind == NodeKind::Number { PortKind::Number } else { PortKind::Picture };
        let mut v: Vec<(String, PortKind)> = node.kind.inputs().iter().map(|s| (s.to_string(), own)).collect();
        let layer = node.parts.first().and_then(|p| self.layer(*p));
        match node.kind {
            NodeKind::Effect => {
                if let Some(d) = layer.and_then(|l| l.effects.first()).and_then(|e| crate::shaders::effect(&e.effect)) {
                    v.extend(d.params.iter().map(|p| (p.name.to_string(), PortKind::Number)));
                }
            }
            NodeKind::Picture => {
                if let Some(SourceSpec::Generator(id)) = layer.map(|l| &l.source)
                    && let Some(d) = crate::shaders::generator(id)
                {
                    v.extend(d.params.iter().map(|p| (p.name.to_string(), PortKind::Number)));
                }
            }
            NodeKind::Mix => v.push(("Opacity".to_string(), PortKind::Number)),
            _ => {}
        }
        v
    }

    /// Wires box `from` into input `input` of box `to` in comp `cid`'s network, pictures to picture inputs and
    /// numbers to number inputs. False when refused (a loop, the wrong kind, no such input).
    pub fn connect_boxes(&mut self, cid: u64, from: u32, to: u32, input: usize) -> bool {
        let Some(mut net) = self.comp(cid).and_then(|c| c.network.clone()) else { return false };
        let (Some(f), Some(t)) = (net.node(from).cloned(), net.node(to).cloned()) else { return false };
        let inputs = self.node_inputs(&t);
        if f.kind == NodeKind::Output || inputs.get(input).is_none_or(|(_, k)| *k != f.kind.output()) {
            return false;
        }
        let ok = graph::connect(&mut net.wires, from, to, input, inputs.len());
        if ok && let Some(c) = self.comp_mut(cid) {
            c.network = Some(net);
        }
        ok
    }

    /// Makes comp `comp_id`'s layers and modulators match its network: a layer (or a two-layer group) behind each
    /// picture box and a shared modulator behind each Number, Feed sources along picture wires, linked modulations
    /// along number wires, only the box wired into Output in the mix, sources before the boxes they feed. Parts of
    /// boxes that are gone are removed. Does nothing for a comp without a network.
    pub fn sync_network(&mut self, comp_id: u64) {
        let Some(mut net) = self.comp(comp_id).and_then(|c| c.network.clone()) else { return };
        // Parts of boxes that are gone.
        let owned: Vec<u64> = net.parts().collect();
        let stale: Vec<u64> = self.layers.iter().filter(|l| l.comp == Some(comp_id) && l.designer && !owned.contains(&l.id)).map(|l| l.id).collect();
        self.remove_parts(&stale);
        let stale_groups: Vec<u64> = self.groups.iter().filter(|g| g.designer && !owned.contains(&g.id)).map(|g| g.id).collect();
        for g in stale_groups {
            self.ungroup(g);
        }
        self.modulators.retain(|m| !m.designer || owned.contains(&m.id));
        // Parts for new boxes.
        for i in 0..net.nodes.len() {
            let (kind, id) = (net.nodes[i].kind, net.nodes[i].id);
            let have = net.nodes[i].parts.len();
            let need = match kind {
                NodeKind::Picture | NodeKind::Effect | NodeKind::Number => 1,
                NodeKind::Mix => 3,
                NodeKind::Output => 0,
            };
            if have == need {
                continue;
            }
            self.remove_parts(&net.nodes[i].parts.clone());
            let name = |what: &str| format!("Box {id} {what}").trim().to_string();
            net.nodes[i].parts = match kind {
                NodeKind::Picture | NodeKind::Effect => vec![self.add_part(comp_id, &name(""))],
                NodeKind::Mix => {
                    let a = self.add_part(comp_id, &name("A"));
                    let b = self.add_part(comp_id, &name("B"));
                    let g = self.create_group(&[a, b]).unwrap_or(0);
                    if let Some(g) = self.groups.iter_mut().find(|x| x.id == g) {
                        g.name = name("");
                        g.designer = true;
                        g.in_mix = false;
                    }
                    vec![g, a, b]
                }
                NodeKind::Number => {
                    // (The shared list has room for 16: a box past that stays without one until one goes.)
                    if self.modulators.len() >= crate::control::MAX_SHARED {
                        Vec::new()
                    } else {
                        let mid = self.new_id();
                        let signal = Modulation { source: ModSource::LfoSine, rate: 0.5, ..Default::default() };
                        self.modulators.push(Modulator { id: mid, name: name(""), signal, designer: true, ..Default::default() });
                        vec![mid]
                    }
                }
                NodeKind::Output => Vec::new(),
            };
        }
        // Wires to inputs a box no longer has (its effect changed) go.
        let inputs: Vec<(u32, Vec<(String, PortKind)>)> = net.nodes.iter().map(|n| (n.id, self.node_inputs(n))).collect();
        let count = |id: u32| inputs.iter().find(|(n, _)| *n == id).map_or(0, |(_, v)| v.len());
        net.wires.retain(|w| w.input < count(w.to));
        // Picture sources along the wires.
        let feed = |net: &Network, to: u32, input: usize| -> SourceSpec {
            match net.source_of(to, input).and_then(|from| net.node(from)).and_then(|n| n.picture().map(|p| (p, n))) {
                Some((p, n)) => SourceSpec::Feed { id: p, name: format!("Box {}", n.id), before_fx: false },
                None => SourceSpec::None,
            }
        };
        for n in &net.nodes {
            match n.kind {
                NodeKind::Effect => {
                    let s = feed(&net, n.id, 0);
                    if let Some(l) = self.layer_mut(n.parts[0]) {
                        l.source = s;
                    }
                }
                NodeKind::Mix => {
                    for (k, part) in [n.parts[1], n.parts[2]].into_iter().enumerate() {
                        let s = feed(&net, n.id, k);
                        if let Some(l) = self.layer_mut(part) {
                            l.source = s;
                        }
                    }
                }
                _ => {}
            }
        }
        // Numbers along the wires: each number input follows the Number box wired in (or stops following a box: a
        // link to a box's modulator, or to one that's gone, is dropped; a control's own modulation is left alone).
        let designer_mods: Vec<u64> = self.modulators.iter().filter(|m| m.designer).map(|m| m.id).collect();
        let existing: Vec<u64> = self.modulators.iter().map(|m| m.id).collect();
        let drop_link = move |id: u64| designer_mods.contains(&id) || !existing.contains(&id);
        for n in &net.nodes {
            let pic_n = n.kind.inputs().len();
            let total = count(n.id);
            for k in pic_n..total {
                let from = net.source_of(n.id, k).and_then(|f| net.node(f)).and_then(|f| f.modulator());
                self.follow_box(n, k - pic_n, from, &drop_link);
            }
            if n.kind == NodeKind::Number
                && let Some(mid) = n.modulator()
            {
                let from = net.source_of(n.id, 0).and_then(|f| net.node(f)).and_then(|f| f.modulator());
                if let Some(m) = self.modulators.iter_mut().find(|m| m.id == mid) {
                    m.chain = match (from, m.chain) {
                        (Some(f), Some(c)) => Some(ModChain { from: f, ..c }),
                        (Some(f), None) => Some(ModChain { from: f, ..Default::default() }),
                        (None, Some(c)) if drop_link(c.from) => None,
                        (None, c) => c,
                    };
                }
            }
        }
        // Only the box wired into Output shows; Mix members mix inside their group.
        let shown = net.shown().and_then(|id| net.node(id)).and_then(|n| n.picture());
        for n in &net.nodes {
            for (k, part) in n.parts.iter().enumerate() {
                let member = n.kind == NodeKind::Mix && k > 0;
                let on = member || Some(*part) == shown;
                if let Some(l) = self.layers.iter_mut().find(|l| l.id == *part) {
                    l.in_mix = on;
                } else if let Some(g) = self.groups.iter_mut().find(|g| g.id == *part) {
                    g.in_mix = on;
                }
            }
        }
        // Sources before the boxes they feed (so a wire carries this frame's picture, not the last one's); a
        // chained modulator after the one it takes from.
        let ids: Vec<u32> = net.nodes.iter().map(|n| n.id).collect();
        let ordered = graph::order(&ids, &net.wires);
        let order: Vec<u64> = ordered.iter().filter_map(|id| net.node(*id)).flat_map(|n| n.parts.iter().copied().filter(|p| self.layers.iter().any(|l| l.id == *p)).collect::<Vec<_>>()).collect();
        if let Some(first) = self.layers.iter().position(|l| order.contains(&l.id)) {
            let mut taken: Vec<LayerState> = Vec::new();
            self.layers.retain_mut(|l| {
                if order.contains(&l.id) {
                    taken.push(l.clone());
                    false
                } else {
                    true
                }
            });
            let mut sorted: Vec<LayerState> = order.iter().filter_map(|id| taken.iter().find(|l| l.id == *id).cloned()).collect();
            let at = first.min(self.layers.len());
            self.layers.splice(at..at, sorted.drain(..));
            self.normalize_groups();
        }
        let mod_order: Vec<u64> = ordered.iter().filter_map(|id| net.node(*id)).filter_map(|n| n.modulator()).collect();
        let mut mods: Vec<Modulator> = Vec::new();
        self.modulators.retain_mut(|m| {
            if m.designer {
                mods.push(m.clone());
                false
            } else {
                true
            }
        });
        self.modulators.extend(mod_order.iter().filter_map(|id| mods.iter().find(|m| m.id == *id).cloned()));
        if let Some(c) = self.comp_mut(comp_id) {
            c.network = Some(net);
        }
    }

    /// Number input `k` of box `n` follows modulator `from` (a linked modulation that sweeps the setting's whole
    /// range), or stops following a box (its own modulations are left alone).
    fn follow_box(&mut self, n: &Node, k: usize, from: Option<u64>, drop_link: &dyn Fn(u64) -> bool) {
        let mods: &mut Vec<ParamMod> = match n.kind {
            NodeKind::Effect => match self.layer_mut(n.parts[0]).and_then(|l| l.effects.first_mut()) {
                Some(fx) => &mut fx.mods,
                None => return,
            },
            NodeKind::Picture => match self.layer_mut(n.parts[0]) {
                Some(l) => &mut l.generator_mods,
                None => return,
            },
            NodeKind::Mix => {
                let Some(b) = self.layer_mut(n.parts[2]) else { return };
                b.opacity_mod = match (from, b.opacity_mod) {
                    (Some(f), Some(m)) if m.link == f => Some(m),
                    (Some(f), _) => Some(linked(f)),
                    (None, Some(m)) if drop_link(m.link) => None,
                    (None, m) => m,
                };
                return;
            }
            _ => return,
        };
        let at = mods.iter().position(|m| m.index == k);
        match (from, at) {
            (Some(f), Some(i)) if mods[i].m.link == f => {}
            (Some(f), Some(i)) => mods[i].m = linked(f),
            (Some(f), None) => mods.push(ParamMod { index: k, m: linked(f) }),
            (None, Some(i)) if drop_link(mods[i].m.link) => {
                mods.remove(i);
            }
            (None, _) => {}
        }
    }

    /// Starts a network in comp `comp_id` (a Picture wired into Output) unless it has one.
    pub fn start_network(&mut self, comp_id: u64) {
        if let Some(c) = self.comp_mut(comp_id)
            && c.network.is_none()
        {
            c.network = Some(Network::new());
            self.sync_network(comp_id);
        }
    }

    /// A hidden layer for a box, at the end of its comp.
    fn add_part(&mut self, comp_id: u64, name: &str) -> u64 {
        let id = self.new_id();
        let transition_kind = self.launch.style;
        let at = self.layers.iter().rposition(|l| l.comp == Some(comp_id)).map_or(self.layers.len(), |i| i + 1);
        self.layers.insert(at, LayerState { id, name: name.to_string(), transition_kind, comp: Some(comp_id), in_mix: false, designer: true, ..Default::default() });
        id
    }

    /// Removes a box's layers, group or modulator.
    fn remove_parts(&mut self, parts: &[u64]) {
        for p in parts {
            if self.groups.iter().any(|g| g.id == *p) {
                self.ungroup(*p);
            }
        }
        self.layers.retain(|l| !parts.contains(&l.id));
        self.modulators.retain(|m| !parts.contains(&m.id));
        self.normalize_groups();
    }
}

/// A setting following modulator `id` across its whole range (what a number wire means).
fn linked(id: u64) -> Modulation {
    Modulation { link: id, range: true, min: 0.0, max: 1.0, amount: 1.0, ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_of(comp: &Composition, layer: u64) -> Option<u64> {
        match &comp.layer(layer)?.source {
            SourceSpec::Feed { id, .. } => Some(*id),
            _ => None,
        }
    }

    /// A network's boxes get layers; wires become Feed sources; only the box into Output is in the mix; sources
    /// come before what they feed; removing a box removes its layers.
    #[test]
    fn a_network_is_backed_by_hidden_layers() {
        let mut comp = Composition::default();
        let cid = comp.add_comp("Designer");
        comp.start_network(cid);
        let net = comp.comp(cid).unwrap().network.clone().unwrap();
        let (pic, out) = (net.nodes[0].id, net.nodes[1].id);
        let pic_layer = net.nodes[0].parts[0];
        assert!(comp.layer(pic_layer).unwrap().in_mix, "wired into Output: shows");
        assert!(comp.layer(pic_layer).unwrap().designer);

        // Picture → Effect → Mix(B) with a second Picture on A → Output.
        let mut net = net;
        let fx = net.add(NodeKind::Effect, [200.0, 60.0]);
        let mix = net.add(NodeKind::Mix, [300.0, 60.0]);
        let pic2 = net.add(NodeKind::Picture, [40.0, 200.0]);
        assert!(net.connect(pic, fx, 0));
        assert!(net.connect(fx, mix, 1));
        assert!(net.connect(pic2, mix, 0));
        assert!(net.connect(mix, out, 0));
        assert!(!net.connect(out, fx, 0), "Output has no output");
        comp.comp_mut(cid).unwrap().network = Some(net);
        comp.sync_network(cid);
        let net = comp.comp(cid).unwrap().network.clone().unwrap();
        let n = |id: u32| net.node(id).unwrap().clone();
        assert_eq!(n(mix).parts.len(), 3);
        assert_eq!(feed_of(&comp, n(fx).parts[0]), Some(pic_layer));
        assert_eq!(feed_of(&comp, n(mix).parts[2]), Some(n(fx).parts[0]), "B = the effect");
        assert_eq!(feed_of(&comp, n(mix).parts[1]), Some(n(pic2).parts[0]), "A = the second picture");
        assert!(!comp.layer(pic_layer).unwrap().in_mix, "no longer the one shown");
        assert!(comp.groups.iter().find(|g| g.id == n(mix).parts[0]).unwrap().in_mix, "the mix shows");
        let pos = |id: u64| comp.layers.iter().position(|l| l.id == id).unwrap();
        assert!(pos(pic_layer) < pos(n(fx).parts[0]) && pos(n(fx).parts[0]) < pos(n(mix).parts[2]), "sources first");
        assert_eq!(pos(n(mix).parts[1]) + 1, pos(n(mix).parts[2]), "group members together");

        // Removing the effect takes its layer away; the mix's B is empty.
        let mut net = net;
        net.remove(fx);
        comp.comp_mut(cid).unwrap().network = Some(net);
        comp.sync_network(cid);
        let net = comp.comp(cid).unwrap().network.clone().unwrap();
        assert!(comp.layers.iter().all(|l| l.name != "Box 3"));
        assert_eq!(comp.layer(net.node(mix).unwrap().parts[2]).unwrap().source, SourceSpec::None);
        // A project saved before networks existed still opens: the field is optional.
        let json = serde_json::to_string(&comp).unwrap();
        let back: Composition = serde_json::from_str(&json).unwrap();
        assert_eq!(back.comp(cid).unwrap().network, comp.comp(cid).unwrap().network);
    }

    /// A Number box is a shared modulator; wired into an effect's setting it becomes a linked modulation over the
    /// setting's whole range; into another Number it chains; the wrong kind of wire is refused; unwiring and
    /// removing clean up.
    #[test]
    fn number_boxes_drive_settings() {
        let mut comp = Composition::default();
        let cid = comp.add_comp("Designer");
        comp.start_network(cid);
        let mut net = comp.comp(cid).unwrap().network.clone().unwrap();
        let (pic, out) = (net.nodes[0].id, net.nodes[1].id);
        let fx = net.add(NodeKind::Effect, [200.0, 60.0]);
        let lfo = net.add(NodeKind::Number, [200.0, 300.0]);
        let lfo2 = net.add(NodeKind::Number, [40.0, 300.0]);
        net.connect(pic, fx, 0);
        net.connect(fx, out, 0);
        comp.comp_mut(cid).unwrap().network = Some(net);
        comp.sync_network(cid);
        let net = comp.comp(cid).unwrap().network.clone().unwrap();
        let (fx_layer, m1, m2) = (net.node(fx).unwrap().parts[0], net.node(lfo).unwrap().modulator().unwrap(), net.node(lfo2).unwrap().modulator().unwrap());
        assert!(comp.modulators.iter().any(|m| m.id == m1 && m.designer));
        // The effect gets its parameters as number inputs once it has an effect.
        let e = comp.new_effect("invert").unwrap();
        comp.layer_mut(fx_layer).unwrap().effects = vec![e];
        let inputs = comp.node_inputs(net.node(fx).unwrap());
        assert_eq!(inputs[0], ("In".to_string(), PortKind::Picture));
        assert_eq!(inputs[1].1, PortKind::Number);
        assert!(!comp.connect_boxes(cid, lfo, fx, 0), "a number into a picture input");
        assert!(!comp.connect_boxes(cid, pic, fx, 1), "a picture into a number input");
        assert!(comp.connect_boxes(cid, lfo, fx, 1));
        assert!(comp.connect_boxes(cid, lfo2, lfo, 0), "chained");
        comp.sync_network(cid);
        let m = &comp.layer(fx_layer).unwrap().effects[0].mods;
        assert_eq!((m.len(), m[0].index, m[0].m.link, m[0].m.range), (1, 0, m1, true));
        assert_eq!(comp.modulators.iter().find(|m| m.id == m1).unwrap().chain.map(|c| c.from), Some(m2));
        let pos = |id: u64| comp.modulators.iter().position(|m| m.id == id).unwrap();
        assert!(pos(m2) < pos(m1), "the one taken from comes first");
        // Unwire: the setting stops following; remove the box: its modulator goes.
        let mut net = comp.comp(cid).unwrap().network.clone().unwrap();
        net.disconnect(fx, 1);
        net.remove(lfo2);
        comp.comp_mut(cid).unwrap().network = Some(net);
        comp.sync_network(cid);
        assert!(comp.layer(fx_layer).unwrap().effects[0].mods.is_empty());
        assert!(!comp.modulators.iter().any(|m| m.id == m2));
        assert_eq!(comp.modulators.iter().find(|m| m.id == m1).unwrap().chain, None);
    }
}
