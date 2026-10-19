//! Other Phosphors on the network: each one advertises its OSC input over Bonjour as "Phosphor on <computer>"
//! (tagged `app=phosphor`); `players()` lists the ones heard, kept fresh by a background browser. The groundwork
//! for playing to several computers at once; today it shows where to send OSC.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Another Phosphor heard on the network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Player {
    pub name: String,
    /// Its IPv4 address and OSC port.
    pub address: String,
    pub port: u16,
    pub version: String,
}

/// This computer's name for people ("Studio Mac") and for Bonjour hosts ("Studio-Mac").
fn computer_names() -> &'static (String, String) {
    static N: OnceLock<(String, String)> = OnceLock::new();
    N.get_or_init(|| {
        let ask = |args: &[&str]| {
            std::process::Command::new(args[0]).args(&args[1..]).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|s| !s.is_empty())
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

/// Other Phosphors heard so far (starts listening on the first call).
pub fn players() -> Vec<Player> {
    static LIST: OnceLock<Arc<Mutex<BTreeMap<String, Player>>>> = OnceLock::new();
    let list = LIST.get_or_init(|| {
        let list: Arc<Mutex<BTreeMap<String, Player>>> = Default::default();
        let out = list.clone();
        let _ = std::thread::Builder::new().name("phosphor-discovery".into()).spawn(move || {
            let Ok(daemon) = mdns_sd::ServiceDaemon::new() else { return };
            let Ok(events) = daemon.browse("_osc._udp.local.") else { return };
            let me = format!("{}._osc._udp.local.", identity().0);
            while let Ok(event) = events.recv() {
                match event {
                    mdns_sd::ServiceEvent::ServiceResolved(s) => {
                        if s.get_property_val_str("app") != Some("phosphor") || s.get_fullname() == me {
                            continue;
                        }
                        let Some(ip) = s.get_addresses_v4().into_iter().next() else { continue };
                        let name = s.get_fullname().trim_end_matches("._osc._udp.local.").to_string();
                        let version = s.get_property_val_str("version").unwrap_or("").to_string();
                        out.lock().unwrap().insert(s.get_fullname().to_string(), Player { name, address: ip.to_string(), port: s.get_port(), version });
                    }
                    mdns_sd::ServiceEvent::ServiceRemoved(_, fullname) => {
                        out.lock().unwrap().remove(&fullname);
                    }
                    _ => {}
                }
            }
        });
        list
    });
    list.lock().unwrap().values().cloned().collect()
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
