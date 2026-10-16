//! MIDI in/out (midir), OSC in (UDP listener thread + Zeroconf) and OSC feedback out.

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
        clock_tx: mpsc::Sender<(super::ClockEvent, std::time::Instant)>,
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
            let Ok(mut input) = midir::MidiInput::new("Phosphor") else { continue };
            // Keep clock and Start/Stop (MIDI clock sync); drop SysEx and active sensing.
            input.ignore(midir::Ignore::SysexAndActiveSense);
            let tx = tx.clone();
            let clock_tx = clock_tx.clone();
            let activity = activity.clone();
            let seen_name = name.clone();
            let conn = input.connect(
                &port,
                "phosphor-in",
                move |_, msg, _| {
                    activity.lock().unwrap().midi_seen.insert(seen_name.clone(), std::time::Instant::now());
                    let clock = match msg.first() {
                        Some(0xF1) => Some(super::ClockEvent::Quarter(msg.get(1).copied().unwrap_or(0))),
                        Some(0xF8) => Some(super::ClockEvent::Tick),
                        Some(0xFA) => Some(super::ClockEvent::Start),
                        Some(0xFB) => Some(super::ClockEvent::Continue),
                        Some(0xFC) => Some(super::ClockEvent::Stop),
                        _ => None,
                    };
                    if let Some(e) = clock {
                        let _ = clock_tx.send((e, std::time::Instant::now()));
                        return;
                    }
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
