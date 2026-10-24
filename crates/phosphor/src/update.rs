//! In-app updates: checks GitHub releases (as soon as Phosphor opens, then every hour, and on request). With
//! "Install updates by themselves" (on by default) a newer version downloads and swaps itself in place in the
//! background, then the Update window offers to relaunch; otherwise the new version opens next time. Only the real
//! app (an .app bundle, not a bench run) installs by itself. `PHOSPHOR_UPDATE_URL` points the check elsewhere (tests use a local server).

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};

use eframe::egui;
use egui_phosphor::regular as icon;

pub const RELEASES_API: &str = "https://api.github.com/repos/fillinthe/phosphor/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/fillinthe/phosphor/releases";
/// Redirects to the newest release's page; no API rate limit (the fallback check reads the tag from where it lands).
pub const LATEST_PAGE: &str = "https://github.com/fillinthe/phosphor/releases/latest";
/// This build's download name (what `scripts/release-github.sh` uploads): Intel Macs have their own build.
pub const ZIP_NAME: &str = if cfg!(windows) {
    "Phosphor-windows.zip"
} else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
    "Phosphor-mac-intel.zip"
} else {
    "Phosphor-mac.zip"
};

/// This build's download of a given version.
pub fn zip_url(version: &str) -> String {
    format!("https://github.com/fillinthe/phosphor/releases/download/v{version}/{ZIP_NAME}")
}

/// "…/releases/tag/v0.2.42" → "0.2.42".
fn version_from_url(url: &str) -> Option<String> {
    let tag = url.split("/releases/tag/").nth(1)?.split(['?', '#', '/']).next()?;
    let v = tag.trim_start_matches(['v', 'V']);
    (!parse_version(v).is_empty()).then(|| v.to_string())
}
const CHECK_EVERY: f64 = 60.0 * 60.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub page: String,
    /// The Mac app zip, if the release has one.
    pub zip: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Downloading(Release),
    /// The new app is in place at this path; relaunch to use it.
    Ready(PathBuf),
    /// Downloaded but not installed (not running from an .app): where it landed.
    Downloaded(PathBuf),
    /// The check failed (network, GitHub busy): tried again at the next daily check.
    Failed(String),
    /// A newer version exists but installing it here failed: the window sends the user to download it themselves.
    InstallFailed(Release, String),
}

pub struct Updater {
    pub status: Status,
    pub window: bool,
    /// The window popped out into its own OS window.
    pub detached: bool,
    /// Version the user chose to skip.
    pub skipped: String,
    next_check: f64,
    check_rx: Option<Receiver<Result<Option<Release>, String>>>,
    install_rx: Option<Receiver<Result<(PathBuf, bool), String>>>,
    progress: Arc<Mutex<String>>,
    /// Bench: install as soon as an update shows up, print `UPDATE_READY`, never relaunch.
    pub test: bool,
    /// Download and install new versions by themselves (setting; on by default).
    pub auto: bool,
}

impl Default for Updater {
    fn default() -> Self {
        Self { status: Status::Idle, window: false, detached: false, skipped: String::new(), next_check: 2.0, check_rx: None, install_rx: None, progress: Default::default(), test: false, auto: true }
    }
}

impl Updater {
    /// The saved settings: the version skipped, and whether updates install by themselves.
    pub fn with_settings(skipped: String, auto: bool) -> Self {
        Self { skipped, auto, ..Default::default() }
    }

    /// Installs updates by itself here: the setting is on, Phosphor runs from an app bundle, and it isn't a bench.
    fn installs_by_itself(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        return self.auto && current_bundle().is_some() && std::env::var("PHOSPHOR_NO_UPDATE_CHECK").is_err();
        #[cfg(target_arch = "wasm32")]
        false
    }
}

/// "v0.2.1" → [0, 2, 1]
fn parse_version(v: &str) -> Vec<u32> {
    v.trim().trim_start_matches(['v', 'V']).split(['.', '-', '+']).map_while(|p| p.parse::<u32>().ok()).collect()
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    let (a, b) = (parse_version(candidate), parse_version(current));
    !a.is_empty() && a > b
}

fn parse_release(text: &str) -> Result<Option<Release>, String> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let Some(tag) = v.get("tag_name").and_then(|t| t.as_str()) else {
        return Ok(None); // no releases yet
    };
    let zip = v
        .get("assets")
        .and_then(|a| a.as_array())
        .and_then(|a| {
            // Exactly this build's download (an Intel Mac never gets the Apple Silicon zip, or the other way round).
            a.iter().find(|x| x.get("name").and_then(|n| n.as_str()) == Some(ZIP_NAME))
        })
        .and_then(|x| x.get("browser_download_url").and_then(|u| u.as_str()).map(str::to_string));
    Ok(Some(Release {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        notes: v.get("body").and_then(|b| b.as_str()).unwrap_or("").to_string(),
        page: v.get("html_url").and_then(|b| b.as_str()).unwrap_or(RELEASES_PAGE).to_string(),
        zip,
    }))
}

impl Updater {
    /// Asks GitHub for the newest release (off the UI thread).
    pub fn check(&mut self, ctx: &egui::Context) {
        if matches!(self.status, Status::Checking | Status::Downloading(_)) {
            return;
        }
        self.status = Status::Checking;
        // PHOSPHOR_UPDATE_TEST_FALLBACK: the API "fails" (unreachable), so the release-page fallback runs (tests).
        let url = if std::env::var("PHOSPHOR_UPDATE_TEST_FALLBACK").is_ok() {
            "https://127.0.0.1:1/".to_string()
        } else {
            std::env::var("PHOSPHOR_UPDATE_URL").unwrap_or_else(|_| RELEASES_API.into())
        };
        let (tx, rx) = channel();
        self.check_rx = Some(rx);
        let ctx = ctx.clone();
        let mut req = ehttp::Request::get(url);
        req.headers = ehttp::Headers::new(&[("User-Agent", "Phosphor"), ("Accept", "application/vnd.github+json")]);
        let custom = std::env::var("PHOSPHOR_UPDATE_URL").is_ok();
        ehttp::fetch(req, move |res| {
            let out = match res {
                Ok(r) if r.ok => parse_release(r.text().unwrap_or("")),
                Ok(r) if r.status == 404 => Ok(None),
                Ok(r) => Err(format!("Couldn't check for updates (HTTP {}). Try again later.", r.status)),
                Err(_) => Err("Couldn't reach the update server. Check the internet connection and try again.".into()),
            };
            // GitHub's API limits how often one address may ask; the release page itself doesn't. On a failed API
            // check, follow /releases/latest and read the version from where it lands.
            if out.is_err() && !custom {
                let mut req = ehttp::Request::get(LATEST_PAGE);
                req.headers = ehttp::Headers::new(&[("User-Agent", "Phosphor")]);
                let tx2 = tx.clone();
                let ctx2 = ctx.clone();
                ehttp::fetch(req, move |res| {
                    let fallback = match res {
                        Ok(r) if r.ok => match version_from_url(&r.url) {
                            Some(v) => Ok(Some(Release { zip: Some(zip_url(&v)), page: r.url.clone(), notes: String::new(), version: v })),
                            None => out,
                        },
                        _ => out,
                    };
                    let _ = tx2.send(fallback);
                    ctx2.request_repaint();
                });
                return;
            }
            let _ = tx.send(out);
            ctx.request_repaint();
        });
    }

    /// Call every frame: the daily check, and results coming back from the threads.
    pub fn tick(&mut self, ctx: &egui::Context, now: f64) {
        if now >= self.next_check && !cfg!(target_arch = "wasm32") {
            self.next_check = now + CHECK_EVERY;
            if std::env::var("PHOSPHOR_NO_UPDATE_CHECK").is_err() && matches!(self.status, Status::Idle | Status::UpToDate | Status::Failed(_)) {
                self.check(ctx);
            }
        }
        if let Some(rx) = &self.check_rx
            && let Ok(res) = rx.try_recv()
        {
            self.check_rx = None;
            if self.test {
                match &res {
                    Ok(Some(r)) => println!("UPDATE_CHECK latest {} (this is {}) zip {:?}", r.version, env!("CARGO_PKG_VERSION"), r.zip),
                    Ok(None) => println!("UPDATE_CHECK no releases"),
                    Err(e) => println!("UPDATE_CHECK failed: {e}"),
                }
            }
            self.status = match res {
                Ok(Some(r)) if is_newer(&r.version, env!("CARGO_PKG_VERSION")) => Status::Available(r),
                Ok(_) => Status::UpToDate,
                Err(e) => Status::Failed(e),
            };
            if let Status::Available(r) = &self.status
                && (self.test || (self.installs_by_itself() && r.version != self.skipped))
            {
                self.install();
            }
        }
        if let Some(rx) = &self.install_rx
            && let Ok(res) = rx.try_recv()
        {
            self.install_rx = None;
            let release = match &self.status {
                Status::Downloading(r) => Some(r.clone()),
                _ => None,
            };
            self.status = match (res, release) {
                // Installed: say so (the window offers to relaunch; otherwise it opens next time).
                (Ok((path, true)), _) => {
                    if !self.test {
                        self.window = true;
                    }
                    Status::Ready(path)
                }
                (Ok((path, false)), _) => Status::Downloaded(path),
                // Keep the release: the top bar still offers it and the window links its download.
                (Err(e), Some(r)) => {
                    self.window = true;
                    Status::InstallFailed(r, e)
                }
                (Err(e), None) => Status::Failed(e),
            };
            if self.test {
                match &self.status {
                    Status::Ready(p) | Status::Downloaded(p) => println!("UPDATE_READY {}", p.display()),
                    Status::Failed(e) => println!("UPDATE_FAILED {e}"),
                    Status::InstallFailed(r, e) => println!("UPDATE_INSTALL_FAILED {} ({e}); download offered: {}", r.version, r.zip.clone().unwrap_or_else(|| r.page.clone())),
                    _ => {}
                }
            }
            ctx.request_repaint();
        }
    }

    /// The update is worth a button in the top bar (not skipped, not dismissed as up to date).
    pub fn pending(&self) -> Option<&Release> {
        match &self.status {
            Status::Available(r) | Status::Downloading(r) | Status::InstallFailed(r, _) if r.version != self.skipped => Some(r),
            _ => None,
        }
    }

    /// Downloads and installs in the background.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn install(&mut self) {
        let Status::Available(r) = self.status.clone() else { return };
        let Some(zip) = r.zip.clone() else {
            self.status = Status::Failed("This release has no Mac download; get it from the releases page.".into());
            return;
        };
        self.status = Status::Downloading(r);
        let (tx, rx) = channel();
        self.install_rx = Some(rx);
        let progress = self.progress.clone();
        *progress.lock().unwrap() = "Downloading…".into();
        std::thread::Builder::new()
            .name("phosphor-update".into())
            .spawn(move || {
                let _ = tx.send(download_and_install(&zip, &progress));
            })
            .ok();
    }

    #[cfg(target_arch = "wasm32")]
    pub fn install(&mut self) {}

    pub fn progress(&self) -> String {
        self.progress.lock().unwrap().clone()
    }

    /// Top bar: a button only while an update is waiting.
    pub fn top_bar_button(&mut self, ui: &mut egui::Ui) {
        let text = match &self.status {
            Status::Ready(_) => Some(format!("{} Relaunch to update", icon::ARROW_CLOCKWISE)),
            _ => self.pending().map(|r| format!("{} Update {}", icon::ARROW_CIRCLE_UP, r.version)),
        };
        let text = match &self.status {
            Status::InstallFailed(r, _) if r.version != self.skipped => Some(format!("{} Download {}", icon::DOWNLOAD_SIMPLE, r.version)),
            _ => text,
        };
        if let Some(text) = text
            && ui
                .add(crate::ui::callout(text, crate::theme::accent()))
                .on_hover_text("A new Phosphor is out: click to see what's new and install it")
                .clicked()
        {
            self.window = true;
        }
    }
}

/// The current app bundle, when running from one (…/Phosphor.app/Contents/MacOS/phosphor).
fn current_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let bundle = exe.parent()?.parent()?.parent()?;
    (bundle.extension().is_some_and(|e| e == "app") && bundle.join("Contents/Info.plist").exists()).then(|| bundle.to_path_buf())
}

/// curl → unzip → swap the bundle. Returns (path, installed?).
#[cfg(not(target_arch = "wasm32"))]
fn download_and_install(zip_url: &str, progress: &Arc<Mutex<String>>) -> Result<(PathBuf, bool), String> {
    use std::process::Command;
    let work = std::env::temp_dir().join(format!("phosphor-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let zip = work.join("Phosphor.zip");
    // curl ships with macOS and with Windows 10 and later.
    let out = Command::new("curl").args(["-L", "-f", "-s", "-S", "-o"]).arg(&zip).arg(zip_url).output().map_err(|e| format!("curl: {e}"))?;
    if !out.status.success() {
        return Err(format!("Download failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    *progress.lock().unwrap() = "Unpacking…".into();
    let unpacked = work.join("unpacked");
    std::fs::create_dir_all(&unpacked).map_err(|e| e.to_string())?;
    // ditto on the Mac (keeps the bundle's attributes); Windows' tar reads zips.
    let out = if cfg!(target_os = "macos") {
        Command::new("ditto").args(["-x", "-k"]).arg(&zip).arg(&unpacked).output()
    } else {
        Command::new("tar").arg("-xf").arg(&zip).arg("-C").arg(&unpacked).output()
    }
    .map_err(|e| format!("unpack: {e}"))?;
    if !out.status.success() {
        return Err(format!("Unpack failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    *progress.lock().unwrap() = "Installing…".into();
    let installed = if cfg!(target_os = "macos") {
        install_mac(&unpacked)?
    } else {
        install_folder(&unpacked, &std::env::current_exe().map_err(|e| e.to_string())?)?
    };
    let _ = std::fs::remove_dir_all(&work);
    Ok(installed)
}

/// Mac: swap the .app bundle we run from (or hand back the unpacked app when we don't run from one).
fn install_mac(unpacked: &std::path::Path) -> Result<(PathBuf, bool), String> {
    use std::process::Command;
    let new_app = find_app(unpacked).ok_or("The download has no Phosphor.app in it")?;
    if !new_app.join("Contents/MacOS/phosphor").exists() {
        return Err("The downloaded app is incomplete".into());
    }
    let Some(dest) = current_bundle() else {
        return Ok((new_app, false));
    };
    let old = dest.with_extension("old.app");
    let _ = std::fs::remove_dir_all(&old);
    std::fs::rename(&dest, &old).map_err(|e| format!("Couldn't replace {}: {e}", dest.display()))?;
    let copied = Command::new("ditto").arg(&new_app).arg(&dest).output().map_err(|e| e.to_string())?;
    if !copied.status.success() {
        let _ = std::fs::remove_dir_all(&dest);
        let _ = std::fs::rename(&old, &dest);
        return Err(format!("Install failed: {}", String::from_utf8_lossy(&copied.stderr).trim()));
    }
    let _ = Command::new("xattr").args(["-dr", "com.apple.quarantine"]).arg(&dest).output();
    let _ = std::fs::remove_dir_all(&old);
    Ok((dest, true))
}

/// Windows (and other folder installs): copy the new files over the folder Phosphor.exe runs from. Files in use
/// (the exe, FFmpeg's DLLs) can't be overwritten but can be renamed, so each goes to "<name>.old" first; the next
/// update clears those.
fn install_folder(unpacked: &std::path::Path, exe: &std::path::Path) -> Result<(PathBuf, bool), String> {
    let exe_name = if cfg!(windows) { "Phosphor.exe" } else { "phosphor" };
    let src = find_file(unpacked, exe_name).and_then(|p| p.parent().map(|d| d.to_path_buf())).ok_or("The download has no Phosphor.exe in it")?;
    let exe = exe.to_path_buf();
    let dest = exe.parent().ok_or("Can't tell where Phosphor is installed")?.to_path_buf();
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    let undo = |moved: &[(PathBuf, PathBuf)]| {
        for (target, old) in moved.iter().rev() {
            let _ = std::fs::remove_file(target);
            let _ = std::fs::rename(old, target);
        }
    };
    for entry in std::fs::read_dir(&src).map_err(|e| e.to_string())?.flatten() {
        let from = entry.path();
        if !from.is_file() {
            continue;
        }
        let name = entry.file_name();
        let target = if name.to_string_lossy().eq_ignore_ascii_case(exe_name) { exe.clone() } else { dest.join(&name) };
        if target.exists() {
            let old = PathBuf::from(format!("{}.old", target.display()));
            let _ = std::fs::remove_file(&old);
            if let Err(e) = std::fs::rename(&target, &old) {
                undo(&moved);
                return Err(format!("Couldn't replace {}: {e}", target.display()));
            }
            moved.push((target.clone(), old));
        }
        if let Err(e) = std::fs::copy(&from, &target) {
            undo(&moved);
            return Err(format!("Install failed at {}: {e}", target.display()));
        }
    }
    Ok((exe, true))
}

fn find_file(dir: &std::path::Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        if p.is_file() && p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name)) {
            return Some(p);
        }
        if p.is_dir()
            && let Some(found) = find_file(&p, name)
        {
            return Some(found);
        }
    }
    None
}

fn find_app(dir: &std::path::Path) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "app") {
            return Some(p);
        }
        if p.is_dir()
            && let Some(found) = find_app(&p)
        {
            return Some(found);
        }
    }
    None
}

/// Quits and opens the (new) app a moment later.
#[cfg(not(target_arch = "wasm32"))]
pub fn relaunch(ctx: &egui::Context, app: &std::path::Path) {
    let path = app.to_string_lossy().to_string();
    if cfg!(windows) {
        // ping is cmd's portable "wait a moment".
        let _ = std::process::Command::new("cmd").args(["/C", &format!("ping -n 3 127.0.0.1 >nul & start \"\" \"{path}\"")]).spawn();
    } else {
        let _ = std::process::Command::new("sh").arg("-c").arg(format!("sleep 1.5; open \"{path}\"")).spawn();
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
}

/// The Update window (Help › Check for Updates, or the top bar button).
pub fn update_window(ctx: &egui::Context, up: &mut Updater) {
    let (mut open, mut detached) = (up.window, up.detached);
    crate::ui::tool_window(ctx, &format!("{} Update Phosphor", icon::ARROW_CIRCLE_UP), &mut open, &mut detached, 440.0, |ui| {
        ui.label(format!("You have Phosphor {}.", env!("CARGO_PKG_VERSION")));
        crate::ui::toggle(ui, &mut up.auto, "Install updates by themselves").on_hover_text("Phosphor checks when it opens and every hour; a new version downloads and installs in the background, and opens when you relaunch");
        match up.status.clone() {
            Status::Idle => {
                if crate::ui::command(ui, "Check now").clicked() {
                    up.check(ctx);
                }
            }
            Status::Checking => {
                crate::ui::toolbar(ui, |ui| {
                    ui.spinner();
                    ui.label("Checking…");
                });
            }
            Status::UpToDate => {
                ui.colored_label(crate::theme::accent(), "You're up to date.");
                if crate::ui::command(ui, "Check again").clicked() {
                    up.check(ctx);
                }
            }
            Status::Available(r) => {
                ui.colored_label(crate::theme::accent(), format!("Phosphor {} is out.", r.version));
                if !r.notes.trim().is_empty() {
                    crate::ui::column_scroll(ui).max_height(220.0).show(ui, |ui| {
                        ui.add(egui::Label::new(egui::RichText::new(r.notes.trim()).small()).wrap());
                    });
                }
                crate::ui::toolbar(ui, |ui| {
                    #[cfg(not(target_arch = "wasm32"))]
                    if crate::ui::command(ui, format!("{} Download and install", icon::DOWNLOAD_SIMPLE)).clicked() {
                        up.install();
                    }
                    if crate::ui::command(ui, "Release page").clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab(r.page.clone()));
                    }
                    if crate::ui::command(ui, "Skip this version").clicked() {
                        up.skipped = r.version.clone();
                        up.window = false;
                    }
                });
                crate::ui::hint(ui, "Your projects, layouts and settings stay as they are.");
            }
            Status::Downloading(r) => {
                crate::ui::toolbar(ui, |ui| {
                    ui.spinner();
                    ui.label(format!("{} Phosphor {}", up.progress(), r.version));
                });
            }
            Status::Ready(path) => {
                ui.colored_label(crate::theme::accent(), "The new Phosphor is installed. Relaunch to start using it.");
                #[cfg(not(target_arch = "wasm32"))]
                if crate::ui::command(ui, format!("{} Relaunch now", icon::ARROW_CLOCKWISE)).clicked() {
                    relaunch(ctx, &path);
                }
                #[cfg(target_arch = "wasm32")]
                let _ = path;
                crate::ui::hint(ui, "Or keep playing; the new version opens next time.");
            }
            Status::Downloaded(path) => {
                ui.label("Downloaded. Phosphor isn't running from an app bundle, so drag the new app to Applications yourself.");
                #[cfg(not(target_arch = "wasm32"))]
                if crate::ui::command(ui, "Show in Finder").clicked() {
                    crate::os::reveal(&path);
                }
                #[cfg(target_arch = "wasm32")]
                let _ = path;
            }
            Status::InstallFailed(r, e) => {
                ui.colored_label(crate::theme::accent(), format!("Phosphor {} is out, but it couldn't install itself here.", r.version));
                crate::ui::hint(ui, e);
                ui.add_space(4.0);
                ui.label("Download it and install it by hand, it takes a minute:");
                ui.label("1. Download Phosphor (button below) and open the zip in Downloads.");
                ui.label("2. Quit this Phosphor, drag the new Phosphor into Applications and choose Replace.");
                ui.label("3. Open it. If macOS warns, go to System Settings > Privacy & Security and click Open Anyway.");
                crate::ui::toolbar(ui, |ui| {
                    let url = r.zip.clone().unwrap_or_else(|| r.page.clone());
                    if crate::ui::command(ui, egui::RichText::new(format!("{} Download Phosphor {}", icon::DOWNLOAD_SIMPLE, r.version)).color(crate::theme::accent())).clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab(url));
                    }
                    if crate::ui::command(ui, "Release page").clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab(r.page.clone()));
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    if crate::ui::command(ui, "Try again").clicked() {
                        up.status = Status::Available(r.clone());
                        up.install();
                    }
                });
                crate::ui::hint(ui, "Your projects, layouts and settings stay as they are.");
            }
            Status::Failed(e) => {
                ui.colored_label(crate::theme::WARN, e);
                crate::ui::toolbar(ui, |ui| {
                    if crate::ui::command(ui, "Try again").clicked() {
                        up.check(ctx);
                    }
                    if crate::ui::command(ui, "Release page").clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab(RELEASES_PAGE));
                    }
                });
            }
        }
    });
    up.window = open;
    up.detached = detached;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("v0.0.9", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }

    #[test]
    fn folder_install_keeps_the_old_files_aside() {
        let root = std::env::temp_dir().join(format!("phosphor-install-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (unpacked, dest) = (root.join("unpacked/Phosphor"), root.join("installed"));
        std::fs::create_dir_all(&unpacked).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        let exe_name = if cfg!(windows) { "Phosphor.exe" } else { "phosphor" };
        std::fs::write(unpacked.join(exe_name), "new exe").unwrap();
        std::fs::write(unpacked.join("avcodec.dll"), "new dll").unwrap();
        // Installed under a lower-case name (cargo's), the way a dev build runs.
        let exe = dest.join(exe_name.to_lowercase());
        std::fs::write(&exe, "old exe").unwrap();
        std::fs::write(dest.join("avcodec.dll"), "old dll").unwrap();
        let (path, swapped) = super::install_folder(&root.join("unpacked"), &exe).unwrap();
        assert!(swapped && path == exe);
        assert_eq!(std::fs::read_to_string(&exe).unwrap(), "new exe");
        assert_eq!(std::fs::read_to_string(dest.join("avcodec.dll")).unwrap(), "new dll");
        assert_eq!(std::fs::read_to_string(format!("{}.old", exe.display())).unwrap(), "old exe");
        // A second update replaces the leftovers of the first.
        assert!(super::install_folder(&root.join("unpacked"), &exe).is_ok());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn version_from_the_latest_redirect() {
        assert_eq!(version_from_url("https://github.com/fillinthe/phosphor/releases/tag/v0.2.42").as_deref(), Some("0.2.42"));
        assert_eq!(version_from_url("https://github.com/fillinthe/phosphor/releases"), None);
        assert_eq!(zip_url("0.2.42"), format!("https://github.com/fillinthe/phosphor/releases/download/v0.2.42/{ZIP_NAME}"));
    }

    #[test]
    fn release_json() {
        let json = format!(
            r#"{{"tag_name":"v0.3.0","body":"notes","html_url":"https://x/y","assets":[{{"name":"Phosphor-mac-intel.zip","browser_download_url":"https://x/intel.zip"}},{{"name":"Phosphor-mac.zip","browser_download_url":"https://x/arm.zip"}},{{"name":"Phosphor-windows.zip","browser_download_url":"https://x/win.zip"}}]}}"#
        );
        let r = parse_release(&json).unwrap().unwrap();
        assert_eq!(r.version, "0.3.0");
        let want = match ZIP_NAME {
            "Phosphor-mac-intel.zip" => "https://x/intel.zip",
            "Phosphor-mac.zip" => "https://x/arm.zip",
            _ => "https://x/win.zip",
        };
        assert_eq!(r.zip.as_deref(), Some(want), "this build's own download");
        let only_arm = r#"{"tag_name":"v0.3.1","assets":[{"name":"Phosphor-mac.zip","browser_download_url":"https://x/arm.zip"}]}"#;
        assert_eq!(parse_release(only_arm).unwrap().unwrap().zip.is_some(), ZIP_NAME == "Phosphor-mac.zip", "never another build's zip");
        assert!(parse_release(r#"{"message":"Not Found"}"#).unwrap().is_none());
    }
}
