//! Audience play: guests on the play page take turns at the controls. Settings are `Composition::play`;
//! this is the queue (who plays now, who's next) and what a guest is allowed to touch.

use phosphor_engine::{ControlTarget, SourceSpec};
use serde_json::{Value, json};

use super::State;
use crate::net::PeerKind;

/// Name of the layer that shows the QR code on the output.
pub const QR_LAYER: &str = "Scan to play";

#[derive(Default)]
pub struct Jam {
    /// Seat id of the guest at the controls, and when their turn started.
    pub current: Option<(u64, f64)>,
    /// Seats that already had a turn this round (cleared when everyone has played).
    served: Vec<u64>,
}

impl State {
    /// Guests connected right now, oldest first (seat ids grow with time).
    fn guests(&self) -> Vec<u64> {
        let mut ids: Vec<u64> = self.net.peers().iter().filter(|p| p.kind == PeerKind::Guest).map(|p| p.id).collect();
        ids.sort_unstable();
        ids
    }

    /// Once per frame: ends turns that ran out (or whose guest left) and hands the controls to the next guest.
    pub(crate) fn run_play(&mut self, now: f64) {
        if !self.comp.play.enabled || !self.net.running() {
            self.jam = Jam::default();
            return;
        }
        let guests = self.guests();
        let turn = self.comp.play.turn_secs.max(5) as f64;
        if let Some((id, started)) = self.jam.current {
            let gone = !guests.contains(&id);
            // One guest alone keeps the controls; the clock only matters when someone is waiting.
            let over = now - started >= turn && guests.len() > 1;
            if gone || over {
                self.jam.served.push(id);
                self.jam.current = None;
            }
        }
        if self.jam.current.is_none() && !guests.is_empty() {
            self.jam.served.retain(|s| guests.contains(s));
            if guests.iter().all(|g| self.jam.served.contains(g)) {
                self.jam.served.clear();
            }
            if let Some(next) = guests.iter().find(|g| !self.jam.served.contains(g)) {
                self.jam.current = Some((*next, now));
            }
        }
    }

    /// Guests after the current player, in the order they'll get the controls.
    pub fn play_queue(&self) -> Vec<u64> {
        let guests = self.guests();
        let cur = self.jam.current.map(|c| c.0);
        let mut later: Vec<u64> = guests.iter().copied().filter(|g| Some(*g) != cur && !self.jam.served.contains(g)).collect();
        later.extend(guests.iter().copied().filter(|g| Some(*g) != cur && self.jam.served.contains(g)));
        later
    }

    /// Seconds left in the current turn (None = no one is waiting, so no limit).
    pub fn play_left(&self, now: f64) -> Option<f64> {
        let (_, started) = self.jam.current?;
        (self.guests().len() > 1).then(|| (self.comp.play.turn_secs.max(5) as f64 - (now - started)).max(0.0))
    }

    /// What the play page needs, part of the remote state JSON.
    pub fn play_json(&self, now: f64) -> Value {
        let p = &self.comp.play;
        json!({
            "on": p.enabled,
            "title": p.title,
            "turn": p.turn_secs,
            "current": self.jam.current.map(|c| c.0),
            "left": self.play_left(now),
            "queue": self.play_queue(),
            "page": p.surface_page,
            "columns": if p.columns { self.comp.clip_columns } else { 0 },
            "surprise": p.surprise,
        })
    }

    /// May the guest in seat `from` send this command right now? Only the player, only the
    /// controls the show allows: the Control Surface (its chosen page), clip columns, Surprise.
    pub fn guest_may(&self, from: u64, cmd: &Value) -> bool {
        let p = &self.comp.play;
        if !p.enabled || self.jam.current.map(|c| c.0) != Some(from) {
            return false;
        }
        match cmd.get("t").and_then(Value::as_str).unwrap_or("") {
            "column" => p.columns,
            "surprise" => p.surprise,
            "set" => {
                let Some(t) = cmd.get("target").and_then(|t| serde_json::from_value::<ControlTarget>(t.clone()).ok()) else { return false };
                let widget = match t {
                    ControlTarget::Surface(id) | ControlTarget::SurfaceY(id) => id,
                    _ => return false,
                };
                self.comp.surface.widgets.iter().any(|w| w.id == widget && p.surface_page.is_none_or(|pg| w.page == pg))
            }
            _ => false,
        }
    }

    /// The device label of a seat (Devices panel knows it).
    pub fn peer_label(&self, id: u64) -> String {
        self.net.peers().iter().find(|p| p.id == id).map(|p| p.device.clone()).unwrap_or_else(|| format!("#{id}"))
    }

    /// Puts the play page's QR code on the output as a picture layer (or takes it off again).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn toggle_play_qr(&mut self, now: f64) {
        if let Some(l) = self.comp.layers.iter().find(|l| l.name == QR_LAYER) {
            let id = l.id;
            crate::panels::remove_layer(self, id);
            return;
        }
        let url = self.net.play_url();
        let Some(dir) = crate::show::storage_dir() else { return };
        let path = dir.join("scan-to-play.png");
        let Some(png) = crate::ui::qr_png(&url, 1024) else { return };
        if std::fs::write(&path, png).is_err() {
            self.notice = Some("Couldn't write the QR picture".into());
            return;
        }
        let layer = self.comp.add_layer();
        layer.name = QR_LAYER.into();
        layer.fit = phosphor_engine::FitMode::Fit;
        let id = layer.id;
        let spec = SourceSpec::Image { name: QR_LAYER.into(), path: Some(path.to_string_lossy().to_string()) };
        self.launch_source(id, spec, Vec::new(), QR_LAYER.into(), now);
        self.selected = Some(id);
    }
}
