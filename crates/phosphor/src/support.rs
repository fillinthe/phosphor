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
    /// What happened to the report sent from the window (None = none sent / sending).
    pub bug_sent: std::sync::Arc<std::sync::Mutex<Option<Result<(), String>>>>,
    pub sending: bool,
    /// macOS version, read once.
    os: Option<String>,
    /// The diagnostics shown in the window, and when they were put together (redone every few seconds, not each frame).
    shown: Option<(f64, String)>,
}

impl BugReport {
    /// `send_crashes`: the saved "Send crash reports by themselves" choice.
    pub fn new(send_crashes: bool) -> Self {
        Self { include_diagnostics: true, send_crashes, ..Default::default() }
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

pub fn diagnostics(st: &mut State) -> String {
    diagnostics_from(st, false)
}

fn diagnostics_from(st: &mut State, crash: bool) -> String {
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
        let tail: String = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
        let tail = tail_of(&tail, 3000).to_string();
        if !tail.trim().is_empty() {
            out.push_str("\ncrash.log (end):\n");
            out.push_str(&tail);
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
                    .on_hover_text("After an unexpected quit, the diagnostics go privately to the Phosphor team: version, macOS, the project's shape and what kind of things happened just before (no project content, no file, layer or device names)")
                    .on_disabled_hover_text("Not available yet: the crash server isn't set up");
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
                crate::crash_report::send_bug(text, move |r| {
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
            if let Some(p) = crate::recovery::crash_log_path()
                && p.exists()
                && crate::ui::command(ui, format!("{} crash.log", icon::FILE_TEXT)).on_hover_text("Show the log file in the Finder").clicked()
            {
                crate::os::reveal(&p);
            }
        });
        crate::ui::help(ui, "Everything stays on your Mac until you file or paste it.");
    });
    st.bug.open = open;
    st.bug.detached = detached;
}

#[cfg(test)]
mod tests {
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
        // Crash reports keep the kind of event, not names or paths.
        assert_eq!(super::without_names("12:00:01 clip played: layer \"Rings\" \u{2190} /Volumes/WD/a.mov"), "12:00:01 clip played: layer");
    }
}
