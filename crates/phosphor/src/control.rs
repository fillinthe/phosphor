//! External control: MIDI (every input port) and OSC (UDP) in, with learn mode, applied to any
//! parameter each frame. The OSC port is advertised over Zeroconf (`_osc._udp`) so TouchOSC and
//! similar apps discover Phosphor by name.

use std::collections::HashMap;
use std::sync::mpsc;

use phosphor_engine::{Composition, ControlMap, ControlSource, ControlTarget, EffectState, shaders};

pub const DEFAULT_OSC_PORT: u16 = 8000;

#[derive(Clone, Debug)]
pub struct Incoming {
    pub source: ControlSource,
    /// Normalised 0..1.
    pub value: f32,
}

pub struct ControlHub {
    rx: mpsc::Receiver<Incoming>,
    tx: mpsc::Sender<Incoming>,
    pub midi_ports: Vec<String>,
    pub osc_port: u16,
    pub osc_status: String,
    pub local_ip: Option<String>,
    pub last: Option<Incoming>,
    /// Target waiting for the next message to bind to.
    pub learning: Option<ControlTarget>,
    fresh: HashMap<ControlSource, f32>,
    #[cfg(not(target_arch = "wasm32"))]
    backend: native::Backend,
}

impl ControlHub {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let mut hub = Self {
            rx,
            tx,
            midi_ports: Vec::new(),
            osc_port: DEFAULT_OSC_PORT,
            osc_status: "OSC off".into(),
            local_ip: None,
            last: None,
            learning: None,
            fresh: HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            backend: native::Backend::default(),
        };
        hub.reconnect();
        hub
    }

    /// (Re)opens every MIDI input and the OSC listener.
    pub fn reconnect(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.midi_ports = self.backend.open_midi(self.tx.clone());
            self.local_ip = local_ip();
            self.osc_status = match self.backend.open_osc(self.osc_port, self.tx.clone(), self.local_ip.as_deref()) {
                Ok(()) => format!(
                    "OSC listening on udp/{} at {}",
                    self.osc_port,
                    self.local_ip.clone().unwrap_or_else(|| "this machine".into())
                ),
                Err(e) => format!("OSC failed: {e}"),
            };
        }
    }

    /// Drains incoming messages; binds the learning target if one is waiting.
    pub fn poll(&mut self, comp: &mut Composition) {
        while let Ok(msg) = self.rx.try_recv() {
            if let Some(target) = self.learning.take() {
                comp.control_maps.retain(|m| m.target != target);
                comp.control_maps.push(ControlMap { source: msg.source.clone(), target });
            }
            self.fresh.insert(msg.source.clone(), msg.value);
            self.last = Some(msg);
        }
    }

    /// Writes this frame's values into their targets, then forgets them so the UI stays in charge
    /// until the controller moves again.
    pub fn apply(&mut self, comp: &mut Composition, now: f32) {
        if self.fresh.is_empty() {
            return;
        }
        for map in comp.control_maps.clone() {
            let Some(v) = self.fresh.get(&map.source).copied() else { continue };
            set_target(comp, &map.target, v, now);
        }
        self.fresh.clear();
    }
}

fn scaled(p: &shaders::Param, v: f32) -> f32 {
    p.min + v.clamp(0.0, 1.0) * (p.max - p.min)
}

pub fn find_effect_mut(comp: &mut Composition, id: u64) -> Option<&mut EffectState> {
    comp.master_effects.iter_mut().chain(comp.layers.iter_mut().flat_map(|l| l.effects.iter_mut())).find(|e| e.id == id)
}

fn set_target(comp: &mut Composition, target: &ControlTarget, v: f32, now: f32) {
    match target {
        ControlTarget::TriggerClip { layer, slot } => {
            if v > 0.5 && let Some(l) = comp.layer_mut(*layer) {
                l.trigger_clip(*slot, now);
            }
        }
        ControlTarget::TriggerColumn(col) => {
            if v > 0.5 {
                comp.trigger_column(*col, now);
            }
        }
        ControlTarget::MasterFade => comp.master_fade = v.clamp(0.0, 1.0),
        ControlTarget::LayerOpacity(id) => {
            if let Some(l) = comp.layer_mut(*id) {
                l.opacity = v.clamp(0.0, 1.0);
            }
        }
        ControlTarget::LayerEnabled(id) => {
            if let Some(l) = comp.layer_mut(*id) {
                l.enabled = v > 0.5;
            }
        }
        ControlTarget::GeneratorParam { layer, index } => {
            if let Some(l) = comp.layer_mut(*layer)
                && let phosphor_engine::SourceSpec::Generator(g) = &l.source
                && let Some(desc) = shaders::generator(g)
                && let Some(p) = desc.params.get(*index)
            {
                if l.generator_values.len() <= *index {
                    l.generator_values.resize(index + 1, p.default);
                }
                l.generator_values[*index] = scaled(p, v);
            }
        }
        ControlTarget::EffectParam { effect, index } => {
            if let Some(fx) = find_effect_mut(comp, *effect)
                && let Some(desc) = shaders::effect(&fx.effect)
                && let Some(p) = desc.params.get(*index)
            {
                if fx.values.len() <= *index {
                    fx.values.resize(index + 1, p.default);
                }
                fx.values[*index] = scaled(p, v);
            }
        }
        ControlTarget::EffectMix(id) => {
            if let Some(fx) = find_effect_mut(comp, *id) {
                fx.mix = v.clamp(0.0, 1.0);
            }
        }
        ControlTarget::EffectEnabled(id) => {
            if let Some(fx) = find_effect_mut(comp, *id) {
                fx.enabled = v > 0.5;
            }
        }
    }
}

/// Every bindable target in the composition, with a readable name.
pub fn all_targets(comp: &Composition) -> Vec<(ControlTarget, String)> {
    let mut out = vec![(ControlTarget::MasterFade, "Master fade".to_string())];
    for col in 0..comp.clip_columns {
        out.push((ControlTarget::TriggerColumn(col), format!("Column {} (trigger)", col + 1)));
    }
    let effects = |prefix: &str, effects: &[EffectState], out: &mut Vec<(ControlTarget, String)>| {
        for fx in effects {
            let Some(desc) = shaders::effect(&fx.effect) else { continue };
            out.push((ControlTarget::EffectEnabled(fx.id), format!("{prefix} · {} on/off", desc.name)));
            out.push((ControlTarget::EffectMix(fx.id), format!("{prefix} · {} mix", desc.name)));
            for (i, p) in desc.params.iter().enumerate() {
                out.push((ControlTarget::EffectParam { effect: fx.id, index: i }, format!("{prefix} · {} · {}", desc.name, p.name)));
            }
        }
    };
    effects("MASTER", &comp.master_effects, &mut out);
    for l in &comp.layers {
        out.push((ControlTarget::LayerOpacity(l.id), format!("{} · opacity", l.name)));
        out.push((ControlTarget::LayerEnabled(l.id), format!("{} · on/off", l.name)));
        for (slot, clip) in l.clips.iter().enumerate() {
            if !clip.is_empty() {
                out.push((ControlTarget::TriggerClip { layer: l.id, slot }, format!("{} · clip {} ({})", l.name, slot + 1, clip.name)));
            }
        }
        if let phosphor_engine::SourceSpec::Generator(g) = &l.source
            && let Some(desc) = shaders::generator(g)
        {
            for (i, p) in desc.params.iter().enumerate() {
                out.push((ControlTarget::GeneratorParam { layer: l.id, index: i }, format!("{} · {} · {}", l.name, desc.name, p.name)));
            }
        }
        effects(&l.name, &l.effects, &mut out);
    }
    out
}

pub fn target_label(comp: &Composition, target: &ControlTarget) -> String {
    all_targets(comp).into_iter().find(|(t, _)| t == target).map(|(_, n)| n).unwrap_or_else(|| "(removed)".into())
}

pub fn local_ip() -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        native::local_ip()
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::net::UdpSocket;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    use phosphor_engine::ControlSource;

    use super::Incoming;

    #[derive(Default)]
    pub struct Backend {
        midi: Vec<midir::MidiInputConnection<()>>,
        osc_stop: Option<Arc<AtomicBool>>,
        mdns: Option<mdns_sd::ServiceDaemon>,
    }

    impl Backend {
        pub fn open_midi(&mut self, tx: mpsc::Sender<Incoming>) -> Vec<String> {
            self.midi.clear();
            let mut names = Vec::new();
            let Ok(probe) = midir::MidiInput::new("Phosphor") else { return names };
            for port in probe.ports() {
                let name = probe.port_name(&port).unwrap_or_else(|_| "MIDI".into());
                let Ok(input) = midir::MidiInput::new("Phosphor") else { continue };
                let tx = tx.clone();
                let conn = input.connect(
                    &port,
                    "phosphor-in",
                    move |_, msg, _| {
                        if let Some(incoming) = parse_midi(msg) {
                            let _ = tx.send(incoming);
                        }
                    },
                    (),
                );
                if let Ok(conn) = conn {
                    self.midi.push(conn);
                    names.push(name);
                }
            }
            names
        }

        pub fn open_osc(&mut self, port: u16, tx: mpsc::Sender<Incoming>, ip: Option<&str>) -> Result<(), String> {
            if let Some(stop) = self.osc_stop.take() {
                stop.store(true, Ordering::Relaxed);
            }
            self.mdns = None;
            let socket = UdpSocket::bind(("0.0.0.0", port)).map_err(|e| e.to_string())?;
            socket.set_read_timeout(Some(Duration::from_millis(200))).map_err(|e| e.to_string())?;
            let stop = Arc::new(AtomicBool::new(false));
            self.osc_stop = Some(stop.clone());
            std::thread::Builder::new()
                .name("phosphor-osc".into())
                .spawn(move || {
                    let mut buf = [0u8; 65_536];
                    while !stop.load(Ordering::Relaxed) {
                        let Ok((n, _)) = socket.recv_from(&mut buf) else { continue };
                        if let Ok((_, packet)) = rosc::decoder::decode_udp(&buf[..n]) {
                            deliver(packet, &tx);
                        }
                    }
                })
                .map_err(|e| e.to_string())?;
            // Zeroconf, so controllers can pick "Phosphor" from a list instead of typing an IP.
            if let Some(ip) = ip
                && let Ok(daemon) = mdns_sd::ServiceDaemon::new()
            {
                let props = std::collections::HashMap::<String, String>::new();
                if let Ok(info) = mdns_sd::ServiceInfo::new("_osc._udp.local.", "Phosphor", "phosphor.local.", ip, port, props)
                    && daemon.register(info).is_ok()
                {
                    self.mdns = Some(daemon);
                }
            }
            Ok(())
        }
    }

    fn parse_midi(msg: &[u8]) -> Option<Incoming> {
        let (status, channel) = (msg.first()? & 0xF0, msg.first()? & 0x0F);
        match status {
            0xB0 => Some(Incoming { source: ControlSource::MidiCc { channel, cc: *msg.get(1)? }, value: *msg.get(2)? as f32 / 127.0 }),
            0x90 => Some(Incoming { source: ControlSource::MidiNote { channel, note: *msg.get(1)? }, value: *msg.get(2)? as f32 / 127.0 }),
            0x80 => Some(Incoming { source: ControlSource::MidiNote { channel, note: *msg.get(1)? }, value: 0.0 }),
            _ => None,
        }
    }

    fn deliver(packet: rosc::OscPacket, tx: &mpsc::Sender<Incoming>) {
        match packet {
            rosc::OscPacket::Message(m) => {
                let value = m.args.iter().find_map(|a| match a {
                    rosc::OscType::Float(f) => Some(*f),
                    rosc::OscType::Double(d) => Some(*d as f32),
                    rosc::OscType::Int(i) => Some(*i as f32),
                    rosc::OscType::Long(i) => Some(*i as f32),
                    rosc::OscType::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
                    _ => None,
                });
                let _ = tx.send(Incoming { source: ControlSource::Osc { address: m.addr }, value: value.unwrap_or(1.0) });
            }
            rosc::OscPacket::Bundle(b) => {
                for p in b.content {
                    deliver(p, tx);
                }
            }
        }
    }

    /// The LAN address other devices should talk to.
    pub fn local_ip() -> Option<String> {
        let s = UdpSocket::bind("0.0.0.0:0").ok()?;
        s.connect("8.8.8.8:80").ok()?;
        Some(s.local_addr().ok()?.ip().to_string())
    }
}
