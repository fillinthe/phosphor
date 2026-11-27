//! Hard crashes written down. A panic logs itself and a freeze is logged by the watchdog, but a crash below Rust
//! (bad memory access in a library, an uncaught Objective-C exception, an abort) ended Phosphor with nothing in
//! crash.log: five Mac reports in a day said only "this run ended without a panic or a freeze being logged"
//! (2026-11-25). The signal that ends the process now writes one line there first, with the stack when it can be
//! read, and then lets the signal do what it always did.
//!
//! Windows has no signals: its crashes (an access violation in a driver or library, an unhandled C++ exception) are
//! written down by an unhandled-exception filter instead (`windows.rs`), in the same words and the same way.
//!
//! "Blessed is the man that endureth temptation: for when he is tried, he shall receive the crown of life."
//! (James 1:12, KJV)

#[cfg(unix)]
use std::sync::atomic::{AtomicBool, AtomicI32};
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::install;

/// crash.log, opened for appending when the handlers go in (nothing can be opened safely inside one).
#[cfg(unix)]
static LOG: AtomicI32 = AtomicI32::new(-1);
/// One crash is written down; a second signal while that happens just ends the process.
#[cfg(unix)]
static HANDLING: AtomicBool = AtomicBool::new(false);

/// When the last frame was drawn (unix seconds, 0 = none yet): an end from outside says how long before it that was.
static LAST_FRAME: AtomicU64 = AtomicU64::new(0);

/// A frame was drawn (called with the heartbeat, about once a second).
pub fn frame() {
    LAST_FRAME.store(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()), Ordering::Relaxed);
}

#[cfg(unix)]
/// The signals that end a process, what crash.log says for each, and whether it's a crash. The stack is read for
/// all of them: for a crash it's where it crashed, and for an end from outside it's what Phosphor was doing when
/// someone gave up on it (two 0.5.23 reports, 2026-11-25: "ended by SIGTERM" after a window stopped answering, one
/// of them in the second the watchdog looked, so its own look saw nothing).
const SIGNALS: [(i32, &str, bool); 7] = [
    (libc::SIGSEGV, "crashed: SIGSEGV (touched memory it doesn't have)", true),
    (libc::SIGBUS, "crashed: SIGBUS (touched memory that isn't there)", true),
    (libc::SIGILL, "crashed: SIGILL (ran something that isn't an instruction)", true),
    (libc::SIGFPE, "crashed: SIGFPE (a number operation the processor refused)", true),
    (libc::SIGABRT, "crashed: SIGABRT (a library or the system ended it on purpose)", true),
    (libc::SIGTRAP, "crashed: SIGTRAP (a library or the system ended it on purpose)", true),
    (libc::SIGTERM, "ended by SIGTERM (told to end from outside, not by quitting)", false),
];

/// How many of the stack's steps go into the log.
const STEPS: usize = 14;
#[cfg(unix)]
/// Seconds the stack may take to read before the process is ended anyway (reading it isn't safe after every kind of
/// crash: it can wait forever on a lock the crashed code held, and a show must come back quickly).
const STACK_SECS: u32 = 3;

#[cfg(unix)]
/// Writes crashes into `log` from now on.
pub fn install(log: &std::path::Path) {
    use std::os::fd::IntoRawFd;
    let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(log) else { return };
    LOG.store(file.into_raw_fd(), Ordering::SeqCst);
    for (signal, _, _) in SIGNALS {
        // SAFETY: a zeroed sigaction with our handler and flags set; the handler only does what its comment says.
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = on_signal as extern "C" fn(libc::c_int) as usize;
            // On the thread's spare stack when it has one (a stack that ran out can't run a handler on itself).
            action.sa_flags = libc::SA_ONSTACK;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(signal, &action, std::ptr::null_mut());
        }
    }
}

#[cfg(unix)]
/// One line with no memory asked for (the crash may have been inside the allocator), then the stack as a second try,
/// then the signal again with its usual ending.
extern "C" fn on_signal(signal: libc::c_int) {
    let fd = LOG.load(Ordering::SeqCst);
    let first = !HANDLING.swap(true, Ordering::SeqCst);
    if first && fd >= 0 {
        let (what, crash) = SIGNALS.iter().find(|(s, _, _)| *s == signal).map_or(("crashed", true), |(_, what, crash)| (*what, *crash));
        let mut line = [0u8; 160];
        // SAFETY: time(NULL) only reads the clock.
        let now = unsafe { libc::time(std::ptr::null_mut()) } as u64;
        let n = first_line(&mut line, now, what);
        write(fd, &line[..n]);
        let last = LAST_FRAME.load(Ordering::Relaxed);
        if !crash && last != 0 {
            let n = frame_line(&mut line, now.saturating_sub(last));
            write(fd, &line[..n]);
        }
        // SAFETY: alarm only sets a timer; if reading the stack hangs, SIGALRM ends the process.
        unsafe { libc::alarm(STACK_SECS) };
        let steps = stack_line(&std::backtrace::Backtrace::force_capture().to_string());
        if !steps.is_empty() {
            write(fd, format!("  {}: {steps}\n", thread_words(on_main_thread())).as_bytes());
        }
        // SAFETY: as above.
        unsafe { libc::alarm(0) };
    }
    // The usual ending: the signal's own action back, and the signal again (delivered when this handler returns).
    // SAFETY: signal() and raise() are safe to call in a handler.
    unsafe {
        libc::signal(signal, libc::SIG_DFL);
        libc::raise(signal);
    }
}

#[cfg(unix)]
fn write(fd: i32, bytes: &[u8]) {
    // SAFETY: a valid buffer written to the log's descriptor; write() is safe in a handler.
    unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
}

#[cfg(unix)]
/// Is this the main thread (the one that draws)? None where that can't be asked.
fn on_main_thread() -> Option<bool> {
    // SAFETY: both only read which thread this is.
    #[cfg(target_os = "macos")]
    return Some(unsafe { libc::pthread_main_np() } != 0);
    #[cfg(target_os = "linux")]
    return Some(unsafe { libc::syscall(libc::SYS_gettid) } == i64::from(unsafe { libc::getpid() }));
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    None
}

/// What the stack's line is called: the main thread's stack is where the window was (stuck or waiting).
fn thread_words(main: Option<bool>) -> &'static str {
    match main {
        Some(true) => "main thread at",
        Some(false) => "another thread at",
        None => "at",
    }
}

/// "[1795611146] crashed: SIGSEGV (…)\n" into `out`, without asking for memory. Returns its length.
fn first_line(out: &mut [u8], now: u64, what: &str) -> usize {
    let mut digits = [0u8; 20];
    let d = number(&mut digits, now);
    put(out, &[b"[", &digits[d..], b"] ", what.as_bytes(), b"\n"])
}

/// "  last frame: 21 s before the end\n" into `out` (the watchdog's words for the same thing), without asking for
/// memory. Returns its length.
#[cfg(any(unix, test))]
fn frame_line(out: &mut [u8], ago: u64) -> usize {
    let mut digits = [0u8; 20];
    let d = number(&mut digits, ago);
    put(out, &[b"  last frame: ", &digits[d..], b" s before the end\n"])
}

/// `n` in decimal at the end of `digits`; returns where it starts.
fn number(digits: &mut [u8; 20], n: u64) -> usize {
    let mut d = digits.len();
    let mut left = n;
    loop {
        d -= 1;
        digits[d] = b'0' + (left % 10) as u8;
        left /= 10;
        if left == 0 {
            break;
        }
    }
    d
}

/// The parts one after another into `out`, cut at its end. Returns the length.
fn put(out: &mut [u8], parts: &[&[u8]]) -> usize {
    let mut n = 0;
    for part in parts {
        let take = part.len().min(out.len() - n);
        out[n..n + take].copy_from_slice(&part[..take]);
        n += take;
    }
    n
}

/// The stack as one line, innermost first: function names only (no file paths: reports carry no folder names), the
/// handler's own steps left out. `text` is a `std::backtrace::Backtrace` printed.
fn stack_line(text: &str) -> String {
    let names: Vec<&str> = text
        .lines()
        .filter_map(|l| {
            let (number, name) = l.trim().split_once(": ")?;
            (!number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())).then_some(name.trim())
        })
        .collect();
    // Everything up to the handler (and the system's step that called it) is this module reading the stack.
    let ours = |n: &str| n.contains("crash_signals::on_signal") || n.contains("_sigtramp") || n.contains("__restore_rt");
    let start = names.iter().rposition(|n| ours(n)).map_or(0, |i| i + 1);
    names[start..].iter().take(STEPS).map(|n| without_hash(n)).collect::<Vec<_>>().join(" < ")
}

/// "phosphor::app::draw::h0123456789abcdef" → "phosphor::app::draw".
fn without_hash(name: &str) -> &str {
    match name.rsplit_once("::h") {
        Some((rest, hash)) if hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit()) => rest,
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_line_is_stamped_like_the_rest_of_the_log() {
        let mut out = [0u8; 160];
        let n = first_line(&mut out, 1795611146, "crashed: SIGSEGV (touched memory it doesn't have)");
        assert_eq!(std::str::from_utf8(&out[..n]).unwrap(), "[1795611146] crashed: SIGSEGV (touched memory it doesn't have)\n");
        // Never past the buffer, whatever the words.
        let mut small = [0u8; 8];
        assert_eq!(first_line(&mut small, 0, "crashed"), 8);
        let n = frame_line(&mut out, 21);
        assert_eq!(std::str::from_utf8(&out[..n]).unwrap(), "  last frame: 21 s before the end\n");
        assert_eq!(thread_words(Some(true)), "main thread at");
    }

    #[test]
    fn the_stack_keeps_names_only_and_leaves_the_handler_out() {
        let text = "   0: std::backtrace::Backtrace::force_capture\n             at /rustc/abc/library/std/src/backtrace.rs:312:9\n   1: phosphor::crash_signals::on_signal::h0123456789abcdef\n             at /Users/someone/phosphor/crates/phosphor/src/crash_signals.rs:70:25\n   2: __sigtramp\n   3: avcodec_send_packet\n   4: phosphor::media::video::decode::h00112233445566ff\n             at /Users/someone/phosphor/crates/phosphor/src/media/video.rs:9:1\n   5: std::thread::start\n";
        assert_eq!(stack_line(text), "avcodec_send_packet < phosphor::media::video::decode < std::thread::start");
        assert!(!stack_line(text).contains("someone"));
        assert_eq!(stack_line("disabled backtrace"), "");
    }

    /// The other half of the test below: when asked to, crashes for real with the handlers in.
    #[cfg(unix)]
    #[test]
    fn crash_child() {
        let Some(log) = std::env::var_os("PHOSPHOR_TEST_CRASH_LOG") else { return };
        install(std::path::Path::new(&log));
        frame();
        let signal = if std::env::var_os("PHOSPHOR_TEST_CRASH_TERM").is_some() { libc::SIGTERM } else { libc::SIGSEGV };
        // SAFETY: sending this test process a signal: it is meant to end here.
        unsafe { libc::raise(signal) };
        std::thread::sleep(std::time::Duration::from_secs(5));
        unreachable!("the signal should have ended this process");
    }

    /// A real process ended by a real signal: the line is in the log, and the process still ended by that signal.
    #[cfg(unix)]
    #[test]
    fn a_crash_is_written_down_and_still_ends_the_process() {
        use std::os::unix::process::ExitStatusExt;
        for (term, signal, words, frame) in [(false, libc::SIGSEGV, "crashed: SIGSEGV", false), (true, libc::SIGTERM, "ended by SIGTERM", true)] {
            let log = std::env::temp_dir().join(format!("phosphor-crash-test-{}-{term}.log", std::process::id()));
            let _ = std::fs::remove_file(&log);
            let mut child = std::process::Command::new(std::env::current_exe().unwrap());
            child.args(["crash_signals::tests::crash_child", "--exact", "--test-threads=1"]).env("PHOSPHOR_TEST_CRASH_LOG", &log);
            child.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
            if term {
                child.env("PHOSPHOR_TEST_CRASH_TERM", "1");
            }
            let status = child.status().unwrap();
            let text = std::fs::read_to_string(&log).unwrap_or_default();
            let _ = std::fs::remove_file(&log);
            assert_eq!(status.signal(), Some(signal), "ended by the signal itself; log: {text}");
            let first = text.lines().next().unwrap_or("");
            assert!(first.starts_with('[') && first.contains(words), "log: {text}");
            // Where it was, for a crash and for an end from outside alike (the test's own thread, not the main one).
            assert!(text.contains("\n  another thread at: "), "log: {text}");
            assert_eq!(text.contains("\n  last frame: "), frame, "log: {text}");
        }
    }
}
