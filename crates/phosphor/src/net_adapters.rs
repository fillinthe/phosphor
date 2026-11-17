//! This computer's IPv4 network adapters (Wi-Fi, Ethernet, a USB Ethernet with an extra address…): for sending LED /
//! DMX out a chosen one (a rig on its own network beside the venue's Wi-Fi) and for listening on all of them.

use std::net::Ipv4Addr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// One IPv4 address on one adapter (an adapter with two addresses is listed twice).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Adapter {
    /// The system's name for it ("en8", "Ethernet 2").
    pub name: String,
    pub ip: Ipv4Addr,
    pub mask: Ipv4Addr,
}

impl Adapter {
    /// Its network's broadcast address (192.168.0.255 for 192.168.0.200/24).
    pub fn broadcast(&self) -> Ipv4Addr {
        directed_broadcast(self.ip, self.mask)
    }

    /// Is `other` on this adapter's network?
    pub fn reaches(&self, other: Ipv4Addr) -> bool {
        same_network(self.ip, self.mask, other)
    }

    /// "en8 · 192.168.0.200" for menus.
    pub fn label(&self) -> String {
        format!("{} \u{b7} {}", self.name, self.ip)
    }
}

pub fn directed_broadcast(ip: Ipv4Addr, mask: Ipv4Addr) -> Ipv4Addr {
    Ipv4Addr::from(u32::from(ip) | !u32::from(mask))
}

pub fn same_network(ip: Ipv4Addr, mask: Ipv4Addr, other: Ipv4Addr) -> bool {
    u32::from(ip) & u32::from(mask) == u32::from(other) & u32::from(mask)
}

/// Every IPv4 address on this computer except loopback, by adapter name. Asks the system: call `cached` from UI.
pub fn ipv4() -> Vec<Adapter> {
    let mut out: Vec<Adapter> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.addr {
            if_addrs::IfAddr::V4(v4) => Some(Adapter { name: i.name, ip: v4.ip, mask: v4.netmask }),
            _ => None,
        })
        .filter(|a| !a.ip.is_link_local())
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name).then(a.ip.cmp(&b.ip)));
    out.dedup();
    out
}

/// `ipv4()`, asked again at most every few seconds (a menu draws every frame; cables get plugged in).
pub fn cached() -> Vec<Adapter> {
    static CACHE: Mutex<Option<(Instant, Vec<Adapter>)>> = Mutex::new(None);
    let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if c.as_ref().is_none_or(|(at, _)| at.elapsed() > Duration::from_secs(3)) {
        *c = Some((Instant::now(), ipv4()));
    }
    c.as_ref().map(|(_, a)| a.clone()).unwrap_or_default()
}

/// The adapter holding `ip` right now, if any.
pub fn with_ip(ip: Ipv4Addr) -> Option<Adapter> {
    ipv4().into_iter().find(|a| a.ip == ip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn networks_and_broadcasts() {
        let a = Adapter { name: "en8".into(), ip: Ipv4Addr::new(192, 168, 0, 200), mask: Ipv4Addr::new(255, 255, 255, 0) };
        assert_eq!(a.broadcast(), Ipv4Addr::new(192, 168, 0, 255));
        assert!(a.reaches(Ipv4Addr::new(192, 168, 0, 52)));
        assert!(!a.reaches(Ipv4Addr::new(192, 168, 1, 52)));
        assert_eq!(directed_broadcast(Ipv4Addr::new(2, 0, 0, 10), Ipv4Addr::new(255, 0, 0, 0)), Ipv4Addr::new(2, 255, 255, 255));
        assert_eq!(a.label(), "en8 \u{b7} 192.168.0.200");
        // (Whatever this computer has: never loopback.)
        assert!(ipv4().iter().all(|a| !a.ip.is_loopback()));
    }
}
