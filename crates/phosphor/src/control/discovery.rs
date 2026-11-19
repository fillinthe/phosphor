//! Other Phosphors on the network: each one advertises its OSC input over Bonjour as "Phosphor on <computer>"
//! (tagged `app=phosphor`); `players()` lists the ones heard, kept fresh by a background browser. The groundwork
//! for playing to several computers at once; today it shows where to send OSC, and which Sessions can be sent a
//! show (their Session advertises itself the same way while it runs).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Another Phosphor heard on the network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Player {
    pub name: String,
    /// Its IPv4 address and OSC port (0 = its OSC wasn't heard, only its Session).
    pub address: String,
    pub port: u16,
    pub version: String,
    /// Its Session's port while its Session runs (shows can be sent to it then).
    pub session: Option<u16>,
}

/// How a Phosphor's Session (`net::NetSession`) advertises itself, named like its OSC ("Phosphor on Studio Mac").
pub const SESSION_SERVICE: &str = "_http._tcp.local.";
const OSC_SERVICE: &str = "_osc._udp.local.";

/// This computer's name for people ("Studio Mac") and for Bonjour hosts ("Studio-Mac").
fn computer_names() -> &'static (String, String) {
    static N: OnceLock<(String, String)> = OnceLock::new();
    N.get_or_init(|| {
        let ask = |args: &[&str]| {
            crate::os::command(args[0]).args(&args[1..]).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|s| !s.is_empty())
        };
        let pretty = if cfg!(target_os = "macos") { ask(&["scutil", "--get", "ComputerName"]) } else { std::env::var("COMPUTERNAME").ok().or_else(|| ask(&["hostname"])) };
        let pretty = pretty.unwrap_or_else(|| "this computer".into());
        let host = if cfg!(target_os = "macos") { ask(&["scutil", "--get", "LocalHostName"]) } else { None };
        let host = host.unwrap_or_else(|| pretty.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect());
        (pretty, host)
    })
}

/// How this Phosphor shows up to others: "Phosphor on Studio Mac" (an exported show or a separate profile says
/// so, so two on one computer don't clash), and its Bonjour host "Studio-Mac.local.".
pub fn identity() -> (String, String) {
    let (pretty, host) = computer_names();
    let app = crate::show::storage_name();
    (format!("{app} on {pretty}"), format!("{host}.local."))
}

/// TXT properties every Phosphor advertisement carries.
pub fn properties() -> std::collections::HashMap<String, String> {
    [("app".to_string(), "phosphor".to_string()), ("version".to_string(), env!("CARGO_PKG_VERSION").to_string())].into_iter().collect()
}

/// Heard on one Bonjour service: full name → (name, IPv4 address, port, version).
type Heard = Arc<Mutex<BTreeMap<String, (String, String, u16, String)>>>;

/// Listens for Phosphors on `service` (a thread of its own) into `out`, leaving this one out.
fn listen(service: &'static str, out: Heard) {
    let _ = std::thread::Builder::new().name("phosphor-discovery".into()).spawn(move || {
        let Ok(daemon) = mdns_sd::ServiceDaemon::new() else { return };
        let Ok(events) = daemon.browse(service) else { return };
        let suffix = format!(".{service}");
        let me = format!("{}{suffix}", identity().0);
        while let Ok(event) = events.recv() {
            match event {
                mdns_sd::ServiceEvent::ServiceResolved(s) => {
                    if s.get_property_val_str("app") != Some("phosphor") || s.get_fullname() == me {
                        continue;
                    }
                    let Some(ip) = s.get_addresses_v4().into_iter().next() else { continue };
                    let name = s.get_fullname().trim_end_matches(suffix.as_str()).to_string();
                    let version = s.get_property_val_str("version").unwrap_or("").to_string();
                    out.lock().unwrap().insert(s.get_fullname().to_string(), (name, ip.to_string(), s.get_port(), version));
                }
                mdns_sd::ServiceEvent::ServiceRemoved(_, fullname) => {
                    out.lock().unwrap().remove(&fullname);
                }
                _ => {}
            }
        }
    });
}

/// Other Phosphors heard so far (starts listening on the first call): their OSC, and their Session when it runs.
pub fn players() -> Vec<Player> {
    static LISTS: OnceLock<(Heard, Heard)> = OnceLock::new();
    let (osc, sessions) = LISTS.get_or_init(|| {
        let (osc, sessions): (Heard, Heard) = Default::default();
        listen(OSC_SERVICE, osc.clone());
        listen(SESSION_SERVICE, sessions.clone());
        (osc, sessions)
    });
    let sessions: Vec<_> = sessions.lock().unwrap().values().cloned().collect();
    let mut list: Vec<Player> = osc
        .lock()
        .unwrap()
        .values()
        .map(|(name, address, port, version)| Player { name: name.clone(), address: address.clone(), port: *port, version: version.clone(), session: None })
        .collect();
    for (name, address, port, version) in sessions {
        match list.iter_mut().find(|p| p.name == name) {
            Some(p) => p.session = Some(port),
            None => list.push(Player { name, address, port: 0, version, session: Some(port) }),
        }
    }
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list
}

#[cfg(test)]
mod tests {
    #[test]
    fn identity_names_this_computer() {
        let (name, host) = super::identity();
        assert!(name.starts_with("Phosphor") && name.contains(" on "), "{name}");
        assert!(host.ends_with(".local.") && !host.contains(' '), "{host}");
    }
}
