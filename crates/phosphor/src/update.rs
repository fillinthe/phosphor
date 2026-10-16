//! In-app updates: checks GitHub releases (once a day and on request), downloads the new app, swaps it in
//! place and relaunches. `PHOSPHOR_UPDATE_URL` points the check elsewhere (tests use a local server).

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};

use eframe::egui;
use egui_phosphor::regular as icon;

pub const RELEASES_API: &str = "https://api.github.com/repos/fillinthe/phosphor/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/fillinthe/phosphor/releases";
const CHECK_EVERY: f64 = 24.0 * 3600.0;

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
    Failed(String),
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
}

impl Default for Updater {
    fn default() -> Self {
        Self { status: Status::Idle, window: false, detached: false, skipped: String::new(), next_check: 8.0, check_rx: None, install_rx: None, progress: Default::default(), test: false }
    }
}

impl Updater {
    pub fn with_skipped(skipped: String) -> Self {
        Self { skipped, ..Default::default() }
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
            let zips: Vec<&serde_json::Value> = a.iter().filter(|x| x.get("name").and_then(|n| n.as_str()).is_some_and(|n| n.ends_with(".zip"))).collect();
            zips.iter().find(|x| x.get("name").and_then(|n| n.as_str()).is_some_and(|n| n.to_lowercase().contains("mac"))).or(zips.first()).copied()
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
        let url = std::env::var("PHOSPHOR_UPDATE_URL").unwrap_or_else(|_| RELEASES_API.into());
        let (tx, rx) = channel();
        self.check_rx = Some(rx);
        let ctx = ctx.clone();
        let mut req = ehttp::Request::get(url);
        req.headers = ehttp::Headers::new(&[("User-Agent", "Phosphor"), ("Accept", "application/vnd.github+json")]);
        ehttp::fetch(req, move |res| {
            let out = match res {
                Ok(r) if r.ok => parse_release(r.text().unwrap_or("")),
                Ok(r) if r.status == 404 => Ok(None),
                Ok(r) => Err(format!("Couldn't check for updates (HTTP {}). Try again later.", r.status)),
                Err(_) => Err("Couldn't reach the update server. Check the internet connection and try again.".into()),
            };
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
            self.status = match res {
                Ok(Some(r)) if is_newer(&r.version, env!("CARGO_PKG_VERSION")) => Status::Available(r),
                Ok(_) => Status::UpToDate,
                Err(e) => Status::Failed(e),
            };
            if self.test && let Status::Available(_) = &self.status {
                self.install();
            }
        }
        if let Some(rx) = &self.install_rx
            && let Ok(res) = rx.try_recv()
        {
            self.install_rx = None;
            self.status = match res {
                Ok((path, true)) => Status::Ready(path),
                Ok((path, false)) => Status::Downloaded(path),
                Err(e) => Status::Failed(e),
            };
            if self.test {
                match &self.status {
                    Status::Ready(p) | Status::Downloaded(p) => println!("UPDATE_READY {}", p.display()),
                    Status::Failed(e) => println!("UPDATE_FAILED {e}"),
                    _ => {}
                }
            }
            ctx.request_repaint();
        }
    }

    /// The update is worth a button in the top bar (not skipped, not dismissed as up to date).
    pub fn pending(&self) -> Option<&Release> {
        match &self.status {
            Status::Available(r) | Status::Downloading(r) if r.version != self.skipped => Some(r),
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
        if let Some(text) = text
            && ui.button(egui::RichText::new(text).color(crate::theme::ACCENT)).on_hover_text("A new Phosphor is out").clicked()
        {
            self.window = true;
        }
    }
}

/// The current app bundle, when running from one (…/Phosphor.app/Contents/MacOS/phosphor).
#[cfg(not(target_arch = "wasm32"))]
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
    let out = Command::new("curl").args(["-L", "-f", "-s", "-S", "-o"]).arg(&zip).arg(zip_url).output().map_err(|e| format!("curl: {e}"))?;
    if !out.status.success() {
        return Err(format!("Download failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    *progress.lock().unwrap() = "Unpacking…".into();
    let unpacked = work.join("unpacked");
    let out = Command::new("ditto").args(["-x", "-k"]).arg(&zip).arg(&unpacked).output().map_err(|e| format!("ditto: {e}"))?;
    if !out.status.success() {
        return Err(format!("Unpack failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let new_app = find_app(&unpacked).ok_or("The download has no Phosphor.app in it")?;
    if !new_app.join("Contents/MacOS/phosphor").exists() {
        return Err("The downloaded app is incomplete".into());
    }
    let Some(dest) = current_bundle() else {
        return Ok((new_app, false));
    };
    *progress.lock().unwrap() = "Installing…".into();
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
    let _ = std::fs::remove_dir_all(&work);
    Ok((dest, true))
}

#[cfg(not(target_arch = "wasm32"))]
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
    let _ = std::process::Command::new("sh").arg("-c").arg(format!("sleep 1.5; open \"{path}\"")).spawn();
    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
}

/// The Update window (Help › Check for Updates, or the top bar button).
pub fn update_window(ctx: &egui::Context, up: &mut Updater) {
    let (mut open, mut detached) = (up.window, up.detached);
    crate::ui::tool_window(ctx, &format!("{} Update Phosphor", icon::ARROW_CIRCLE_UP), &mut open, &mut detached, 440.0, |ui| {
        ui.label(format!("You have Phosphor {}.", env!("CARGO_PKG_VERSION")));
        match up.status.clone() {
            Status::Idle => {
                if ui.button("Check now").clicked() {
                    up.check(ctx);
                }
            }
            Status::Checking => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Checking…");
                });
            }
            Status::UpToDate => {
                ui.colored_label(crate::theme::ACCENT, "You're up to date.");
                if ui.button("Check again").clicked() {
                    up.check(ctx);
                }
            }
            Status::Available(r) => {
                ui.colored_label(crate::theme::ACCENT, format!("Phosphor {} is out.", r.version));
                if !r.notes.trim().is_empty() {
                    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                        ui.add(egui::Label::new(egui::RichText::new(r.notes.trim()).small()).wrap());
                    });
                }
                ui.horizontal(|ui| {
                    #[cfg(not(target_arch = "wasm32"))]
                    if ui.button(format!("{} Download and install", icon::DOWNLOAD_SIMPLE)).clicked() {
                        up.install();
                    }
                    if ui.button("Release page").clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab(r.page.clone()));
                    }
                    if ui.button("Skip this version").clicked() {
                        up.skipped = r.version.clone();
                        up.window = false;
                    }
                });
                crate::ui::hint(ui, "Your projects, layouts and settings stay as they are.");
            }
            Status::Downloading(r) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!("{} Phosphor {}", up.progress(), r.version));
                });
            }
            Status::Ready(path) => {
                ui.colored_label(crate::theme::ACCENT, "Installed. Relaunch to start using it.");
                #[cfg(not(target_arch = "wasm32"))]
                if ui.button(format!("{} Relaunch now", icon::ARROW_CLOCKWISE)).clicked() {
                    relaunch(ctx, &path);
                }
                #[cfg(target_arch = "wasm32")]
                let _ = path;
                crate::ui::hint(ui, "Or keep playing; the new version opens next time.");
            }
            Status::Downloaded(path) => {
                ui.label("Downloaded. Phosphor isn't running from an app bundle, so drag the new app to Applications yourself.");
                #[cfg(not(target_arch = "wasm32"))]
                if ui.button("Show in Finder").clicked() {
                    let _ = std::process::Command::new("open").arg("-R").arg(&path).spawn();
                }
                #[cfg(target_arch = "wasm32")]
                let _ = path;
            }
            Status::Failed(e) => {
                ui.colored_label(crate::theme::WARN, e);
                ui.horizontal(|ui| {
                    if ui.button("Try again").clicked() {
                        up.check(ctx);
                    }
                    if ui.button("Release page").clicked() {
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
    fn release_json() {
        let r = parse_release(r#"{"tag_name":"v0.3.0","body":"notes","html_url":"https://x/y","assets":[{"name":"Phosphor-mac.zip","browser_download_url":"https://x/z.zip"}]}"#).unwrap().unwrap();
        assert_eq!(r.version, "0.3.0");
        assert_eq!(r.zip.as_deref(), Some("https://x/z.zip"));
        assert!(parse_release(r#"{"message":"Not Found"}"#).unwrap().is_none());
    }
}
