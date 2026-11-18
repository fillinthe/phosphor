//! Network helpers. `HostCache`: host names → addresses, looked up on a thread of their own and kept a while, for
//! anything that sends often to a name ("studio-mac.local"): a lookup can take seconds (a `.local` name nobody
//! answers), and must never happen on the UI thread or once per message. `bind_retrying`: open a port that a
//! listener being closed still holds for a moment.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long a found address is used before it's looked up again (a laptop's address can change).
const KEEP: Duration = Duration::from_secs(60);
/// How long a name that wasn't found waits before the next try.
const RETRY: Duration = Duration::from_secs(10);

struct Entry {
    addr: Option<IpAddr>,
    /// When the last lookup finished (None = never).
    at: Option<Instant>,
    looking: bool,
}

#[derive(Default, Clone)]
pub struct HostCache {
    names: Arc<Mutex<HashMap<String, Entry>>>,
}

impl HostCache {
    /// Where to send to `host`: an IP address straight away; a name from the last lookup (None until the first
    /// one finishes, or when it isn't found). Starts a lookup in the background when one is due. Never waits.
    pub fn addr(&self, host: &str, port: u16) -> Option<SocketAddr> {
        let host = host.trim();
        if let Ok(ip) = host.parse::<IpAddr>() {
            return Some(SocketAddr::new(ip, port));
        }
        if host.is_empty() {
            return None;
        }
        let mut names = self.names.lock().unwrap();
        let now = Instant::now();
        let e = names.entry(host.to_string()).or_insert(Entry { addr: None, at: None, looking: false });
        let due = e.at.is_none_or(|at| now.duration_since(at) >= if e.addr.is_some() { KEEP } else { RETRY });
        if due && !e.looking {
            e.looking = true;
            let (names, name) = (self.names.clone(), host.to_string());
            let started = std::thread::Builder::new().name("host-lookup".into()).spawn(move || {
                // (IPv4 first: the sending socket is IPv4.)
                let all: Vec<SocketAddr> = (name.as_str(), 0).to_socket_addrs().map(Iterator::collect).unwrap_or_default();
                let found = all.iter().find(|a| a.is_ipv4()).or(all.first()).map(|a| a.ip());
                if let Some(e) = names.lock().unwrap().get_mut(&name) {
                    // A name that stops answering keeps its last address until it's found again.
                    e.addr = found.or(e.addr);
                    e.at = Some(Instant::now());
                    e.looking = false;
                }
            });
            if started.is_err() {
                e.looking = false;
            }
        }
        e.addr.map(|ip| SocketAddr::new(ip, port))
    }
}

/// Opens a port with `bind`, trying again for a short while when it's still in use: a listener that was just told
/// to stop lets go of its port at its next wake-up (restarting the Session, reconnecting OSC).
pub fn bind_retrying<T>(mut bind: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let until = Instant::now() + Duration::from_millis(600);
    loop {
        match bind() {
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse && Instant::now() < until => std::thread::sleep(Duration::from_millis(25)),
            other => return other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A port let go of a moment later is opened once it's free; one that stays taken still fails.
    #[test]
    fn a_port_being_let_go_opens() {
        let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = held.local_addr().unwrap().port();
        assert!(bind_retrying(|| std::net::UdpSocket::bind(("127.0.0.1", port))).is_err(), "still taken");
        let freed = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            drop(held);
        });
        assert!(bind_retrying(|| std::net::UdpSocket::bind(("127.0.0.1", port))).is_ok());
        freed.join().unwrap();
    }

    /// An IP address is used as it is; a name is looked up in the background and then kept.
    #[test]
    fn names_are_looked_up_in_the_background() {
        let hosts = HostCache::default();
        assert_eq!(hosts.addr("192.168.1.20", 9000), Some("192.168.1.20:9000".parse().unwrap()));
        assert_eq!(hosts.addr("  ", 9000), None);
        let started = Instant::now();
        let first = hosts.addr("localhost", 9000);
        assert!(started.elapsed() < Duration::from_millis(50), "never waits for the lookup");
        let mut found = first;
        for _ in 0..200 {
            if found.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
            found = hosts.addr("localhost", 9000);
        }
        let found = found.expect("localhost is found");
        assert!(found.ip().is_loopback() && found.port() == 9000, "{found}");
        assert_eq!(hosts.addr("localhost", 7000).map(|a| a.port()), Some(7000), "kept, any port");
    }
}
