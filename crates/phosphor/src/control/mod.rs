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
    /// A clip pad / key was let go (flash and piano clips react).
    ClipRelease(u64, usize),
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
    source.is_momentary()
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

/// MIDI clock and transport messages.
#[derive(Clone, Copy, Debug)]
pub enum ClockEvent {
    /// MIDI timecode quarter frame (its data byte).
    Quarter(u8),
    Tick,
    Start,
    Continue,
    Stop,
}

pub struct ControlHub {
    rx: mpsc::Receiver<Incoming>,
    tx: mpsc::Sender<Incoming>,
    clock_rx: mpsc::Receiver<(ClockEvent, std::time::Instant)>,
    clock_tx: mpsc::Sender<(ClockEvent, std::time::Instant)>,
    /// Incoming MIDI clock (tempo and beat position); the app's clock follows it.
    pub midi_clock: crate::clock::MidiClock,
    /// MIDI timecode in (cue list).
    pub mtc: crate::clock::Mtc,
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
    /// Faders and knobs only change a value once they reach it (no jump when the hardware is elsewhere).
    pub soft_takeover: bool,
    /// The last messages that came in, newest last (the MIDI / OSC monitor): when, from what, value.
    pub recent: std::collections::VecDeque<(std::time::Instant, ControlSource, f32)>,
    /// The monitor is paused (messages still work; the list stops moving).
    pub monitor_paused: bool,
    pickups: HashMap<String, Pickup>,
    /// Smoothed mappings on their way to a new value.
    glides: HashMap<String, Glide>,
    fresh: HashMap<ControlSource, f32>,
    /// Last value sent to (or received from) each controller input: only changes go out, nothing echoes back.
    sent: HashMap<ControlSource, f32>,
    feedback_ips: Vec<String>,
    next_feedback: f64,
    /// Control Surface OSC out: what each widget last sent.
    surface_sent: HashMap<u64, Vec<f32>>,
    #[cfg(not(target_arch = "wasm32"))]
    backend: native::Backend,
    /// OSC Query server (browse every address), with when it last got the project's controls.
    #[cfg(not(target_arch = "wasm32"))]
    pub oscquery: Option<oscquery::Server>,
    next_query: f64,
}

/// Controller settings kept between launches.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ControlSettings {
    pub osc_port: u16,
    pub feedback: bool,
    pub feedback_port: u16,
    pub soft_takeover: bool,
}

impl Default for ControlSettings {
    fn default() -> Self {
        Self { osc_port: DEFAULT_OSC_PORT, feedback: true, feedback_port: DEFAULT_FEEDBACK_PORT, soft_takeover: true }
    }
}

/// Soft takeover for one mapping: has the controller caught the value, and what it last set.
#[derive(Clone, Copy, Default)]
struct Pickup {
    caught: bool,
    /// The controller's last value (mapped to the target's range), to see it pass the current value.
    last_in: Option<f32>,
    /// What this mapping last set the target to (if the target is different now, something else moved it).
    last_set: Option<f32>,
}

/// A smoothed value gliding from `from` to `to` over `secs` (app seconds from `start`).
struct Glide {
    target: ControlTarget,
    from: f32,
    to: f32,
    start: f32,
    secs: f32,
}

/// Messages the MIDI / OSC monitor keeps.
pub(crate) const MONITOR_LEN: usize = 200;

/// Within this of the current value (0..1), a fader counts as caught.
const PICKUP_NEAR: f32 = 0.03;

impl ControlHub {
    /// Opens MIDI and OSC with saved settings.
    pub fn with_settings(s: ControlSettings) -> Self {
        let mut hub = Self::new_unopened();
        hub.osc_port = s.osc_port;
        hub.feedback = s.feedback;
        hub.feedback_port = s.feedback_port;
        hub.soft_takeover = s.soft_takeover;
        hub.reconnect();
        hub
    }

    pub fn settings(&self) -> ControlSettings {
        ControlSettings { osc_port: self.osc_port, feedback: self.feedback, feedback_port: self.feedback_port, soft_takeover: self.soft_takeover }
    }

    /// A hub for tests: values apply at once (soft takeover off; its own test turns it on).
    #[cfg(test)]
    pub fn new() -> Self {
        let mut hub = Self::new_unopened();
        hub.soft_takeover = false;
        hub.reconnect();
        hub
    }

    fn new_unopened() -> Self {
        let (tx, rx) = mpsc::channel();
        let (clock_tx, clock_rx) = mpsc::channel();
        Self {
            rx,
            tx,
            clock_rx,
            clock_tx,
            midi_clock: Default::default(),
            mtc: Default::default(),
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
            soft_takeover: true,
            recent: std::collections::VecDeque::new(),
            monitor_paused: false,
            pickups: HashMap::new(),
            glides: HashMap::new(),
            fresh: HashMap::new(),
            sent: HashMap::new(),
            feedback_ips: Vec::new(),
            next_feedback: 0.0,
            surface_sent: HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            backend: native::Backend::default(),
            #[cfg(not(target_arch = "wasm32"))]
            oscquery: None,
            next_query: 0.0,
        }
    }

    /// Hands the OSC Query server the project's controls (about once a second).
    pub fn publish_query(&mut self, comp: &Composition, now: f64) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(q) = &self.oscquery
            && now >= self.next_query
        {
            self.next_query = now + 1.0;
            q.publish(&oscquery::entries(comp));
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (comp, now);
    }

    /// (Re)opens every MIDI input and the OSC listener.
    pub fn reconnect(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (all, open) = self.backend.open_midi(self.tx.clone(), self.clock_tx.clone(), &self.midi_disabled, self.activity.clone());
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
            self.oscquery = None;
            self.oscquery = oscquery::Server::start(self.osc_port, self.local_ip.as_deref()).ok();
            native::start_gamepads(self.tx.clone());
            self.next_query = 0.0;
            if let Some(q) = &self.oscquery {
                self.osc_status.push_str(&format!(" · OSC Query on http://{}:{}", self.local_ip.clone().unwrap_or_else(|| "localhost".into()), q.port));
            }
        }
    }

    /// Drains incoming messages; binds the learning target if one is waiting.
    pub fn poll(&mut self, comp: &mut Composition) {
        while let Ok(msg) = self.rx.try_recv() {
            self.handle(comp, msg);
        }
    }

    /// Feeds clock messages into `midi_clock`, timed in app seconds (`now`) from when they arrived.
    pub fn poll_clock(&mut self, now: f64) {
        let instant_now = std::time::Instant::now();
        while let Ok((e, at)) = self.clock_rx.try_recv() {
            let t = now - instant_now.saturating_duration_since(at).as_secs_f64();
            match e {
                ClockEvent::Quarter(d) => self.mtc.quarter(d, t),
                ClockEvent::Tick => self.midi_clock.tick(t),
                ClockEvent::Start => self.midi_clock.start(t),
                ClockEvent::Continue => self.midi_clock.resume(),
                ClockEvent::Stop => self.midi_clock.stop(),
            }
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
        if !self.monitor_paused {
            self.recent.push_back((std::time::Instant::now(), msg.source.clone(), msg.value));
            while self.recent.len() > MONITOR_LEN {
                self.recent.pop_front();
            }
        }
        self.fresh.insert(msg.source.clone(), msg.value);
        self.sent.insert(msg.source.clone(), msg.value);
        self.last = Some(msg);
    }

    /// Control Surface widgets with an OSC address send their value to the surface's OSC host when it changes
    /// (whoever moved it: the Mac, a phone, MIDI, a modulator).
    pub fn surface_osc(&mut self, comp: &Composition) {
        let messages = comp.surface.osc_messages();
        if messages.is_empty() {
            self.surface_sent.clear();
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let host = vec![comp.surface.osc_host.trim().to_string()];
            for (id, address, values) in messages {
                if self.surface_sent.get(&id) != Some(&values) {
                    self.backend.send_osc_args(&host, comp.surface.osc_port, &address, &values);
                    self.surface_sent.insert(id, values);
                }
            }
        }
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
                    ControlSource::Key { .. } | ControlSource::Gamepad { .. } => {}
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = comp;
    }

    /// Writes this frame's values into their targets, then forgets them so the UI stays in charge
    /// until the controller moves again.
    pub fn apply(&mut self, comp: &mut Composition, now: f32) {
        self.glide(comp, now);
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
                map.range.apply(map.shape(v))
            };
            // (A smoothed mapping glides instead of jumping, so it doesn't need soft takeover.)
            if self.soft_takeover && v != FLIP && map.smoothing() == 0.0 && !momentary(&map.source) && !is_toggle(comp, &map.target) && !self.picked_up(comp, &map, v) {
                continue;
            }
            let secs = map.smoothing();
            if secs > 0.0 && v != FLIP {
                // Glide there over `secs` (from where it is now), frame by frame in `glide`.
                let from = get_target(comp, &map.target).unwrap_or(v);
                self.glides.insert(format!("{:?}", map.target), Glide { target: map.target.clone(), from, to: v, start: now, secs });
                continue;
            }
            set_target(comp, &map.target, v, &mut self.triggers);
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
                if let Some((t, _)) = targets.iter().find(|(t, _)| default_osc_address(t) == address) {
                    set_target(comp, &t.clone(), v, &mut self.triggers);
                }
            }
        }
        self.fresh.clear();
    }
}

impl ControlHub {
    /// Moves smoothed mappings along (ease out), and drops the ones that arrived.
    fn glide(&mut self, comp: &mut Composition, now: f32) {
        if self.glides.is_empty() {
            return;
        }
        let mut done = Vec::new();
        for (key, g) in &self.glides {
            let t = ((now - g.start) / g.secs.max(1e-3)).clamp(0.0, 1.0);
            let eased = 1.0 - (1.0 - t) * (1.0 - t);
            set_target(comp, &g.target, g.from + (g.to - g.from) * eased, &mut self.triggers);
            if t >= 1.0 {
                done.push(key.clone());
            }
        }
        for k in done {
            self.glides.remove(&k);
        }
    }

    /// Soft takeover: may this fader / knob (at `v`, the target's 0..1) change its target now? Only once it has
    /// caught the current value (near it, or passed it), and until something else moves the value.
    fn picked_up(&mut self, comp: &Composition, map: &phosphor_engine::ControlMap, v: f32) -> bool {
        let Some(current) = get_target(comp, &map.target) else { return true };
        let p = self.pickups.entry(format!("{:?}|{:?}", map.source, map.target)).or_default();
        // Moved by something else since we last set it (mouse, phone, modulator, Surprise): catch it again.
        if p.last_set.is_some_and(|s| (s - current).abs() > 0.005) {
            p.caught = false;
        }
        let near = (v - current).abs() <= PICKUP_NEAR;
        let passed = p.last_in.is_some_and(|last| (last - current) * (v - current) <= 0.0);
        if !p.caught && (near || passed) {
            p.caught = true;
        }
        p.last_in = Some(v);
        p.last_set = if p.caught { Some(v) } else { None };
        p.caught
    }
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

mod targets;
pub use targets::*;
#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub mod oscquery;
#[cfg(not(target_arch = "wasm32"))]
pub mod discovery;
#[cfg(test)]
mod tests;
