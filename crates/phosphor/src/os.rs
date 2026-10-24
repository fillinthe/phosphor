//! Opening and revealing files the platform's way: Finder on the Mac, Explorer on Windows, xdg-open elsewhere.
//! One place for it (it used to be `Command::new("open")` all over the app).

use std::path::Path;
use std::process::Command;

/// Opens a file, folder or app with the system (a folder shows in Finder / Explorer).
pub fn open(path: impl AsRef<Path>) {
    let p = path.as_ref();
    let _ = if cfg!(target_os = "macos") {
        Command::new("open").arg(p).spawn()
    } else if cfg!(windows) {
        Command::new("explorer").arg(p).spawn()
    } else {
        Command::new("xdg-open").arg(p).spawn()
    };
}

/// Shows a file selected in its folder.
pub fn reveal(path: impl AsRef<Path>) {
    let p = path.as_ref();
    let _ = if cfg!(target_os = "macos") {
        Command::new("open").arg("-R").arg(p).spawn()
    } else if cfg!(windows) {
        Command::new("explorer").arg(format!("/select,{}", p.display())).spawn()
    } else {
        Command::new("xdg-open").arg(p.parent().unwrap_or(p)).spawn()
    };
}

/// The macOS version this runs on (e.g. (13, 4)); (0, 0) elsewhere or when it can't tell. Read once.
pub fn macos_version() -> (u32, u32) {
    static VERSION: std::sync::OnceLock<(u32, u32)> = std::sync::OnceLock::new();
    *VERSION.get_or_init(|| {
        #[cfg(target_os = "macos")]
        {
            let mut buf = [0u8; 32];
            let mut len = buf.len();
            // SAFETY: sysctlbyname writes at most `len` bytes into the buffer we own.
            let ok = unsafe { libc::sysctlbyname(c"kern.osproductversion".as_ptr(), buf.as_mut_ptr().cast(), &mut len, std::ptr::null_mut(), 0) } == 0;
            if ok {
                return parse_version(&String::from_utf8_lossy(&buf[..len.saturating_sub(1)]));
            }
        }
        (0, 0)
    })
}

fn parse_version(s: &str) -> (u32, u32) {
    let mut parts = s.trim().trim_end_matches('\0').split('.').map(|p| p.parse().unwrap_or(0));
    (parts.next().unwrap_or(0), parts.next().unwrap_or(0))
}

/// Running on macOS `major.minor` or later (false on other systems).
pub fn macos_at_least(major: u32, minor: u32) -> bool {
    macos_version() >= (major, minor)
}

#[cfg(test)]
mod tests {
    #[test]
    fn versions_are_read() {
        assert_eq!(super::parse_version("10.15.7"), (10, 15));
        assert_eq!(super::parse_version("13.6"), (13, 6));
        #[cfg(target_os = "macos")]
        assert!(super::macos_version().0 >= 10, "{:?}", super::macos_version());
    }
}
