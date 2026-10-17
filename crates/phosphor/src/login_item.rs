//! Start Phosphor when the user logs in (installations on a Mac Mini): a LaunchAgent plist that opens the app
//! bundle. Nothing is loaded while the app runs; macOS starts it at the next login.

use std::path::PathBuf;

const LABEL: &str = "com.phosphorfx.phosphor";

fn plist_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/LaunchAgents").join(format!("{LABEL}.plist")))
}

/// The .app this binary runs from (None when run from a plain build directory).
fn own_app() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let app = exe.parent()?.parent()?.parent()?.to_path_buf();
    (app.extension().is_some_and(|e| e == "app")).then_some(app)
}

pub fn is_enabled() -> bool {
    plist_path().is_some_and(|p| p.is_file())
}

/// Writes (or removes) the LaunchAgent. Returns what happened, for a notice.
pub fn set(on: bool) -> Result<String, String> {
    let path = plist_path().ok_or("no home folder")?;
    if !on {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        return Ok("Phosphor won't start at login".into());
    }
    let app = own_app().or_else(|| Some(PathBuf::from("/Applications/Phosphor.app")).filter(|p| p.exists())).ok_or("Install Phosphor.app first (Applications)")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key><array><string>/usr/bin/open</string><string>-a</string><string>{}</string></array>
  <key>RunAtLoad</key><true/>
</dict>
</plist>
"#,
        app.display()
    );
    std::fs::write(&path, plist).map_err(|e| e.to_string())?;
    Ok(format!("Phosphor will start when you log in ({})", app.display()))
}
