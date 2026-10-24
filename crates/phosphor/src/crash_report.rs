//! Reports to the Phosphor team: bug reports someone sends from Report a Bug (one click), and crash reports sent by
//! themselves (opt-in: after Phosphor reopens from a crash, when "Send crash reports" is on). A report is the words
//! plus the diagnostics the window shows (version, system, the project's shape as counts, the end of crash.log; no
//! project content, no file names), as JSON `{"version", "os", "arch", "kind", "report"}` to `ENDPOINT`, which keeps it
//! privately (nothing public). `PHOSPHOR_CRASH_URL` points it anywhere (tests).

/// Where reports go (the phosphorfx.com report receiver). Empty = sending off.
const ENDPOINT: &str = "https://phosphorfx.com/api/report";

/// Where reports go, if anywhere.
pub fn endpoint() -> Option<String> {
    std::env::var("PHOSPHOR_CRASH_URL").ok().filter(|u| u.starts_with("http")).or_else(|| (!ENDPOINT.is_empty()).then(|| ENDPOINT.to_string()))
}

/// The JSON body for one report (`kind`: "crash" sent by itself, "bug" written by someone).
pub fn body(kind: &str, report: &str) -> String {
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "kind": kind,
        "report": report,
    })
    .to_string()
}

/// Sends a crash report in the background; `done` hears whether it arrived.
pub fn send(report: String, done: impl FnOnce(Result<(), String>) + Send + 'static) {
    send_kind("crash", report, done)
}

/// Sends a report someone wrote (Report a problem).
pub fn send_bug(report: String, done: impl FnOnce(Result<(), String>) + Send + 'static) {
    send_kind("bug", report, done)
}

fn send_kind(kind: &str, report: String, done: impl FnOnce(Result<(), String>) + Send + 'static) {
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
        let v: serde_json::Value = serde_json::from_str(&body("crash", "crash.log (end):\npanicked at x")).unwrap();
        assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!((v["kind"].as_str(), v["report"].as_str()), (Some("crash"), Some("crash.log (end):\npanicked at x")));
        assert_eq!(v.as_object().unwrap().len(), 5);
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
        send("crash.log (end):\npanicked at y".into(), move |r| {
            let _ = tx.send(r);
        });
        let result = rx.recv_timeout(std::time::Duration::from_secs(10)).expect("an answer");
        let request = server.join().unwrap();
        assert!(result.is_ok(), "{result:?}");
        assert!(request.starts_with("POST /crash"), "{request}");
        assert!(request.contains("application/json") && request.contains("panicked at y"), "{request}");
    }
}
