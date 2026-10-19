//! The session log: what happened while Phosphor ran (what you clicked, messages shown, warnings, errors), written
//! to `logs/session.log` in Phosphor's settings folder (the previous run is kept as `session-previous.log`). Only
//! on this computer; nothing is sent anywhere. Read it to see what went on in a run-through.

use std::io::Write;
use std::sync::Mutex;

/// Where the log goes (None in the browser or without a settings folder).
pub fn path() -> Option<std::path::PathBuf> {
    crate::show::storage_dir().map(|d| d.join("logs").join("session.log"))
}

/// Writes every line to the terminal and the log file.
struct Tee(Mutex<Option<std::fs::File>>);

impl Write for Tee {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buf);
        if let Some(f) = self.0.lock().unwrap().as_mut() {
            let _ = f.write_all(buf);
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(f) = self.0.lock().unwrap().as_mut() {
            let _ = f.flush();
        }
        Ok(())
    }
}

/// Starts logging: Phosphor's own messages from "info" up, other libraries' warnings and errors (RUST_LOG
/// overrides). `to_file` = also keep the session log (not for benchmarks).
pub fn init(to_file: bool) {
    let file = to_file.then(path).flatten().and_then(|p| {
        let dir = p.parent()?;
        std::fs::create_dir_all(dir).ok()?;
        let _ = std::fs::rename(&p, dir.join("session-previous.log"));
        std::fs::File::create(&p).ok()
    });
    let mut b = env_logger::Builder::new();
    b.filter_level(log::LevelFilter::Warn).filter_module("phosphor", log::LevelFilter::Info);
    if let Ok(spec) = std::env::var("RUST_LOG") {
        b.parse_filters(&spec);
    }
    b.target(env_logger::Target::Pipe(Box::new(Tee(Mutex::new(file))))).format_timestamp_secs();
    let _ = b.try_init();
    log::info!("Phosphor {} on {} {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH);
}

#[cfg(test)]
mod tests {
    /// The log file gets the start line and later messages (run with PHOSPHOR_PROFILE set so it's a test folder).
    #[test]
    fn writes_the_session_log() {
        if std::env::var("PHOSPHOR_PROFILE").is_err() {
            return; // never touch the real settings folder from tests
        }
        super::init(true);
        log::info!("notice: test line");
        log::logger().flush();
        let text = std::fs::read_to_string(super::path().unwrap()).unwrap();
        assert!(text.contains("Phosphor ") && text.contains("notice: test line"), "{text}");
    }
}
