//! Support: the Donate button (every Phosphor surface links to the same page) and the in-app bug report
//! (description + diagnostics → a prefilled GitHub issue, or copied to the clipboard).

use eframe::egui;
use egui_phosphor::regular as icon;

use crate::app::State;
use phosphor_engine::SourceSpec;

pub const DONATE_URL: &str = "https://donate.stripe.com/eVqdRb8kv3XW08LcNaaEE04";
pub const ISSUES_URL: &str = "https://github.com/fillinthe/phosphor/issues/new";

/// Help › User Manual: the manual on the website, in the language the app shows (the site keeps a folder per
/// language: /docs/ for English, /es/docs/, /pt/docs/, /zh/docs/…).
pub fn manual_url() -> String {
    manual_url_for(crate::i18n::chosen().resolved().code())
}

fn manual_url_for(code: &str) -> String {
    match code.split('-').next().unwrap_or("en").to_ascii_lowercase().as_str() {
        "en" | "auto" | "" => "https://phosphorfx.com/docs/".to_string(),
        lang => format!("https://phosphorfx.com/{lang}/docs/"),
    }
}

#[cfg(test)]
mod manual_tests {
    #[test]
    fn the_manual_opens_in_the_apps_language() {
        use super::manual_url_for as url;
        assert_eq!(url("en"), "https://phosphorfx.com/docs/");
        assert_eq!((url("pt-BR"), url("zh-CN"), url("de")), ("https://phosphorfx.com/pt/docs/".into(), "https://phosphorfx.com/zh/docs/".into(), "https://phosphorfx.com/de/docs/".into()));
    }
}

/// Donate's orange, the same as GCarve's (the one orange in either app).
pub const DONATE_ORANGE: egui::Color32 = crate::theme::DONATE;

/// Heart button that opens the donation page (top bar, menus): filled orange with dark text, like GCarve's.
/// The top bar's Donate button is on show (the user can hide it; kept in the settings).
static DONATE_SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn donate_shown() -> bool {
    DONATE_SHOWN.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn set_donate_shown(on: bool) {
    DONATE_SHOWN.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn donate_button(ui: &mut egui::Ui) {
    if !donate_shown() {
        return;
    }
    // Anyone may put it away for good: an x inside the orange button, at its right end (and the same in its
    // right-click menu). Donating stays in the Help menu, and the brightness menu's Donate button switch brings the
    // button back.
    let (r, closed) = crate::ui::callout_closable(
        ui,
        format!("{} Donate", icon::HEART),
        DONATE_ORANGE,
        "Phosphor is free and open source. A donation keeps it going. The x at its end hides this button",
        "Hide the Donate button. Donating stays in the Help menu; the brightness menu (the sun) has a switch to bring the button back",
    );
    if closed {
        set_donate_shown(false);
        crate::ui::notice::tell(ui.ctx(), "Donate button hidden: Help › Donate to Phosphor… is always there, and the brightness menu brings the button back".into());
    }
    if r.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(DONATE_URL));
    }
    use crate::ui::RightMenu;
    r.right_menu(|ui| {
        if crate::ui::command(ui, crate::ui::label(icon::EYE_SLASH, "Hide this button")).on_hover_text("It won't show again. Donating stays in the Help menu; the brightness menu (the sun) has a switch to bring the button back").clicked() {
            set_donate_shown(false);
            ui.close();
        }
    });
}

/// The Report a Bug window's state.
#[derive(Default)]
pub struct BugReport {
    pub open: bool,
    /// Popped out into its own OS window.
    pub detached: bool,
    pub text: String,
    pub include_diagnostics: bool,
    /// "Send problem reports" (on unless turned off): a report goes by itself after a crash or freeze, and every
    /// report leaves its one line (with the account when signed in) on the account server's Problems list.
    pub send_problems: bool,
    /// What happened to the last automatic report, for the banner.
    pub sent: std::sync::Arc<std::sync::Mutex<Option<Result<(), String>>>>,
    /// What happened to the report sent from the window (None = none sent / sending).
    pub bug_sent: std::sync::Arc<std::sync::Mutex<Option<Result<(), String>>>>,
    pub sending: bool,
    /// macOS version, read once.
    os: Option<String>,
    /// The cameras and capture cards this computer has (looked up once, on a thread: listing can take a moment).
    cameras: std::sync::Arc<std::sync::Mutex<Option<Vec<String>>>>,
    cameras_asked: bool,
    /// "Delete my reports": what the last request did (None = none yet / still going), and whether one is going.
    pub deleted: std::sync::Arc<std::sync::Mutex<Option<Result<usize, String>>>>,
    pub deleting: bool,
    /// The one-time notice about what Phosphor sends by itself has been seen (saved).
    pub notice_seen: bool,
    /// A crash report held back because the notice hasn't been answered yet: sent on "Keep sharing", dropped on "Turn off".
    pub held_report: bool,
    /// The diagnostics shown in the window, and when they were put together (redone every few seconds, not each frame).
    shown: Option<(f64, String)>,
}

impl BugReport {
    /// `send_problems`: the saved "Send problem reports" choice.
    pub fn new(send_problems: bool, notice_seen: bool) -> Self {
        // Nothing goes by itself before the notice has been answered.
        crate::crash_report::set_sharing(send_problems && notice_seen);
        Self { include_diagnostics: true, send_problems, notice_seen, ..Default::default() }
    }
}

/// Everything a bug report should carry besides the words: version, system, project shape, the end of crash.log.
/// The end of `text`, at most `max` bytes (never cutting a character in half).
fn tail_of(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut from = text.len() - max;
    while !text.is_char_boundary(from) {
        from += 1;
    }
    &text[from..]
}

/// A log line for a crash report sent by itself: only what kind of thing happened, no file paths or names (they're
/// cut at the first path, quote or arrow).
fn without_names(line: &str) -> &str {
    let cut = line.find(['/', '"', '\'', '«', '“', '\u{2190}', '\u{2192}']).unwrap_or(line.len());
    line[..cut].trim_end()
}

/// Text for a crash report sent by itself with the user's name taken out of paths: the home folder becomes "~", and
/// any other "/Users/<name>", "/home/<name>" or "C:\\Users\\<name>" becomes "~" too (the crash log's panic and
/// stack lines hold paths).
fn without_user(text: &str, home: Option<&str>) -> String {
    let mut out = text.to_string();
    if let Some(h) = home.map(|h| h.trim_end_matches(['/', '\\'])).filter(|h| h.len() > 1) {
        // Only the whole folder name ("/Users/sam", not the start of "/Users/samuel").
        let mut done = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(i) = rest.find(h) {
            let after = &rest[i + h.len()..];
            done.push_str(&rest[..i]);
            if after.chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.') {
                done.push_str(h);
            } else {
                done.push('~');
            }
            rest = after;
        }
        done.push_str(rest);
        out = done;
    }
    for root in ["/Users/", "/home/", "\\Users\\", "/users/"] {
        let mut done = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(i) = rest.find(root) {
            let after = &rest[i + root.len()..];
            // The name runs to the next separator (or the end).
            let name_len = after.find(['/', '\\', '\n', ' ', '"', '\'', ':', ')']).unwrap_or(after.len());
            if name_len == 0 || after[..name_len].eq_ignore_ascii_case("shared") {
                done.push_str(&rest[..i + root.len()]);
                rest = after;
                continue;
            }
            // A drive letter before "\Users\" (C:) goes too.
            let drive = i >= 2 && rest.as_bytes()[i - 1] == b':' && rest.as_bytes()[i - 2].is_ascii_alphabetic();
            let start = if root.starts_with('\\') && drive { i - 2 } else { i };
            done.push_str(&rest[..start]);
            done.push('~');
            rest = &after[name_len..];
        }
        done.push_str(rest);
        out = done;
    }
    out
}

/// The home folder, for `without_user`.
fn home_dir() -> Option<String> {
    std::env::var("HOME").ok().or_else(|| std::env::var("USERPROFILE").ok())
}

/// The diagnostics for a crash report sent by itself (after a relaunch): the run that crashed, nothing named.
pub fn crash_diagnostics(st: &mut State) -> String {
    diagnostics_from(st, true)
}

/// The Report a Bug window's diagnostics, put together at most every 2 s (it reads files).
pub fn shown_diagnostics(st: &mut State, now: f64) -> String {
    if let Some((at, text)) = &st.bug.shown
        && now - at < 2.0
    {
        return text.clone();
    }
    let text = diagnostics(st);
    st.bug.shown = Some((now, text.clone()));
    text
}

/// "Cameras and capture cards: Cam Link 4K, FaceTime HD Camera" (what Phosphor can see, for "my card doesn't show").
#[cfg(not(target_arch = "wasm32"))]
fn cameras_line(st: &mut State) -> String {
    if !st.bug.cameras_asked {
        st.bug.cameras_asked = true;
        let found = st.bug.cameras.clone();
        let _ = std::thread::Builder::new().name("bug-cameras".into()).spawn(move || {
            let names: Vec<String> = crate::media::live::cameras_when_settled(10.0).into_iter().map(|c| c.name).collect();
            *found.lock().unwrap() = Some(names);
        });
    }
    match st.bug.cameras.lock().unwrap().as_deref() {
        None => "Cameras and capture cards: looking…\n".into(),
        Some([]) => "Cameras and capture cards: none found\n".into(),
        Some(names) => format!("Cameras and capture cards: {}\n", names.join(", ")),
    }
}

/// The "Send problem reports" switch (Report a Bug window and the settings menu).
pub fn problem_reports_toggle(ui: &mut egui::Ui, st: &mut State) -> egui::Response {
    let r = crate::ui::toggle(ui, &mut st.bug.send_problems, "Share problem reports and anonymous counts").on_hover_text(SHARING_HELP);
    if r.changed() {
        crate::crash_report::set_sharing(st.bug.send_problems && st.bug.notice_seen);
    }
    r
}

/// What Phosphor sends by itself, in plain words (the notice, the Privacy section and the switch's tooltip).
const SHARING_HELP: &str = "To fix problems faster, Phosphor sends a report to its team by itself when it crashes or freezes: the version, your system, the project's shape (counts, no content) and what happened just before (no file, layer or device names). If you're signed in to G Workshop, the report is linked to your account so we can follow up. When an update downloads, it also sends an anonymous count (just the version numbers). Reports are kept 30 days.";

/// The Privacy section (settings menu): the switch, what it means, and "Delete my reports".
pub fn privacy_section(ui: &mut egui::Ui, st: &mut State) {
    crate::ui::heading_help(ui, "Privacy", SHARING_HELP);
    problem_reports_toggle(ui, st);
    crate::ui::toolbar(ui, |ui| {
        if crate::ui::command_if(ui, !st.bug.deleting, crate::ui::label(icon::TRASH, "Delete my reports"))
            .on_hover_text("Deletes every report this computer sent from the Phosphor report server. Reports linked to your G Workshop account: write to support@gworkshop.app.")
            .clicked()
        {
            st.bug.deleting = true;
            let out = st.bug.deleted.clone();
            crate::crash_report::delete_my_reports(move |r| {
                *out.lock().unwrap() = Some(r);
            });
        }
        if st.bug.deleting {
            ui.spinner();
        }
    });
}

/// Once (first start, and once after the update that brought it): what Phosphor sends by itself, with Keep sharing /
/// Turn off. Drawn app-level every frame; nothing while the bench runs.
pub fn privacy_notice(ctx: &egui::Context, st: &mut State, bench: bool) {
    // "Delete my reports" answered (heard here so the message shows even after the menu closed).
    if st.bug.deleting
        && let Some(r) = st.bug.deleted.lock().unwrap().take()
    {
        st.bug.deleting = false;
        st.notice = Some(match r {
            Ok(0) => "There were no reports from this computer to delete.".into(),
            Ok(n) => crate::ui::plural(n, "Deleted {0} report from this computer.", "Deleted {0} reports from this computer."),
            Err(e) => format!("Couldn't delete the reports ({e}). Try again later."),
        });
    }
    // (Bench pictures: PHOSPHOR_PRIVACY_NOTICE=1 shows it.)
    if st.bug.notice_seen || bench && std::env::var_os("PHOSPHOR_PRIVACY_NOTICE").is_none() {
        return;
    }
    let (mut keep, mut off) = (false, false);
    crate::ui::dialog(ctx, crate::ui::label(icon::SHIELD_CHECK, "Problem reports")).id(egui::Id::new("privacy_notice")).resizable(false).collapsible(false).default_width(460.0).show(ctx, |ui| {
        ui.add(egui::Label::new(SHARING_HELP).wrap());
        crate::ui::hint(ui, "Change it any time, or delete your reports: the brightness (sun) button at the top › Privacy.");
        crate::ui::toolbar(ui, |ui| {
            keep = crate::ui::action_button(ui, crate::ui::Act::Go, "Keep sharing").clicked();
            off = crate::ui::command(ui, "Turn off").clicked();
        });
    });
    if keep || off {
        st.bug.send_problems = keep;
        crate::crash_report::set_sharing(keep);
        st.bug.notice_seen = true;
        // The crash report that waited for this answer.
        if std::mem::take(&mut st.bug.held_report) && keep {
            send_crash_report(st);
        }
    }
}

/// Sends the report of the unexpected quit Phosphor just came back from (by itself: "Share problem reports" is on).
#[cfg(not(target_arch = "wasm32"))]
pub fn send_crash_report(st: &mut State) {
    let report = crash_diagnostics(st);
    crate::recovery::mark_crash_log_reported();
    let sent = st.bug.sent.clone();
    crate::crash_report::send(report, Some(st.account.reporter()), move |r| {
        *sent.lock().unwrap() = Some(r);
    });
}

#[cfg(target_arch = "wasm32")]
pub fn send_crash_report(_st: &mut State) {}

/// Bench: starts the camera lookup and waits (up to `secs`) until it's in, so a snapshot lists them.
#[cfg(not(target_arch = "wasm32"))]
pub fn wait_for_cameras(st: &mut State, secs: f64) {
    let _ = cameras_line(st);
    let until = std::time::Instant::now() + std::time::Duration::from_secs_f64(secs);
    while st.bug.cameras.lock().unwrap().is_none() && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

pub fn diagnostics(st: &mut State) -> String {
    diagnostics_from(st, false)
}

fn diagnostics_from(st: &mut State, crash: bool) -> String {
    let out = diagnostics_named(st, crash);
    if crash { without_user(&out, home_dir().as_deref()) } else { out }
}

fn diagnostics_named(st: &mut State, crash: bool) -> String {
    let os = st.bug.os.get_or_insert_with(os_version).clone();
    let s = st.stats.latest;
    let comp = &st.comp;
    let effects: usize = comp.layers.iter().map(|l| l.effects.len()).sum::<usize>() + comp.master_effects.len();
    let mut out = format!(
        "Phosphor {} · {os} {}\n{:.0} fps · engine {:.2} ms · CPU {:.0}% · {:.0} MB\nProject: {}×{}, {} layers, {} groups, {} effects, {} outputs, {} clips/row; session {}; stream {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::ARCH,
        s.fps,
        s.render_ms,
        s.cpu_percent,
        s.memory_mb,
        comp.width,
        comp.height,
        comp.layers.len(),
        comp.groups.len(),
        effects,
        comp.outputs.len(),
        comp.clip_columns,
        if st.net.running() { "on" } else { "off" },
        if st.stream.running() { "live" } else { "off" },
    );
    out.push_str(&format!("GPU {:.2} ms a frame · longest gap {:.0} ms · {} passes\n", s.gpu_ms, s.worst_ms, s.passes));
    out.push_str(&gpu_lines(st));
    if let Some(mb) = st.stats.gpu_mb {
        out.push_str(&format!("Graphics memory: {mb:.0} MB\n"));
    }
    out.push_str(&effects_line(comp.all_effects().map(|e| e.effect.as_str())));
    out.push_str(&movies_line(st));
    // (Device names can carry a person's name, "Jane's iPhone Camera": only in reports people write and see first.)
    #[cfg(not(target_arch = "wasm32"))]
    if !crash {
        out.push_str(&cameras_line(st));
    }
    #[cfg(not(target_arch = "wasm32"))]
    // (A crash report carries only what crash.log gained since the last one: never an old crash again.)
    if let Some(log) = if crash { Some(crate::recovery::unreported_crash_log()) } else { crate::recovery::crash_log_path().and_then(|p| std::fs::read_to_string(p).ok()) } {
        let tail: Vec<&str> = log.lines().rev().take(40).collect();
        let tail: String = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
        let tail = tail_of(&tail, 3000).to_string();
        if !tail.trim().is_empty() {
            out.push_str("\ncrash.log (end):\n");
            out.push_str(&tail);
        } else if crash {
            out.push_str("\nNothing new in crash.log: this run ended without a panic or a freeze being logged.\n");
        }
    }
    // What happened just before: the end of this run's session log (what was played, effects added, outputs, slow
    // frames, warnings), without the lines that only say a movie opened.
    // (After a crash, Phosphor has been relaunched: the run that crashed is the previous log.)
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = crate::session_log::path().map(|p| if crash { p.with_file_name("session-previous.log") } else { p })
        && let Ok(log) = std::fs::read_to_string(&path)
    {
        let tail = recent_activity(&log, 60);
        let tail = if crash { tail.lines().map(without_names).collect::<Vec<_>>().join("\n") } else { tail };
        if !tail.trim().is_empty() {
            out.push_str("\nsession log (recent):\n");
            out.push_str(&tail);
            out.push('\n');
        }
    }
    out
}

/// The graphics card drawing the picture, and the others this computer has (a report of low frame rates on a
/// laptop is often the wrong card).
fn gpu_lines(st: &State) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(rs) = &st.render_state {
        let about = |i: eframe::wgpu::AdapterInfo| {
            let driver = format!("{} {}", i.driver, i.driver_info);
            let driver = if driver.trim().is_empty() { String::new() } else { format!(", driver {}", driver.trim()) };
            format!("{} ({:?}, {:?}{driver})", i.name, i.backend, i.device_type)
        };
        let now = rs.adapter.get_info();
        let others: Vec<String> = rs.available_adapters.iter().map(|a| a.get_info()).filter(|i| i.name != now.name || i.backend != now.backend).map(about).collect();
        let others = if others.is_empty() { String::new() } else { format!("Other cards: {}\n", others.join("; ")) };
        return format!("Graphics card: {}\n{others}", about(now));
    }
    let _ = st;
    String::new()
}

/// Which effects the project uses ("Effects: blur ×2, isf:bad_tv"): a heavy one is often why a weak graphics card
/// runs out of memory. A shader of the person's own is only counted (its name is theirs).
fn effects_line<'a>(ids: impl Iterator<Item = &'a str>) -> String {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for id in ids {
        let id = if id.starts_with(phosphor_engine::shaders::CUSTOM_EFFECT) { "own shader" } else { id };
        match counts.iter_mut().find(|(n, _)| n == id) {
            Some((_, k)) => *k += 1,
            None => counts.push((id.to_string(), 1)),
        }
    }
    if counts.is_empty() {
        return String::new();
    }
    let words: Vec<String> = counts.into_iter().map(|(n, k)| if k > 1 { format!("{n} ×{k}") } else { n }).collect();
    format!("Effects: {}\n", words.join(", "))
}

/// How the movies playing now are decoded: on the graphics card or on the processor (many big movies on the
/// processor is the usual reason for a low frame rate).
fn movies_line(st: &State) -> String {
    let (mut card, mut cpu) = (0, 0);
    for l in &st.comp.layers {
        match st.core.lock().media.video_info(l.id) {
            Some(i) if i.contains("hardware decode") => card += 1,
            Some(i) if i.contains("software decode") => cpu += 1,
            _ => {}
        }
    }
    if card + cpu == 0 { String::new() } else { format!("Movies playing: {card} decoded on the graphics card, {cpu} on the processor\n") }
}

/// Everything about the program right now, as one text file a person can attach to a report: the diagnostics, every
/// layer (what it plays, how its movie decodes, its effects), the outputs, this run's whole session log and the
/// project itself. It names files and layers, so it's only ever saved where the user says (never sent by itself).
pub fn snapshot_text(st: &mut State) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    let when = crate::backups::stamp(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()));
    #[cfg(target_arch = "wasm32")]
    let when = String::new();
    let words = st.bug.text.trim().to_string();
    let mut out = format!("Phosphor snapshot {when} (UTC)\n\n");
    if !words.is_empty() {
        out.push_str(&format!("{words}\n\n"));
    }
    out.push_str("== Now ==\n");
    out.push_str(&diagnostics(st));
    out.push_str(&format!("\n== Settings ==\nRender rate: {:?} · canvas {}×{} · {} bpm\n", st.render_rate, st.comp.width, st.comp.height, st.comp.bpm));
    out.push_str("\n== Layers (top first) ==\n");
    let shares = st.core.lock().media.cpu_shares();
    for l in st.comp.layers.iter().rev() {
        let source = match &l.source {
            SourceSpec::None => "empty".to_string(),
            SourceSpec::Generator(g) => format!("generator {g}"),
            SourceSpec::Image { name, .. } => format!("image {name}"),
            SourceSpec::Movie { name, .. } => format!("movie {name}"),
            SourceSpec::Live { name, format, .. } => format!("live {name} ({format})"),
            SourceSpec::Text(_) => "text".to_string(),
            SourceSpec::Feed { name, .. } => format!("feed of {name}"),
        };
        let effects: Vec<&str> = l.effects.iter().map(|e| e.effect.as_str()).collect();
        let effects = if effects.is_empty() { String::new() } else { format!(" · effects: {}", effects.join(", ")) };
        out.push_str(&format!("{} · {} · {} · opacity {:.2} · {:?}{effects}\n", l.name, if l.enabled { "on" } else { "off" }, source, l.opacity, l.blend));
        if let Some(info) = st.core.lock().media.video_info(l.id) {
            let share = shares.get(&l.id).map(|c| format!(" · {:.0}% of a core", c * 100.0)).unwrap_or_default();
            out.push_str(&format!("    {info}{share}\n"));
        }
    }
    if !st.comp.master_effects.is_empty() {
        out.push_str(&format!("MASTER effects: {}\n", st.comp.master_effects.iter().map(|e| e.effect.as_str()).collect::<Vec<_>>().join(", ")));
    }
    out.push_str("\n== Outputs ==\n");
    for o in &st.comp.outputs {
        let size = if o.width == 0 { "canvas size".to_string() } else { format!("{}×{}", o.width, o.height) };
        let screen = if o.screen.is_empty() { "no screen chosen" } else { o.screen.as_str() };
        out.push_str(&format!("{} · {} · {size} · {screen} · {} slices\n", o.name, if o.enabled { "on" } else { "off" }, o.mapping.surfaces.len()));
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(log) = crate::session_log::path().and_then(|p| std::fs::read_to_string(p).ok()) {
        out.push_str("\n== Session log (this run) ==\n");
        out.push_str(tail_of(&log, 400_000));
        out.push('\n');
    }
    out.push_str("\n== Project ==\n");
    out.push_str(&serde_json::to_string_pretty(&st.comp).unwrap_or_else(|e| format!("(couldn't write the project: {e})")));
    out.push('\n');
    out
}

/// The last `n` meaningful lines of a session log: timestamps shortened, plumbing lines left out.
pub fn recent_activity(log: &str, n: usize) -> String {
    let keep: Vec<String> = log
        .lines()
        .filter(|l| !l.contains("first frame") && !l.contains("sound output:") && !l.contains("ISF "))
        .map(|l| {
            // "[2026-10-25T09:37:09Z INFO  phosphor::app::menus] menu: Save" -> "09:37:09 menu: Save"
            let time = l.get(12..20).unwrap_or("");
            let text = l.split_once("] ").map_or(l, |(_, t)| t);
            format!("{time} {text}")
        })
        .collect();
    let from = keep.len().saturating_sub(n);
    tail_of(&keep[from..].join("\n"), 6000).to_string()
}

fn os_version() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Ok(o) = crate::os::command("sw_vers").arg("-productVersion").output() {
            return format!("macOS {}", String::from_utf8_lossy(&o.stdout).trim());
        }
    }
    std::env::consts::OS.to_string()
}

/// The full report as text (what gets copied / filed).
pub fn report_text(st: &mut State) -> String {
    let words = st.bug.text.trim().to_string();
    let mut body = if words.is_empty() { "(no description)".to_string() } else { words };
    if st.bug.include_diagnostics {
        body.push_str("\n\n---\n");
        body.push_str(&diagnostics(st));
    }
    body
}

/// Percent-encoding for a URL query value.
pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The Report a Bug window.
pub fn bug_window(ctx: &egui::Context, st: &mut State) {
    let (mut open, mut detached) = (st.bug.open, st.bug.detached);
    crate::ui::tool_window(ctx, &format!("{} Report a Bug", icon::BUG), &mut open, &mut detached, 460.0, |ui| {
        ui.label("What happened, and what you were doing? One report per problem is perfect.");
        ui.add(egui::TextEdit::multiline(&mut st.bug.text).desired_rows(6).desired_width(f32::INFINITY).hint_text("e.g. Dragging a movie onto the Clips grid shows a black cell. Expected: the first frame."));
        crate::ui::toggle(ui, &mut st.bug.include_diagnostics, "Include diagnostics (version, macOS, project shape, end of crash.log)");
        #[cfg(not(target_arch = "wasm32"))]
        {
            let ready = crate::crash_report::endpoint().is_some();
            ui.add_enabled_ui(ready, |ui| {
                problem_reports_toggle(ui, st).on_disabled_hover_text("Not available yet: the crash server isn't set up");
            });
        }
        if st.bug.include_diagnostics {
            let diag = shown_diagnostics(st, ui.input(|i| i.time));
            crate::ui::section(ui, "bug_diag", "What gets included", false, |ui| {
                ui.add(egui::Label::new(egui::RichText::new(diag).monospace().small()).wrap());
            });
        }
        ui.add_space(6.0);
        // What the last send did.
        if let Some(r) = st.bug.bug_sent.lock().unwrap().take() {
            st.bug.sending = false;
            st.notice = Some(match r {
                Ok(()) => "Thanks: your report reached the Phosphor team".into(),
                Err(e) => format!("The report wasn't sent ({e}): copy it or file it on GitHub instead"),
            });
            if st.notice.as_deref().is_some_and(|n| n.starts_with("Thanks")) {
                st.bug.text.clear();
                st.bug.open = false;
            }
        }
        crate::ui::toolbar(ui, |ui| {
            let can_send = crate::crash_report::endpoint().is_some() && !st.bug.sending && !st.bug.text.trim().is_empty();
            if crate::ui::command_if(ui, can_send, format!("{} Send to the Phosphor team", icon::PAPER_PLANE_TILT))
                .on_hover_text("Sends this report privately (no account needed). It's the one-click way.")
                .on_disabled_hover_text(if crate::crash_report::endpoint().is_none() { "Sending isn't set up in this build: copy the report or file it on GitHub" } else { "Write what happened first" })
                .clicked()
            {
                let text = report_text(st);
                st.bug.sending = true;
                let out = st.bug.bug_sent.clone();
                let to = st.bug.send_problems.then(|| st.account.reporter());
                crate::crash_report::send_bug(text, to, move |r| {
                    *out.lock().unwrap() = Some(r);
                });
            }
            if st.bug.sending {
                ui.spinner();
            }
            if crate::ui::command(ui, format!("{} File it on GitHub", icon::GITHUB_LOGO)).on_hover_text("Opens a prefilled issue on github.com/fillinthe/phosphor (needs a GitHub account; the report is public)").clicked() {
                let title = st.bug.text.lines().next().unwrap_or("").chars().take(80).collect::<String>();
                let title = if title.trim().is_empty() { "Bug report".to_string() } else { title };
                let body = report_text(st);
                let url = format!("{ISSUES_URL}?title={}&body={}", url_encode(&title), url_encode(&body));
                ctx.open_url(egui::OpenUrl::new_tab(url));
            }
            if crate::ui::command(ui, format!("{} Copy report", icon::COPY)).on_hover_text("Copies the report to paste into an email or message").clicked() {
                let text = report_text(st);
                ctx.copy_text(text);
                st.notice = Some("Report copied. Paste it wherever you like.".into());
            }
            #[cfg(not(target_arch = "wasm32"))]
            if crate::ui::command(ui, format!("{} Save a snapshot…", icon::FLOPPY_DISK))
                .on_hover_text("One text file with everything about Phosphor right now: the graphics card, every layer and how its movie decodes, the outputs, this run's log and the project. Save it while the problem is on screen and attach it to your report. It names your files and layers; it holds no pictures or video")
                .clicked()
            {
                st.dialogs.save_file(crate::dialogs::Purpose::ReportSnapshot, ("Text", &["txt"]), "Phosphor snapshot.txt");
            }
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(p) = crate::recovery::crash_log_path()
                && p.exists()
                && crate::ui::command(ui, format!("{} crash.log", icon::FILE_TEXT)).on_hover_text("Show the log file in the Finder").clicked()
            {
                crate::os::reveal(&p);
            }
        });
        crate::ui::help(ui, "Everything stays on this computer until you file or paste it.");
    });
    st.bug.open = open;
    st.bug.detached = detached;
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_report_names_the_effects_but_not_own_shaders() {
        let ids = ["blur", "isf:bad_tv", "blur", "custom:my_secret_name"];
        assert_eq!(super::effects_line(ids.into_iter()), "Effects: blur ×2, isf:bad_tv, own shader\n");
        assert_eq!(super::effects_line([].into_iter()), "");
    }

    #[test]
    fn the_report_carries_the_recent_log_without_plumbing() {
        let log = "[2026-10-25T09:00:00Z INFO  phosphor::media::video] clip.mov: opened in 6 ms\n[2026-10-25T09:00:00Z INFO  phosphor::media::video] first frame 0 ms after starting (1x1)\n[2026-10-25T09:00:01Z INFO  phosphor::action] effect added: invert on layer \"A\"\n[2026-10-25T09:00:02Z WARN  phosphor::hitch] a frame took 180 ms (render 3.0 ms, GPU 1.0 ms)";
        let t = super::recent_activity(log, 10);
        assert!(!t.contains("first frame"), "{t}");
        assert!(t.contains("09:00:01 effect added: invert on layer \"A\"") && t.contains("a frame took 180 ms"), "{t}");
        assert_eq!(super::recent_activity(log, 1).lines().count(), 1);
        // A long tail is cut on a character, never inside one.
        let long = "é".repeat(5000);
        assert!(super::tail_of(&long, 6001).len() <= 6001);
        // Crash reports carry no user name in paths (the crash log's panic and stack lines).
        let crash = "panicked at /Users/sam/src/a.rs:3\n  stuck at: C:\\Users\\Sam\\x.dll\n/home/pi/show.phosphor /Users/Shared/x";
        let clean = super::without_user(crash, Some("/Users/sam"));
        assert_eq!(clean, "panicked at ~/src/a.rs:3\n  stuck at: ~\\x.dll\n~/show.phosphor /Users/Shared/x");
        assert_eq!(super::without_user("/Users/bob", None), "~");
        assert_eq!(super::without_user("/var/u/sam/a and /var/u/samuel", Some("/var/u/sam")), "~/a and /var/u/samuel");
        // Crash reports keep the kind of event, not names or paths.
        assert_eq!(super::without_names("12:00:01 clip played: layer \"Rings\" \u{2190} /Volumes/WD/a.mov"), "12:00:01 clip played: layer");
    }
}
