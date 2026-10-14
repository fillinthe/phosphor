//! External control: MIDI (every input port) and OSC (UDP) in, with learn mode, applied to any
//! parameter each frame. The OSC port is advertised over Zeroconf (`_osc._udp`) so TouchOSC and
//! similar apps discover Phosphor by name.

use std::collections::HashMap;
use std::sync::mpsc;

use phosphor_engine::{AppControl, Composition, ControlMap, ControlSource, ControlTarget, EffectState, shaders};

/// Controller actions the composition can't carry out by itself; the app handles them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Deferred {
    Clip(u64, usize),
    Column(usize),
    /// Value 0..1, or `FLIP` to toggle an on/off control.
    App(AppControl, f32),
}

/// Sent instead of a value when a pad or key press should flip an on/off control.
pub const FLIP: f32 = -1.0;

/// Pads and keys send press (1) / release (0); on/off targets toggle on press instead of following.
fn momentary(source: &ControlSource) -> bool {
    matches!(source, ControlSource::MidiNote { .. } | ControlSource::Key { .. })
}

fn is_toggle(target: &ControlTarget) -> bool {
    match target {
        ControlTarget::LayerEnabled(_) | ControlTarget::EffectEnabled(_) | ControlTarget::LayerFxBypass(_) => true,
        ControlTarget::App(a) => a.is_toggle(),
        _ => false,
    }
}

fn toggled(current: bool, v: f32) -> bool {
    if v == FLIP { !current } else { v > 0.5 }
}

pub const DEFAULT_OSC_PORT: u16 = 8000;

#[derive(Clone, Debug)]
pub struct Incoming {
    pub source: ControlSource,
    /// Normalised 0..1.
    pub value: f32,
}

/// An OSC sender seen on the network.
#[derive(Clone, Debug)]
pub struct OscSender {
    pub ip: String,
    pub count: u64,
    pub last_address: String,
    pub last_seen: std::time::Instant,
}

/// Who's talking to us, shared with the listener threads.
#[derive(Default)]
pub struct Activity {
    pub osc: std::collections::BTreeMap<String, OscSender>,
    pub osc_blocked: std::collections::BTreeSet<String>,
    pub midi_seen: std::collections::HashMap<String, std::time::Instant>,
}

pub struct ControlHub {
    rx: mpsc::Receiver<Incoming>,
    tx: mpsc::Sender<Incoming>,
    /// Connected (enabled) MIDI inputs.
    pub midi_ports: Vec<String>,
    /// Every MIDI input on the system, enabled or not.
    pub midi_all: Vec<String>,
    /// MIDI inputs switched off in the Devices panel.
    pub midi_disabled: std::collections::BTreeSet<String>,
    pub activity: std::sync::Arc<std::sync::Mutex<Activity>>,
    pub osc_port: u16,
    pub osc_status: String,
    pub local_ip: Option<String>,
    pub last: Option<Incoming>,
    /// Target waiting for the next message to bind to.
    pub learning: Option<ControlTarget>,
    /// Clip/column launches and app-wide controls from controllers, for the app to carry out.
    pub triggers: Vec<Deferred>,
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
            midi_all: Vec::new(),
            midi_disabled: Default::default(),
            activity: Default::default(),
            osc_port: DEFAULT_OSC_PORT,
            osc_status: "OSC off".into(),
            local_ip: None,
            last: None,
            learning: None,
            triggers: Vec::new(),
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
            let (all, open) = self.backend.open_midi(self.tx.clone(), &self.midi_disabled, self.activity.clone());
            self.midi_all = all;
            self.midi_ports = open;
            self.local_ip = local_ip();
            self.osc_status = match self.backend.open_osc(self.osc_port, self.tx.clone(), self.local_ip.as_deref(), self.activity.clone()) {
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
            self.handle(comp, msg);
        }
    }

    /// Feeds a message from inside the app (keyboard) through the same path as MIDI/OSC.
    pub fn inject(&mut self, comp: &mut Composition, msg: Incoming) {
        self.handle(comp, msg);
    }

    fn handle(&mut self, comp: &mut Composition, msg: Incoming) {
        if let Some(target) = self.learning.take() {
            comp.control_maps.retain(|m| m.target != target);
            comp.control_maps.push(ControlMap { source: msg.source.clone(), target });
        }
        self.fresh.insert(msg.source.clone(), msg.value);
        self.last = Some(msg);
    }

    /// Writes this frame's values into their targets, then forgets them so the UI stays in charge
    /// until the controller moves again.
    pub fn apply(&mut self, comp: &mut Composition, now: f32) {
        if self.fresh.is_empty() {
            return;
        }
        for map in comp.control_maps.clone() {
            let Some(mut v) = self.fresh.get(&map.source).copied() else { continue };
            if momentary(&map.source) && is_toggle(&map.target) {
                if v <= 0.5 {
                    continue;
                }
                v = FLIP;
            }
            if let Some(t) = set_target(comp, &map.target, v, now) {
                self.triggers.push(t);
            }
        }
        // Built-in addresses work without any mapping: /phosphor/master, /phosphor/layer/3/opacity…
        let osc: Vec<(String, f32)> = self
            .fresh
            .iter()
            .filter_map(|(s, v)| match s {
                ControlSource::Osc { address } if address.starts_with("/phosphor/") => Some((address.clone(), *v)),
                _ => None,
            })
            .collect();
        if !osc.is_empty() {
            let targets = all_targets(comp);
            for (address, v) in osc {
                if let Some((t, _)) = targets.iter().find(|(t, _)| default_osc_address(t) == address)
                    && let Some(trig) = set_target(comp, &t.clone(), v, now)
                {
                    self.triggers.push(trig);
                }
            }
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

/// Maps 0..1 onto a range, logarithmically when `log`.
pub fn unit_to(v: f32, min: f32, max: f32, log: bool) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if log { min * (max / min).powf(v) } else { min + v * (max - min) }
}

/// Sets a value target directly; returns launches and app-wide controls for the app.
fn set_target(comp: &mut Composition, target: &ControlTarget, v: f32, _now: f32) -> Option<Deferred> {
    match target {
        ControlTarget::TriggerClip { layer, slot } => {
            return (v > 0.5).then_some(Deferred::Clip(*layer, *slot));
        }
        ControlTarget::TriggerColumn(col) => {
            return (v > 0.5).then_some(Deferred::Column(*col));
        }
        ControlTarget::LayerFxBypass(id) => {
            if let Some(l) = comp.layer_mut(*id) {
                l.fx_bypass = toggled(l.fx_bypass, v);
            }
        }
        ControlTarget::LayerSpeed(id) => {
            if let Some(l) = comp.layer_mut(*id) {
                l.rate = unit_to(v, 0.1, 4.0, true);
            }
        }
        ControlTarget::App(a) => match a {
            AppControl::LaunchFade => comp.launch.fade = toggled(comp.launch.fade, v),
            AppControl::FadeTime => comp.launch.fade_secs = unit_to(v, 0.05, 10.0, true),
            AppControl::LaunchTiming => {
                let all = phosphor_engine::LaunchTiming::ALL;
                comp.launch.timing = all[((v.clamp(0.0, 1.0) * all.len() as f32) as usize).min(all.len() - 1)];
            }
            AppControl::MasterFxBypass => comp.master_fx_bypass = toggled(comp.master_fx_bypass, v),
            AppControl::Syphon => comp.syphon = toggled(comp.syphon, v),
            AppControl::Ndi => comp.ndi.enabled = toggled(comp.ndi.enabled, v),
            _ => return Some(Deferred::App(*a, v)),
        },
        ControlTarget::MasterFade => comp.master_fade = v.clamp(0.0, 1.0),
        ControlTarget::LayerOpacity(id) => {
            if let Some(l) = comp.layer_mut(*id) {
                l.opacity = v.clamp(0.0, 1.0);
            }
        }
        ControlTarget::LayerEnabled(id) => {
            if let Some(l) = comp.layer_mut(*id) {
                l.enabled = toggled(l.enabled, v);
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
                fx.enabled = toggled(fx.enabled, v);
            }
        }
    }
    None
}

/// Every bindable target in the composition, with a readable name.
pub fn all_targets(comp: &Composition) -> Vec<(ControlTarget, String)> {
    let mut out = vec![(ControlTarget::MasterFade, "Master fade".to_string())];
    out.extend(AppControl::ALL.iter().map(|a| (ControlTarget::App(*a), a.name().to_string())));
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
        out.push((ControlTarget::LayerSpeed(l.id), format!("{} · speed", l.name)));
        out.push((ControlTarget::LayerFxBypass(l.id), format!("{} · effects bypass", l.name)));
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

/// The OSC address every control answers to without a mapping (values 0..1; 1 = press for triggers).
pub fn default_osc_address(target: &ControlTarget) -> String {
    match target {
        ControlTarget::MasterFade => "/phosphor/master".into(),
        ControlTarget::LayerOpacity(id) => format!("/phosphor/layer/{id}/opacity"),
        ControlTarget::LayerEnabled(id) => format!("/phosphor/layer/{id}/on"),
        ControlTarget::GeneratorParam { layer, index } => format!("/phosphor/layer/{layer}/source/{}", index + 1),
        ControlTarget::EffectParam { effect, index } => format!("/phosphor/fx/{effect}/{}", index + 1),
        ControlTarget::EffectMix(id) => format!("/phosphor/fx/{id}/mix"),
        ControlTarget::EffectEnabled(id) => format!("/phosphor/fx/{id}/on"),
        ControlTarget::TriggerClip { layer, slot } => format!("/phosphor/layer/{layer}/clip/{}", slot + 1),
        ControlTarget::TriggerColumn(col) => format!("/phosphor/column/{}", col + 1),
        ControlTarget::LayerSpeed(id) => format!("/phosphor/layer/{id}/speed"),
        ControlTarget::LayerFxBypass(id) => format!("/phosphor/layer/{id}/fxbypass"),
        ControlTarget::App(a) => format!("/phosphor/{}", a.osc()),
    }
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
        /// Opens every MIDI input except the disabled ones. Returns (all inputs, opened inputs).
        pub fn open_midi(
            &mut self,
            tx: mpsc::Sender<Incoming>,
            disabled: &std::collections::BTreeSet<String>,
            activity: Arc<std::sync::Mutex<super::Activity>>,
        ) -> (Vec<String>, Vec<String>) {
            self.midi.clear();
            let mut all = Vec::new();
            let mut open = Vec::new();
            let Ok(probe) = midir::MidiInput::new("Phosphor") else { return (all, open) };
            for port in probe.ports() {
                let name = probe.port_name(&port).unwrap_or_else(|_| "MIDI".into());
                all.push(name.clone());
                if disabled.contains(&name) {
                    continue;
                }
                let Ok(input) = midir::MidiInput::new("Phosphor") else { continue };
                let tx = tx.clone();
                let activity = activity.clone();
                let seen_name = name.clone();
                let conn = input.connect(
                    &port,
                    "phosphor-in",
                    move |_, msg, _| {
                        activity.lock().unwrap().midi_seen.insert(seen_name.clone(), std::time::Instant::now());
                        if let Some(incoming) = parse_midi(msg) {
                            let _ = tx.send(incoming);
                        }
                    },
                    (),
                );
                if let Ok(conn) = conn {
                    self.midi.push(conn);
                    open.push(name);
                }
            }
            (all, open)
        }

        pub fn open_osc(&mut self, port: u16, tx: mpsc::Sender<Incoming>, ip: Option<&str>, activity: Arc<std::sync::Mutex<super::Activity>>) -> Result<(), String> {
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
                        let Ok((n, from)) = socket.recv_from(&mut buf) else { continue };
                        let Ok((_, packet)) = rosc::decoder::decode_udp(&buf[..n]) else { continue };
                        let ip = from.ip().to_string();
                        {
                            let mut a = activity.lock().unwrap();
                            if a.osc_blocked.contains(&ip) {
                                continue;
                            }
                            let address = match &packet {
                                rosc::OscPacket::Message(m) => m.addr.clone(),
                                rosc::OscPacket::Bundle(_) => "(bundle)".into(),
                            };
                            let e = a.osc.entry(ip.clone()).or_insert(super::OscSender {
                                ip: ip.clone(),
                                count: 0,
                                last_address: String::new(),
                                last_seen: std::time::Instant::now(),
                            });
                            e.count += 1;
                            e.last_address = address;
                            e.last_seen = std::time::Instant::now();
                        }
                        deliver(packet, &tx);
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

#[cfg(test)]
mod tests {
    use super::*;
    use phosphor_engine::{ControlMap, LaunchTiming};

    fn send(hub: &mut ControlHub, comp: &mut Composition, source: ControlSource, value: f32) {
        hub.inject(comp, Incoming { source, value });
        hub.apply(comp, 0.0);
    }

    #[test]
    fn app_controls_and_toggles() {
        let mut hub = ControlHub::new();
        let mut comp = Composition::default();
        let id = comp.add_layer().id;
        let osc = |a: &str| ControlSource::Osc { address: a.to_string() };

        // Built-in OSC addresses for app-wide controls.
        send(&mut hub, &mut comp, osc("/phosphor/launch/timing"), 0.5);
        assert_eq!(comp.launch.timing, LaunchTiming::Beat);
        send(&mut hub, &mut comp, osc("/phosphor/launch/fade"), 0.0);
        assert!(!comp.launch.fade);
        send(&mut hub, &mut comp, osc("/phosphor/tempo"), 0.5);
        assert_eq!(hub.triggers.pop(), Some(Deferred::App(AppControl::Tempo, 0.5)));
        send(&mut hub, &mut comp, osc(&format!("/phosphor/layer/{id}/speed")), 1.0);
        assert!((comp.layer_mut(id).unwrap().rate - 4.0).abs() < 1e-3);

        // A key mapped to an on/off control flips it on each press and ignores the release.
        let key = ControlSource::Key { key: "Q".into() };
        comp.control_maps.push(ControlMap { source: key.clone(), target: ControlTarget::LayerEnabled(id) });
        let before = comp.layer_mut(id).unwrap().enabled;
        send(&mut hub, &mut comp, key.clone(), 1.0);
        send(&mut hub, &mut comp, key.clone(), 0.0);
        assert_eq!(comp.layer_mut(id).unwrap().enabled, !before);
        send(&mut hub, &mut comp, key.clone(), 1.0);
        assert_eq!(comp.layer_mut(id).unwrap().enabled, before);

        // Learn: the next message binds to the chosen control.
        hub.learning = Some(ControlTarget::LayerOpacity(id));
        let cc = ControlSource::MidiCc { channel: 0, cc: 21 };
        send(&mut hub, &mut comp, cc.clone(), 0.25);
        assert!(hub.learning.is_none());
        assert!(comp.control_maps.iter().any(|m| m.source == cc && m.target == ControlTarget::LayerOpacity(id)));
        send(&mut hub, &mut comp, cc.clone(), 0.75);
        assert!((comp.layer_mut(id).unwrap().opacity - 0.75).abs() < 1e-3);

        // An OSC toggle (TouchOSC style 1/0) follows the value.
        comp.control_maps.push(ControlMap { source: osc("/t/bypass"), target: ControlTarget::LayerFxBypass(id) });
        send(&mut hub, &mut comp, osc("/t/bypass"), 1.0);
        assert!(comp.layer_mut(id).unwrap().fx_bypass);
        send(&mut hub, &mut comp, osc("/t/bypass"), 0.0);
        assert!(!comp.layer_mut(id).unwrap().fx_bypass);
    }
}
