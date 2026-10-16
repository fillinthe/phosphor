//! Audience play: guests on the play page take turns at the controls. Settings are `Composition::play`;
//! this is the queue (who plays now, who's next) and what a guest is allowed to touch.

use phosphor_engine::ControlTarget;
use serde_json::{Value, json};

use super::State;
use crate::net::PeerKind;

/// Name of the layer that shows the QR code on the output.
pub const QR_LAYER: &str = "Scan to play";

#[derive(Default)]
pub struct Jam {
    /// Who has the controls (their identity) and when their turn started.
    pub current: Option<(String, f64)>,
    /// Identities that already had a turn this round (cleared when everyone has played).
    served: Vec<String>,
    /// When each identity first showed up: the line is in arrival order even across reconnects.
    seen: std::collections::HashMap<String, f64>,
    /// The player's phone dropped off at this time; they keep the turn for a few seconds (Wi-Fi blips, app switch).
    missing_since: Option<f64>,
}

/// How long a player can be disconnected before the turn moves on.
const GRACE: f64 = 6.0;

/// One guest: identity (the phone's token, else its seat) and the seat it's on right now.
struct Guest {
    who: String,
    seat: u64,
}

impl State {
    /// Guests connected right now, in the order they first arrived. A phone that reconnects keeps its place
    /// (same token); two tabs with the same token count once.
    fn guests(&mut self, now: f64) -> Vec<Guest> {
        let mut peers: Vec<_> = self.net.peers().into_iter().filter(|p| p.kind == PeerKind::Guest).collect();
        peers.sort_by_key(|p| p.id);
        let mut out: Vec<Guest> = Vec::new();
        for p in peers {
            let who = if p.token.is_empty() { format!("seat{}", p.id) } else { p.token.clone() };
            if out.iter().any(|g| g.who == who) {
                continue;
            }
            self.jam.seen.entry(who.clone()).or_insert(now);
            out.push(Guest { who, seat: p.id });
        }
        // Forget people who've been gone a while (a token that comes back later starts at the end).
        let present: Vec<&String> = out.iter().map(|g| &g.who).collect();
        let seen = &mut self.jam.seen;
        seen.retain(|k, t| present.contains(&k) || now - *t < 600.0);
        for g in &out {
            if let Some(t) = seen.get_mut(&g.who)
                && !present.contains(&&g.who)
            {
                *t = now;
            }
        }
        let seen = &self.jam.seen;
        out.sort_by(|a, b| seen.get(&a.who).partial_cmp(&seen.get(&b.who)).unwrap_or(std::cmp::Ordering::Equal));
        out
    }

    /// Once per frame: ends turns that ran out (or whose guest left) and hands the controls to the next guest.
    pub(crate) fn run_play(&mut self, now: f64) {
        if !self.comp.play.enabled || !self.net.running() {
            self.jam = Jam::default();
            return;
        }
        let guests = self.guests(now);
        let turn = self.comp.play.turn_secs.max(5) as f64;
        if let Some((who, started)) = self.jam.current.clone() {
            let present = guests.iter().any(|g| g.who == who);
            let gone = match (present, self.jam.missing_since) {
                (true, _) => {
                    self.jam.missing_since = None;
                    false
                }
                (false, None) => {
                    self.jam.missing_since = Some(now);
                    false
                }
                (false, Some(t)) => now - t > GRACE,
            };
            // One guest alone keeps the controls; the clock only runs while someone is waiting.
            if guests.len() <= 1 {
                self.jam.current = Some((who.clone(), now));
            }
            let over = now - started >= turn && guests.len() > 1;
            if gone || over {
                self.jam.served.push(who);
                self.jam.current = None;
                self.jam.missing_since = None;
            }
        }
        if self.jam.current.is_none() && !guests.is_empty() {
            self.jam.served.retain(|s| guests.iter().any(|g| g.who == *s));
            if guests.iter().all(|g| self.jam.served.contains(&g.who)) {
                self.jam.served.clear();
            }
            if let Some(next) = guests.iter().find(|g| !self.jam.served.contains(&g.who)) {
                self.jam.current = Some((next.who.clone(), now));
            }
        }
    }

    /// Host: end the current turn now (the next person gets the controls).
    pub fn skip_turn(&mut self) {
        if let Some((who, _)) = self.jam.current.take() {
            self.jam.served.push(who);
            self.jam.missing_since = None;
        }
    }

    /// Seat of the guest at the controls (what the page compares its own seat with).
    fn current_seat(&self, guests: &[Guest]) -> Option<u64> {
        let who = self.jam.current.as_ref()?.0.clone();
        guests.iter().find(|g| g.who == who).map(|g| g.seat)
    }

    /// Guests after the current player, in the order they'll get the controls (seat ids).
    pub fn play_queue(&mut self, now: f64) -> Vec<u64> {
        let guests = self.guests(now);
        let cur = self.jam.current.as_ref().map(|c| c.0.clone());
        let later = |served: bool| guests.iter().filter(|g| Some(&g.who) != cur.as_ref() && self.jam.served.contains(&g.who) == served).map(|g| g.seat).collect::<Vec<_>>();
        let mut q = later(false);
        q.extend(later(true));
        q
    }

    /// Seconds left in the current turn (None = no one is waiting, so no limit).
    pub fn play_left(&mut self, now: f64) -> Option<f64> {
        let started = self.jam.current.as_ref()?.1;
        (self.guests(now).len() > 1).then(|| (self.comp.play.turn_secs.max(5) as f64 - (now - started)).max(0.0))
    }

    /// What the play page needs, part of the remote state JSON.
    pub fn play_json(&mut self, now: f64) -> Value {
        let guests = self.guests(now);
        let current = self.current_seat(&guests);
        let (left, queue) = (self.play_left(now), self.play_queue(now));
        let p = &self.comp.play;
        json!({
            "on": p.enabled,
            "title": p.title,
            "turn": p.turn_secs,
            "current": current,
            "left": left,
            "queue": queue,
            "page": p.surface_page,
            "columns": if p.columns { self.comp.clip_columns } else { 0 },
            "surprise": p.surprise,
        })
    }

    /// May the guest in seat `from` send this command right now? Only the player, only the
    /// controls the show allows: the Control Surface (its chosen page), clip columns, Surprise.
    pub fn guest_may(&mut self, from: u64, cmd: &Value, now: f64) -> bool {
        if !self.comp.play.enabled {
            return false;
        }
        let guests = self.guests(now);
        let who = guests.iter().find(|g| g.seat == from).map(|g| g.who.clone());
        if who.is_none() || who != self.jam.current.as_ref().map(|c| c.0.clone()) {
            return false;
        }
        let p = &self.comp.play;
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

    /// The device label of whoever has the controls.
    pub fn current_player_label(&mut self, now: f64) -> Option<String> {
        let guests = self.guests(now);
        let seat = self.current_seat(&guests)?;
        Some(self.net.peers().iter().find(|p| p.id == seat).map(|p| p.device.clone()).unwrap_or_else(|| format!("#{seat}")))
    }

    /// Redraws the code layer after a style or title change (no-op when it isn't shown).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn refresh_play_qr(&mut self, now: f64) {
        if self.comp.layers.iter().any(|l| l.name == QR_LAYER) {
            self.toggle_play_qr(now);
            self.toggle_play_qr(now);
        }
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
        let path = dir.join(format!("scan-to-play-{:?}{}.png", self.comp.play.qr_style, if self.comp.play.qr_caption { "" } else { "-plain" }).to_lowercase());
        let Some(png) = qr_card(&url, &self.comp.play.title, self.comp.play.qr_style, self.comp.play.qr_caption) else { return };
        if std::fs::write(&path, png).is_err() {
            self.notice = Some("Couldn't write the QR picture".into());
            return;
        }
        let layer = self.comp.add_layer();
        layer.name = QR_LAYER.into();
        layer.fit = phosphor_engine::FitMode::Fit;
        let id = layer.id;
        let spec = phosphor_engine::SourceSpec::Image { name: QR_LAYER.into(), path: Some(path.to_string_lossy().to_string()) };
        self.launch_source(id, spec, Vec::new(), QR_LAYER.into(), now);
        self.selected = Some(id);
    }
}

/// The picture for the output: the code with the title and the address under it, in the chosen colours
/// (a card, or just the modules and words over the visuals), 16:9 safe.
#[cfg(not(target_arch = "wasm32"))]
fn qr_card(url: &str, title: &str, style: phosphor_engine::QrStyle, caption: bool) -> Option<Vec<u8>> {
    use image::{GenericImage, Rgba, RgbaImage};
    let (fg, bg) = style.colors();
    let clear = [0, 0, 0, 0];
    let qr = crate::ui::qr_image(url, 900, fg, bg.unwrap_or(clear))?;
    let (w, cap_h) = (1280u32, if caption { 300u32 } else { 0 });
    let h = qr.height() + cap_h + 40;
    let mut card = RgbaImage::from_pixel(w, h, Rgba(bg.unwrap_or(clear)));
    let x = (w - qr.width()) / 2;
    card.copy_from(&qr, x, 20).ok()?;
    // Short address for people to type if the camera won't: drop the scheme.
    let short = url.trim_start_matches("http://").trim_end_matches('/');
    let text = if title.trim().is_empty() { short.to_string() } else { format!("{}\n{short}", title.trim()) };
    // Words in the module colour; over the visuals (no card) white words get a dark edge to stay readable.
    let outline = if bg.is_none() && fg[0] > 128 { 0.06 } else { 0.0 };
    let style = phosphor_engine::TextStyle { text, size: 0.3, color: fg, outline, line_spacing: 1.1, ..crate::text::default_style() };
    if caption && let Ok(caption) = crate::text::render(&style, w, cap_h) {
        let y0 = qr.height() + 30;
        for (px, py, p) in caption.enumerate_pixels().map(|(x, y, p)| (x, y, *p)) {
            if p[3] > 0 && py + y0 < h {
                let under = *card.get_pixel(px, py + y0);
                let a = p[3] as u32;
                let mix = |c: u8, b: u8| ((c as u32 * a + b as u32 * (255 - a)) / 255) as u8;
                let alpha = (a + under[3] as u32 * (255 - a) / 255).min(255) as u8;
                card.put_pixel(px, py + y0, Rgba([mix(p[0], under[0]), mix(p[1], under[1]), mix(p[2], under[2]), alpha]));
            }
        }
    }
    let mut out = std::io::Cursor::new(Vec::new());
    card.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}
