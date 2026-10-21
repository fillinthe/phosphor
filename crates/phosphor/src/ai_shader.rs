//! Describe a shader in words and the AI writes it (Shader Editor › Describe it…). Uses your own Anthropic API key,
//! kept in the macOS Keychain (elsewhere: a file only you can read in Phosphor's settings folder), or
//! `ANTHROPIC_API_KEY`. Each answer is checked with the engine's own compiler (`custom::check`, no GPU) on the job's
//! thread; if it doesn't compile, the error goes back to the AI to fix (up to `REPAIRS` times) before you see it.
//!
//! The conversation is only ever appended to (the AI's answers go back exactly as they came), so a fix keeps the
//! reasoning behind the first try. `PHOSPHOR_AI_URL` points it at another server (tests use a local one).

use std::process::{Command, Stdio};
use std::sync::mpsc;

use phosphor_engine::custom;
use serde_json::{Value, json};

/// The model that writes shaders.
pub const MODEL: &str = "ai-model";
/// How many times a shader that doesn't compile goes back for a fix.
pub const REPAIRS: usize = 2;
const API_URL: &str = "https://api.anthropic.com/v1/messages";
const KEYCHAIN_SERVICE: &str = "Phosphor: Anthropic API key";

// ---- the key ----

/// A key looks like `sk-ant-…` (letters, digits, - and _ only, so it's safe to hand to the Keychain tool).
pub fn looks_like_key(key: &str) -> bool {
    key.starts_with("sk-ant-") && key.len() > 20 && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn key_file() -> Option<std::path::PathBuf> {
    crate::show::storage_dir().map(|d| d.join("anthropic.key"))
}

/// The saved key (`ANTHROPIC_API_KEY` first), if any.
pub fn load_key() -> Option<String> {
    if let Ok(k) = std::env::var("ANTHROPIC_API_KEY")
        && !k.trim().is_empty()
    {
        return Some(k.trim().to_string());
    }
    if cfg!(target_os = "macos") {
        let out = Command::new("/usr/bin/security").args(["find-generic-password", "-s", KEYCHAIN_SERVICE, "-w"]).stderr(Stdio::null()).output().ok()?;
        let k = String::from_utf8_lossy(&out.stdout).trim().to_string();
        return (out.status.success() && !k.is_empty()).then_some(k);
    }
    std::fs::read_to_string(key_file()?).ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty())
}

/// Saves the key (an empty one removes it). On the Mac it goes to the Keychain, written through the tool's stdin so
/// it never shows in a process list.
pub fn store_key(key: &str) -> Result<(), String> {
    if !key.is_empty() && !looks_like_key(key) {
        return Err("That doesn't look like an Anthropic API key (they start with sk-ant-)".into());
    }
    if cfg!(target_os = "macos") {
        let line = if key.is_empty() {
            format!("delete-generic-password -s \"{KEYCHAIN_SERVICE}\"\n")
        } else {
            format!("add-generic-password -U -s \"{KEYCHAIN_SERVICE}\" -a phosphor -w {key}\n")
        };
        let mut child = Command::new("/usr/bin/security").arg("-i").stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e| e.to_string())?;
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(line.as_bytes());
        }
        let _ = child.wait();
        return Ok(());
    }
    let path = key_file().ok_or("no settings folder")?;
    if key.is_empty() {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&path, key).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

// ---- what the AI is told ----

/// Everything the AI needs to write a Phosphor shader: the format, the helpers, and the starter templates as examples.
pub fn system_prompt() -> String {
    let mut s = String::from(SYSTEM);
    s.push_str("\n\n# Examples (Phosphor's own starter templates)\n");
    for t in custom::TEMPLATES {
        s.push_str(&format!("\n## {} ({})\n```wgsl\n{}```\n", t.name, t.about, t.source));
    }
    s
}

const SYSTEM: &str = r#"You write shaders for Phosphor, a VJ app that plays live visuals. The user describes a look in plain words and
you answer with one complete Phosphor shader in a single ```wgsl code block, then one or two short sentences saying
what its sliders do. Nothing else.

# The format
WGSL. The header is comment lines:
- `// @name Name` (short, title case)
- `// @kind effect` (processes a layer's picture) or `// @kind generator` (makes its own picture)
- `// @param Name min max default`, one per slider, at most 12; read them with P(0u), P(1u), … in order.
  Give every visible quantity a slider (amount, speed, size, colour…) with sensible ranges, and make the default look good.
- optional `// @feedback`: last(uv) returns this shader's own previous frame (trails, echoes, smears).
- optional `// @pass name [scale]` lines: passes drawn before main, each `fn name(in: VOut) -> vec4f` (no attributes),
  read by later passes and main with read_name(uv). scale 0.05..1 (0.5 = half size, cheaper for blurs).
Exactly one entry point:
```
@fragment
fn main(in: VOut) -> @location(0) vec4f { … }
```

# What's available (already declared: never redeclare them)
- in.uv: 0..1 across and down (0,0 = top left). in.pos: pixel position.
- u.time (seconds), u.resolution (vec2f, pixels), aspect() = width / height.
- src(uv) -> vec4f: the layer's picture (effects only). Straight alpha; keep the input's alpha unless the look needs otherwise.
- luma(rgb), rgb2hsv(rgb), hsv2rgb(hsv), palette(t) (cosine rainbow), hash21(vec2f) -> 0..1, value_noise(vec2f) -> 0..1,
  centered(uv) (origin in the middle, x scaled by aspect), TAU.
- textureSampleLevel(tex0, samp_mirror, uv, 0.0) samples the picture with mirrored edges.

# Rules
- Valid WGSL for naga/wgpu: f32 literals need a decimal point or suffix (1.0, not 1); no implicit int/float mixing;
  `var` for anything reassigned; vec3f/vec4f shorthand; loops need fixed bounds (at most 64 steps); no recursion;
  sample only with textureSampleLevel (no textureSample, no derivatives in loops).
- Return colour 0..1 with alpha. Generators usually return alpha 1.
- It runs every frame at full resolution: keep it cheap (no more than ~40 texture reads per pixel).
- Animate with u.time scaled by a Speed slider so a slider at 0 freezes it.
- Write helper functions above main with unique names (not main, src, P, luma, palette, hash21, value_noise, centered).
- If asked to change an existing shader, answer with the whole changed shader, keeping its name and sliders unless asked.
"#;

// ---- the request ----

/// The request body: one model, adaptive thinking at medium effort (plenty for a shader), the instructions cached so
/// a fix costs little, and the default refusal fallback.
pub fn request_body(messages: &[Value]) -> Value {
    json!({
        "model": MODEL,
        "max_tokens": 16000,
        "system": [{"type": "text", "text": system_prompt(), "cache_control": {"type": "ephemeral"}}],
        "output_config": {"effort": "medium"},
        "fallbacks": "default",
        "messages": messages,
    })
}

/// The first message: the description, plus the shader to change and whether it's an effect or a source.
pub fn first_message(description: &str, generator: bool, change: Option<&str>) -> Value {
    let text = match change {
        Some(src) => format!("Change this shader: {description}\n\n```wgsl\n{src}```"),
        None => format!("Make a {}: {description}", if generator { "generator (it makes its own picture)" } else { "effect (it processes a layer's picture)" }),
    };
    json!({"role": "user", "content": text})
}

/// What came back: the assistant's content (kept exactly, for the next turn), its shader and its note.
#[derive(Debug)]
pub struct Answer {
    pub content: Value,
    pub shader: Option<String>,
    pub note: String,
}

/// Reads a Messages API response. Errors are written for people (no key, out of credit, declined…).
pub fn read_response(status: u16, body: &[u8]) -> Result<Answer, String> {
    let v: Value = serde_json::from_slice(body).map_err(|_| format!("unexpected answer from the server ({status})"))?;
    if status != 200 {
        let msg = v["error"]["message"].as_str().unwrap_or("unknown error");
        return Err(match status {
            401 => "The API key wasn't accepted: check it at console.anthropic.com".into(),
            402 | 403 => format!("Your Anthropic account can't run this: {msg}"),
            429 => "Too many requests right now: try again in a minute".into(),
            529 | 500..=599 => "the AI is busy: try again in a minute".into(),
            _ => format!("the AI couldn't do that: {msg}"),
        });
    }
    if v["stop_reason"] == "refusal" {
        return Err("the AI declined that description: try describing the look differently".into());
    }
    let text: String = v["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n");
    let (shader, note) = split_code(&text);
    if shader.is_none() && v["stop_reason"] == "max_tokens" {
        return Err("The answer ran too long: try a simpler description".into());
    }
    Ok(Answer { content: v["content"].clone(), shader, note })
}

/// The first ```wgsl (or plain ```) block, and the words outside it.
fn split_code(text: &str) -> (Option<String>, String) {
    let Some(start) = text.find("```") else { return (None, text.trim().to_string()) };
    let after = &text[start + 3..];
    let body_at = after.find('\n').map(|i| i + 1).unwrap_or(0);
    let body = &after[body_at..];
    let Some(end) = body.find("```") else { return (None, text.trim().to_string()) };
    let mut code = body[..end].to_string();
    if !code.ends_with('\n') {
        code.push('\n');
    }
    let note = format!("{} {}", text[..start].trim(), body[end + 3..].trim()).trim().to_string();
    (Some(code), note)
}

// ---- the job ----

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Working(String),
    /// A shader that compiles, and what the AI said about it.
    Done { source: String, note: String },
    Failed(String),
}

/// One description → shader, on its own thread (network and compile checks never touch the UI thread).
pub struct Job {
    rx: mpsc::Receiver<Status>,
    pub status: Status,
}

impl Job {
    pub fn start(key: String, description: String, generator: bool, change: Option<String>, ctx: Option<eframe::egui::Context>) -> Job {
        let (tx, rx) = mpsc::channel();
        let _ = std::thread::Builder::new().name("ai shader".into()).spawn(move || {
            let send = |s: Status| {
                let _ = tx.send(s);
                if let Some(c) = &ctx {
                    c.request_repaint();
                }
            };
            send(Status::Working("the AI is writing it…".into()));
            send(run(&key, &description, generator, change.as_deref(), |m| send(Status::Working(m))));
        });
        Job { rx, status: Status::Working("starting…".into()) }
    }

    /// Picks up news from the thread; returns the status.
    pub fn poll(&mut self) -> &Status {
        for s in self.rx.try_iter() {
            self.status = s;
        }
        &self.status
    }
}

fn post(key: &str, body: &Value) -> Result<(u16, Vec<u8>), String> {
    let url = std::env::var("PHOSPHOR_AI_URL").unwrap_or_else(|_| API_URL.to_string());
    let mut req = ehttp::Request::post(url, serde_json::to_vec(body).unwrap_or_default());
    req.headers = ehttp::Headers::new(&[
        ("content-type", "application/json"),
        ("x-api-key", key),
        ("anthropic-version", "2023-06-01"),
        ("anthropic-beta", "server-side-fallback-2026-07-01"),
    ]);
    // A considered answer can take a minute or two.
    req.timeout = Some(std::time::Duration::from_secs(300));
    let r = ehttp::fetch_blocking(&req).map_err(|e| format!("couldn't reach the AI: {e}"))?;
    Ok((r.status, r.bytes))
}

/// Ask, check it compiles, send any error back for a fix.
fn run(key: &str, description: &str, generator: bool, change: Option<&str>, progress: impl Fn(String)) -> Status {
    let mut messages = vec![first_message(description, generator, change)];
    for attempt in 0..=REPAIRS {
        let answer = match post(key, &request_body(&messages)).and_then(|(status, body)| read_response(status, &body)) {
            Ok(a) => a,
            Err(e) => return Status::Failed(e),
        };
        messages.push(json!({"role": "assistant", "content": answer.content}));
        let problem = match &answer.shader {
            None => "Answer with the whole shader in one ```wgsl code block.".to_string(),
            Some(src) => match custom::check(src) {
                Ok(_) => return Status::Done { source: src.clone(), note: answer.note },
                Err(e) => format!("Phosphor's compiler says: {e}\nSend the whole fixed shader."),
            },
        };
        if attempt == REPAIRS {
            return Status::Failed(format!("the AI's shader still doesn't compile ({problem})"));
        }
        progress(format!("fixing a compile error (try {} of {REPAIRS})…", attempt + 1));
        messages.push(json!({"role": "user", "content": problem}));
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_instructions_hold_every_template_and_the_request_is_complete() {
        let s = system_prompt();
        for t in custom::TEMPLATES {
            assert!(s.contains(t.name));
        }
        let b = request_body(&[first_message("plasma", true, None)]);
        assert_eq!(b["model"], MODEL);
        assert_eq!(b["messages"][0]["role"], "user");
        assert!(b["messages"][0]["content"].as_str().unwrap().contains("generator"));
        assert!(b.get("thinking").is_none() && b.get("temperature").is_none(), "Opus 5.5: adaptive by default, no sampling params");
    }

    #[test]
    fn answers_and_errors_are_read() {
        let body = json!({"content": [{"type": "thinking", "thinking": ""}, {"type": "text", "text": "Here:\n```wgsl\n// @name X\nfn main() {}\n```\nAmount sets it."}], "stop_reason": "end_turn"});
        let a = read_response(200, body.to_string().as_bytes()).unwrap();
        assert_eq!(a.shader.as_deref(), Some("// @name X\nfn main() {}\n"));
        assert_eq!(a.note, "Here: Amount sets it.");
        assert_eq!(a.content.as_array().unwrap().len(), 2, "kept exactly, thinking included");
        let refused = json!({"content": [], "stop_reason": "refusal"});
        assert!(read_response(200, refused.to_string().as_bytes()).unwrap_err().contains("declined"));
        let bad_key = json!({"type": "error", "error": {"type": "authentication_error", "message": "invalid x-api-key"}});
        assert!(read_response(401, bad_key.to_string().as_bytes()).unwrap_err().contains("API key"));
        assert!(looks_like_key("sk-ant-api03-abcdefghijklmnop_QR-st"));
        assert!(!looks_like_key("sk-ant-x; rm -rf"));
    }
}
