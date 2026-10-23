//! Sends LED / DMX universes over the network: Art-Net (ArtDmx, UDP 6454; broadcast when no address is
//! set) or sACN / E1.31 (UDP 5568; multicast per universe when no address is set), or to a USB interface
//! (`usb_dmx`). Does nothing while LED output is off; paced to its own frame rate.

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

use phosphor_engine::{DmxProtocol, DmxSettings};

use crate::pacing::FramePacer;

const ARTNET_PORT: u16 = 6454;
const SACN_PORT: u16 = 5568;

#[derive(Default)]
pub struct DmxOut {
    socket: Option<UdpSocket>,
    pacer: FramePacer,
    seq: u8,
    /// Universes sent per frame last time (for the panel), and the last send error.
    pub universes: usize,
    pub error: Option<String>,
    pub packets: u64,
    cid: [u8; 16],
    /// The DMX monitor is on screen: keep a copy of what goes out (`sent`). The panel sets it each frame it draws.
    pub watching: bool,
    pub sent: BTreeMap<u16, [u8; 512]>,
    /// A USB interface (`DmxProtocol::UsbPro`), opened on its port.
    usb: Option<crate::usb_dmx::ProOut>,
}

impl DmxOut {
    /// True when a frame should be sampled for the lights now.
    pub fn frame_due(&mut self, now: f64, s: &DmxSettings) -> bool {
        if !s.enabled || s.fixtures.iter().all(|f| !f.enabled) {
            self.socket = None;
            self.usb = None;
            return false;
        }
        self.pacer.due(now, s.fps.clamp(1.0, 120.0) as u32)
    }

    /// Sends one frame of universes.
    pub fn send(&mut self, s: &DmxSettings, universes: &BTreeMap<u16, [u8; 512]>) {
        if s.protocol == DmxProtocol::UsbPro {
            self.send_usb(s, universes);
            return;
        }
        self.usb = None;
        if self.socket.is_none() {
            match UdpSocket::bind(("0.0.0.0", 0)).and_then(|sock| {
                sock.set_broadcast(true)?;
                sock.set_nonblocking(true)?;
                Ok(sock)
            }) {
                Ok(sock) => {
                    OWN_PORT.store(sock.local_addr().map_or(0, |a| a.port()), std::sync::atomic::Ordering::Relaxed);
                    self.socket = Some(sock)
                }
                Err(e) => {
                    self.error = Some(format!("can't open a network socket ({e})"));
                    return;
                }
            }
            if self.cid == [0; 16] {
                // A per-run source id (sACN receivers tell senders apart with it).
                let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
                self.cid = t.to_le_bytes();
            }
        }
        let Some(sock) = &self.socket else { return };
        let target: Option<Ipv4Addr> = s.target.trim().parse().ok();
        if !s.target.trim().is_empty() && target.is_none() {
            self.error = Some(format!("\"{}\" isn't an IP address", s.target.trim()));
            return;
        }
        self.seq = self.seq.wrapping_add(1).max(1);
        self.error = None;
        for (uni, data) in universes {
            let (packet, addr) = match s.protocol {
                DmxProtocol::ArtNet => (artnet_packet(*uni, self.seq, data), SocketAddr::from((target.unwrap_or(Ipv4Addr::BROADCAST), ARTNET_PORT))),
                DmxProtocol::Sacn => {
                    let uni = (*uni).max(1);
                    let multicast = Ipv4Addr::new(239, 255, (uni >> 8) as u8, uni as u8);
                    (sacn_packet(&self.cid, uni, self.seq, data), SocketAddr::from((target.unwrap_or(multicast), SACN_PORT)))
                }
                DmxProtocol::UsbPro => continue,
            };
            match sock.send_to(&packet, addr) {
                Ok(_) => self.packets += 1,
                // A full send buffer drops this frame's packet; the next frame resends everything.
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => self.error = Some(format!("send to {addr} failed ({e})")),
            }
        }
        self.universes = universes.len();
        if std::mem::take(&mut self.watching) {
            self.sent.clone_from(universes);
        } else if !self.sent.is_empty() {
            self.sent.clear();
        }
    }
}

impl DmxOut {
    /// A USB interface: its one universe (the lowest the fixtures use), to the port in `target`.
    fn send_usb(&mut self, s: &DmxSettings, universes: &BTreeMap<u16, [u8; 512]>) {
        let path = s.target.trim();
        if path.is_empty() {
            self.error = Some("choose the interface's port".into());
            return;
        }
        if self.usb.as_ref().is_none_or(|u| u.path != path) {
            match crate::usb_dmx::ProOut::open(path) {
                Ok(u) => self.usb = Some(u),
                Err(e) => {
                    self.usb = None;
                    self.error = Some(e);
                    return;
                }
            }
        }
        let Some(usb) = &self.usb else { return };
        let empty = [0u8; 512];
        let data = universes.values().next().unwrap_or(&empty);
        if !usb.send(data) {
            self.usb = None;
            self.error = Some("the interface stopped".into());
            return;
        }
        self.packets += 1;
        self.error = usb.error.lock().ok().and_then(|e| e.clone());
        self.universes = universes.len().min(1);
        if std::mem::take(&mut self.watching) {
            self.sent = universes.iter().take(1).map(|(u, d)| (*u, *d)).collect();
        } else if !self.sent.is_empty() {
            self.sent.clear();
        }
    }
}

/// Art-Net ArtDmx: one universe (15-bit port address: net, sub-net, universe).
/// The universe and DMX slots (channel 1 first) in an Art-Net ArtDmx or sACN data packet; None for anything else.
pub fn parse_dmx(p: &[u8]) -> Option<(u16, &[u8])> {
    if p.len() >= 18 && p.starts_with(b"Art-Net\0") && u16::from_le_bytes([p[8], p[9]]) == 0x5000 {
        let universe = u16::from(p[14]) | (u16::from(p[15] & 0x7f) << 8);
        let len = (u16::from_be_bytes([p[16], p[17]]) as usize).min(512).min(p.len() - 18);
        return Some((universe, &p[18..18 + len]));
    }
    if p.len() >= 126 && &p[4..16] == b"ASC-E1.17\0\0\0" && u32::from_be_bytes([p[18], p[19], p[20], p[21]]) == 4 && p[125] == 0 {
        let universe = u16::from_be_bytes([p[113], p[114]]);
        let count = (u16::from_be_bytes([p[123], p[124]]) as usize).saturating_sub(1).min(512).min(p.len() - 126);
        return Some((universe, &p[126..126 + count]));
    }
    None
}

/// The source port Phosphor's own DMX output sends from (its packets aren't controller input).
static OWN_PORT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);

/// DMX in: listens for Art-Net (udp/6454) and sACN (udp/5568, universes 1–16 by multicast) and sends each channel
/// that changes as controller input (`ControlSource::Dmx`, 0..1). Stops when dropped.
pub struct DmxListener {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    status: std::sync::Arc<std::sync::Mutex<String>>,
}

impl DmxListener {
    pub fn start(tx: std::sync::mpsc::Sender<crate::control::Incoming>) -> Self {
        use std::sync::atomic::Ordering;
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let status = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let mut sockets = Vec::new();
        let mut open = Vec::new();
        for (port, name) in [(6454u16, "Art-Net"), (5568, "sACN")] {
            match UdpSocket::bind(("0.0.0.0", port)) {
                Ok(s) => {
                    if port == 5568 {
                        for u in 1..=16u16 {
                            let _ = s.join_multicast_v4(&Ipv4Addr::new(239, 255, (u >> 8) as u8, u as u8), &Ipv4Addr::UNSPECIFIED);
                        }
                    }
                    let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(200)));
                    open.push(format!("{name} on udp/{port}"));
                    sockets.push(s);
                }
                Err(e) => open.push(format!("{name} port {port} busy ({e})")),
            }
        }
        *status.lock().unwrap() = open.join(" · ");
        for sock in sockets {
            let (stop, tx, status) = (stop.clone(), tx.clone(), status.clone());
            let _ = std::thread::Builder::new().name("phosphor-dmx-in".into()).spawn(move || {
                let mut last: BTreeMap<u16, [u8; 512]> = BTreeMap::new();
                let mut buf = [0u8; 700];
                while !stop.load(Ordering::Relaxed) {
                    let Ok((n, from)) = sock.recv_from(&mut buf) else { continue };
                    if from.port() == OWN_PORT.load(Ordering::Relaxed) {
                        continue;
                    }
                    let Some((universe, slots)) = parse_dmx(&buf[..n]) else { continue };
                    let fresh = !last.contains_key(&universe);
                    let prev = last.entry(universe).or_insert([0; 512]);
                    for (i, v) in slots.iter().enumerate() {
                        // The first packet of a universe only sets where things are; changes after that are input.
                        if *v != prev[i] && !fresh {
                            let source = phosphor_engine::ControlSource::Dmx { universe, channel: i as u16 + 1 };
                            if tx.send(crate::control::Incoming { source, value: f32::from(*v) / 255.0 }).is_err() {
                                return;
                            }
                        }
                        prev[i] = *v;
                    }
                    if fresh && let Ok(mut s) = status.lock() {
                        let base = s.split(" · hearing").next().unwrap_or_default().to_string();
                        *s = format!("{base} · hearing {} universe(s)", last.len());
                    }
                }
            });
        }
        Self { stop, status }
    }

    pub fn status(&self) -> String {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

impl Drop for DmxListener {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

pub fn artnet_packet(universe: u16, seq: u8, data: &[u8; 512]) -> Vec<u8> {
    let mut p = Vec::with_capacity(18 + 512);
    p.extend_from_slice(b"Art-Net\0");
    p.extend_from_slice(&0x5000u16.to_le_bytes()); // OpDmx
    p.extend_from_slice(&[0, 14]); // protocol version 14
    p.push(seq);
    p.push(0); // physical port
    p.push((universe & 0xff) as u8); // SubUni
    p.push(((universe >> 8) & 0x7f) as u8); // Net
    p.extend_from_slice(&512u16.to_be_bytes());
    p.extend_from_slice(data);
    p
}

/// sACN (ANSI E1.31) data packet: root, framing and DMP layers, 512 slots.
pub fn sacn_packet(cid: &[u8; 16], universe: u16, seq: u8, data: &[u8; 512]) -> Vec<u8> {
    let flags_len = |len: usize| (0x7000u16 | len as u16).to_be_bytes();
    let mut p = Vec::with_capacity(638);
    // Root layer.
    p.extend_from_slice(&0x0010u16.to_be_bytes());
    p.extend_from_slice(&0x0000u16.to_be_bytes());
    p.extend_from_slice(b"ASC-E1.17\0\0\0");
    p.extend_from_slice(&flags_len(638 - 16));
    p.extend_from_slice(&0x0000_0004u32.to_be_bytes());
    p.extend_from_slice(cid);
    // Framing layer.
    p.extend_from_slice(&flags_len(638 - 38));
    p.extend_from_slice(&0x0000_0002u32.to_be_bytes());
    let mut name = [0u8; 64];
    name[..8].copy_from_slice(b"Phosphor");
    p.extend_from_slice(&name);
    p.push(100); // priority
    p.extend_from_slice(&0u16.to_be_bytes()); // sync address
    p.push(seq);
    p.push(0); // options
    p.extend_from_slice(&universe.to_be_bytes());
    // DMP layer.
    p.extend_from_slice(&flags_len(638 - 115));
    p.push(0x02);
    p.push(0xa1);
    p.extend_from_slice(&0u16.to_be_bytes()); // first address
    p.extend_from_slice(&1u16.to_be_bytes()); // increment
    p.extend_from_slice(&513u16.to_be_bytes()); // start code + 512 slots
    p.push(0); // start code
    p.extend_from_slice(data);
    p
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_back_what_it_sends() {
        let mut data = [0u8; 512];
        data[0] = 255;
        data[511] = 7;
        let art = artnet_packet(0x123, 1, &data);
        let (u, slots) = parse_dmx(&art).unwrap();
        assert_eq!((u, slots.len(), slots[0], slots[511]), (0x123, 512, 255, 7));
        let sacn = sacn_packet(&[9; 16], 3, 1, &data);
        let (u, slots) = parse_dmx(&sacn).unwrap();
        assert_eq!((u, slots.len(), slots[0], slots[511]), (3, 512, 255, 7));
        assert!(parse_dmx(b"Art-Net\0junk").is_none());
    }

    #[test]
    fn a_desk_fader_becomes_controller_input() {
        let (tx, rx) = std::sync::mpsc::channel();
        let listener = DmxListener::start(tx);
        if !listener.status().contains("Art-Net on") {
            return; // port 6454 taken by another app on this machine
        }
        let send = |data: &[u8; 512]| {
            let s = UdpSocket::bind("127.0.0.1:0").unwrap();
            s.send_to(&artnet_packet(2, 1, data), ("127.0.0.1", ARTNET_PORT)).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(80));
        };
        let mut data = [0u8; 512];
        data[4] = 10;
        send(&data);
        assert!(rx.try_recv().is_err(), "the first packet only says where the faders are");
        data[4] = 255;
        send(&data);
        let got = rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
        assert_eq!(got.source, phosphor_engine::ControlSource::Dmx { universe: 2, channel: 5 });
        assert_eq!(got.value, 1.0);
        assert!(listener.status().contains("hearing 1 universe"));
    }

    use super::*;

    #[test]
    fn packets_have_the_right_shape() {
        let mut data = [0u8; 512];
        data[0] = 255;
        let a = artnet_packet(0x0123, 7, &data);
        assert_eq!(a.len(), 530);
        assert_eq!(&a[8..10], &[0x00, 0x50]);
        assert_eq!((a[14], a[15]), (0x23, 0x01));
        assert_eq!(a[18], 255);
        let s = sacn_packet(&[1; 16], 7, 3, &data);
        assert_eq!(s.len(), 638);
        assert_eq!(&s[4..16], b"ASC-E1.17\0\0\0");
        assert_eq!(u16::from_be_bytes([s[113], s[114]]), 7, "universe");
        assert_eq!(s[125], 0, "start code");
        assert_eq!(s[126], 255, "first slot");
    }

    /// Sends to a socket on this Mac and reads it back.
    #[test]
    fn sends_artnet_to_an_address() {
        let rx = UdpSocket::bind("127.0.0.1:0").unwrap();
        rx.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
        let port = rx.local_addr().unwrap().port();
        let tx = UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut data = [0u8; 512];
        data[2] = 42;
        tx.send_to(&artnet_packet(1, 1, &data), ("127.0.0.1", port)).unwrap();
        let mut buf = [0u8; 600];
        let n = rx.recv(&mut buf).unwrap();
        assert_eq!(n, 530);
        assert_eq!(buf[20], 42);
    }
}

#[cfg(test)]
mod monitor_tests {
    #[test]
    fn the_monitor_sees_what_was_sent_only_while_open() {
        let s = phosphor_engine::DmxSettings { enabled: true, target: "127.0.0.1".into(), ..Default::default() };
        let mut out = super::DmxOut::default();
        let mut unis = std::collections::BTreeMap::new();
        let mut data = [0u8; 512];
        data[4] = 200;
        unis.insert(3u16, data);
        out.send(&s, &unis);
        assert!(out.sent.is_empty(), "not watching: no copy");
        out.watching = true;
        out.send(&s, &unis);
        assert_eq!(out.sent.get(&3).map(|d| d[4]), Some(200));
        out.send(&s, &unis);
        assert!(out.sent.is_empty(), "panel closed: copy dropped");
    }
}
