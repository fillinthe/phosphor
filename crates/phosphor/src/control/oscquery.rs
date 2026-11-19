//! OSC Query: a small HTTP server describing every OSC address Phosphor answers to (type, range, current
//! value, name), advertised over Zeroconf as `_oscjson._tcp`, so TouchOSC, Chataigne, Vezér and friends
//! can browse and bind Phosphor's controls without typing addresses.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use phosphor_engine::Composition;
use serde_json::{Map, Value, json};

/// One address for the namespace: (OSC address, description, current value 0..1 or None for triggers).
pub type Entry = (String, String, Option<f32>);

/// The namespace tree (OSC Query JSON) for a list of addresses.
pub fn namespace(entries: &[Entry]) -> Value {
    let mut root = Map::new();
    for (addr, desc, value) in entries {
        let parts: Vec<&str> = addr.trim_start_matches('/').split('/').filter(|p| !p.is_empty()).collect();
        let mut node = &mut root;
        let mut path = String::new();
        for (i, part) in parts.iter().enumerate() {
            path.push('/');
            path.push_str(part);
            let child = node.entry(part.to_string()).or_insert_with(|| json!({ "FULL_PATH": path.clone() }));
            let obj = child.as_object_mut().expect("namespace nodes are objects");
            if i + 1 == parts.len() {
                obj.insert("TYPE".into(), "f".into());
                obj.insert("DESCRIPTION".into(), desc.clone().into());
                obj.insert("RANGE".into(), json!([{ "MIN": 0.0, "MAX": 1.0 }]));
                match value {
                    // Readable and writable, with its value.
                    Some(v) => {
                        obj.insert("ACCESS".into(), 3.into());
                        obj.insert("VALUE".into(), json!([v]));
                    }
                    // Triggers: write only.
                    None => {
                        obj.insert("ACCESS".into(), 2.into());
                    }
                }
            }
            if i + 1 == parts.len() {
                break;
            }
            node = obj.entry("CONTENTS").or_insert_with(|| Value::Object(Map::new())).as_object_mut().expect("contents are objects");
        }
    }
    json!({ "FULL_PATH": "/", "CONTENTS": root })
}

/// The subtree at `path` ("/phosphor/layer" …), or None.
fn subtree<'a>(tree: &'a Value, path: &str) -> Option<&'a Value> {
    let mut node = tree;
    for part in path.split('/').filter(|p| !p.is_empty()) {
        node = node.get("CONTENTS")?.get(part)?;
    }
    Some(node)
}

/// Every control in the project as namespace entries.
pub fn entries(comp: &Composition) -> Vec<Entry> {
    super::all_targets(comp)
        .into_iter()
        .map(|(t, name)| (super::default_osc_address(&t), name, super::get_target(comp, &t)))
        .collect()
}

/// Requests answered at once; more are closed straight away.
const MAX_CLIENTS: usize = 8;
/// The longest request line read (a path is short; this keeps a client from sending an endless one).
const MAX_LINE: u64 = 8192;

/// Answers one HTTP request: the host info or the namespace subtree asked for.
fn answer(mut stream: std::net::TcpStream, tree: &Mutex<Value>, osc_port: u16) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    // A client that stops reading lets the thread go.
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let mut line = String::new();
    if BufReader::new((&stream).take(MAX_LINE)).read_line(&mut line).is_err() {
        return;
    }
    // "GET /path?QUERY HTTP/1.1"
    let target = line.split_whitespace().nth(1).unwrap_or("/");
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let (status, body) = if query.starts_with("HOST_INFO") {
        let info = json!({
            "NAME": "Phosphor",
            "OSC_PORT": osc_port,
            "OSC_TRANSPORT": "UDP",
            "EXTENSIONS": { "ACCESS": true, "VALUE": true, "RANGE": true, "DESCRIPTION": true, "TYPE": true },
        });
        ("200 OK", info.to_string())
    } else {
        // Written out under the lock (no copy of the whole tree).
        let found = tree.lock().ok().and_then(|t| subtree(&t, path).map(Value::to_string));
        match found {
            Some(text) => ("200 OK", text),
            None => ("404 Not Found", "{}".into()),
        }
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

/// The running server: serves whatever namespace was last published.
pub struct Server {
    pub port: u16,
    tree: Arc<Mutex<Value>>,
    stop: Arc<AtomicBool>,
    _mdns: Option<mdns_sd::ServiceDaemon>,
}

impl Server {
    pub fn start(osc_port: u16, ip: Option<&str>) -> Result<Self, String> {
        let listener = TcpListener::bind(("0.0.0.0", 0)).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let tree = Arc::new(Mutex::new(namespace(&[])));
        let stop = Arc::new(AtomicBool::new(false));
        let (tree2, stop2) = (tree.clone(), stop.clone());
        let busy = Arc::new(AtomicUsize::new(0));
        std::thread::Builder::new()
            .name("phosphor-oscquery".into())
            .spawn(move || {
                while !stop2.load(Ordering::Relaxed) {
                    let Ok((stream, _)) = listener.accept() else {
                        std::thread::sleep(Duration::from_millis(50));
                        continue;
                    };
                    // A thread per request (a slow or silent client used to hold up everyone behind it), a few at most.
                    if busy.load(Ordering::Relaxed) >= MAX_CLIENTS {
                        continue;
                    }
                    busy.fetch_add(1, Ordering::Relaxed);
                    let (tree, done) = (tree2.clone(), busy.clone());
                    let spawned = std::thread::Builder::new().name("phosphor-oscquery-client".into()).spawn(move || {
                        answer(stream, &tree, osc_port);
                        done.fetch_sub(1, Ordering::Relaxed);
                    });
                    if spawned.is_err() {
                        busy.fetch_sub(1, Ordering::Relaxed);
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let mdns = ip.and_then(|ip| {
            let daemon = mdns_sd::ServiceDaemon::new().ok()?;
            let (name, host) = super::discovery::identity();
            let info = mdns_sd::ServiceInfo::new("_oscjson._tcp.local.", &name, &host, ip, port, super::discovery::properties()).ok()?;
            daemon.register(info).ok()?;
            Some(daemon)
        });
        Ok(Self { port, tree, stop, _mdns: mdns })
    }

    pub fn publish(&self, entries: &[Entry]) {
        if let Ok(mut t) = self.tree.lock() {
            *t = namespace(entries);
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_the_namespace_over_http() {
        let server = Server::start(9000, None).unwrap();
        server.publish(&[("/phosphor/master".into(), "Master fade".into(), Some(0.5)), ("/phosphor/column/1".into(), "Column 1".into(), None)]);
        let get = |path: &str| {
            let mut s = std::net::TcpStream::connect(("127.0.0.1", server.port)).unwrap();
            write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
            let mut out = String::new();
            std::io::Read::read_to_string(&mut s, &mut out).unwrap();
            let body = out.split("\r\n\r\n").nth(1).unwrap().to_string();
            serde_json::from_str::<Value>(&body).unwrap()
        };
        let root = get("/");
        assert_eq!(root["CONTENTS"]["phosphor"]["CONTENTS"]["master"]["VALUE"][0], 0.5);
        assert_eq!(root["CONTENTS"]["phosphor"]["CONTENTS"]["column"]["CONTENTS"]["1"]["ACCESS"], 2);
        assert_eq!(get("/phosphor/master")["DESCRIPTION"], "Master fade");
        assert_eq!(get("/?HOST_INFO")["OSC_PORT"], 9000);
    }

    /// A client that connects and says nothing (or sends an endless line) doesn't hold up the next one: it used
    /// to wait for the silent one's 2 s timeout.
    #[test]
    fn a_silent_client_does_not_hold_up_others() {
        let server = Server::start(9000, None).unwrap();
        server.publish(&[("/phosphor/master".into(), "Master fade".into(), Some(0.5))]);
        let _silent = std::net::TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        let mut endless = std::net::TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        let _ = endless.write_all(&[b'x'; 64 * 1024]);
        std::thread::sleep(Duration::from_millis(100));
        let started = std::time::Instant::now();
        let mut s = std::net::TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        write!(s, "GET /phosphor/master HTTP/1.1\r\n\r\n").unwrap();
        let mut out = String::new();
        std::io::Read::read_to_string(&mut s, &mut out).unwrap();
        assert!(out.contains("Master fade"), "{out}");
        assert!(started.elapsed() < Duration::from_millis(1500), "answered in {:?}", started.elapsed());
    }
}
