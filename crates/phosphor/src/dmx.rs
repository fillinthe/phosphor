//! Sends LED / DMX universes over the network: Art-Net (ArtDmx, UDP 6454; broadcast when no address is
//! set) or sACN / E1.31 (UDP 5568; multicast per universe when no address is set). Does nothing while
//! LED output is off; paced to its own frame rate.

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
}

impl DmxOut {
    /// True when a frame should be sampled for the lights now.
    pub fn frame_due(&mut self, now: f64, s: &DmxSettings) -> bool {
        if !s.enabled || s.fixtures.iter().all(|f| !f.enabled) {
            self.socket = None;
            return false;
        }
        self.pacer.due(now, s.fps.clamp(1.0, 120.0) as u32)
    }

    /// Sends one frame of universes.
    pub fn send(&mut self, s: &DmxSettings, universes: &BTreeMap<u16, [u8; 512]>) {
        if self.socket.is_none() {
            match UdpSocket::bind(("0.0.0.0", 0)).and_then(|sock| {
                sock.set_broadcast(true)?;
                sock.set_nonblocking(true)?;
                Ok(sock)
            }) {
                Ok(sock) => self.socket = Some(sock),
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
            };
            match sock.send_to(&packet, addr) {
                Ok(_) => self.packets += 1,
                // A full send buffer drops this frame's packet; the next frame resends everything.
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => self.error = Some(format!("send to {addr} failed ({e})")),
            }
        }
        self.universes = universes.len();
    }
}

/// Art-Net ArtDmx: one universe (15-bit port address: net, sub-net, universe).
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
