//! Reaching this Phosphor's Session from the internet, with no server of ours in between: the router is asked (UPnP)
//! to send the Session's ports to this computer, and says its own public address. Another Phosphor then follows it,
//! or takes its live picture as a layer, by that address. The opening is leased for an hour and renewed while it's
//! on, and closed again when it's switched off or Phosphor quits (a router that misses that closes it within the hour).

use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::mpsc;
use std::time::Duration;

/// How long the router keeps a port open without hearing from us again (seconds), and how often we renew it.
const LEASE: u32 = 3600;
const RENEW: Duration = Duration::from_secs(1500);

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Asking,
    /// Open: the public address and the ports the router now sends here.
    Open { public: IpAddr, ports: Vec<u16> },
    /// The router said no, or there's no router that does UPnP: forward the ports by hand.
    Failed(String),
}

/// The router's port opening while it's on (drop it to close the ports again).
pub struct Opening {
    rx: mpsc::Receiver<Status>,
    stop: mpsc::Sender<()>,
    pub status: Status,
}

impl Opening {
    /// Asks the router for `ports` (TCP, the same number outside as here) on a thread of its own.
    pub fn open(ports: Vec<u16>) -> Self {
        let (tx, rx) = mpsc::channel();
        let (stop, stopped) = mpsc::channel();
        let _ = std::thread::Builder::new().name("router".into()).spawn(move || run(&ports, &tx, &stopped));
        Self { rx, stop, status: Status::Asking }
    }

    /// Once a frame while shown: takes the newest news from the router thread.
    pub fn poll(&mut self) {
        if let Some(s) = self.rx.try_iter().last() {
            self.status = s;
        }
    }

    /// The address others use (public address and the Session's port), once open.
    pub fn address(&self, port: u16) -> Option<String> {
        match &self.status {
            Status::Open { public, .. } => Some(format!("{public}:{port}")),
            _ => None,
        }
    }
}

impl Drop for Opening {
    fn drop(&mut self) {
        let _ = self.stop.send(());
    }
}

fn run(ports: &[u16], tx: &mpsc::Sender<Status>, stopped: &mpsc::Receiver<()>) {
    let _ = tx.send(Status::Asking);
    let mut opts = igd_next::SearchOptions::default();
    opts.timeout = Some(Duration::from_secs(4));
    let gateway = match igd_next::search_gateway(opts) {
        Ok(g) => g,
        Err(e) => {
            let _ = tx.send(Status::Failed(format!("no router answered ({e})")));
            return;
        }
    };
    // This computer's address on the router's network (the one the router reaches us on).
    let Some(local) = local_toward(gateway.addr) else {
        let _ = tx.send(Status::Failed("this computer has no address on the router's network".into()));
        return;
    };
    let ask = || -> Result<IpAddr, String> {
        for p in ports {
            gateway
                .add_port(igd_next::PortMappingProtocol::TCP, *p, SocketAddr::V4(SocketAddrV4::new(local, *p)), LEASE, "Phosphor session")
                .map_err(|e| format!("the router wouldn't open port {p} ({e})"))?;
        }
        gateway.get_external_ip().map_err(|e| format!("the router didn't say its public address ({e})"))
    };
    loop {
        let status = match ask() {
            Ok(public) => Status::Open { public, ports: ports.to_vec() },
            Err(e) => Status::Failed(e),
        };
        let failed = matches!(status, Status::Failed(_));
        let _ = tx.send(status);
        if failed {
            break;
        }
        // Renew until told to stop (or the app goes: the sender drops).
        match stopped.recv_timeout(RENEW) {
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            _ => break,
        }
    }
    for p in ports {
        let _ = gateway.remove_port(igd_next::PortMappingProtocol::TCP, *p);
    }
}

/// Our IPv4 address on the same network as `router` (any IPv4 address when none matches its /24).
fn local_toward(router: SocketAddr) -> Option<Ipv4Addr> {
    let all = crate::net_adapters::ipv4();
    let SocketAddr::V4(r) = router else { return all.first().map(|a| a.ip) };
    all.iter().find(|a| a.reaches(*r.ip())).or(all.first()).map(|a| a.ip)
}

/// Is `ip` a public internet address (not this network's, not a carrier-grade NAT's)? A router whose own outside
/// address isn't public sits behind another one: the opening won't reach us from outside.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_private() || v4.is_loopback() || v4.is_link_local() || v4.is_unspecified() || (o[0] == 100 && (64..128).contains(&o[1])))
        }
        IpAddr::V6(v6) => !(v6.is_loopback() || v6.is_unspecified()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carrier_nat_and_home_addresses_are_not_public() {
        for ip in ["192.168.1.1", "10.0.0.5", "172.20.10.1", "100.72.3.4", "127.0.0.1"] {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
        assert!(is_public("203.0.113.9".parse().unwrap()));
    }
}
