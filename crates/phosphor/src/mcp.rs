//! AI assistants control Phosphor through MCP (the Model Context Protocol): `Phosphor --mcp` is a small server the
//! assistant starts (any MCP client), talking JSON-RPC over stdin / stdout. It drives
//! the Phosphor that's open the same way TouchOSC does: OSC Query to list every control with its value, OSC to set
//! them (`/phosphor/...` addresses, names and wildcards work), and the Session server's picture to look at the
//! output. Nothing new listens on the network: the open Phosphor writes its ports to `ai-link.json` in its settings
//! folder (`write_link`) and this reads them.

use std::io::{BufRead, Write};
use std::time::Duration;

use serde_json::{Value, json};

/// Where the open Phosphor answers.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Link {
    pub osc_port: u16,
    pub query_port: u16,
}

const LINK_FILE: &str = "ai-link.json";

/// Called by the open app whenever OSC (re)starts.
pub fn write_link(link: &Link) {
    if let Some(dir) = crate::show::storage_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(LINK_FILE), serde_json::to_string(link).unwrap_or_default());
    }
}

fn read_link() -> Option<Link> {
    serde_json::from_str(&std::fs::read_to_string(crate::show::storage_dir()?.join(LINK_FILE)).ok()?).ok()
}

/// How many controls `list_controls` returns unless asked for more.
const LIST_LIMIT: usize = 150;

const INSTRUCTIONS: &str = "Phosphor is a VJ app (live visuals for shows). Every control has an OSC address \
(/phosphor/master, /phosphor/layer/<id or name>/opacity, effect and generator settings…) and a value from 0 to 1. \
Use list_controls with search words to find addresses, set_controls to change values (several at once; * wildcards \
reach many), trigger for buttons (clips, columns, cues, Surprise…) and look to see the output. Changes show \
straight away, so change things step by step during a show.";

/// Runs the server until the client closes stdin.
pub fn run() {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(&msg, read_link),
            Err(e) => Some(json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": format!("not JSON: {e}") } })),
        };
        if let Some(reply) = reply {
            let _ = writeln!(out, "{reply}");
            let _ = out.flush();
        }
    }
}

/// One JSON-RPC message → the reply (None for notifications). `link` finds the open Phosphor (tests pass their own).
pub fn handle(msg: &Value, link: impl Fn() -> Option<Link>) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params.get("protocolVersion").and_then(Value::as_str).unwrap_or("2025-06-18"),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "phosphor", "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            Ok(match call(name, &args, &link) {
                Ok(content) => json!({ "content": content }),
                Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
            })
        }
        _ => Err(json!({ "code": -32601, "message": format!("unknown method {method}") })),
    };
    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": e }),
    })
}

fn tools() -> Value {
    json!([
        {
            "name": "list_controls",
            "description": "Lists Phosphor's controls: OSC address, what it is, and its value (0 to 1; triggers have none). \
                Search words narrow it down (all must match the address or name), e.g. \"layer opacity\", \"blur\", \"cue\".",
            "inputSchema": { "type": "object", "properties": {
                "search": { "type": "string", "description": "Words to look for (optional)" },
                "limit": { "type": "integer", "description": "Most to return (default 150)" }
            } }
        },
        {
            "name": "set_controls",
            "description": "Sets controls to values from 0 to 1 (on / off controls: 1 or 0). Addresses as listed, or with a \
                layer's name instead of its number (/phosphor/layer/intro/opacity), or * wildcards (/phosphor/layer/*/opacity).",
            "inputSchema": { "type": "object", "required": ["changes"], "properties": {
                "changes": { "type": "array", "items": { "type": "object", "required": ["address", "value"], "properties": {
                    "address": { "type": "string" },
                    "value": { "type": "number", "minimum": 0, "maximum": 1 }
                } } }
            } }
        },
        {
            "name": "trigger",
            "description": "Presses a button control (a clip, a column, a cue, Surprise, Blackout…): any listed address without a value.",
            "inputSchema": { "type": "object", "required": ["address"], "properties": { "address": { "type": "string" } } }
        },
        {
            "name": "look",
            "description": "A picture of Phosphor's output right now (needs the Session turned on in Phosphor: Outputs › Session).",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ])
}

fn call(name: &str, args: &Value, link: &impl Fn() -> Option<Link>) -> Result<Value, String> {
    let link = link().ok_or("Phosphor isn't open (or hasn't been opened yet on this Mac). Open it, then try again.")?;
    match name {
        "list_controls" => {
            let all = controls(&link)?;
            let words: Vec<String> = args.get("search").and_then(Value::as_str).unwrap_or("").to_lowercase().split_whitespace().map(String::from).collect();
            let limit = args.get("limit").and_then(Value::as_u64).map_or(LIST_LIMIT, |n| n.max(1) as usize);
            let found: Vec<&Control> = all.iter().filter(|c| words.iter().all(|w| c.address.to_lowercase().contains(w) || c.name.to_lowercase().contains(w))).collect();
            let mut text = found.iter().take(limit).map(|c| c.line()).collect::<Vec<_>>().join("\n");
            if found.len() > limit {
                text.push_str(&format!("\n… and {} more (search for fewer, or raise the limit)", found.len() - limit));
            }
            if found.is_empty() {
                text = format!("No control matches (Phosphor has {}).", all.len());
            }
            Ok(json!([{ "type": "text", "text": text }]))
        }
        "set_controls" => {
            let changes = args.get("changes").and_then(Value::as_array).ok_or("changes: a list of {address, value}")?;
            let mut done = Vec::new();
            for c in changes {
                let address = c.get("address").and_then(Value::as_str).ok_or("each change needs an address")?;
                let value = c.get("value").and_then(Value::as_f64).ok_or("each change needs a value from 0 to 1")?.clamp(0.0, 1.0) as f32;
                send(&link, address, value)?;
                done.push(format!("{address} = {value:.3}"));
            }
            Ok(json!([{ "type": "text", "text": format!("Set {}.", done.join(", ")) }]))
        }
        "trigger" => {
            let address = args.get("address").and_then(Value::as_str).ok_or("address: the button's OSC address")?;
            // Pressed, then let go (clips that play while held stop on the release).
            send(&link, address, 1.0)?;
            std::thread::sleep(Duration::from_millis(60));
            send(&link, address, 0.0)?;
            Ok(json!([{ "type": "text", "text": format!("Pressed {address}.") }]))
        }
        "look" => {
            let port = std::env::var("PHOSPHOR_SESSION_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(crate::net::DEFAULT_PORT);
            let bytes = get_bytes(&format!("http://127.0.0.1:{port}/snapshot.jpg"))
                .map_err(|_| "Can't see the output: turn on the Session in Phosphor (Outputs › Session), then look again.".to_string())?;
            use base64::Engine as _;
            Ok(json!([{ "type": "image", "data": base64::engine::general_purpose::STANDARD.encode(bytes), "mimeType": "image/jpeg" }]))
        }
        _ => Err(format!("No tool called {name}.")),
    }
}

struct Control {
    address: String,
    name: String,
    value: Option<f64>,
}

impl Control {
    fn line(&self) -> String {
        match self.value {
            Some(v) => format!("{} · {} · {v:.3}", self.address, self.name),
            None => format!("{} · {} · (button)", self.address, self.name),
        }
    }
}

/// Every control the open Phosphor lists (its OSC Query tree, flattened).
fn controls(link: &Link) -> Result<Vec<Control>, String> {
    let body = get_bytes(&format!("http://127.0.0.1:{}/", link.query_port)).map_err(|_| "Phosphor isn't answering (is it open?).".to_string())?;
    let tree: Value = serde_json::from_slice(&body).map_err(|e| format!("Phosphor's list didn't read: {e}"))?;
    let mut out = Vec::new();
    flatten(&tree, &mut out);
    Ok(out)
}

fn flatten(node: &Value, out: &mut Vec<Control>) {
    if node.get("TYPE").is_some()
        && let Some(address) = node.get("FULL_PATH").and_then(Value::as_str)
    {
        out.push(Control {
            address: address.to_string(),
            name: node.get("DESCRIPTION").and_then(Value::as_str).unwrap_or("").to_string(),
            value: node.get("VALUE").and_then(|v| v.get(0)).and_then(Value::as_f64),
        });
    }
    if let Some(children) = node.get("CONTENTS").and_then(Value::as_object) {
        for child in children.values() {
            flatten(child, out);
        }
    }
}

fn send(link: &Link, address: &str, value: f32) -> Result<(), String> {
    if !address.starts_with("/phosphor/") {
        return Err(format!("{address}: Phosphor's addresses start with /phosphor/ (list_controls shows them)."));
    }
    let msg = rosc::OscPacket::Message(rosc::OscMessage { addr: address.to_string(), args: vec![rosc::OscType::Float(value)] });
    let bytes = rosc::encoder::encode(&msg).map_err(|e| e.to_string())?;
    let sock = std::net::UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    sock.send_to(&bytes, ("127.0.0.1", link.osc_port)).map_err(|e| e.to_string())?;
    Ok(())
}

fn get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url).timeout(Duration::from_secs(5)).call().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(resp.into_reader(), 32 << 20), &mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The assistant's whole round: start, list tools, find a control, set it (arrives as OSC), a bad address.
    #[test]
    fn lists_and_sets_controls_over_osc() {
        let query = crate::control::oscquery::Server::start(0, None).unwrap();
        query.publish(&[
            ("/phosphor/master".into(), "Master fade".into(), Some(0.5)),
            ("/phosphor/layer/3/opacity".into(), "Layer 3 · Opacity".into(), Some(0.25)),
            ("/phosphor/column/1".into(), "Column 1".into(), None),
        ]);
        let osc = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        osc.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let link = Link { osc_port: osc.local_addr().unwrap().port(), query_port: query.port };
        let ask = |id: u64, method: &str, params: Value| handle(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }), || Some(link.clone())).unwrap();
        let text = |r: &Value| r["result"]["content"][0]["text"].as_str().unwrap_or("").to_string();

        assert_eq!(ask(1, "initialize", json!({ "protocolVersion": "2025-06-18" }))["result"]["serverInfo"]["name"], "phosphor");
        assert!(handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }), || None).is_none(), "notifications get no reply");
        assert_eq!(ask(2, "tools/list", json!({}))["result"]["tools"].as_array().unwrap().len(), 4);

        let list = text(&ask(3, "tools/call", json!({ "name": "list_controls", "arguments": { "search": "layer opacity" } })));
        assert_eq!(list, "/phosphor/layer/3/opacity · Layer 3 · Opacity · 0.250");
        assert!(text(&ask(4, "tools/call", json!({ "name": "list_controls", "arguments": {} }))).contains("/phosphor/column/1 · Column 1 · (button)"));

        let set = ask(5, "tools/call", json!({ "name": "set_controls", "arguments": { "changes": [{ "address": "/phosphor/master", "value": 1.5 }] } }));
        assert_eq!(set["result"]["isError"], Value::Null);
        let mut buf = [0u8; 256];
        let (n, _) = osc.recv_from(&mut buf).unwrap();
        let (_, packet) = rosc::decoder::decode_udp(&buf[..n]).unwrap();
        let rosc::OscPacket::Message(m) = packet else { panic!("a message") };
        assert_eq!((m.addr.as_str(), m.args), ("/phosphor/master", vec![rosc::OscType::Float(1.0)]), "values are kept to 0..1");

        let bad = ask(6, "tools/call", json!({ "name": "set_controls", "arguments": { "changes": [{ "address": "master", "value": 0.2 }] } }));
        assert_eq!(bad["result"]["isError"], true);
        let closed = handle(&json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": { "name": "list_controls" } }), || None).unwrap();
        assert!(text(&closed).contains("isn't open"));
        assert_eq!(ask(8, "nope", json!({}))["error"]["code"], -32601);
    }
}
