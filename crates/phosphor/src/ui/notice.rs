//! The message bar: whatever `State::notice` says, at the top of the window. Plain news (Saved, Watching…) goes by
//! itself after a few seconds; a problem (Couldn't open…, failed…) stays until OK, with Report a problem (the bug
//! window, filled in) so anyone can tell the Phosphor team in one click.

use eframe::egui;
use egui_phosphor::regular as icon;

use crate::app::State;

/// How long plain news stays.
const NEWS_SECS: f64 = 8.0;

/// Does a message say something went wrong?
pub fn is_problem(text: &str) -> bool {
    let t = text.to_lowercase();
    ["couldn't", "can't", "cannot", "failed", "error", "wasn't", "didn't", "not connected", "took too long", "said no", "missing", "unexpected", "no such", "denied", "refused", "unsupported"].iter().any(|w| t.contains(w))
}

pub(crate) fn notice_bar(ctx: &egui::Context, st: &mut State, now: f64) {
    let Some(text) = st.notice.clone() else { return };
    let problem = is_problem(&text);
    if !problem && now - st.notice_since > NEWS_SECS {
        st.notice = None;
        return;
    }
    if !problem {
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }
    let fill = if problem { egui::Color32::from_rgb(54, 30, 24) } else { egui::Color32::from_rgb(24, 34, 30) };
    egui::Area::new(egui::Id::new("notice_bar")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 44.0)).show(ctx, |ui| {
        ui.set_max_width(ctx.content_rect().width() - 40.0);
        egui::Frame::popup(ui.style()).fill(fill).show(ui, |ui| {
            crate::ui::toolbar(ui, |ui| {
                let glyph = if problem { icon::WARNING } else { icon::INFO };
                let color = if problem { crate::theme::WARN } else { crate::theme::accent() };
                ui.label(egui::RichText::new(glyph).color(color));
                ui.add(egui::Label::new(&text).wrap());
                if problem && crate::ui::command(ui, format!("{} Report a problem", icon::BUG)).on_hover_text("Tell the Phosphor team: opens the report with this message filled in").clicked() {
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
    #[test]
    fn problems_are_told_from_news() {
        for p in ["Couldn't open a.mov: no such file", "Open failed: bad json", "X took too long to answer", "Not connected right now: Lights"] {
            assert!(super::is_problem(p), "{p}");
        }
        for n in ["Saved show.phosphor", "Watching \"Loops\": new clips show up on their own", "New layer with 12 clips", "Report copied."] {
            assert!(!super::is_problem(n), "{n}");
        }
    }
}
