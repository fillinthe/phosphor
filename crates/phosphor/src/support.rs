//! Support: the Donate button (every Phosphor surface links to the same page) and the in-app bug report
//! (description + diagnostics → a prefilled GitHub issue, or copied to the clipboard).

use eframe::egui;
use egui_phosphor::regular as icon;

use crate::app::State;

pub const DONATE_URL: &str = "https://phosphorfx.com/donate/";
pub const ISSUES_URL: &str = "https://github.com/fillinthe/phosphor/issues/new";

/// Donate's orange, the same as GCarve's (the one orange in either app).
pub const DONATE_ORANGE: egui::Color32 = egui::Color32::from_rgb(0xff, 0x9a, 0x2e);

/// Heart button that opens the donation page (top bar, menus): filled orange with dark text, like GCarve's.
pub fn donate_button(ui: &mut egui::Ui) {
    if crate::ui::callout_button(ui, format!("{} Donate", icon::HEART), DONATE_ORANGE).on_hover_text("Phosphor is free and open source. A donation keeps it going.").clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(DONATE_URL));
    }
}

/// The Report a Bug window's state.
#[derive(Default)]
pub struct BugReport {
    pub open: bool,
    /// Popped out into its own OS window.
    pub detached: bool,
    pub text: String,
    pub include_diagnostics: bool,
    /// Send a report by itself after a crash (`crash_report`; off unless chosen).
    pub send_crashes: bool,
    /// What happened to the last automatic report, for the banner.
    pub sent: std::sync::Arc<std::sync::Mutex<Option<Result<(), String>>>>,
    /// macOS version, read once.
    os: Option<String>,
}

impl BugReport {
    /// `send_crashes`: the saved "Send crash reports by themselves" choice.
    pub fn new(send_crashes: bool) -> Self {
        Self { include_diagnostics: true, send_crashes, ..Default::default() }
    }
}

/// Everything a bug report should carry besides the words: version, system, project shape, the end of crash.log.
pub fn diagnostics(st: &mut State) -> String {
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
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = crate::recovery::crash_log_path()
        && let Ok(log) = std::fs::read_to_string(&path)
    {
        let tail: Vec<&str> = log.lines().rev().take(40).collect();
        let mut tail: String = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
        if tail.len() > 3000 {
            tail = tail[tail.len() - 3000..].to_string();
        }
        if !tail.trim().is_empty() {
            out.push_str("\ncrash.log (end):\n");
            out.push_str(&tail);
        }
    }
    out
}

fn os_version() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Ok(o) = std::process::Command::new("sw_vers").arg("-productVersion").output() {
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
                crate::ui::toggle(ui, &mut st.bug.send_crashes, "Send crash reports by themselves")
                    .on_hover_text("After an unexpected quit, the diagnostics above (no project content, no file names) go privately to the Phosphor team")
                    .on_disabled_hover_text("Not available yet: the crash server isn't set up");
            });
        }
        if st.bug.include_diagnostics {
            let diag = diagnostics(st);
            egui::CollapsingHeader::new("What gets included").id_salt("bug_diag").show(ui, |ui| {
                ui.add(egui::Label::new(egui::RichText::new(diag).monospace().small()).wrap());
            });
        }
        ui.add_space(6.0);
        crate::ui::toolbar(ui, |ui| {
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
            if let Some(p) = crate::recovery::crash_log_path()
                && p.exists()
                && crate::ui::command(ui, format!("{} crash.log", icon::FILE_TEXT)).on_hover_text("Show the log file in the Finder").clicked()
            {
                crate::os::reveal(&p);
            }
        });
        crate::ui::hint(ui, "Everything stays on your Mac until you file or paste it.");
    });
    st.bug.open = open;
    st.bug.detached = detached;
}
