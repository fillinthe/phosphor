//! Show-ending actions ask first. Quitting or closing the output windows while they're showing, and replacing a
//! project that has unsaved edits or is on the outputs (New Project, a template, a project from the account), each
//! wait for an "are you sure?" (`ui::ask_confirm_always`): one slip of the hand at a show must never black out the
//! room or lose the night's work. Undo can't bring a replaced project back (`History::start_over`), so the question
//! is the only safety there is. Nothing is asked when nothing would be lost.

use super::*;

/// An action waiting for its answer.
pub(crate) enum Guarded {
    Quit,
    /// Close every output window.
    OutputsOff,
    NewProject,
    /// `phosphor_engine::TEMPLATES[i]`.
    Template(usize),
    /// A project opened from the account: (id, name, the project).
    FromAccount(String, String, Box<Composition>),
}

/// The pop-up's id (one question at a time).
const ASK: &str = "show_guard";

/// What a question says: (title, body, the button that goes ahead). `showing` = outputs open, `edited` = unsaved edits.
fn question(g: &Guarded, showing: usize, edited: bool) -> (String, String, &'static str) {
    let replaced = |title: String, yes| {
        let mut body = Vec::new();
        if edited {
            body.push("The project open now has changes that aren't saved, and Undo can't bring it back.".to_string());
        }
        if showing > 0 {
            body.push(crate::ui::plural(showing, "Its output is showing: it changes at once.", "Its {0} outputs are showing: they change at once."));
        }
        (title, body.join(" "), yes)
    };
    match g {
        Guarded::Quit => (
            "Quit while the outputs are showing?".into(),
            crate::ui::plural(showing, "1 output is on screen: quitting turns it off and its screen goes dark.", "{0} outputs are on screen: quitting turns them off and their screens go dark."),
            "Quit",
        ),
        Guarded::OutputsOff => (
            "Close the output windows?".into(),
            format!("Every screen they fill goes dark. Blackout ({}) fades the show to black instead and keeps the windows where they are.", crate::menu::shortcut_of(crate::menu::Action::Blackout)),
            "Close outputs",
        ),
        Guarded::NewProject => replaced("Start a new project?".into(), "New project"),
        Guarded::Template(k) => replaced(format!("New project from \"{}\"?", phosphor_engine::TEMPLATES.get(*k).map_or("", |t| t.0)), "New project"),
        Guarded::FromAccount(_, name, _) => replaced(format!("Open \"{name}\" from your account?"), "Open"),
    }
}

/// Does `g` need asking? (Outputs open, edits not saved.)
fn needs_asking(g: &Guarded, showing: usize, edited: bool) -> bool {
    match g {
        Guarded::Quit | Guarded::OutputsOff => showing > 0,
        Guarded::NewProject | Guarded::Template(_) | Guarded::FromAccount(..) => showing > 0 || edited,
    }
}

impl PhosphorApp {
    /// Does `g` now, or asks first when it would end the show or lose edits (it's done once the answer is yes).
    pub(crate) fn guard(&mut self, ctx: &egui::Context, g: Guarded) {
        let showing = self.state.output.lock().map_or(0, |m| m.values().filter(|o| o.open).count());
        let edited = self.state.edited();
        if self.bench_session || !needs_asking(&g, showing, edited) {
            self.perform(ctx, g);
            return;
        }
        let (title, body, yes) = question(&g, showing, edited);
        log::info!(target: "phosphor::action", "asking first: {title}");
        crate::ui::ask_confirm_always(ctx, ASK, &title, &body, yes);
        self.guarded = Some(g);
    }

    /// Each frame: the answer to a question `guard` asked (yes = it's done now; no = it's dropped).
    pub(crate) fn guard_answers(&mut self, ctx: &egui::Context) {
        if self.guarded.is_none() {
            return;
        }
        if crate::ui::confirmed(ctx, ASK) {
            if let Some(g) = self.guarded.take() {
                self.perform(ctx, g);
            }
        } else if !crate::ui::asking(ctx, ASK) {
            self.guarded = None;
        }
    }

    /// The main window asked to close (⌘Q, the red button, the Dock): while outputs are showing it waits for a yes.
    pub(crate) fn guard_close(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) || self.quit_confirmed || self.bench_session || !self.state.output_open() {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        // (Quitting is the question now, whatever was being asked before.)
        if !matches!(self.guarded, Some(Guarded::Quit)) {
            self.guard(ctx, Guarded::Quit);
        }
    }

    fn perform(&mut self, ctx: &egui::Context, g: Guarded) {
        let st = &mut self.state;
        match g {
            Guarded::Quit => {
                self.quit_confirmed = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Guarded::OutputsOff => {
                if st.output_open() {
                    st.toggle_output();
                }
            }
            Guarded::NewProject => {
                st.load_composition(Composition::starter());
                st.project_path = None;
                st.cloud_project = None;
                st.notice = Some("New project".into());
            }
            Guarded::Template(k) => {
                let Some((name, _, make)) = phosphor_engine::TEMPLATES.get(k).copied() else { return };
                st.load_composition(make());
                st.project_path = None;
                st.cloud_project = None;
                st.notice = Some(format!("New project from \"{name}\""));
            }
            Guarded::FromAccount(id, name, comp) => {
                st.load_composition(*comp);
                st.project_path = None;
                st.account_ui.name = name.clone();
                st.notice = Some(format!("Opened {name} from your account"));
                st.cloud_project = Some((id, name));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asked only when something would be lost: an output showing, or edits not saved.
    #[test]
    fn asks_only_when_the_show_or_edits_are_at_stake() {
        for g in [Guarded::Quit, Guarded::OutputsOff] {
            assert!(!needs_asking(&g, 0, true), "nothing showing: nothing goes dark");
            assert!(needs_asking(&g, 1, false));
        }
        for g in [Guarded::NewProject, Guarded::Template(0)] {
            assert!(!needs_asking(&g, 0, false), "a saved project with no output open is replaced at once");
            assert!(needs_asking(&g, 0, true), "unsaved edits");
            assert!(needs_asking(&g, 2, false), "on the outputs");
        }
    }

    /// The words never promise an Undo that doesn't exist, and say what's at stake.
    #[test]
    fn questions_say_what_is_lost() {
        let (_, body, yes) = question(&Guarded::NewProject, 0, true);
        assert!(body.contains("aren't saved") && body.contains("can't bring it back"), "{body}");
        assert_eq!(yes, "New project");
        let (_, body, _) = question(&Guarded::Template(0), 2, false);
        assert!(body.contains("2 outputs") && !body.contains("saved"), "{body}");
        let (title, body, _) = question(&Guarded::Quit, 1, false);
        assert!(title.starts_with("Quit") && body.contains("1 output"), "{body}");
    }
}
