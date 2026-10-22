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
    /// Jump a layer's movie one step back / forward.
    Jump(u64, bool),
}

/// Sent instead of a value when a pad or key press should flip an on/off control.
pub const FLIP: f32 = -1.0;

/// Pads and keys send press (1) / release (0); on/off targets toggle on press instead of following.
fn momentary(source: &ControlSource) -> bool {
    source.is_momentary()
}

fn is_toggle(comp: &Composition, target: &ControlTarget) -> bool {
    match target {
        ControlTarget::LayerEnabled(_) | ControlTarget::EffectEnabled(_) | ControlTarget::LayerFxBypass(_) | ControlTarget::SliceEnabled(_) => true,
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
    /// More ports to listen on (apps that only send to their own port).
    pub osc_extra_ports: Vec<u16>,
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
    /// How close (0..1 of the range) a fader must come to count as having reached the value.
    pub pickup_near: f32,
    /// Soft takeover glides the value to the fader instead of waiting for the fader to reach it.
    pub takeover_glide: bool,
    /// The last messages that came in, newest last (the MIDI / OSC monitor): when, from what, value.
    pub recent: std::collections::VecDeque<(std::time::Instant, ControlSource, f32)>,
    /// The monitor is paused (messages still work; the list stops moving).
    pub monitor_paused: bool,
    pickups: HashMap<String, Pickup>,
    /// Dead band: the last value each mapping let through.
    passed: HashMap<String, f32>,
    /// Smoothed mappings on their way to a new value.
    glides: HashMap<String, Glide>,
    /// Moves waiting for their Delay: (when, target, value, smoothing seconds).
    delayed: Vec<(f32, ControlTarget, f32, f32)>,
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
    /// DMX in (Art-Net / sACN) while it's switched on.
    #[cfg(not(target_arch = "wasm32"))]
    dmx_listener: Option<crate::dmx::DmxListener>,
}

/// Controller settings kept between launches.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ControlSettings {
    pub osc_port: u16,
    pub osc_extra_ports: Vec<u16>,
    pub feedback: bool,
    pub feedback_port: u16,
    pub soft_takeover: bool,
    pub pickup_near: f32,
    pub takeover_glide: bool,
    /// Listen for DMX (Art-Net / sACN) from a lighting desk as controller input.
    pub dmx_in: bool,
}

impl Default for ControlSettings {
    fn default() -> Self {
        Self { osc_port: DEFAULT_OSC_PORT, osc_extra_ports: Vec::new(), feedback: true, feedback_port: DEFAULT_FEEDBACK_PORT, soft_takeover: true, pickup_near: PICKUP_NEAR, takeover_glide: false, dmx_in: false }
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

/// Within this of the current value (0..1), a fader counts as caught (the default; `pickup_near`).
pub(crate) const PICKUP_NEAR: f32 = 0.03;

/// Soft takeover's glide (`takeover_glide`): how long the value takes to reach the fader.
const TAKEOVER_GLIDE_SECS: f32 = 0.3;

impl ControlHub {
    /// Opens MIDI and OSC with saved settings.
    pub fn with_settings(s: ControlSettings) -> Self {
        let mut hub = Self::new_unopened();
        hub.osc_port = s.osc_port;
        hub.osc_extra_ports = s.osc_extra_ports;
        hub.feedback = s.feedback;
        hub.feedback_port = s.feedback_port;
        hub.soft_takeover = s.soft_takeover;
        hub.pickup_near = s.pickup_near.clamp(0.005, 0.5);
        hub.takeover_glide = s.takeover_glide;
        hub.reconnect();
        hub.set_dmx_in(s.dmx_in);
        hub
    }

    pub fn settings(&self) -> ControlSettings {
        ControlSettings { osc_port: self.osc_port, osc_extra_ports: self.osc_extra_ports.clone(), feedback: self.feedback, feedback_port: self.feedback_port, soft_takeover: self.soft_takeover, pickup_near: self.pickup_near, takeover_glide: self.takeover_glide, dmx_in: self.dmx_in() }
    }

    /// Starts or stops listening for DMX (Art-Net on udp/6454, sACN on udp/5568) as controller input.
    pub fn set_dmx_in(&mut self, on: bool) {
        #[cfg(not(target_arch = "wasm32"))]
        if on != self.dmx_listener.is_some() {
            self.dmx_listener = on.then(|| crate::dmx::DmxListener::start(self.tx.clone()));
        }
        #[cfg(target_arch = "wasm32")]
        let _ = on;
    }

    pub fn dmx_in(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        return self.dmx_listener.is_some();
        #[cfg(target_arch = "wasm32")]
        false
    }

    /// What the DMX listener has heard ("Art-Net on udp/6454 · 2 universes"), or why it can't listen.
    pub fn dmx_status(&self) -> String {
        #[cfg(not(target_arch = "wasm32"))]
        return self.dmx_listener.as_ref().map_or_else(String::new, |l| l.status());
        #[cfg(target_arch = "wasm32")]
        String::new()
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
            osc_extra_ports: Vec::new(),
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
            pickup_near: PICKUP_NEAR,
            takeover_glide: false,
            recent: std::collections::VecDeque::new(),
            monitor_paused: false,
            pickups: HashMap::new(),
            passed: HashMap::new(),
            glides: HashMap::new(),
            delayed: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            dmx_listener: None,
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
            self.osc_status = match self.backend.open_osc(self.osc_port, &self.osc_extra_ports, self.tx.clone(), self.local_ip.as_deref(), self.activity.clone()) {
                Ok(failed) => {
                    let also: Vec<String> = self.osc_extra_ports.iter().filter(|p| **p != self.osc_port).map(|p| p.to_string()).collect();
                    let also = if also.is_empty() { String::new() } else { format!(" (and {})", also.join(", ")) };
                    let failed = if failed.is_empty() { String::new() } else { format!(" · couldn't open {}", failed.join("; ")) };
                    format!("OSC listening on udp/{}{also} at {}{failed}", self.osc_port, self.local_ip.clone().unwrap_or_else(|| "this machine".into()))
                }
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
                    ControlSource::Key { .. } | ControlSource::Gamepad { .. } | ControlSource::Dmx { .. } => {}
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
        self.land_delayed(comp, now);
        if self.fresh.is_empty() {
            return;
        }
        for map in comp.control_maps.clone() {
            if !map.enabled {
                continue;
            }
            let v = match &map.source {
                // An OSC address with wildcards (`/fader/*`, `/btn/[1-4]`, `/{a,b}/x`) takes any matching message.
                ControlSource::Osc { address } if is_osc_pattern(address) => self
                    .fresh
                    .iter()
                    .find_map(|(s, v)| matches!(s, ControlSource::Osc { address: a } if osc_match(address, a)).then_some(*v)),
                s => self.fresh.get(s).copied(),
            };
            let Some(v) = v else { continue };
            // Endless knob: the message is a turn (clicks up / down), added to where the value is now.
            if let Some(step) = map.endless() {
                let clicks = phosphor_engine::encoder_clicks(v);
                if clicks != 0
                    && let Some(current) = get_target(comp, &map.target)
                {
                    set_target(comp, &map.target, (current + clicks as f32 * step).clamp(0.0, 1.0), &mut self.triggers);
                }
                continue;
            }
            // Dead band: too small a change from the last value let through is ignored.
            let band = map.dead_band();
            if band > 0.0 {
                let key = format!("{:?}|{:?}", map.source, map.target);
                if self.passed.get(&key).is_some_and(|last| (v - last).abs() < band) {
                    continue;
                }
                self.passed.insert(key, v);
            }
            let v = if momentary(&map.source) && is_toggle(comp, &map.target) {
                if v <= 0.5 {
                    continue;
                }
                FLIP
            } else {
                // A slider with limits: the controller's whole travel is spread over them.
                comp.control_prefs.within_limits(&map.target, map.range.apply(map.shape(v)))
            };
            // (A smoothed mapping glides instead of jumping, so it doesn't need soft takeover.)
            // (Delayed moves land later, so the takeover check would never see them arrive; they skip it too.)
            if self.soft_takeover && v != FLIP && map.smoothing() == 0.0 && map.delay() == 0.0 && map.fall() == 0.0 && !momentary(&map.source) && !is_toggle(comp, &map.target) && !self.picked_up(comp, &map, v) {
                // Glide: the value comes to the fader (caught once it gets there) instead of waiting for it.
                if self.takeover_glide {
                    let from = get_target(comp, &map.target).unwrap_or(v);
                    self.glide_to(map.target.clone(), from, v, now, TAKEOVER_GLIDE_SECS);
                }
                continue;
            }
            if map.delay() > 0.0 {
                self.delayed.push((now + map.delay(), map.target.clone(), v, map.smoothing()));
                continue;
            }
            // Fall: a drop glides down over its seconds; a rise lands at once (and stops the fall).
            let fall = map.fall();
            if fall > 0.0 && v != FLIP {
                let from = get_target(comp, &map.target).unwrap_or(v);
                let key = format!("{:?}", map.target);
                if v < from {
                    // Keep a fall that's already on its way down to this value (each message would restart it).
                    if self.glides.get(&key).is_none_or(|g| (g.to - v).abs() > 1e-6) {
                        self.glides.insert(key, Glide { target: map.target.clone(), from, to: v, start: now, secs: fall });
                    }
                } else {
                    self.glides.remove(&key);
                    set_target(comp, &map.target, v, &mut self.triggers);
                }
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
    /// Delayed moves whose time has come land now (gliding when their mapping is smoothed).
    fn land_delayed(&mut self, comp: &mut Composition, now: f32) {
        if self.delayed.is_empty() {
            return;
        }
        let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.delayed).into_iter().partition(|d| d.0 <= now);
        self.delayed = later;
        for (_, target, v, secs) in due {
            if secs > 0.0 && v != FLIP {
                let from = get_target(comp, &target).unwrap_or(v);
                self.glide_to(target, from, v, now, secs);
            } else {
                set_target(comp, &target, v, &mut self.triggers);
            }
        }
    }

    /// Glides `target` from `from` to `to` over `secs` (eased; replaces a glide already moving it).
    pub fn glide_to(&mut self, target: ControlTarget, from: f32, to: f32, now: f32, secs: f32) {
        self.glides.insert(format!("{target:?}"), Glide { target, from, to, start: now, secs: secs.max(0.0) });
    }

    /// Where `target` is gliding to, while it is.
    pub fn glide_target(&self, target: &ControlTarget) -> Option<f32> {
        self.glides.get(&format!("{target:?}")).map(|g| g.to)
    }

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
        let near = (v - current).abs() <= self.pickup_near;
        let passed = p.last_in.is_some_and(|last| (last - current) * (v - current) <= 0.0);
        if !p.caught && (near || passed) {
            p.caught = true;
        }
        p.last_in = Some(v);
        p.last_set = if p.caught { Some(v) } else { None };
        p.caught
    }
}

/// An OSC address with OSC 1.0 pattern characters in it.
pub(crate) fn is_osc_pattern(address: &str) -> bool {
    address.contains(['*', '?', '[', '{'])
}

/// OSC 1.0 address matching: `*` any run within a part, `?` one character, `[abc]` / `[a-z]` / `[!a]` one of a set,
/// `{one,two}` one of the words. Parts (between `/`) match one by one.
pub(crate) fn osc_match(pattern: &str, address: &str) -> bool {
    let (p, a): (Vec<&str>, Vec<&str>) = (pattern.split('/').collect(), address.split('/').collect());
    p.len() == a.len() && p.iter().zip(&a).all(|(p, a)| part_match(p.as_bytes(), a.as_bytes()))
}

fn part_match(p: &[u8], a: &[u8]) -> bool {
    match p.first() {
        None => a.is_empty(),
        Some(b'*') => (0..=a.len()).any(|k| part_match(&p[1..], &a[k..])),
        Some(b'?') => !a.is_empty() && part_match(&p[1..], &a[1..]),
        Some(b'[') => {
            let Some(end) = p.iter().position(|c| *c == b']') else { return false };
            let Some(&c) = a.first() else { return false };
            let set = &p[1..end];
            let (negate, set) = if set.first() == Some(&b'!') { (true, &set[1..]) } else { (false, set) };
            let mut hit = false;
            let mut i = 0;
            while i < set.len() {
                if i + 2 < set.len() && set[i + 1] == b'-' {
                    hit |= (set[i]..=set[i + 2]).contains(&c);
                    i += 3;
                } else {
                    hit |= set[i] == c;
                    i += 1;
                }
            }
            hit != negate && part_match(&p[end + 1..], &a[1..])
        }
        Some(b'{') => {
            let Some(end) = p.iter().position(|c| *c == b'}') else { return false };
            p[1..end].split(|c| *c == b',').any(|w| a.starts_with(w) && part_match(&p[end + 1..], &a[w.len()..]))
        }
        Some(c) => a.first() == Some(c) && part_match(&p[1..], &a[1..]),
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
