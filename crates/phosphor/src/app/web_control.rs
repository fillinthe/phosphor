//! Web control: phones and browsers (the remote page and audience play) changing the visuals. You can see when
//! someone is (status line, preview badge), and take over in one click: their changes are ignored and their pages
//! say the booth has the controls, until you hand control back.

use super::*;
use crate::net::PeerKind;
use eframe::egui::RichText;

/// Seconds after the last web change that "… is controlling" stays lit.
const ACTIVE_SECS: f64 = 3.0;

impl State {
    /// Remembers who just changed something from the web (for the badge).
    pub(crate) fn note_web_activity(&mut self, cmd: &serde_json::Value, now: f64) {
        let seat = cmd.get("_by").or_else(|| cmd.get("_from")).and_then(serde_json::Value::as_u64);
        let who = seat
            .and_then(|id| self.net.peers().into_iter().find(|p| p.id == id))
            .map(|p| if !p.name.is_empty() { p.name } else if !p.device.is_empty() { p.device } else { p.ip })
            .unwrap_or_else(|| "A phone".to_string());
        self.web_activity = Some((now, who));
    }

    /// How many phones / browsers can change the visuals right now (unlocked remotes, and guests while audience
    /// play is on).
    pub(crate) fn web_controllers(&self) -> usize {
        if !self.net.running() {
            return 0;
        }
        self.net.peers().iter().filter(|p| (p.kind == PeerKind::Remote && p.controlling) || (p.kind == PeerKind::Guest && self.comp.play.enabled)).count()
    }

    /// Who changed something from the web in the last few seconds.
    pub(crate) fn web_active(&self, now: f64) -> Option<&str> {
        self.web_activity.as_ref().filter(|(t, _)| now - t < ACTIVE_SECS && !self.comp.web_control_off).map(|(_, w)| w.as_str())
    }

    /// Take over (web control off) or hand control back.
    pub(crate) fn set_web_control(&mut self, on: bool) {
        self.comp.web_control_off = !on;
        self.web_activity = None;
        log::info!("web control: {}", if on { "on" } else { "off (taken over)" });
        self.notice = Some(if on { "Web control is on: phones can change the visuals again".into() } else { "You have the controls: phones and the web can only watch".into() });
    }
}

/// The web control badge: nothing while no session runs; "Web control off" (click: hand back), "N can control",
/// or "<who> is controlling" in orange (click: take over). Returns true when clicked (the caller toggles).
pub(crate) fn web_badge(ui: &mut egui::Ui, st: &State, now: f64) -> bool {
    if !st.net.running() {
        return false;
    }
    let small = |t: String, c: Color32| RichText::new(t).small().color(c);
    let (text, tip) = if st.comp.web_control_off {
        (small(format!("{} Web control off", icon::HAND_PALM), crate::theme::WARN), "You have the controls: phones only watch. Click to let them control again.".to_string())
    } else if let Some(who) = st.web_active(now) {
        (small(format!("{} {who} is controlling", icon::DEVICE_MOBILE), crate::support::DONATE_ORANGE).strong(), "Someone is changing the visuals from a phone or browser. Click to take over.".to_string())
    } else {
        let n = st.web_controllers();
        if n == 0 {
            return false;
        }
        (small(format!("{} {n} can control", icon::DEVICE_MOBILE), Color32::GRAY), format!("{n} phone(s) or browser(s) can change the visuals. Click to take over."))
    };
    ui.add(egui::Label::new(text).sense(egui::Sense::click())).on_hover_text(tip).clicked()
}
