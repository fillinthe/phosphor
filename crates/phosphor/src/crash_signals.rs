//! Hard crashes written down. A panic logs itself and a freeze is logged by the watchdog, but a crash below Rust
//! (bad memory access in a library, an uncaught Objective-C exception, an abort) ended Phosphor with nothing in
//! crash.log: five Mac reports in a day said only "this run ended without a panic or a freeze being logged"
//! (2026-11-25). The signal that ends the process now writes one line there first, with the stack when it can be
//! read, and then lets the signal do what it always did.
//!
//! "Blessed is the man that endureth temptation: for when he is tried, he shall receive the crown of life."
//! (James 1:12, KJV)
#![cfg(unix)]

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

/// crash.log, opened for appending when the handlers go in (nothing can be opened safely inside one).
static LOG: AtomicI32 = AtomicI32::new(-1);
/// One crash is written down; a second signal while that happens just ends the process.
static HANDLING: AtomicBool = AtomicBool::new(false);

/// The signals that end a process, what crash.log says for each, and whether a stack is worth reading (a crash
/// inside Phosphor: yes; asked to end from outside: no).
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
/// Seconds the stack may take to read before the process is ended anyway (reading it isn't safe after every kind of
/// crash: it can wait forever on a lock the crashed code held, and a show must come back quickly).
const STACK_SECS: u32 = 3;

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

/// One line with no memory asked for (the crash may have been inside the allocator), then the stack as a second try,
/// then the signal again with its usual ending.
extern "C" fn on_signal(signal: libc::c_int) {
    let fd = LOG.load(Ordering::SeqCst);
    let first = !HANDLING.swap(true, Ordering::SeqCst);
    if first && fd >= 0 {
        let (what, stack) = SIGNALS.iter().find(|(s, _, _)| *s == signal).map_or(("crashed", true), |(_, what, stack)| (*what, *stack));
        let mut line = [0u8; 160];
        // SAFETY: time(NULL) only reads the clock.
        let now = unsafe { libc::time(std::ptr::null_mut()) } as u64;
        let n = first_line(&mut line, now, what);
        write(fd, &line[..n]);
        if stack {
            // SAFETY: alarm only sets a timer; if reading the stack hangs, SIGALRM ends the process.
            unsafe { libc::alarm(STACK_SECS) };
            let steps = stack_line(&std::backtrace::Backtrace::force_capture().to_string());
            if !steps.is_empty() {
                write(fd, format!("  at: {steps}\n").as_bytes());
            }
            // SAFETY: as above.
            unsafe { libc::alarm(0) };
        }
    }
    // The usual ending: the signal's own action back, and the signal again (delivered when this handler returns).
    // SAFETY: signal() and raise() are safe to call in a handler.
    unsafe {
        libc::signal(signal, libc::SIG_DFL);
        libc::raise(signal);
    }
}

fn write(fd: i32, bytes: &[u8]) {
    // SAFETY: a valid buffer written to the log's descriptor; write() is safe in a handler.
    unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
}

/// "[1795611146] crashed: SIGSEGV (…)\n" into `out`, without asking for memory. Returns its length.
fn first_line(out: &mut [u8], now: u64, what: &str) -> usize {
    let mut digits = [0u8; 20];
    let mut d = digits.len();
    let mut left = now;
    loop {
        d -= 1;
        digits[d] = b'0' + (left % 10) as u8;
        left /= 10;
        if left == 0 {
            break;
        }
    }
    let mut n = 0;
    for part in [b"[".as_slice(), &digits[d..], b"] ", what.as_bytes(), b"\n"] {
        let room = out.len() - n;
        let take = part.len().min(room);
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
    }

    #[test]
    fn the_stack_keeps_names_only_and_leaves_the_handler_out() {
        let text = "   0: std::backtrace::Backtrace::force_capture\n             at /rustc/abc/library/std/src/backtrace.rs:312:9\n   1: phosphor::crash_signals::on_signal::h0123456789abcdef\n             at /Users/someone/phosphor/crates/phosphor/src/crash_signals.rs:70:25\n   2: __sigtramp\n   3: avcodec_send_packet\n   4: phosphor::media::video::decode::h00112233445566ff\n             at /Users/someone/phosphor/crates/phosphor/src/media/video.rs:9:1\n   5: std::thread::start\n";
        assert_eq!(stack_line(text), "avcodec_send_packet < phosphor::media::video::decode < std::thread::start");
        assert!(!stack_line(text).contains("someone"));
        assert_eq!(stack_line("disabled backtrace"), "");
    }

    /// The other half of the test below: when asked to, crashes for real with the handlers in.
    #[test]
    fn crash_child() {
        let Some(log) = std::env::var_os("PHOSPHOR_TEST_CRASH_LOG") else { return };
        install(std::path::Path::new(&log));
        let signal = if std::env::var_os("PHOSPHOR_TEST_CRASH_TERM").is_some() { libc::SIGTERM } else { libc::SIGSEGV };
        // SAFETY: sending this test process a signal: it is meant to end here.
        unsafe { libc::raise(signal) };
        std::thread::sleep(std::time::Duration::from_secs(5));
        unreachable!("the signal should have ended this process");
    }

    /// A real process ended by a real signal: the line is in the log, and the process still ended by that signal.
    #[test]
    fn a_crash_is_written_down_and_still_ends_the_process() {
        use std::os::unix::process::ExitStatusExt;
        for (term, signal, words, stack) in [(false, libc::SIGSEGV, "crashed: SIGSEGV", true), (true, libc::SIGTERM, "ended by SIGTERM", false)] {
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
            assert_eq!(text.contains("\n  at: "), stack, "log: {text}");
        }
    }
}
