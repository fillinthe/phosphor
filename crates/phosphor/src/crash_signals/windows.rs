//! Windows: a crash below Rust (an access violation in a graphics driver or a library, an unhandled C++ exception)
//! ended Phosphor with nothing in crash.log, and the Windows app has no console to say anything either. An
//! unhandled-exception filter now writes one line first, as the Mac's signal handler does: what happened, where (the
//! module's file name and the offset in it, no folders), then the stack when it can be read. Then Windows ends the
//! process as it always did (Windows Error Reporting still sees it).
//!
//! Not seen here: Rust's own aborts and stack overflows (they end the process by a "fast fail" that runs no filter;
//! a stack overflow prints its line to the console) and panics (the panic hook logs those).

use std::io::Write as _;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use windows_sys::Win32::System::Diagnostics::Debug::{EXCEPTION_CONTINUE_SEARCH, EXCEPTION_POINTERS, SetUnhandledExceptionFilter};
use windows_sys::Win32::System::LibraryLoader::{GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleFileNameW, GetModuleHandleExW};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;

/// crash.log, opened for appending when the filter goes in (nothing should be opened inside it).
static LOG: OnceLock<std::fs::File> = OnceLock::new();
/// One crash is written down; another one while that happens just lets Windows end the process.
static HANDLING: AtomicBool = AtomicBool::new(false);
/// The thread that draws (the one `install` runs on).
static MAIN_THREAD: AtomicU32 = AtomicU32::new(0);

/// Writes crashes into `log` from now on.
pub fn install(log: &std::path::Path) {
    let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(log) else { return };
    let _ = LOG.set(file);
    // SAFETY: reads this thread's id; installs a filter with the signature Windows calls.
    unsafe {
        MAIN_THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
        SetUnhandledExceptionFilter(Some(on_exception));
    }
}

/// What crash.log says for an exception code.
fn words(code: u32) -> &'static str {
    match code {
        0xC000_0005 => "crashed: access violation (touched memory it doesn't have)",
        0xC000_0006 => "crashed: in-page error (a file it was reading went away: a drive unplugged?)",
        0xC000_001D => "crashed: illegal instruction (ran something that isn't an instruction for this processor)",
        0xC000_0096 => "crashed: privileged instruction",
        0xC000_008C | 0xC000_0094 => "crashed: integer division by zero or out of range",
        0xC000_0095 => "crashed: integer overflow",
        0xC000_00FD => "crashed: stack overflow",
        0xC000_0374 => "crashed: heap corruption (memory a library freed twice or wrote past)",
        0xC000_0409 => "crashed: stack buffer overrun / fast fail",
        0xE06D_7363 => "crashed: C++ exception nobody caught (in a driver or library)",
        0x8000_0003 => "crashed: breakpoint (a library or the system ended it on purpose)",
        _ => "crashed: unhandled exception",
    }
}

/// One line with no memory asked for (the crash may have been inside the allocator), then the stack as a second try.
unsafe extern "system" fn on_exception(info: *const EXCEPTION_POINTERS) -> i32 {
    if HANDLING.swap(true, Ordering::SeqCst) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    let Some(mut log) = LOG.get() else { return EXCEPTION_CONTINUE_SEARCH };
    // SAFETY: Windows hands the filter valid exception pointers; the record is read, never written.
    let (code, address, access) = unsafe {
        let record = info.as_ref().map(|i| i.ExceptionRecord).filter(|r| !r.is_null());
        match record {
            Some(r) => {
                let r = &*r;
                // An access violation carries (0 read / 1 write / 8 run, the address).
                let access = (r.ExceptionCode as u32 == 0xC000_0005 && r.NumberParameters >= 2).then(|| (r.ExceptionInformation[0], r.ExceptionInformation[1]));
                (r.ExceptionCode as u32, r.ExceptionAddress as usize, access)
            }
            None => (0, 0, None),
        }
    };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let mut line = [0u8; 400];
    let n = super::first_line(&mut line, now, words(code));
    // "[t] crashed: …\n" + "  code 0xC0000005 at nvwgf2umx.dll+0x1a2b3c, writing 0x0\n"
    let mut detail = [0u8; 400];
    let mut code_hex = [0u8; 16];
    let c = hex(&mut code_hex, code as u64);
    let mut module = [0u8; 128];
    let (m, base) = module_of(address, &mut module);
    let mut offset_hex = [0u8; 16];
    let o = hex(&mut offset_hex, address.wrapping_sub(base) as u64);
    let mut target_hex = [0u8; 16];
    let (verb, t): (&[u8], usize) = match access {
        Some((0, at)) => (b", reading 0x", hex(&mut target_hex, at as u64)),
        Some((1, at)) => (b", writing 0x", hex(&mut target_hex, at as u64)),
        Some((8, at)) => (b", running 0x", hex(&mut target_hex, at as u64)),
        _ => (b"", target_hex.len()),
    };
    let d = super::put(
        &mut detail,
        &[b"  code 0x", &code_hex[c..], b" at ", if m == 0 { b"?" } else { &module[..m] }, b"+0x", &offset_hex[o..], verb, &target_hex[t..], b"\n"],
    );
    let _ = log.write_all(&line[..n]);
    let _ = log.write_all(&detail[..d]);
    // SAFETY: reads this thread's id.
    let main = unsafe { GetCurrentThreadId() } == MAIN_THREAD.load(Ordering::SeqCst);
    let steps = super::stack_line(&std::backtrace::Backtrace::force_capture().to_string());
    if !steps.is_empty() {
        let _ = log.write_all(format!("  {}: {steps}\n", super::thread_words(Some(main))).as_bytes());
    }
    let _ = log.flush();
    EXCEPTION_CONTINUE_SEARCH
}

/// The file name (ASCII; anything else as '?') of the module holding `address` into `out`, and the module's base.
/// (0, 0) when no module holds it (a jump to nowhere).
fn module_of(address: usize, out: &mut [u8]) -> (usize, usize) {
    let mut module = std::ptr::null_mut();
    let mut wide = [0u16; 260];
    // SAFETY: asks which module holds an address (without taking a reference on it) and its path into our buffer.
    let len = unsafe {
        if GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, address as *const u16, &mut module) == 0 {
            return (0, 0);
        }
        GetModuleFileNameW(module, wide.as_mut_ptr(), wide.len() as u32) as usize
    };
    let path = &wide[..len.min(wide.len())];
    // The name only: reports carry no folder names.
    let name = path.iter().rposition(|&c| c == b'\\' as u16 || c == b'/' as u16).map_or(path, |i| &path[i + 1..]);
    let n = name.len().min(out.len());
    for (o, &c) in out.iter_mut().zip(name.iter()) {
        *o = if (0x20..0x7f).contains(&c) { c as u8 } else { b'?' };
    }
    (n, module as usize)
}

/// `n` in hexadecimal (capitals) at the end of `digits`; returns where it starts.
fn hex(digits: &mut [u8; 16], n: u64) -> usize {
    let mut d = digits.len();
    let mut left = n;
    loop {
        d -= 1;
        digits[d] = b"0123456789ABCDEF"[(left & 15) as usize];
        left >>= 4;
        if left == 0 {
            break;
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_and_numbers_read_as_they_should() {
        let mut d = [0u8; 16];
        let at = hex(&mut d, 0xC000_0005);
        assert_eq!(&d[at..], b"C0000005");
        let at = hex(&mut d, 0);
        assert_eq!(&d[at..], b"0");
        assert!(words(0xC000_0005).contains("access violation"));
        assert_eq!(words(0x1234), "crashed: unhandled exception");
        // This test's own code is in the test program.
        let mut name = [0u8; 128];
        let (n, base) = module_of(codes_and_numbers_read_as_they_should as fn() as usize, &mut name);
        assert!(n > 0 && base != 0 && std::str::from_utf8(&name[..n]).unwrap().to_lowercase().ends_with(".exe"));
    }

    /// The other half of the test below: when asked to, crashes for real with the filter in.
    #[test]
    fn crash_child() {
        let Some(log) = std::env::var_os("PHOSPHOR_TEST_CRASH_LOG") else { return };
        // No Windows Error Reporting dialog waiting for a click on a test machine.
        // SAFETY: sets this process's error mode.
        unsafe { windows_sys::Win32::System::Diagnostics::Debug::SetErrorMode(windows_sys::Win32::System::Diagnostics::Debug::SEM_NOGPFAULTERRORBOX) };
        install(std::path::Path::new(&log));
        // SAFETY: none: this test process is meant to crash here (an aligned address nothing is mapped at; a null
        // pointer would be stopped by Rust's own checks first).
        unsafe { std::ptr::write_volatile(0x10 as *mut u32, 1) };
        unreachable!("the write should have crashed this process");
    }

    /// A real access violation: the lines are in the log, and the process still ended with the exception's code.
    #[test]
    fn a_crash_is_written_down_and_still_ends_the_process() {
        let log = std::env::temp_dir().join(format!("phosphor-crash-test-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&log);
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["crash_signals::windows::tests::crash_child", "--exact", "--test-threads=1"])
            .env("PHOSPHOR_TEST_CRASH_LOG", &log)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        let _ = std::fs::remove_file(&log);
        assert_eq!(status.code().map(|c| c as u32), Some(0xC000_0005), "ended by the access violation; log: {text}");
        let first = text.lines().next().unwrap_or("");
        assert!(first.starts_with('[') && first.contains("access violation"), "log: {text}");
        assert!(text.contains("  code 0xC0000005 at ") && text.contains(", writing 0x10"), "log: {text}");
        // (The crashing thread is the one that put the filter in: "main thread at".)
        assert!(text.contains("thread at: "), "log: {text}");
    }
}
