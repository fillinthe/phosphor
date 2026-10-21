//! Audience play: guests on the play page take turns at the controls. Settings are `Composition::play`;
//! this is the queue (who plays now, who's next) and what a guest is allowed to touch.

use phosphor_engine::ControlTarget;
use serde_json::{Value, json};

use super::State;
use crate::net::PeerKind;

/// Name of the layer that shows the QR code on the output.
pub const QR_LAYER: &str = "Scan to play";
/// Name of the text layer that shows who's playing.
pub const NOW_PLAYING_LAYER: &str = "Now playing";

#[derive(Default)]
pub struct Jam {
    /// Who has the controls right now (up to `AudiencePlay::players` at once).
    pub current: Vec<Player>,
    /// Identities that already had a turn this round (cleared when everyone waiting has played).
    served: Vec<String>,
    /// When each identity first showed up: the line is in arrival order even across reconnects.
    seen: std::collections::HashMap<String, f64>,
    /// When each identity's last turn ended (they sit out `AudiencePlay::rest_secs` from then).
    rested: std::collections::HashMap<String, f64>,
}

/// One person at the controls.
#[derive(Clone, Debug)]
pub struct Player {
    pub who: String,
    pub started: f64,
    /// Which of the simultaneous places they hold (decides their Surface page).
    pub slot: usize,
    /// Their phone dropped off at this time; they keep the turn for a few seconds (Wi-Fi blips, app switch).
    missing_since: Option<f64>,
}

/// How long a player can be disconnected before the turn moves on.
const GRACE: f64 = 6.0;

/// One guest: identity (the phone's token, else its seat) and the seat it's on right now.
struct Guest {
    who: String,
    seat: u64,
}

/// What the panel and the page show about a player.
pub struct PlayerStatus {
    pub who: String,
    pub seat: u64,
    pub label: String,
    pub page: Option<usize>,
    pub left: Option<f64>,
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
        let present: Vec<String> = out.iter().map(|g| g.who.clone()).collect();
        self.jam.seen.retain(|k, t| present.contains(k) || now - *t < 600.0);
        let seen = &self.jam.seen;
        out.sort_by(|a, b| seen.get(&a.who).partial_cmp(&seen.get(&b.who)).unwrap_or(std::cmp::Ordering::Equal));
        out
    }

    /// How many people play at once.
    fn player_slots(&self) -> usize {
        self.comp.play.players.clamp(1, 8) as usize
    }

    /// The Surface page a place plays: the chosen page, else (several players) one page per place.
    pub fn slot_page(&self, slot: usize) -> Option<usize> {
        if let Some(p) = self.comp.play.place_pages.get(slot).copied().flatten() {
            return Some(p);
        }
        if let Some(p) = self.comp.play.surface_page {
            return Some(p);
        }
        (self.player_slots() > 1).then(|| slot % self.comp.surface.pages.len().max(1))
    }

    /// Once per frame: ends turns that ran out (or whose guest left) and hands free places to the next guests.
    pub(crate) fn run_play(&mut self, now: f64) {
        if !self.comp.play.enabled || !self.net.running() {
            self.jam = Jam::default();
            return;
        }
        let guests = self.guests(now);
        let turn = self.comp.play.turn_secs.max(5) as f64;
        let rest = self.comp.play.rest_secs as f64;
        let slots = self.player_slots();
        self.jam.rested.retain(|_, t| now - *t < rest);
        let playing: Vec<String> = self.jam.current.iter().map(|p| p.who.clone()).collect();
        // People who could take the controls now: not playing, not resting after their last turn.
        let rested = &self.jam.rested;
        let guests: Vec<Guest> = guests.into_iter().filter(|g| !rested.contains_key(&g.who) || playing.contains(&g.who)).collect();
        let waiting = guests.iter().filter(|g| !playing.contains(&g.who)).count();
        // Turns end when the clock runs out (only while someone waits) or the phone has been gone a while.
        let mut ended: Vec<String> = Vec::new();
        for p in &mut self.jam.current {
            let present = guests.iter().any(|g| g.who == p.who);
            let gone = match (present, p.missing_since) {
                (true, _) => {
                    p.missing_since = None;
                    false
                }
                (false, None) => {
                    p.missing_since = Some(now);
                    false
                }
                (false, Some(t)) => now - t > GRACE,
            };
            // With no rest and nobody waiting, the clock doesn't run.
            if waiting == 0 && rest <= 0.0 {
                p.started = now;
            }
            if gone || (now - p.started >= turn && (waiting > 0 || rest > 0.0)) {
                ended.push(p.who.clone());
            }
        }
        for who in &ended {
            self.jam.current.retain(|p| p.who != *who);
            self.jam.served.push(who.clone());
            if rest > 0.0 {
                self.jam.rested.insert(who.clone(), now);
            }
        }
        // Someone whose turn just ended rests before they can be picked again (even this same frame).
        let guests: Vec<Guest> = guests.into_iter().filter(|g| rest <= 0.0 || !ended.contains(&g.who)).collect();
        // Fewer places than before (setting lowered): the newest players step back into the line.
        while self.jam.current.len() > slots {
            if let Some(p) = self.jam.current.pop() {
                self.jam.served.retain(|s| *s != p.who);
            }
        }
        // Fill free places from the line: unserved first; when everyone waiting has played, a new round.
        self.jam.served.retain(|s| guests.iter().any(|g| g.who == *s));
        let mut guard = 0;
        while self.jam.current.len() < slots && guard < 16 {
            guard += 1;
            let playing: Vec<String> = self.jam.current.iter().map(|p| p.who.clone()).collect();
            let next = guests.iter().find(|g| !playing.contains(&g.who) && !self.jam.served.contains(&g.who));
            match next {
                Some(g) => {
                    let taken: Vec<usize> = self.jam.current.iter().map(|p| p.slot).collect();
                    let slot = (0..slots).find(|s| !taken.contains(s)).unwrap_or(0);
                    self.jam.current.push(Player { who: g.who.clone(), started: now, slot, missing_since: None });
                }
                None => {
                    let anyone_waiting = guests.iter().any(|g| !playing.contains(&g.who));
                    if anyone_waiting && !self.jam.served.is_empty() {
                        self.jam.served.clear();
                    } else {
                        break;
                    }
                }
            }
        }
    }

    /// Keeps the "Now playing" text layer in step with the players (a line at the bottom of the picture);
    /// removes it when the setting is off or nobody plays.
    pub(crate) fn update_now_playing(&mut self, now: f64) {
        let names: Vec<String> = if self.comp.play.enabled && self.comp.play.show_names {
            self.players(now).into_iter().filter(|p| !p.label.is_empty()).map(|p| p.label).collect()
        } else {
            Vec::new()
        };
        let existing = self.comp.layers.iter().position(|l| l.name == NOW_PLAYING_LAYER);
        if names.is_empty() {
            if let Some(i) = existing {
                let id = self.comp.layers[i].id;
                crate::panels::remove_layer(self, id);
            }
            return;
        }
        let text = format!("Now playing: {}", names.join("  ·  "));
        let style = phosphor_engine::TextStyle { text, size: 0.07, outline: 0.08, ..crate::text::default_style() };
        match existing {
            Some(i) => {
                if let phosphor_engine::SourceSpec::Text(cur) = &self.comp.layers[i].source
                    && cur.text == style.text
                {
                    return;
                }
                self.comp.layers[i].source = phosphor_engine::SourceSpec::Text(style);
            }
            None => {
                let layer = self.comp.add_layer();
                layer.name = NOW_PLAYING_LAYER.into();
                layer.source = phosphor_engine::SourceSpec::Text(style);
                // Along the bottom edge, out of the way of the picture.
                layer.transform.y = -0.42;
            }
        }
    }

    /// Host: end one player's turn now (or everyone's with `None`); the next people get the controls.
    pub fn skip_turn(&mut self, who: Option<&str>, now: f64) {
        let ended: Vec<String> = self.jam.current.iter().filter(|p| who.is_none_or(|w| p.who == w)).map(|p| p.who.clone()).collect();
        for w in ended {
            self.jam.current.retain(|p| p.who != w);
            if self.comp.play.rest_secs > 0 {
                self.jam.rested.insert(w.clone(), now);
            }
            self.jam.served.push(w);
        }
    }

    /// Seats sitting out after a turn, with the seconds until they may play again.
    pub fn resting(&mut self, now: f64) -> Vec<(u64, f64)> {
        let rest = self.comp.play.rest_secs as f64;
        let guests = self.guests(now);
        let rested = &self.jam.rested;
        guests.iter().filter_map(|g| rested.get(&g.who).map(|t| (g.seat, (rest - (now - t)).max(0.0)))).filter(|(_, left)| *left > 0.0).collect()
    }

    /// Guests not at the controls, in the order they'll get them (seat ids).
    pub fn play_queue(&mut self, now: f64) -> Vec<u64> {
        let guests = self.guests(now);
        let playing: Vec<String> = self.jam.current.iter().map(|p| p.who.clone()).collect();
        let resting: Vec<u64> = self.resting(now).into_iter().map(|(seat, _)| seat).collect();
        let guests: Vec<Guest> = guests.into_iter().filter(|g| !resting.contains(&g.seat)).collect();
        let later = |served: bool| {
            guests.iter().filter(|g| !playing.contains(&g.who) && self.jam.served.contains(&g.who) == served).map(|g| g.seat).collect::<Vec<_>>()
        };
        let mut q = later(false);
        q.extend(later(true));
        q
    }

    /// Everyone at the controls, with their seat, device, page and time left (None = nobody waiting, no clock).
    pub fn players(&mut self, now: f64) -> Vec<PlayerStatus> {
        let guests = self.guests(now);
        let playing: Vec<String> = self.jam.current.iter().map(|p| p.who.clone()).collect();
        let resting: Vec<u64> = self.resting(now).into_iter().map(|(seat, _)| seat).collect();
        let waiting = guests.iter().filter(|g| !playing.contains(&g.who) && !resting.contains(&g.seat)).count();
        let clock_runs = waiting > 0 || self.comp.play.rest_secs > 0;
        let turn = self.comp.play.turn_secs.max(5) as f64;
        let peers = self.net.peers();
        let mut out = Vec::new();
        for p in &self.jam.current {
            let Some(g) = guests.iter().find(|g| g.who == p.who) else { continue };
            let label = peers
                .iter()
                .find(|x| x.id == g.seat)
                .map(|x| if x.name.is_empty() { x.device.clone() } else { x.name.clone() })
                .unwrap_or_else(|| format!("#{}", g.seat));
            let left = clock_runs.then(|| (turn - (now - p.started)).max(0.0));
            out.push(PlayerStatus { who: p.who.clone(), seat: g.seat, label, page: self.slot_page(p.slot), left });
        }
        out
    }

    /// What the play page needs, part of the remote state JSON.
    pub fn play_json(&mut self, now: f64) -> Value {
        let players = self.players(now);
        let queue = self.play_queue(now);
        let resting: Vec<Value> = self.resting(now).into_iter().map(|(seat, left)| json!([seat, left])).collect();
        let soonest = players.iter().filter_map(|p| p.left).fold(None, |m: Option<f64>, l| Some(m.map_or(l, |m| m.min(l))));
        let p = &self.comp.play;
        let list: Vec<Value> = players.iter().map(|x| json!({"seat": x.seat, "page": x.page, "left": x.left})).collect();
        json!({
            "on": p.enabled,
            "title": p.title,
            "turn": p.turn_secs,
            "players": list,
            "current": players.first().map(|x| x.seat),
            "left": soonest,
            "queue": queue,
            "resting": resting,
            "rest": p.rest_secs,
            "page": p.surface_page,
            "columns": if p.columns { self.comp.clip_columns } else { 0 },
            "surprise": p.surprise,
        })
    }

    /// May the guest in seat `from` send this command right now? Only a player, only the
    /// controls the show allows: the Control Surface (their page), clip columns, Surprise.
    pub fn guest_may(&mut self, from: u64, cmd: &Value, now: f64) -> bool {
        if !self.comp.play.enabled {
            return false;
        }
        let guests = self.guests(now);
        let Some(who) = guests.iter().find(|g| g.seat == from).map(|g| g.who.clone()) else { return false };
        let Some(slot) = self.jam.current.iter().find(|p| p.who == who).map(|p| p.slot) else { return false };
        let page = self.slot_page(slot);
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
                self.comp.surface.widgets.iter().any(|w| w.id == widget && w.guests && page.is_none_or(|pg| w.page == pg))
            }
            _ => false,
        }
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
