//! Web control: phones and browsers (the remote page and audience play) changing the visuals. You can see when
//! someone is (status line, preview badge), and take over in one click: their changes are ignored and their pages
//! say the booth has the controls, until you hand control back.

use super::*;
use crate::net::PeerKind;
use eframe::egui::RichText;

/// Seconds after the last web change that "… is controlling" stays lit.
const ACTIVE_SECS: f64 = 3.0;
/// Changes kept in the Devices panel's log.
const LOG_MAX: usize = 100;

/// One change made from the web, for the log.
#[derive(Clone)]
pub(crate) struct WebChange {
    /// App time of the latest change of this kind in a row (a dragged fader is one entry).
    pub at: f64,
    pub wall: String,
    pub who: String,
    pub what: String,
}

/// A remote / guest command in words ("Master fade 45%", "Clip 3 on Layer 2", "Surprise").
fn describe(st: &State, cmd: &serde_json::Value) -> String {
    use serde_json::Value;
    let f = |k: &str| cmd.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    let layer = |k: &str| {
        let id = cmd.get(k).and_then(Value::as_u64).unwrap_or(0);
        st.comp.layers.iter().find(|l| l.id == id).map_or_else(|| "a layer".to_string(), |l| l.name.clone())
    };
    let pct = format!("{:.0}%", f("v") * 100.0);
    let on = if cmd.get("v").and_then(Value::as_bool).unwrap_or(false) { "on" } else { "off" };
    match cmd.get("t").and_then(Value::as_str).unwrap_or("") {
        "master" => format!("Master fade {pct}"),
        "opacity" => format!("{} opacity {pct}", layer("layer")),
        "enabled" => format!("{} {on}", layer("layer")),
        "solo" => format!("{} solo {on}", layer("layer")),
        "clip" => format!("Clip {} on {}", f("slot") as usize + 1, layer("layer")),
        "column" => format!("Column {}", f("col") as usize + 1),
        "select" => format!("Selected {}", layer("layer")),
        "tap" => "Tap tempo".into(),
        "surprise" => "Surprise".into(),
        "autoshow" => format!("Auto show {on}"),
        "timing" => "Launch timing".into(),
        "mix" | "fxon" | "fx" => "An effect setting".into(),
        "gen" => format!("{} generator setting", layer("layer")),
        "set" => cmd
            .get("target")
            .and_then(|t| serde_json::from_value::<phosphor_engine::ControlTarget>(t.clone()).ok())
            .map_or_else(|| "A control".to_string(), |t| format!("{} {pct}", crate::control::target_label(&st.comp, &t))),
        other => other.to_string(),
    }
}

impl State {
    /// Remembers who just changed something from the web (for the badge).
    pub(crate) fn note_web_activity(&mut self, cmd: &serde_json::Value, now: f64) {
        let seat = cmd.get("_by").or_else(|| cmd.get("_from")).and_then(serde_json::Value::as_u64);
        let who = seat
            .and_then(|id| self.net.peers().into_iter().find(|p| p.id == id))
            .map(|p| if !p.name.is_empty() { p.name } else if !p.device.is_empty() { p.device } else { p.ip })
            .unwrap_or_else(|| "A phone".to_string());
        let what = describe(self, cmd);
        // A fader dragged from a phone is many commands: one log entry, updated.
        let kind = cmd.get("t").and_then(serde_json::Value::as_str).unwrap_or("");
        let same = self.web_log.last().is_some_and(|l| l.who == who && now - l.at < 1.5 && l.what.split_whitespace().next() == what.split_whitespace().next() && kind != "clip" && kind != "column" && kind != "surprise" && kind != "tap");
        if same {
            let l = self.web_log.last_mut().unwrap();
            l.at = now;
            l.what = what;
        } else {
            let wall = chrono_now();
            self.web_log.push(WebChange { at: now, wall, who: who.clone(), what });
            if self.web_log.len() > LOG_MAX {
                self.web_log.remove(0);
            }
        }
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

/// Local wall-clock time "21:04:17" for the log.
/// Local wall-clock time "21:04:17" for the log.
fn chrono_now() -> String {
    let (s, _) = crate::clock::local_time();
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
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
        (small(format!("{} {n} can control", icon::DEVICE_MOBILE), Color32::GRAY), crate::ui::plural(n as usize, "{0} phone or browser can change the visuals. Click to take over.", "{0} phones or browsers can change the visuals. Click to take over."))
    };
    ui.add(egui::Label::new(text).sense(egui::Sense::click())).on_hover_text(tip).clicked()
}
