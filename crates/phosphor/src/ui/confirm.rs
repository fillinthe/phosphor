//! "Are you sure?" before something is removed: a small pop-up in the middle of the window with a "Don't ask
//! again" box. Asking can be switched back on in the brightness menu (`set_confirm_removes`).

use super::*;

/// Ask before removing things (the app's setting; "Don't ask again" switches it off).
static CONFIRM_REMOVES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub(crate) fn confirm_removes() -> bool {
    CONFIRM_REMOVES.load(std::sync::atomic::Ordering::Relaxed)
}

pub(crate) fn set_confirm_removes(on: bool) {
    CONFIRM_REMOVES.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// A question waiting in its pop-up.
#[derive(Clone)]
struct Ask {
    key: egui::Id,
    title: String,
    body: String,
    yes: String,
    seen: f64,
    /// Already answered yes (asking is switched off): `confirmed` says so on its next call.
    answered: bool,
    dont_ask: bool,
}

const SLOT: &str = "phosphor_confirm_ask";

/// Opens the pop-up for `id`: `title` ("Stop watching Loops?"), `body` (what happens, what stays) and the word on
/// the button that goes ahead. With asking switched off it counts as a yes straight away. Pair it with
/// `confirmed` (same `id`), called every frame from where the thing is drawn.
pub(crate) fn ask_confirm(ctx: &egui::Context, id: impl std::hash::Hash + std::fmt::Debug, title: &str, body: &str, yes: &str) {
    let seen = ctx.input(|i| i.time);
    let ask = Ask { key: egui::Id::new(("confirm", id)), title: title.into(), body: body.into(), yes: yes.into(), seen, answered: !confirm_removes(), dont_ask: false };
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(SLOT), ask));
}

/// The pop-up for `ask_confirm(.., id, ..)` while it's open. True once, when the answer is yes. Cancel, Escape or
/// a click outside mean no; a pop-up whose owner stopped drawing it goes away.
pub(crate) fn confirmed(ctx: &egui::Context, id: impl std::hash::Hash + std::fmt::Debug) -> bool {
    let slot = egui::Id::new(SLOT);
    let Some(mut ask) = ctx.data(|d| d.get_temp::<Ask>(slot)) else { return false };
    if ask.key != egui::Id::new(("confirm", id)) {
        return false;
    }
    let now = ctx.input(|i| i.time);
    if ask.answered || now - ask.seen > 0.5 {
        ctx.data_mut(|d| d.remove::<Ask>(slot));
        return ask.answered;
    }
    let (mut yes, mut no) = (false, false);
    let modal = egui::Modal::new(egui::Id::new("phosphor_confirm_modal")).show(ctx, |ui| {
        ui.set_width(340.0);
        heading(ui, ask.title.as_str());
        ui.label(ask.body.as_str());
        ui.add_space(4.0);
        toggle(ui, &mut ask.dont_ask, "Don't ask again").on_hover_text("Removes happen straight away from now on. Switch asking back on in the brightness menu at the top: Ask before removing");
        ui.horizontal(|ui| {
            yes = danger_button(ui, ask.yes.as_str()).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter));
            no = ui.button("Cancel").clicked();
        });
    });
    if yes || no || modal.should_close() {
        ctx.data_mut(|d| d.remove::<Ask>(slot));
        if yes && ask.dont_ask {
            set_confirm_removes(false);
        }
        return yes;
    }
    ask.seen = now;
    ctx.data_mut(|d| d.insert_temp(slot, ask));
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With asking switched off (Don't ask again), a remove goes ahead on the next look, once; with it on, nothing
    /// happens until the pop-up is answered, and another thing's question is never taken for this one's.
    #[test]
    fn dont_ask_again_goes_straight_ahead() {
        let ctx = egui::Context::default();
        set_confirm_removes(false);
        ask_confirm(&ctx, "a", "Remove A?", "It goes.", "Remove");
        assert!(!confirmed(&ctx, "b"), "someone else's question");
        assert!(confirmed(&ctx, "a"));
        assert!(!confirmed(&ctx, "a"), "only once");
        set_confirm_removes(true);
        ask_confirm(&ctx, "a", "Remove A?", "It goes.", "Remove");
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert!(!confirmed(ui.ctx(), "a"), "waits for the answer");
        });
        out.textures_delta.clear();
    }
}
