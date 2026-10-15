//! External control: MIDI (every input port) and OSC (UDP) in, with learn mode, applied to any
//! parameter each frame. The OSC port is advertised over Zeroconf (`_osc._udp`) so TouchOSC and
//! similar apps discover Phosphor by name.

use std::collections::HashMap;
use std::sync::mpsc;

use phosphor_engine::{AppControl, Choice, Composition, ControlMap, ControlSource, ControlTarget, EffectState, shaders};

/// Controller actions the composition can't carry out by itself; the app handles them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Deferred {
    Clip(u64, usize),
    Column(usize),
    /// Value 0..1, or `FLIP` to toggle an on/off control.
    App(AppControl, f32),
    /// Jump a layer's movie to this fraction of its length.
    Scrub(u64, f32),
}

/// Sent instead of a value when a pad or key press should flip an on/off control.
pub const FLIP: f32 = -1.0;

/// Pads and keys send press (1) / release (0); on/off targets toggle on press instead of following.
fn momentary(source: &ControlSource) -> bool {
    matches!(source, ControlSource::MidiNote { .. } | ControlSource::Key { .. })
}

fn is_toggle(comp: &Composition, target: &ControlTarget) -> bool {
    match target {
        ControlTarget::LayerEnabled(_) | ControlTarget::EffectEnabled(_) | ControlTarget::LayerFxBypass(_) => true,
        ControlTarget::App(a) => a.is_toggle(),
        ControlTarget::Surface(id) => comp.surface.widget(*id).is_some_and(|w| w.kind == phosphor_engine::WidgetKind::Toggle),
        _ => false,
    }
}

fn toggled(current: bool, v: f32) -> bool {
    if v == FLIP { !current } else { v > 0.5 }
}

pub const DEFAULT_OSC_PORT: u16 = 8000;
/// Where controllers listen for feedback (TouchOSC's default receive port).
pub const DEFAULT_FEEDBACK_PORT: u16 = 9000;
/// Feedback is checked this often (seconds); only changed values are sent.
const FEEDBACK_EVERY: f64 = 0.05;
/// OSC senders heard from within this long get feedback (seconds).
const FEEDBACK_SENDER_TTL: u64 = 600;

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
    /// Has used a built-in /phosphor/… address (its layout follows Phosphor's own addresses).
    pub uses_builtin: bool,
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
    /// Listen: catch the next input (any knob, pad, key or OSC message) into `heard`.
    pub listen_source: bool,
    pub heard: Option<ControlSource>,
    /// Clip/column launches and app-wide controls from controllers, for the app to carry out.
    pub triggers: Vec<Deferred>,
    /// Send current values back to controllers (OSC to every recent sender on `feedback_port`, MIDI to the
    /// outputs of the same devices) so faders, encoders and LEDs follow changes made anywhere else.
    pub feedback: bool,
    pub feedback_port: u16,
    fresh: HashMap<ControlSource, f32>,
    /// Last value sent to (or received from) each controller input: only changes go out, nothing echoes back.
    sent: HashMap<ControlSource, f32>,
    feedback_ips: Vec<String>,
    next_feedback: f64,
    #[cfg(not(target_arch = "wasm32"))]
    backend: native::Backend,
}

/// Controller settings kept between launches.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ControlSettings {
    pub osc_port: u16,
    pub feedback: bool,
    pub feedback_port: u16,
}

impl Default for ControlSettings {
    fn default() -> Self {
        Self { osc_port: DEFAULT_OSC_PORT, feedback: true, feedback_port: DEFAULT_FEEDBACK_PORT }
    }
}

impl ControlHub {
    /// Opens MIDI and OSC with saved settings.
    pub fn with_settings(s: ControlSettings) -> Self {
        let mut hub = Self::new_unopened();
        hub.osc_port = s.osc_port;
        hub.feedback = s.feedback;
        hub.feedback_port = s.feedback_port;
        hub.reconnect();
        hub
    }

    pub fn settings(&self) -> ControlSettings {
        ControlSettings { osc_port: self.osc_port, feedback: self.feedback, feedback_port: self.feedback_port }
    }

    #[cfg(test)]
    pub fn new() -> Self {
        let mut hub = Self::new_unopened();
        hub.reconnect();
        hub
    }

    fn new_unopened() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
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
            listen_source: false,
            heard: None,
            triggers: Vec::new(),
            feedback: true,
            feedback_port: DEFAULT_FEEDBACK_PORT,
            fresh: HashMap::new(),
            sent: HashMap::new(),
            feedback_ips: Vec::new(),
            next_feedback: 0.0,
            #[cfg(not(target_arch = "wasm32"))]
            backend: native::Backend::default(),
        }
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
        if self.listen_source {
            self.listen_source = false;
            self.heard = Some(msg.source.clone());
        } else if let Some(target) = self.learning.take() {
            comp.control_maps.retain(|m| m.target != target);
            comp.control_maps.push(ControlMap::new(msg.source.clone(), target));
        }
        self.fresh.insert(msg.source.clone(), msg.value);
        self.sent.insert(msg.source.clone(), msg.value);
        self.last = Some(msg);
    }

    /// Sends values that changed since the last check back to controllers. Cheap when nothing listens.
    pub fn feedback(&mut self, comp: &Composition, now: f64) {
        if !self.feedback || now < self.next_feedback {
            return;
        }
        self.next_feedback = now + FEEDBACK_EVERY;
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (ips, builtin): (Vec<String>, bool) = {
                let a = self.activity.lock().unwrap();
                let live: Vec<&OscSender> = a.osc.values().filter(|s| s.last_seen.elapsed().as_secs() < FEEDBACK_SENDER_TTL && !a.osc_blocked.contains(&s.ip)).collect();
                (live.iter().map(|s| s.ip.clone()).collect(), live.iter().any(|s| s.uses_builtin))
            };
            if ips != self.feedback_ips {
                // A device joined (or left): send it everything once.
                self.sent.clear();
                self.feedback_ips = ips;
            }
            let midi = self.backend.has_midi_out();
            if self.feedback_ips.is_empty() && !midi {
                return;
            }
            let mut out: Vec<(ControlSource, f32)> = Vec::new();
            for map in comp.control_maps.iter().filter(|m| m.enabled) {
                if matches!(map.source, ControlSource::Key { .. }) {
                    continue;
                }
                if let Some(v) = get_target(comp, &map.target) {
                    out.push((map.source.clone(), map.range.unapply(v)));
                }
            }
            if builtin {
                for (t, _) in all_targets(comp) {
                    if let Some(v) = get_target(comp, &t) {
                        out.push((ControlSource::Osc { address: default_osc_address(&t) }, v));
                    }
                }
            }
            for (source, v) in out {
                if self.sent.get(&source).is_some_and(|old| (old - v).abs() < 0.002) {
                    continue;
                }
                self.sent.insert(source.clone(), v);
                match &source {
                    ControlSource::Osc { address } => self.backend.send_osc(&self.feedback_ips, self.feedback_port, address, v),
                    ControlSource::MidiCc { channel, cc } => self.backend.send_midi(&[0xB0 | channel, *cc, (v * 127.0).round() as u8]),
                    ControlSource::MidiNote { channel, note } => self.backend.send_midi(&[0x90 | channel, *note, if v > 0.5 { 127 } else { 0 }]),
                    ControlSource::Key { .. } => {}
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = comp;
    }

    /// Writes this frame's values into their targets, then forgets them so the UI stays in charge
    /// until the controller moves again.
    pub fn apply(&mut self, comp: &mut Composition, now: f32) {
        if self.fresh.is_empty() {
            return;
        }
        for map in comp.control_maps.clone() {
            if !map.enabled {
                continue;
            }
            let Some(v) = self.fresh.get(&map.source).copied() else { continue };
            let v = if momentary(&map.source) && is_toggle(comp, &map.target) {
                if v <= 0.5 {
                    continue;
                }
                FLIP
            } else {
                map.range.apply(v)
            };
            set_target(comp, &map.target, v, &mut self.triggers);
        }
        let _ = now;
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
                if let Some((t, _)) = targets.iter().find(|(t, _)| default_osc_address(t) == address) {
                    set_target(comp, &t.clone(), v, &mut self.triggers);
                }
            }
        }
        self.fresh.clear();
    }
}

fn scaled(p: &shaders::Param, v: f32) -> f32 {
    p.min + v.clamp(0.0, 1.0) * (p.max - p.min)
}

pub fn find_effect(comp: &Composition, id: u64) -> Option<&EffectState> {
    comp.master_effects.iter().chain(comp.layers.iter().flat_map(|l| l.effects.iter())).find(|e| e.id == id)
}

pub fn find_effect_mut(comp: &mut Composition, id: u64) -> Option<&mut EffectState> {
    comp.master_effects.iter_mut().chain(comp.layers.iter_mut().flat_map(|l| l.effects.iter_mut())).find(|e| e.id == id)
}

/// Inverse of `unit_to`: where `x` sits in the range, 0..1.
pub fn to_unit(x: f32, min: f32, max: f32, log: bool) -> f32 {
    let v = if log { (x / min).ln() / (max / min).ln() } else { (x - min) / (max - min) };
    if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 }
}

fn param_unit(p: &shaders::Param, x: f32) -> f32 {
    to_unit(x, p.min, p.max, false)
}

/// The current 0..1 value of a target, the inverse of `set_target` (controller feedback, surfaces, remote).
/// None for triggers and for controls that live outside the composition (tempo, audio).
pub fn get_target(comp: &Composition, target: &ControlTarget) -> Option<f32> {
    let on = |b: bool| if b { 1.0 } else { 0.0 };
    let layer = |id: &u64| comp.layers.iter().find(|l| l.id == *id);
    let effect = |id: &u64| find_effect(comp, *id);
    Some(match target {
        ControlTarget::MasterFade => comp.master_fade,
        ControlTarget::LayerOpacity(id) => layer(id)?.opacity,
        ControlTarget::LayerEnabled(id) => on(layer(id)?.enabled),
        ControlTarget::LayerFxBypass(id) => on(layer(id)?.fx_bypass),
        ControlTarget::LayerSpeed(id) => to_unit(layer(id)?.rate, 0.1, 4.0, true),
        ControlTarget::LayerLoopIn(id) => layer(id)?.loop_in,
        ControlTarget::LayerLoopOut(id) => layer(id)?.loop_out,
        ControlTarget::LayerDirection(id) => {
            layer(id)?.direction.unit()
        }
        ControlTarget::GeneratorParam { layer: lid, index } => {
            let l = layer(lid)?;
            let phosphor_engine::SourceSpec::Generator(g) = &l.source else { return None };
            let p = shaders::generator(g)?.params.get(*index)?;
            param_unit(p, l.generator_values.get(*index).copied().unwrap_or(p.default))
        }
        ControlTarget::EffectParam { effect: eid, index } => {
            let fx = effect(eid)?;
            let p = shaders::effect(&fx.effect)?.params.get(*index)?;
            param_unit(p, fx.values.get(*index).copied().unwrap_or(p.default))
        }
        ControlTarget::EffectMix(id) => effect(id)?.mix,
        ControlTarget::EffectEnabled(id) => on(effect(id)?.enabled),
        ControlTarget::Surface(id) => comp.surface.widgets.iter().find(|w| w.id == *id)?.value,
        ControlTarget::SurfaceY(id) => comp.surface.widgets.iter().find(|w| w.id == *id)?.value_y,
        ControlTarget::App(a) => match a {
            AppControl::LaunchFade => on(comp.launch.fade),
            AppControl::FadeTime => to_unit(comp.launch.fade_secs, 0.05, 10.0, true),
            AppControl::LaunchTiming => comp.launch.timing.unit(),
            AppControl::MasterFxBypass => on(comp.master_fx_bypass),
            AppControl::Syphon => on(comp.syphon),
            AppControl::Ndi => on(comp.ndi.enabled),
            AppControl::Crossfader => comp.crossfader,
            _ => return None,
        },
        ControlTarget::TriggerClip { .. } | ControlTarget::TriggerColumn(_) | ControlTarget::LayerScrub(_) => return None,
    })
}

/// Maps 0..1 onto a range, logarithmically when `log`.
pub fn unit_to(v: f32, min: f32, max: f32, log: bool) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if log { min * (max / min).powf(v) } else { min + v * (max - min) }
}

/// Sets any target to a 0..1 value (or `FLIP` for on/off ones). Clip launches and app-wide
/// controls the composition can't do itself are pushed onto `out` for the app.
pub fn set_target(comp: &mut Composition, target: &ControlTarget, v: f32, out: &mut Vec<Deferred>) {
    set_target_depth(comp, target, v, out, 0);
}

/// Surface widgets can drive other widgets; `depth` stops loops (A drives B drives A).
fn set_target_depth(comp: &mut Composition, target: &ControlTarget, v: f32, out: &mut Vec<Deferred>, depth: u8) {
    if depth > 6 {
        return;
    }
    match target {
        ControlTarget::TriggerClip { layer, slot } => {
            if v > 0.5 {
                out.push(Deferred::Clip(*layer, *slot));
            }
        }
        ControlTarget::TriggerColumn(col) => {
            if v > 0.5 {
                out.push(Deferred::Column(*col));
            }
        }
        ControlTarget::Surface(id) | ControlTarget::SurfaceY(id) => {
            let y = matches!(target, ControlTarget::SurfaceY(_));
            let Some(w) = comp.surface.widget_mut(*id) else { return };
            let value = if !y && w.kind == phosphor_engine::WidgetKind::Toggle { toggled(w.value > 0.5, v) as u8 as f32 } else { v.clamp(0.0, 1.0) };
            let bindings = if y {
                w.value_y = value;
                w.bindings_y.clone()
            } else {
                w.value = value;
                w.bindings.clone()
            };
            for b in bindings {
                set_target_depth(comp, &b.target, b.range.apply(value), out, depth + 1);
            }
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
        ControlTarget::LayerLoopIn(id) => {
            if let Some(l) = comp.layer_mut(*id) {
                l.loop_in = v.clamp(0.0, 1.0);
            }
        }
        ControlTarget::LayerLoopOut(id) => {
            if let Some(l) = comp.layer_mut(*id) {
                l.loop_out = v.clamp(0.0, 1.0);
            }
        }
        ControlTarget::LayerDirection(id) => {
            if let Some(l) = comp.layer_mut(*id) {
                l.direction = Choice::from_unit(v);
            }
        }
        ControlTarget::LayerScrub(id) => out.push(Deferred::Scrub(*id, v.clamp(0.0, 1.0))),
        ControlTarget::App(a) => match a {
            AppControl::LaunchFade => comp.launch.fade = toggled(comp.launch.fade, v),
            AppControl::FadeTime => comp.launch.fade_secs = unit_to(v, 0.05, 10.0, true),
            AppControl::LaunchTiming => comp.launch.timing = Choice::from_unit(v),
            AppControl::MasterFxBypass => comp.master_fx_bypass = toggled(comp.master_fx_bypass, v),
            AppControl::Syphon => comp.syphon = toggled(comp.syphon, v),
            AppControl::Ndi => comp.ndi.enabled = toggled(comp.ndi.enabled, v),
            AppControl::Crossfader => comp.crossfader = v.clamp(0.0, 1.0),
            _ => out.push(Deferred::App(*a, v)),
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
}

/// Every bindable target in the composition, with a readable name.
pub fn all_targets(comp: &Composition) -> Vec<(ControlTarget, String)> {
    let mut out = vec![(ControlTarget::MasterFade, "Master fade".to_string())];
    out.extend(AppControl::ALL.iter().map(|a| (ControlTarget::App(*a), a.name().to_string())));
    for w in &comp.surface.widgets {
        if w.kind == phosphor_engine::WidgetKind::Label {
            continue;
        }
        out.push((ControlTarget::Surface(w.id), format!("Surface · {}", w.label)));
        if w.kind == phosphor_engine::WidgetKind::XyPad {
            out.push((ControlTarget::SurfaceY(w.id), format!("Surface · {} (Y)", w.label)));
        }
    }
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
        if matches!(l.source, phosphor_engine::SourceSpec::Movie { .. }) {
            out.push((ControlTarget::LayerScrub(l.id), format!("{} · scrub", l.name)));
            out.push((ControlTarget::LayerLoopIn(l.id), format!("{} · loop in", l.name)));
            out.push((ControlTarget::LayerLoopOut(l.id), format!("{} · loop out", l.name)));
            out.push((ControlTarget::LayerDirection(l.id), format!("{} · direction", l.name)));
        }
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
        ControlTarget::LayerLoopIn(id) => format!("/phosphor/layer/{id}/loop/in"),
        ControlTarget::LayerLoopOut(id) => format!("/phosphor/layer/{id}/loop/out"),
        ControlTarget::LayerDirection(id) => format!("/phosphor/layer/{id}/direction"),
        ControlTarget::LayerScrub(id) => format!("/phosphor/layer/{id}/scrub"),
        ControlTarget::Surface(id) => format!("/phosphor/surface/{id}"),
        ControlTarget::SurfaceY(id) => format!("/phosphor/surface/{id}/y"),
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
        /// Outputs of the same devices as the open inputs (controller feedback: motor faders, LED rings).
        midi_out: Vec<midir::MidiOutputConnection>,
        /// Socket feedback goes out from.
        osc_out: Option<UdpSocket>,
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
            self.midi_out.clear();
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
            // Feedback goes to the outputs whose names match an open input (one device, two directions).
            if let Ok(probe) = midir::MidiOutput::new("Phosphor") {
                for port in probe.ports() {
                    let name = probe.port_name(&port).unwrap_or_default();
                    if !open.contains(&name) {
                        continue;
                    }
                    if let Ok(output) = midir::MidiOutput::new("Phosphor")
                        && let Ok(conn) = output.connect(&port, "phosphor-out")
                    {
                        self.midi_out.push(conn);
                    }
                }
            }
            (all, open)
        }

        pub fn has_midi_out(&self) -> bool {
            !self.midi_out.is_empty()
        }

        pub fn send_midi(&mut self, msg: &[u8]) {
            for c in &mut self.midi_out {
                let _ = c.send(msg);
            }
        }

        /// One float message to every recipient; never blocks (UDP).
        pub fn send_osc(&mut self, ips: &[String], port: u16, address: &str, value: f32) {
            if ips.is_empty() {
                return;
            }
            if self.osc_out.is_none() {
                self.osc_out = UdpSocket::bind("0.0.0.0:0").and_then(|s| s.set_nonblocking(true).map(|()| s)).ok();
            }
            let Some(sock) = &self.osc_out else { return };
            let msg = rosc::OscPacket::Message(rosc::OscMessage { addr: address.to_string(), args: vec![rosc::OscType::Float(value)] });
            let Ok(bytes) = rosc::encoder::encode(&msg) else { return };
            for ip in ips {
                let _ = sock.send_to(&bytes, (ip.as_str(), port));
            }
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
                                uses_builtin: false,
                            });
                            e.uses_builtin |= address.starts_with("/phosphor/");
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
        comp.control_maps.push(ControlMap::new(key.clone(), ControlTarget::LayerEnabled(id)));
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
        comp.control_maps.push(ControlMap::new(osc("/t/bypass"), ControlTarget::LayerFxBypass(id)));
        send(&mut hub, &mut comp, osc("/t/bypass"), 1.0);
        assert!(comp.layer_mut(id).unwrap().fx_bypass);
        send(&mut hub, &mut comp, osc("/t/bypass"), 0.0);
        assert!(!comp.layer_mut(id).unwrap().fx_bypass);
    }

    #[test]
    fn surface_widgets_drive_bindings() {
        use phosphor_engine::{Binding, ValueRange, WidgetKind};
        let mut hub = ControlHub::new();
        let mut comp = Composition::default();
        let a = comp.add_layer().id;
        let b = comp.add_layer().id;
        // A fader driving two layers: one straight 0.2–0.8, one inverted.
        let fader = comp.surface.add(WidgetKind::Fader, 0.0, 0.0);
        let w = comp.surface.widget_mut(fader).unwrap();
        w.bindings.push(Binding { target: ControlTarget::LayerOpacity(a), range: ValueRange { min: 0.2, max: 0.8, invert: false } });
        w.bindings.push(Binding { target: ControlTarget::LayerOpacity(b), range: ValueRange { min: 0.0, max: 1.0, invert: true } });
        // A MIDI knob mapped to the fader.
        let cc = ControlSource::MidiCc { channel: 0, cc: 7 };
        comp.control_maps.push(ControlMap::new(cc.clone(), ControlTarget::Surface(fader)));
        send(&mut hub, &mut comp, cc.clone(), 0.5);
        assert!((comp.surface.widget(fader).unwrap().value - 0.5).abs() < 1e-3);
        assert!((comp.layer_mut(a).unwrap().opacity - 0.5).abs() < 1e-3);
        assert!((comp.layer_mut(b).unwrap().opacity - 0.5).abs() < 1e-3);
        send(&mut hub, &mut comp, cc.clone(), 1.0);
        assert!((comp.layer_mut(a).unwrap().opacity - 0.8).abs() < 1e-3);
        assert!(comp.layer_mut(b).unwrap().opacity < 1e-3);

        // A key flips a surface toggle, which switches a layer.
        let tog = comp.surface.add(WidgetKind::Toggle, 2.0, 0.0);
        comp.surface.widget_mut(tog).unwrap().bindings.push(Binding { target: ControlTarget::LayerEnabled(a), range: ValueRange::default() });
        let key = ControlSource::Key { key: "T".into() };
        comp.control_maps.push(ControlMap::new(key.clone(), ControlTarget::Surface(tog)));
        send(&mut hub, &mut comp, key.clone(), 1.0);
        send(&mut hub, &mut comp, key.clone(), 0.0);
        assert_eq!(comp.surface.widget(tog).unwrap().value, 1.0);
        assert!(comp.layer_mut(a).unwrap().enabled);
        send(&mut hub, &mut comp, key.clone(), 1.0);
        assert!(!comp.layer_mut(a).unwrap().enabled);

        // Two widgets driving each other don't loop forever.
        let x = comp.surface.add(WidgetKind::Slider, 0.0, 2.0);
        let y = comp.surface.add(WidgetKind::Slider, 0.0, 3.0);
        comp.surface.widget_mut(x).unwrap().bindings.push(Binding { target: ControlTarget::Surface(y), range: ValueRange::default() });
        comp.surface.widget_mut(y).unwrap().bindings.push(Binding { target: ControlTarget::Surface(x), range: ValueRange::default() });
        let mut out = Vec::new();
        set_target(&mut comp, &ControlTarget::Surface(x), 0.3, &mut out);
        assert!((comp.surface.widget(y).unwrap().value - 0.3).abs() < 1e-3);

        // A switched-off mapping does nothing; Listen catches the next input.
        comp.control_maps.iter_mut().for_each(|m| m.enabled = false);
        send(&mut hub, &mut comp, cc.clone(), 0.0);
        assert!((comp.layer_mut(a).unwrap().opacity - 0.8).abs() < 1e-3);
        hub.listen_source = true;
        send(&mut hub, &mut comp, ControlSource::Osc { address: "/x".into() }, 0.4);
        assert_eq!(hub.heard, Some(ControlSource::Osc { address: "/x".into() }));
        assert!(!hub.listen_source);
    }

    /// Reading a control back gives what was set (feedback, surfaces and the remote rely on it).
    #[test]
    fn get_target_reads_back_what_set_target_wrote() {
        let mut comp = Composition::starter();
        let mut out = Vec::new();
        let mut checked = 0;
        for (t, name) in all_targets(&comp) {
            if get_target(&comp, &t).is_none() {
                continue;
            }
            for v in [0.0, 0.3, 1.0] {
                set_target(&mut comp, &t, v, &mut out);
                let back = get_target(&comp, &t).unwrap();
                let toggle = is_toggle(&comp, &t);
                let choice = matches!(t, ControlTarget::App(AppControl::LaunchTiming) | ControlTarget::LayerDirection(_));
                let want = if toggle { if v > 0.5 { 1.0 } else { 0.0 } } else { v };
                if choice {
                    let mut again = Vec::new();
                    let before = comp.clone();
                    set_target(&mut comp, &t, back, &mut again);
                    assert_eq!(comp, before, "{name}: choice {v} read back as {back} picks another option");
                } else {
                    assert!((back - want).abs() < 0.01, "{name}: set {v}, read back {back}");
                }
            }
            checked += 1;
        }
        assert!(checked > 10, "only {checked} targets checked");
    }

    /// A TouchOSC-style device that sent a built-in address gets changes made elsewhere, once each.
    #[test]
    fn osc_feedback_reaches_the_sender() {
        use std::net::UdpSocket;
        let mut hub = ControlHub::with_settings(ControlSettings { osc_port: 18_571, feedback: true, feedback_port: 18_572 });
        let device = UdpSocket::bind("127.0.0.1:18572").unwrap();
        device.set_read_timeout(Some(std::time::Duration::from_millis(500))).unwrap();
        let msg = rosc::encoder::encode(&rosc::OscPacket::Message(rosc::OscMessage { addr: "/phosphor/master".into(), args: vec![rosc::OscType::Float(0.8)] })).unwrap();
        device.send_to(&msg, "127.0.0.1:18571").unwrap();
        let mut comp = Composition::starter();
        for _ in 0..50 {
            hub.poll(&mut comp);
            if hub.last.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        hub.apply(&mut comp, 0.0);
        assert!((comp.master_fade - 0.8).abs() < 1e-3, "controller moved master");
        hub.feedback(&comp, 1.0); // first contact: everything once
        let mut buf = [0u8; 4096];
        while device.recv(&mut buf).is_ok() {}
        comp.master_fade = 0.25; // moved with the mouse
        hub.feedback(&comp, 2.0);
        let n = device.recv(&mut buf).expect("feedback for the master fader");
        let (_, packet) = rosc::decoder::decode_udp(&buf[..n]).unwrap();
        let rosc::OscPacket::Message(m) = packet else { panic!("bundle") };
        assert_eq!(m.addr, "/phosphor/master");
        assert_eq!(m.args, vec![rosc::OscType::Float(0.25)]);
        hub.feedback(&comp, 3.0); // unchanged: nothing more
        assert!(device.recv(&mut buf).is_err(), "unchanged values must not be resent");
    }
}
