//! Advatek PixLite pixel controllers: find them on the network and read how their outputs are patched, then make one
//! LED strip per physical output (LED / DMX panel › Find PixLites). These boxes have no web page: they're set up
//! over Advatek's own UDP protocol on port 49150. Phosphor only ever asks (OpPoll) and reads the answer
//! (OpPollReply); it never sends a setting, network change or firmware command.

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use phosphor_engine::{DmxProtocol, DmxSettings, Fixture, FixtureShape, PixelOrder};

use crate::net_adapters::Adapter;

pub const PORT: u16 = 49150;
const ID: &[u8; 9] = b"Advatech\0";
const OP_POLL: u16 = 0x0001;
const OP_POLL_REPLY: u16 = 0x0002;
/// The protocol version a poll says it speaks (boxes answer in their own).
const POLL_VERSION: u8 = 8;

/// One output of a box, as patched on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    pub pixels: u16,
    pub universe: u16,
    /// 1-based.
    pub channel: u16,
    /// Advatek's color order number (0 = RGB), when the box says.
    pub order: Option<u8>,
}

impl Output {
    /// The universe its last pixel is in (a pixel never splits: 170 to a universe).
    pub fn last_universe(&self) -> u16 {
        let first = (512 - (self.channel.clamp(1, 512) - 1)) / 3;
        if self.pixels <= first { self.universe } else { self.universe + 1 + (self.pixels - first - 1) / 170 }
    }
}

/// A box that answered, and what it said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PixLite {
    pub ip: Ipv4Addr,
    pub model: String,
    pub firmware: String,
    /// The name set on the box ("" when none or not reported).
    pub nickname: String,
    /// The reply's protocol version (4: first-generation boxes, 5: MkII…).
    pub version: u8,
    /// Its outputs speak sACN (Some(false) = Art-Net; None = not reported).
    pub sacn: Option<bool>,
    /// Its physical outputs, in order.
    pub outputs: Vec<Output>,
}

impl PixLite {
    pub fn pixels(&self) -> usize {
        self.outputs.iter().map(|o| o.pixels as usize).sum()
    }
}

/// The poll: "Advatech\0", OpPoll (big-endian), the protocol version.
pub fn poll_packet() -> [u8; 12] {
    let mut p = [0u8; 12];
    p[..9].copy_from_slice(ID);
    p[9..11].copy_from_slice(&OP_POLL.to_be_bytes());
    p[11] = POLL_VERSION;
    p
}

/// Is this an OpPollReply?
pub fn is_reply(d: &[u8]) -> bool {
    d.len() > 12 && d.starts_with(ID) && u16::from_be_bytes([d[9], d[10]]) == OP_POLL_REPLY
}

/// Reads a box's OpPollReply (`from` = the address it came from: the v5 layout is found by the box's own address in
/// it). Versions 4 and 5 are known; others are refused rather than guessed.
pub fn parse(from: Ipv4Addr, d: &[u8]) -> Result<PixLite, String> {
    if !is_reply(d) {
        return Err("not an Advatek poll reply".into());
    }
    let box_ = match d[11] {
        4 => parse_v4(from, d),
        5 => parse_v5(from, d),
        v => return Err(format!("protocol version {v} isn't known yet")),
    }
    .ok_or_else(|| format!("couldn't read its setup (protocol version {})", d[11]))?;
    let sane = box_.outputs.iter().all(|o| o.pixels <= 2048 && o.universe <= 63_999 && (1..=512).contains(&o.channel));
    if box_.outputs.is_empty() || !sane {
        return Err(format!("its setup didn't read right (protocol version {})", d[11]));
    }
    Ok(box_)
}

/// A null-padded text field.
fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b.split(|c| *c == 0).next().unwrap_or_default()).trim().to_string()
}

fn u16s(d: &[u8], at: usize, n: usize) -> Option<Vec<u16>> {
    (0..n).map(|i| d.get(at + 2 * i..at + 2 * i + 2).map(|b| u16::from_be_bytes([b[0], b[1]]))).collect()
}

/// How many outputs a model really has ("PixLite 16 MkII" reports 32 slots; 16 are wired).
fn physical(model: &str, reported: usize) -> usize {
    let n = model.split_whitespace().nth(1).and_then(|w| w.parse::<usize>().ok());
    n.filter(|n| *n > 0).map_or(reported, |n| n.min(reported))
}

/// Version 5 (MkII boxes): mac[6], model (length-prefixed), hardware revision and two bytes, firmware
/// (length-prefixed), padding, then from the current address: ip, mask, dhcp, static ip, static mask, protocol (0 =
/// sACN), hold last frame, simple config, max pixels per output (u16), outputs n, then per output arrays: pixels u16,
/// start universe u16, start channel u16, null pixels u8, zig-zag u16, reverse u8, color order u8, grouping u16,
/// brightness u8 (all big-endian).
fn parse_v5(from: Ipv4Addr, d: &[u8]) -> Option<PixLite> {
    let model_len = *d.get(18)? as usize;
    let model = text(d.get(19..19 + model_len)?);
    let q = 19 + model_len;
    let fw_len = *d.get(q + 3)? as usize;
    let firmware = text(d.get(q + 4..q + 4 + fw_len)?);
    // The record starts at the box's own address (padding before it varies with the firmware).
    let start = q + 4 + fw_len;
    let ip = from.octets();
    let mut p = start + d.get(start..)?.windows(4).position(|w| w == ip)?;
    p += 4 + 4; // ip, mask
    p += 1 + 4 + 4; // dhcp, static ip, static mask
    let protocol = *d.get(p)?;
    p += 3; // protocol, hold last frame, simple config
    p += 2; // max pixels per output
    let n = *d.get(p)? as usize;
    p += 1;
    let pixels = u16s(d, p, n)?;
    let universes = u16s(d, p + 2 * n, n)?;
    let channels = u16s(d, p + 4 * n, n)?;
    // null u8[n], zig-zag u16[n], reverse u8[n], then color order u8[n].
    let orders = d.get(p + 6 * n + n + 2 * n + n..p + 6 * n + n + 2 * n + 2 * n)?;
    let keep = physical(&model, n);
    let outputs = (0..keep).map(|i| Output { pixels: pixels[i], universe: universes[i], channel: channels[i], order: Some(orders[i]) }).collect();
    Some(PixLite { ip: from, model, firmware, nickname: String::new(), version: 5, sacn: Some(protocol == 0), outputs })
}

/// Version 4 (first-generation PixLite 16): fixed fields. Model (20 bytes) at 12, address at 32, nickname (40) at
/// 38, firmware (20) at 78, mac at 98, then 16 outputs: pixels u16[16] at 106, start universes at 163, start
/// channels at 195 (read from a PixLite 16 Plug & Play, firmware V1.4.11).
fn parse_v4(from: Ipv4Addr, d: &[u8]) -> Option<PixLite> {
    let model = text(d.get(12..32)?);
    let ip = Ipv4Addr::new(*d.get(32)?, d[33], d[34], d[35]);
    let nickname = text(d.get(38..78)?);
    let firmware = text(d.get(78..98)?);
    let n = 16;
    let pixels = u16s(d, 106, n)?;
    let universes = u16s(d, 163, n)?;
    let channels = u16s(d, 195, n)?;
    let keep = physical(&model, n);
    let outputs = (0..keep).map(|i| Output { pixels: pixels[i], universe: universes[i], channel: channels[i], order: None }).collect();
    Some(PixLite { ip: if ip.is_unspecified() { from } else { ip }, model, firmware, nickname, version: 4, sacn: None, outputs })
}

/// What a search found: the boxes, and anything that answered but couldn't be read.
#[derive(Clone, Debug, Default)]
pub struct Found {
    pub boxes: Vec<PixLite>,
    pub problems: Vec<String>,
}

/// Polls out of each adapter (its network's broadcast, and everyone's) and collects answers for `wait`. Blocks:
/// run it on a thread of its own.
pub fn discover(adapters: &[Adapter], wait: Duration) -> Found {
    let poll = poll_packet();
    let mut sockets: Vec<UdpSocket> = Vec::new();
    // Answers come back to port 49150 (and some boxes answer the poll's own port): listen on both.
    if let Ok(s) = listener() {
        sockets.push(s);
    }
    for a in adapters {
        let Ok(s) = UdpSocket::bind((a.ip, 0)) else { continue };
        let _ = s.set_broadcast(true);
        for to in [a.broadcast(), Ipv4Addr::BROADCAST] {
            let _ = s.send_to(&poll, (to, PORT));
        }
        let _ = s.set_nonblocking(true);
        sockets.push(s);
    }
    let mut replies: Vec<(Ipv4Addr, Vec<u8>)> = Vec::new();
    let end = Instant::now() + wait;
    let mut buf = [0u8; 2048];
    while Instant::now() < end {
        let mut any = false;
        for s in &sockets {
            while let Ok((n, SocketAddr::V4(from))) = s.recv_from(&mut buf) {
                any = true;
                if is_reply(&buf[..n]) && !replies.iter().any(|(ip, _)| ip == from.ip()) {
                    replies.push((*from.ip(), buf[..n].to_vec()));
                }
            }
        }
        if !any {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let mut found = Found::default();
    for (ip, d) in replies {
        match parse(ip, &d) {
            Ok(b) => found.boxes.push(b),
            Err(e) => found.problems.push(format!("{ip}: {e}")),
        }
    }
    found.boxes.sort_by_key(|b| b.ip);
    found
}

/// A search running on its own thread, and what the last one found (the LED panel asks while it's drawn).
#[derive(Default)]
pub struct Search {
    running: Option<std::sync::mpsc::Receiver<Found>>,
    pub found: Option<Found>,
}

impl Search {
    /// Searches out of these adapters (about two seconds).
    pub fn start(&mut self, adapters: Vec<Adapter>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = std::thread::Builder::new().name("pixlite-search".into()).spawn(move || {
            let _ = tx.send(discover(&adapters, Duration::from_millis(2000)));
        });
        self.running = Some(rx);
    }

    pub fn busy(&mut self) -> bool {
        if let Some(rx) = &self.running {
            match rx.try_recv() {
                Ok(found) => {
                    self.found = Some(found);
                    self.running = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.running = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        self.running.is_some()
    }
}

/// A socket on 49150 that shares the port (Advatek's own Assistant may be listening too).
fn listener() -> std::io::Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    s.set_reuse_address(true)?;
    #[cfg(unix)]
    s.set_reuse_port(true)?;
    s.set_broadcast(true)?;
    s.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, PORT)).into())?;
    s.set_nonblocking(true)?;
    Ok(s.into())
}

/// A box's name in fixture names: its own nickname, else "Box A", "Box B"… by its place in the list.
pub fn label(b: &PixLite, index: usize) -> String {
    if b.nickname.is_empty() { format!("Box {}", (b'A' + (index % 26) as u8) as char) } else { b.nickname.clone() }
}

fn order_of(n: Option<u8>) -> PixelOrder {
    // Advatek's numbers: 0 RGB, 1 RBG, 2 GRB, 3 GBR, 4 BRG, 5 BGR. Orders Phosphor has no name for stay RGB.
    match n {
        Some(2) => PixelOrder::Grb,
        Some(4) => PixelOrder::Brg,
        _ => PixelOrder::Rgb,
    }
}

/// One LED strip per physical output that has pixels, named "Box A out 1"…, each box's outputs as parallel lines
/// stacked down its own band of the picture (move them to where the strings really are). Fixtures from an earlier
/// import of the same boxes are replaced. Sets sACN or Art-Net when the boxes say which. Returns (strips, pixels).
pub fn import(dmx: &mut DmxSettings, boxes: &[PixLite], mut new_id: impl FnMut() -> u64) -> (usize, usize) {
    let labels: Vec<String> = boxes.iter().enumerate().map(|(i, b)| label(b, i)).collect();
    dmx.fixtures.retain(|f| !labels.iter().any(|l| f.name.starts_with(&format!("{l} out "))));
    if let Some(sacn) = boxes.iter().find_map(|b| b.sacn) {
        dmx.protocol = if sacn { DmxProtocol::Sacn } else { DmxProtocol::ArtNet };
    }
    let (mut strips, mut pixels) = (0, 0);
    let band = 1.0 / boxes.len().max(1) as f32;
    for (k, (b, l)) in boxes.iter().zip(&labels).enumerate() {
        let used: Vec<(usize, &Output)> = b.outputs.iter().enumerate().filter(|(_, o)| o.pixels > 0).collect();
        let rows = b.outputs.len().max(1) as f32;
        for (i, o) in used {
            let y = band * (k as f32 + (i as f32 + 0.5) / rows);
            dmx.fixtures.push(Fixture {
                id: new_id(),
                name: format!("{l} out {}", i + 1),
                shape: FixtureShape::Strip,
                points: [[0.05, y], [0.95, y], [0.95, y], [0.05, y]],
                cols: o.pixels as u32,
                order: order_of(o.order),
                universe: o.universe,
                channel: o.channel,
                ..Default::default()
            });
            strips += 1;
            pixels += o.pixels as usize;
        }
    }
    (strips, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &[u8] = include_bytes!("testdata/mk2_v5_a.bin");
    const B: &[u8] = include_bytes!("testdata/mk1_v4_b.bin");
    const C: &[u8] = include_bytes!("testdata/mk2_v5_c.bin");

    fn ip(last: u8) -> Ipv4Addr {
        Ipv4Addr::new(192, 168, 0, last)
    }

    #[test]
    fn the_poll_is_twelve_bytes() {
        assert_eq!(&poll_packet(), b"Advatech\0\x00\x01\x08");
    }

    #[test]
    fn a_mk2_reply_reads() {
        let a = parse(ip(52), A).unwrap();
        assert_eq!((a.model.as_str(), a.firmware.as_str(), a.version, a.sacn), ("PixLite 16 MkII", "V2.0.12", 5, Some(true)));
        assert_eq!(a.outputs.len(), 16, "32 slots reported, 16 wired");
        assert!(a.outputs.iter().all(|o| o.pixels == 50 && o.order == Some(0)));
        assert_eq!((a.outputs[0].universe, a.outputs[0].channel), (1, 1));
        assert_eq!((a.outputs[3].universe, a.outputs[3].channel), (1, 451), "a pixel never splits: 20 px left in U1");
        assert_eq!((a.outputs[4].universe, a.outputs[4].channel), (2, 91));
        assert_eq!((a.outputs[15].universe, a.outputs[15].channel), (5, 211));
        assert_eq!((a.outputs[3].last_universe(), a.outputs[15].last_universe()), (2, 5));
        // The box's address is what finds the record: from anywhere else it doesn't read.
        assert!(parse(ip(99), A).is_err());
    }

    #[test]
    fn a_first_generation_reply_reads() {
        let b = parse(ip(66), B).unwrap();
        assert_eq!((b.model.as_str(), b.nickname.as_str(), b.firmware.as_str(), b.version), ("PixLite 16 PP", "Stage", "V1.4.11", 4));
        let starts: Vec<(u16, u16)> = b.outputs.iter().map(|o| (o.universe, o.channel)).collect();
        assert_eq!(
            starts,
            [(6, 1), (6, 151), (6, 301), (6, 451), (7, 91), (7, 241), (7, 391), (8, 31), (8, 181), (8, 331), (8, 481), (9, 121), (9, 271), (9, 421), (10, 61), (10, 211)]
        );
        assert_eq!(b.pixels(), 800);
    }

    #[test]
    fn the_rig_imports_as_48_strips() {
        let boxes = vec![parse(ip(52), A).unwrap(), parse(ip(55), C).unwrap(), parse(ip(66), B).unwrap()];
        let mut dmx = DmxSettings::default();
        let mut id = 0;
        let (strips, pixels) = import(&mut dmx, &boxes, || {
            id += 1;
            id
        });
        assert_eq!((strips, pixels), (48, 3200));
        assert_eq!(dmx.protocol, DmxProtocol::Sacn);
        assert_eq!(dmx.fixtures[0].name, "Box A out 1");
        assert_eq!(dmx.fixtures[47].name, "Stage out 16");
        // Every pixel lands where the boxes say: universes 1 to 19, box C's last pixel at U19 ch 210.
        let colors = vec![[255u8, 255, 255, 255]; 3200];
        let unis = dmx.universes(&colors, 0.0);
        assert_eq!(unis.keys().copied().min(), Some(1));
        assert_eq!(unis.keys().copied().max(), Some(19));
        assert_eq!(unis[&19][209], 255);
        assert_eq!(unis[&19][210], 0);
        // Importing again replaces, never doubles.
        let again = import(&mut dmx, &boxes, || 1000);
        assert_eq!((again, dmx.fixtures.len()), ((48, 3200), 48));
    }
}
