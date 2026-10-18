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
