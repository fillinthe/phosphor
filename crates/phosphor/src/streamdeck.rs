//! Elgato Stream Deck: its keys (and the Stream Deck +'s dials) are controller inputs like MIDI pads and knobs
//! (Learn, mappings, Control Surface), and each mapped key shows what it does: the control's name, lit while it's on.
//!
//! One worker thread looks for a deck every couple of seconds, reads its keys, and draws key pictures as the app
//! asks (`Decks::show`); a deck unplugged and plugged back in carries on with the same pictures.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use phosphor_engine::ControlSource;

use crate::control::Incoming;

/// Dial presses come in as keys from here (`ControlSource::StreamDeck { key: DIAL_KEYS + dial }`).
pub const DIAL_KEYS: u8 = 100;
/// A dial click moves its value this much.
const DIAL_STEP: f32 = 1.0 / 64.0;

/// What one key shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub label: String,
    pub on: bool,
}

/// The connection to Stream Decks while it's switched on.
pub struct Decks {
    looks: Sender<(u8, Look)>,
    sent: HashMap<u8, Look>,
    status: Arc<Mutex<String>>,
    connected: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

impl Decks {
    /// Starts looking for a deck; its input goes to `tx`.
    pub fn start(tx: Sender<Incoming>) -> Self {
        let (looks, rx) = std::sync::mpsc::channel();
        let status = Arc::new(Mutex::new("looking for a Stream Deck…".to_string()));
        let connected = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let (s, c, st) = (status.clone(), connected.clone(), stop.clone());
        let _ = std::thread::Builder::new().name("stream deck".into()).spawn(move || run(tx, rx, s, c, st));
        Self { looks, sent: HashMap::new(), status, connected, stop }
    }

    /// Sets what `key` shows (only sent when it changed).
    pub fn show(&mut self, key: u8, look: Look) {
        if self.sent.get(&key) != Some(&look) {
            self.sent.insert(key, look.clone());
            let _ = self.looks.send((key, look));
        }
    }

    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn status(&self) -> String {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

impl Drop for Decks {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// A key or dial event as controller input: keys and dial presses are pads (1 down, 0 up); a dial turn moves its
/// own value (kept in `dials`, 0..1, starting in the middle).
pub fn input(update: &elgato_streamdeck::DeviceStateUpdate, dials: &mut HashMap<u8, f32>) -> Option<(ControlSource, f32)> {
    use elgato_streamdeck::DeviceStateUpdate as U;
    let key = |key: u8| ControlSource::StreamDeck { key };
    Some(match *update {
        U::ButtonDown(k) => (key(k), 1.0),
        U::ButtonUp(k) => (key(k), 0.0),
        U::EncoderDown(d) => (key(DIAL_KEYS + d), 1.0),
        U::EncoderUp(d) => (key(DIAL_KEYS + d), 0.0),
        U::EncoderTwist(d, ticks) => {
            let v = dials.entry(d).or_insert(0.5);
            *v = (*v + ticks as f32 * DIAL_STEP).clamp(0.0, 1.0);
            (ControlSource::StreamDeckDial { dial: d }, *v)
        }
        _ => return None,
    })
}

/// A key's picture: its control's name on a dark key, on the accent colour while on.
pub fn picture(look: &Look, size: u32) -> image::RgbaImage {
    let [r, g, b, _] = crate::theme::accent().to_array();
    let bg = if look.on { format!("rgb({r},{g},{b})") } else { "rgb(28,28,30)".to_string() };
    let fg = if look.on { "rgb(10,10,10)" } else { "rgb(230,230,230)" };
    // Up to three short lines, words kept whole.
    let mut lines: Vec<String> = Vec::new();
    for word in look.label.split_whitespace() {
        match lines.last_mut() {
            Some(l) if l.chars().count() + 1 + word.chars().count() <= 9 => {
                l.push(' ');
                l.push_str(word);
            }
            _ => lines.push(word.chars().take(10).collect()),
        }
    }
    lines.truncate(3);
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let n = lines.len().max(1) as f32;
    let text: String = lines
        .iter()
        .enumerate()
        .map(|(i, l)| format!("<text x=\"36\" y=\"{:.1}\" text-anchor=\"middle\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"14\" font-weight=\"bold\" fill=\"{fg}\">{}</text>", 36.0 + (i as f32 - (n - 1.0) / 2.0) * 16.0 + 5.0, esc(l)))
        .collect();
    let svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"72\" height=\"72\"><rect width=\"72\" height=\"72\" fill=\"{bg}\"/>{text}</svg>");
    render(&svg, size).unwrap_or_else(|| image::RgbaImage::new(size, size))
}

/// An SVG at `size` square, with the system's fonts (loaded once).
fn render(svg: &str, size: u32) -> Option<image::RgbaImage> {
    use resvg::{tiny_skia, usvg};
    static FONTS: std::sync::OnceLock<Arc<usvg::fontdb::Database>> = std::sync::OnceLock::new();
    let fonts = FONTS.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    });
    let opt = usvg::Options { fontdb: fonts.clone(), ..Default::default() };
    let tree = usvg::Tree::from_str(svg, &opt).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
    let s = size as f32 / 72.0;
    resvg::render(&tree, tiny_skia::Transform::from_scale(s, s), &mut pixmap.as_mut());
    image::RgbaImage::from_raw(size, size, pixmap.data().to_vec())
}

fn set(status: &Mutex<String>, s: impl Into<String>) {
    if let Ok(mut st) = status.lock() {
        *st = s.into();
    }
}

fn run(tx: Sender<Incoming>, looks: Receiver<(u8, Look)>, status: Arc<Mutex<String>>, connected: Arc<AtomicBool>, stop: Arc<AtomicBool>) {
    use elgato_streamdeck::{StreamDeck, list_devices, new_hidapi};
    // Every key's latest look, so a deck plugged in later shows them all.
    let mut shown: HashMap<u8, Look> = HashMap::new();
    let mut dials: HashMap<u8, f32> = HashMap::new();
    while !stop.load(Ordering::Relaxed) {
        let found = new_hidapi().ok().and_then(|hid| {
            let (kind, serial) = list_devices(&hid).into_iter().next()?;
            let deck = StreamDeck::connect(&hid, kind, &serial).ok()?;
            Some((kind, deck))
        });
        let Some((kind, deck)) = found else {
            set(&status, "no Stream Deck found (plug one in; quit Elgato's Stream Deck app if it's open)");
            // Pictures asked for meanwhile are kept for when one arrives.
            while let Ok((k, l)) = looks.try_recv() {
                shown.insert(k, l);
            }
            for _ in 0..20 {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            continue;
        };
        let deck = Arc::new(deck);
        let product = deck.product().unwrap_or_else(|_| "Stream Deck".into());
        let dials_text = if kind.encoder_count() > 0 { format!(", {} dials", kind.encoder_count()) } else { String::new() };
        set(&status, format!("{product}: {} keys{dials_text}", kind.key_count()));
        connected.store(true, Ordering::Relaxed);
        let size = kind.key_image_format().size.0.max(1) as u32;
        let draw = |key: u8, look: &Look| {
            if key < kind.key_count() && kind.is_visual() {
                let _ = deck.set_button_image(key, image::DynamicImage::ImageRgba8(picture(look, size)));
            }
        };
        let _ = deck.set_brightness(70);
        let _ = deck.clear_all_button_images();
        for (k, l) in &shown {
            draw(*k, l);
        }
        let _ = deck.flush();
        let reader = deck.get_reader();
        loop {
            if stop.load(Ordering::Relaxed) {
                let _ = deck.clear_all_button_images();
                let _ = deck.flush();
                connected.store(false, Ordering::Relaxed);
                return;
            }
            match reader.read(Some(Duration::from_millis(30))) {
                Ok(updates) => {
                    for u in &updates {
                        if let Some((source, value)) = input(u, &mut dials) {
                            let _ = tx.send(Incoming { source, value });
                        }
                    }
                }
                Err(e) => {
                    set(&status, format!("the Stream Deck stopped ({e}); looking again…"));
                    break;
                }
            }
            let mut changed = false;
            while let Ok((k, l)) = looks.try_recv() {
                draw(k, &l);
                shown.insert(k, l);
                changed = true;
            }
            if changed {
                let _ = deck.flush();
            }
        }
        connected.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elgato_streamdeck::DeviceStateUpdate as U;

    #[test]
    fn keys_and_dials_become_controller_input() {
        let mut dials = HashMap::new();
        assert_eq!(input(&U::ButtonDown(3), &mut dials), Some((ControlSource::StreamDeck { key: 3 }, 1.0)));
        assert_eq!(input(&U::ButtonUp(3), &mut dials), Some((ControlSource::StreamDeck { key: 3 }, 0.0)));
        assert_eq!(input(&U::EncoderDown(1), &mut dials), Some((ControlSource::StreamDeck { key: DIAL_KEYS + 1 }, 1.0)));
        // A dial starts in the middle and moves a step a click, staying in 0..1.
        assert_eq!(input(&U::EncoderTwist(0, 2), &mut dials), Some((ControlSource::StreamDeckDial { dial: 0 }, 0.5 + 2.0 * DIAL_STEP)));
        assert_eq!(input(&U::EncoderTwist(0, -100), &mut dials).map(|x| x.1), Some(0.0));
        assert_eq!(input(&U::TouchScreenPress(1, 1), &mut dials), None);
        assert!(ControlSource::StreamDeck { key: 3 }.is_momentary() && !ControlSource::StreamDeckDial { dial: 0 }.is_momentary());
    }

    /// Key pictures: the name in light text on a dark key; lit = the accent colour.
    #[test]
    fn a_key_shows_its_control() {
        let off = picture(&Look { label: "Layer 1 opacity".into(), on: false }, 72);
        let on = picture(&Look { label: "Layer 1 opacity".into(), on: true }, 72);
        assert_eq!(off.dimensions(), (72, 72));
        assert_eq!(off.get_pixel(2, 2).0[..3], [28, 28, 30]);
        assert_ne!(on.get_pixel(2, 2), off.get_pixel(2, 2));
        let light = off.pixels().filter(|p| p.0[0] > 150).count();
        assert!(light > 30, "the name is drawn ({light} light pixels)");
        if let Ok(path) = std::env::var("PHOSPHOR_KEY_PICTURES") {
            let mut both = image::RgbaImage::new(150, 72);
            image::imageops::overlay(&mut both, &off, 0, 0);
            image::imageops::overlay(&mut both, &on, 78, 0);
            both.save(path).unwrap();
        }
        // Bigger keys (Stream Deck XL / +) get a bigger picture.
        assert_eq!(picture(&Look { label: "Go".into(), on: false }, 120).dimensions(), (120, 120));
    }

    /// Switched on with no deck plugged in: it says so (and doesn't hang); switched off, the thread stops.
    #[test]
    fn looks_for_a_deck_and_says_what_it_found() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut d = Decks::start(tx);
        d.show(0, Look { label: "Go".into(), on: true });
        let mut status = String::new();
        for _ in 0..100 {
            status = d.status();
            if !status.starts_with("looking") {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(status.starts_with("no Stream Deck found") || status.contains("keys"), "{status}");
        drop(d);
    }
}
