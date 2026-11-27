//! Whatever `State::notice` says. Plain news (Saved, Undone, Watching…) is a small note floating just above the
//! status line: it takes no room, so nothing moves when it comes and goes, and it leaves by itself. A problem
//! (Couldn't open…, files missing…) stays there until OK; a failure also offers Report a problem (the bug window,
//! filled in) so anyone can tell the Phosphor team in one click.

use eframe::egui;
use egui_phosphor::regular as icon;

use crate::app::State;

/// How long plain news stays.
const NEWS_SECS: f64 = 8.0;

/// What a notice is, said where it's made (never guessed from its English words: "No media is missing" isn't a
/// problem, "No clips found" is).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Plain news (Saved, Undone, Watching…): a small note that leaves by itself.
    News,
    /// Something to fix (a drive not connected, nothing playable dropped): stays until OK.
    Problem,
    /// The app itself went wrong (a load or save failed): stays, and offers Report a problem.
    Failure,
}

/// Notices made as problems or failures (text, kind), newest last: `State::notice` stays a plain `String` (set in
/// hundreds of places, some through a `&mut Option<String>`), and the bar looks its text up here.
static TYPED: std::sync::Mutex<Vec<(String, Kind)>> = std::sync::Mutex::new(Vec::new());

/// `text` as a notice of `kind` (News comes back as it is).
pub(crate) fn of(kind: Kind, text: impl Into<String>) -> String {
    let text = text.into();
    if kind != Kind::News
        && let Ok(mut t) = TYPED.lock()
    {
        t.retain(|(x, _)| *x != text);
        t.push((text.clone(), kind));
        if t.len() > 32 {
            t.remove(0);
        }
    }
    text
}

/// A notice that says something needs fixing (stays until OK): `st.notice = Some(notice::problem(...))`.
pub(crate) fn problem(text: impl Into<String>) -> String {
    of(Kind::Problem, text)
}

/// A notice that says the app went wrong (stays, with Report a problem).
pub(crate) fn failure(text: impl Into<String>) -> String {
    of(Kind::Failure, text)
}

/// What kind of notice `text` was made as (anything not made through `problem` / `failure` is news).
pub fn kind_of(text: &str) -> Kind {
    TYPED.lock().ok().and_then(|t| t.iter().rev().find(|(x, _)| x == text).map(|(_, k)| *k)).unwrap_or(Kind::News)
}

/// Says something in the status line from a widget with no `State` at hand (taken in by the frame, `take_told`).
pub(crate) fn tell(ctx: &egui::Context, text: String) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("told_notice"), text));
}

pub(crate) fn take_told(ctx: &egui::Context) -> Option<String> {
    ctx.data_mut(|d| d.remove_temp::<String>(egui::Id::new("told_notice")))
}

pub(crate) fn notice_bar(ctx: &egui::Context, st: &mut State, now: f64) {
    let Some(text) = st.notice.clone() else { return };
    let kind = kind_of(&text);
    let problem = kind != Kind::News;
    // Short news goes sooner (Undone, Saved); a sentence gets time to be read.
    let stays = (2.5 + text.chars().count() as f64 / 14.0).min(NEWS_SECS);
    if !problem && now - st.notice_since > stays {
        st.notice = None;
        return;
    }
    if !problem {
        // A small note floating just above the status line, bottom center: it takes no room, so nothing on screen
        // moves when it comes and goes. (It used to be written into the status line, which wraps: on a full line
        // the news made a second row and the whole window shifted up; owner, 2026-11-16.)
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        let mut clear = false;
        egui::Area::new(egui::Id::new("news_note")).order(egui::Order::Tooltip).interactable(true).anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -34.0)).show(ctx, |ui| {
            ui.set_max_width((ctx.content_rect().width() - 80.0).clamp(120.0, 620.0));
            egui::Frame::popup(ui.style()).fill(crate::theme::GOOD_FILL).inner_margin(egui::Margin::symmetric(10, 5)).show(ui, |ui| {
                let words = egui::RichText::new(format!("{} {text}", icon::INFO)).small().color(crate::theme::accent());
                clear = ui.add(egui::Label::new(words).wrap().selectable(false).sense(egui::Sense::click())).on_hover_text("Click to clear").clicked();
            });
        });
        if clear {
            st.notice = None;
        }
        return;
    }
    let fill = crate::theme::PROBLEM_FILL;
    // (At the bottom, just above the status line: it never covers the menus or the panels' tabs.)
    egui::Area::new(egui::Id::new("notice_bar")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -36.0)).show(ctx, |ui| {
        ui.set_max_width(ctx.content_rect().width() - 40.0);
        egui::Frame::popup(ui.style()).fill(fill).show(ui, |ui| {
            crate::ui::toolbar(ui, |ui| {
                let glyph = if problem { icon::WARNING } else { icon::INFO };
                let color = if problem { crate::theme::WARN } else { crate::theme::accent() };
                ui.label(egui::RichText::new(glyph).color(color));
                ui.add(egui::Label::new(&text).wrap());
                if kind == Kind::Failure && crate::ui::command(ui, format!("{} Report a problem", icon::BUG)).on_hover_text("Tell the Phosphor team: opens the report with this message filled in").clicked() {
                    let bug = &mut st.bug;
                    bug.open = true;
                    bug.include_diagnostics = true;
                    if bug.text.trim().is_empty() {
                        bug.text = format!("Phosphor said: {text}\nWhat I was doing: ");
                    }
                }
                if crate::ui::small_command(ui, "OK").clicked() {
                    st.notice = None;
                }
            });
        });
    });
}


#[cfg(test)]
mod tests {
    use super::*;

    /// A notice is what it was made as, whatever its words say.
    #[test]
    fn notices_are_what_they_were_made_as() {
        assert_eq!(kind_of(&problem("No clips found in that folder")), Kind::Problem, "no 'error' word in it, still a problem");
        assert_eq!(kind_of(&failure(format!("Open failed: {}", "bad json"))), Kind::Failure);
        for news in ["No media is missing.", "Looking for 3 missing files in Loops…", "Saved show.phosphor"] {
            assert_eq!(kind_of(news), Kind::News, "{news}: its words used to make it a problem");
        }
        assert_eq!(of(Kind::News, "Undone"), "Undone");
    }

    /// Every notice whose words say something failed is made as a problem or a failure (a new one written as plain
    /// news would vanish after a few seconds): the check the old keyword guess did at run time, done on the source.
    #[test]
    fn failures_are_typed_where_they_are_made() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![src];
        let mut bad = Vec::new();
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().is_none_or(|x| x != "rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&p).unwrap();
                for (n, line) in text.lines().enumerate() {
                    let says_failed = ["Couldn't", "failed:", "Failed"].iter().any(|w| line.contains(w));
                    if line.contains("notice = Some(") && says_failed && !line.contains("notice::problem(") && !line.contains("notice::failure(") && !line.contains("notice::of(") {
                        bad.push(format!("{}:{}", p.display(), n + 1));
                    }
                }
            }
        }
        assert!(bad.is_empty(), "make these with ui::notice::problem / failure:\n{}", bad.join("\n"));
    }
}
