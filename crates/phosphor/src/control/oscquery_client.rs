//! OSC Query client: read another app's list of OSC addresses (its HTTP JSON namespace) so its settings can be
//! sent to from any Phosphor control (`SendTo::OscTo`). The other half of `oscquery` (Phosphor's own server).

use std::sync::{Arc, Mutex};

use serde_json::Value;

/// One setting another app takes: its address, what it's called, and its range.
#[derive(Clone, Debug, PartialEq)]
pub struct QueryParam {
    pub address: String,
    pub description: String,
    pub min: f32,
    pub max: f32,
}

/// Every writable number / on-off setting in an OSC Query namespace, in address order.
pub fn parse(json: &str) -> Result<Vec<QueryParam>, String> {
    let root: Value = serde_json::from_str(json).map_err(|e| format!("not OSC Query JSON ({e})"))?;
    let mut out = Vec::new();
    walk(&root, &mut out);
    out.sort_by(|a, b| a.address.cmp(&b.address));
    Ok(out)
}

fn walk(node: &Value, out: &mut Vec<QueryParam>) {
    if let (Some(path), Some(kind)) = (node.get("FULL_PATH").and_then(Value::as_str), node.get("TYPE").and_then(Value::as_str)) {
        // Readable only (ACCESS 1) can't be set.
        let writable = node.get("ACCESS").and_then(Value::as_u64).is_none_or(|a| a & 2 != 0);
        let number = kind.starts_with(['f', 'i', 'd', 'h', 'T', 'F']);
        if writable && number {
            let range = node.get("RANGE").and_then(|r| r.get(0));
            let get = |k: &str, or: f32| range.and_then(|r| r.get(k)).and_then(Value::as_f64).map_or(or, |v| v as f32);
            let toggle = kind.starts_with(['T', 'F']);
            let (min, max) = if toggle { (0.0, 1.0) } else { (get("MIN", 0.0), get("MAX", 1.0)) };
            let description = node.get("DESCRIPTION").and_then(Value::as_str).unwrap_or_default().to_string();
            out.push(QueryParam { address: path.to_string(), description, min, max });
        }
    }
    if let Some(children) = node.get("CONTENTS").and_then(Value::as_object) {
        for child in children.values() {
            walk(child, out);
        }
    }
}

/// The OSC port from a `?HOST_INFO` reply.
pub fn osc_port(json: &str) -> Option<u16> {
    serde_json::from_str::<Value>(json).ok()?.get("OSC_PORT")?.as_u64().and_then(|p| u16::try_from(p).ok())
}

/// What was fetched from one app: (host, OSC port, its settings), or why it couldn't be.
pub type Fetched = Result<(String, u16, Vec<QueryParam>), String>;

/// Asks `host:port` (its OSC Query HTTP port) for its namespace and OSC port, in the background. The answer lands
/// in `into`.
pub fn fetch(addr: &str, into: Arc<Mutex<Option<Fetched>>>, ctx: eframe::egui::Context) {
    let addr = addr.trim().trim_start_matches("http://").trim_end_matches('/').to_string();
    let host = addr.split(':').next().unwrap_or_default().to_string();
    let base = format!("http://{addr}");
    let info_into = into.clone();
    let ctx2 = ctx.clone();
    ehttp::fetch(ehttp::Request::get(format!("{base}/?HOST_INFO")), move |info| {
        let port = info.ok().and_then(|r| r.text().and_then(osc_port));
        ehttp::fetch(ehttp::Request::get(format!("{base}/")), move |res| {
            let result = res.map_err(|e| e.to_string()).and_then(|r| {
                let text = r.text().ok_or("an empty answer")?.to_string();
                let params = parse(&text)?;
                // No HOST_INFO: most apps take OSC on the same port as their OSC Query server.
                let port = port.or_else(|| addr.rsplit(':').next()?.parse().ok()).ok_or("no OSC port")?;
                Ok((host, port, params))
            });
            *info_into.lock().unwrap() = Some(result);
            ctx2.request_repaint();
        });
    });
    let _ = (into, ctx);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_oscquery_namespace() {
        // Phosphor's own namespace, as another app would read it.
        let entries = vec![("/phosphor/master".to_string(), "Master fade".to_string(), Some(0.5)), ("/phosphor/cue/go".to_string(), "Cue go".to_string(), None)];
        let json = super::super::oscquery::namespace(&entries).to_string();
        let params = parse(&json).unwrap();
        assert_eq!(params.len(), 2, "{params:?}");
        assert_eq!(params[1], QueryParam { address: "/phosphor/master".into(), description: "Master fade".into(), min: 0.0, max: 1.0 });
        // Other apps: ranges, read-only values, on/off.
        let other = r#"{"FULL_PATH":"/","CONTENTS":{"vol":{"FULL_PATH":"/vol","TYPE":"f","RANGE":[{"MIN":-60,"MAX":6}]},"meter":{"FULL_PATH":"/meter","TYPE":"f","ACCESS":1},"mute":{"FULL_PATH":"/mute","TYPE":"T"},"name":{"FULL_PATH":"/name","TYPE":"s"}}}"#;
        let p = parse(other).unwrap();
        assert_eq!(p.iter().map(|q| q.address.as_str()).collect::<Vec<_>>(), ["/mute", "/vol"]);
        assert_eq!((p[1].min, p[1].max), (-60.0, 6.0));
        assert_eq!(osc_port(r#"{"NAME":"x","OSC_PORT":9000}"#), Some(9000));
        assert!(parse("nope").is_err());
    }

    /// Phosphor reading another Phosphor: its OSC Query server, fetched over HTTP.
    #[test]
    fn fetches_from_a_running_server() {
        let server = super::super::oscquery::Server::start(9123, None).unwrap();
        server.publish(&[("/phosphor/master".into(), "Master fade".into(), Some(0.5))]);
        let into: Arc<Mutex<Option<Fetched>>> = Arc::default();
        fetch(&format!("127.0.0.1:{}", server.port), into.clone(), eframe::egui::Context::default());
        let start = std::time::Instant::now();
        while into.lock().unwrap().is_none() && start.elapsed().as_secs() < 10 {
            std::thread::yield_now();
        }
        let (host, port, params) = into.lock().unwrap().clone().expect("an answer").expect("read");
        assert_eq!((host.as_str(), port), ("127.0.0.1", 9123), "the OSC port from HOST_INFO");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].address, "/phosphor/master");
    }
}
