//! Reports to the Phosphor team: bug reports someone sends from Report a Bug (one click), and crash reports sent by
//! themselves (opt-in: after Phosphor reopens from a crash, when "Send crash reports" is on). A report is the words
//! plus the diagnostics the window shows (version, system, the project's shape as counts, the end of crash.log; no
//! project content, no file names), as JSON `{"version", "os", "arch", "kind", "report"}` to `ENDPOINT`, which keeps it
//! privately (nothing public). `PHOSPHOR_CRASH_URL` points it anywhere (tests).
//!
//! Each report also leaves one line on the G Workshop account server (`POST /api/error`: "Phosphor: crash: panicked at
//! …", the version, and the account when signed in: the admin dashboard's Problems list), with file and folder names
//! taken out (`summary_line`). Only while "Send problem reports" is on.

/// Where reports go (the phosphorfx.com report receiver). Empty = sending off.
const ENDPOINT: &str = "https://phosphorfx.com/api/report";

/// Where reports go, if anywhere.
pub fn endpoint() -> Option<String> {
    std::env::var("PHOSPHOR_CRASH_URL").ok().filter(|u| u.starts_with("http")).or_else(|| (!ENDPOINT.is_empty()).then(|| ENDPOINT.to_string()))
}

/// "Share problem reports and anonymous counts" (the one switch for everything Phosphor sends by itself: crash and
/// freeze reports, the problem line on the account server, the update count). Set from the setting at start and when
/// it's switched.
static SHARING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn set_sharing(on: bool) {
    SHARING.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn sharing() -> bool {
    SHARING.load(std::sync::atomic::Ordering::Relaxed)
}

/// This computer's report ID: random (nothing personal), made the first time a report is sent and kept next to the
/// settings. Reports carry it so "Delete my reports" can find them all on the report server.
#[cfg(not(target_arch = "wasm32"))]
pub fn reporter_id() -> Option<String> {
    if let Some(id) = saved_reporter_id() {
        return Some(id);
    }
    let path = crate::show::storage_dir()?.join("report-id");
    let id = crate::net::new_token();
    let _ = std::fs::create_dir_all(path.parent()?);
    std::fs::write(&path, &id).ok()?;
    Some(id)
}

/// The report ID this computer already has, if it ever sent a report (never makes one).
#[cfg(not(target_arch = "wasm32"))]
fn saved_reporter_id() -> Option<String> {
    let id = std::fs::read_to_string(crate::show::storage_dir()?.join("report-id")).ok()?;
    let id = id.trim();
    (id.len() >= 16 && id.chars().all(|c| c.is_ascii_hexdigit())).then(|| id.to_ascii_lowercase())
}

#[cfg(target_arch = "wasm32")]
pub fn reporter_id() -> Option<String> {
    None
}

#[cfg(target_arch = "wasm32")]
fn saved_reporter_id() -> Option<String> {
    None
}

/// After "Delete my reports": the next report starts a new ID.
#[cfg(not(target_arch = "wasm32"))]
fn forget_reporter_id() {
    if let Some(dir) = crate::show::storage_dir() {
        let _ = std::fs::remove_file(dir.join("report-id"));
    }
}

#[cfg(target_arch = "wasm32")]
fn forget_reporter_id() {}

/// Asks the report server to delete every report from this computer (its report ID); `done` hears how many went.
pub fn delete_my_reports(done: impl FnOnce(Result<usize, String>) + Send + 'static) {
    // No ID = this computer never sent a report: nothing to ask the server.
    let (Some(url), Some(id)) = (endpoint(), saved_reporter_id()) else {
        done(Ok(0));
        return;
    };
    let mut req = ehttp::Request::post(url, serde_json::json!({ "delete": id }).to_string().into_bytes());
    req.headers = ehttp::Headers::new(&[("content-type", "application/json"), ("user-agent", "Phosphor")]);
    ehttp::fetch(req, move |r| {
        done(match r {
            Ok(r) if r.ok => {
                forget_reporter_id();
                let n = serde_json::from_slice::<serde_json::Value>(&r.bytes).ok().and_then(|v| v["deleted"].as_u64()).unwrap_or(0);
                Ok(n as usize)
            }
            Ok(r) => Err(format!("the report server answered {}", r.status)),
            Err(e) => Err(e),
        })
    });
}

/// The JSON body for one report (`kind`: "crash" sent by itself, "bug" written by someone), with this computer's report
/// ID when it has one.
pub fn body(kind: &str, report: &str) -> String {
    body_from(kind, report, reporter_id())
}

fn body_from(kind: &str, report: &str, reporter: Option<String>) -> String {
    let mut v = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "kind": kind,
        "report": report,
    });
    if let Some(id) = reporter {
        v["reporter"] = serde_json::Value::String(id);
    }
    v.to_string()
}

/// Who a report's line on the account server is from: the account server, and the sign-in token when signed in
/// (`Account::reporter`). None = no line there ("Send problem reports" is off).
#[derive(Clone, Debug, Default)]
pub struct Reporter {
    pub api: String,
    pub token: Option<String>,
}

/// Sends a crash report in the background; `done` hears whether it arrived.
pub fn send(report: String, to: Option<Reporter>, done: impl FnOnce(Result<(), String>) + Send + 'static) {
    send_kind("crash", report, to, done)
}

/// Sends a report someone wrote (Report a problem).
pub fn send_bug(report: String, to: Option<Reporter>, done: impl FnOnce(Result<(), String>) + Send + 'static) {
    send_kind("bug", report, to, done)
}

/// The one line the account server keeps for a report: "Phosphor: crash: panicked at panels.rs:12: index out of
/// bounds" (a crash: what crash.log says last; a bug: the first line of the words), without file or folder names.
pub fn summary_line(kind: &str, report: &str) -> String {
    let what = if kind == "crash" {
        // The last panic or freeze in crash.log, with the line after it (the panic's message, where it was stuck).
        let lines: Vec<&str> = report.lines().collect();
        lines
            .iter()
            .rposition(|l| l.contains("panicked at") || l.contains("froze ("))
            .map(|i| {
                let first = lines[i].trim_start_matches(|c: char| c == '[' || c.is_ascii_digit() || c == ']' || c == ' ');
                // (Not the heading of the next section: "session log (recent):", nor the window's state at the last
                // frame, which would split one cause into many.)
                let next = lines
                    .get(i + 1)
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty() && !l.starts_with('[') && !l.ends_with(':') && !l.starts_with("last frame:"));
                match next {
                    Some(n) => format!("{} {n}", first.trim_end_matches(':')),
                    None => first.to_string(),
                }
            })
            .unwrap_or_else(|| "quit unexpectedly".to_string())
    } else {
        report.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("a report").to_string()
    };
    let cleaned: Vec<String> = what.split_whitespace().map(without_names).collect();
    let mut line = format!("Phosphor: {kind}: {}", cleaned.join(" "));
    if line.chars().count() > 300 {
        line = line.chars().take(299).collect::<String>() + "…";
    }
    line
}

/// One word with file and folder names taken out: a path keeps only a source file's or library's name ("/Users/x/…/panels.rs:12:5"
/// → "panels.rs:12:5"), anything else that looks like a path or a file name ("IMG_3976.mov") becomes "…".
fn without_names(word: &str) -> String {
    let core = word.trim_matches(|c: char| "\"'(),;".contains(c));
    let file = core.rsplit(['/', '\\']).next().unwrap_or(core);
    // Source files and system libraries say where, not whose: kept.
    let source = file.split(':').next().is_some_and(|f| [".rs", ".dylib", ".dll", ".so"].iter().any(|e| f.ends_with(e)));
    if core.contains(['/', '\\']) {
        return if source { word.replace(core, file) } else { word.replace(core, "…") };
    }
    let name = core.split(':').next().unwrap_or(core);
    let looks_like_file = name.rsplit_once('.').is_some_and(|(stem, ext)| !stem.is_empty() && (2..=5).contains(&ext.len()) && ext.chars().all(|c| c.is_ascii_alphanumeric()) && ext.chars().any(|c| c.is_ascii_alphabetic()));
    if looks_like_file && !source { word.replace(name, "…") } else { word.to_string() }
}

/// The line for the account server (fire and forget: it never holds anything up).
fn send_summary(kind: &str, report: &str, to: &Reporter) {
    let body = serde_json::json!({
        "message": summary_line(kind, report),
        "where": format!("Phosphor {} desktop", env!("CARGO_PKG_VERSION")),
        "source": "app",
    });
    let mut req = ehttp::Request::post(format!("{}/api/error", to.api.trim_end_matches('/')), body.to_string().into_bytes());
    req.headers = ehttp::Headers::new(&[("content-type", "application/json"), ("user-agent", "Phosphor")]);
    if let Some(t) = to.token.as_ref().filter(|t| !t.is_empty()) {
        req.headers.insert("Authorization", format!("Bearer {t}"));
    }
    ehttp::fetch(req, |_| {});
}

fn send_kind(kind: &str, report: String, to: Option<Reporter>, done: impl FnOnce(Result<(), String>) + Send + 'static) {
    if let Some(to) = &to {
        send_summary(kind, &report, to);
    }
    let Some(url) = endpoint() else {
        done(Err("no report server set up".into()));
        return;
    };
    let mut req = ehttp::Request::post(url, body(kind, &report).into_bytes());
    req.headers = ehttp::Headers::new(&[("content-type", "application/json"), ("user-agent", "Phosphor")]);
    ehttp::fetch(req, move |r| {
        done(match r {
            Ok(r) if r.ok => Ok(()),
            Ok(r) => Err(format!("the crash server answered {}", r.status)),
            Err(e) => Err(e),
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_carries_the_report_and_nothing_else() {
        let v: serde_json::Value = serde_json::from_str(&body_from("crash", "crash.log (end):\npanicked at x", None)).unwrap();
        assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!((v["kind"].as_str(), v["report"].as_str()), (Some("crash"), Some("crash.log (end):\npanicked at x")));
        assert_eq!(v.as_object().unwrap().len(), 5);
        // With this computer's random report ID (so "Delete my reports" finds them): one more field, nothing else.
        let v: serde_json::Value = serde_json::from_str(&body_from("bug", "x", Some("ab".repeat(16)))).unwrap();
        assert_eq!((v.as_object().unwrap().len(), v["reporter"].as_str()), (6, Some("abababababababababababababababab")));
    }

    #[test]
    fn the_account_server_gets_one_line_without_file_names() {
        let crash = "Phosphor quit unexpectedly.\n---\ncrash.log (end):\n[1795036128] panicked at /Users/someone/.cargo/registry/src/objc2-app-kit-0.3.2/src/generated/NSOpenPanel.rs:127:5:\nunexpected NULL returned from +[NSOpenPanel openPanel]\nsession log (recent):\n05:18:28 IMG_3976.mov: opened in 33 ms\n";
        assert_eq!(summary_line("crash", crash), "Phosphor: crash: panicked at NSOpenPanel.rs:127:5 unexpected NULL returned from +[NSOpenPanel openPanel]");
        let froze = "crash.log (end):\n[1795060207] froze (no heartbeat for 20 s); restarted by the watchdog\n  stuck at: PhosphorApp::render > in libsystem_kernel.dylib: __psynch_cvwait\n";
        assert_eq!(summary_line("crash", froze), "Phosphor: crash: froze (no heartbeat for 20 s); restarted by the watchdog stuck at: PhosphorApp::render > in libsystem_kernel.dylib: __psynch_cvwait");
        assert_eq!(summary_line("crash", "nothing in the log"), "Phosphor: crash: quit unexpectedly");
        let windows_freeze = "crash.log (end):\n[1795060207] froze (no heartbeat for 20 s); restarted by the watchdog\nsession log (recent):\n05:18:25 Phosphor 0.5.12\n";
        assert_eq!(summary_line("crash", windows_freeze), "Phosphor: crash: froze (no heartbeat for 20 s); restarted by the watchdog");
        let with_window = "crash.log (end):\n[1795060207] froze (no heartbeat for 20 s); restarted by the watchdog\n  last frame: window minimized=yes focused=no occluded=? fullscreen=no, 1 outputs open\n";
        assert_eq!(summary_line("crash", with_window), "Phosphor: crash: froze (no heartbeat for 20 s); restarted by the watchdog", "the window's state doesn't split causes");
        let bug = "My clip Party Night.mov in /Users/jane/Movies won't play in 0.5.11, e.g. on loop\n---\nPhosphor 0.5.11";
        assert_eq!(summary_line("bug", bug), "Phosphor: bug: My clip Party … in … won't play in 0.5.11, e.g. on loop");
    }

    /// The line goes to the account server with the sign-in token (the dashboard shows whose it is).
    #[test]
    fn the_line_carries_the_sign_in() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            conn.set_read_timeout(Some(std::time::Duration::from_secs(5))).ok();
            let mut got = Vec::new();
            let mut buf = [0u8; 4096];
            while let Ok(n) = conn.read(&mut buf) {
                if n == 0 {
                    break;
                }
                got.extend_from_slice(&buf[..n]);
                if String::from_utf8_lossy(&got).contains("desktop") {
                    break;
                }
            }
            conn.write_all(b"HTTP/1.1 204 No Content\r\nconnection: close\r\n\r\n").ok();
            String::from_utf8_lossy(&got).to_string()
        });
        send_summary("crash", "crash.log (end):\n[1] panicked at a.rs:1:2:\nboom", &Reporter { api: format!("http://127.0.0.1:{port}"), token: Some("tok123".into()) });
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /api/error"), "{request}");
        assert!(request.to_lowercase().contains("authorization: bearer tok123"), "{request}");
        assert!(request.contains("Phosphor: crash: panicked at a.rs:1:2 boom") && request.contains(&format!("Phosphor {} desktop", env!("CARGO_PKG_VERSION"))), "{request}");
    }

    /// A report reaches a server: a local one stands in for the crash server.
    #[test]
    fn a_report_is_posted() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            conn.set_read_timeout(Some(std::time::Duration::from_secs(5))).ok();
            let mut got = Vec::new();
            let mut buf = [0u8; 4096];
            while let Ok(n) = conn.read(&mut buf) {
                if n == 0 {
                    break;
                }
                got.extend_from_slice(&buf[..n]);
                if String::from_utf8_lossy(&got).contains("panicked at y") {
                    break;
                }
            }
            conn.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n").ok();
            String::from_utf8_lossy(&got).to_string()
        });
        // SAFETY: the only test that sets it.
        unsafe { std::env::set_var("PHOSPHOR_CRASH_URL", format!("http://127.0.0.1:{port}/crash")) };
        let (tx, rx) = std::sync::mpsc::channel();
        send("crash.log (end):\npanicked at y".into(), None, move |r| {
            let _ = tx.send(r);
        });
        let result = rx.recv_timeout(std::time::Duration::from_secs(10)).expect("an answer");
        let request = server.join().unwrap();
        assert!(result.is_ok(), "{result:?}");
        assert!(request.starts_with("POST /crash"), "{request}");
        assert!(request.contains("application/json") && request.contains("panicked at y"), "{request}");
    }
}
