//! What feeds a layer (`SourceSpec`), text layers, and clip slots.

use serde::{Deserialize, Serialize};

use crate::shaders;

use super::TriggerMode;

/// What feeds a layer. Media is referenced, never embedded: the app resolves it into
/// GPU textures via `Engine::upload_rgba` / `Engine::upload_nv12`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum SourceSpec {
    #[default]
    None,
    Generator(String),
    Image { name: String, path: Option<String> },
    Movie { name: String, path: Option<String> },
    /// A live input: camera, capture card, or a network stream. `format` is the FFmpeg input
    /// ("avfoundation" for cameras, empty to detect from `url`), `url` the device name or address.
    Live { name: String, format: String, url: String },
    /// Words drawn by the app into the layer's picture (see `TextStyle`).
    Text(TextStyle),
    /// Another layer's or group's finished picture, or the program (`MASTER_ID`) — the previous frame's
    /// when it's drawn later (feedback). `name` is for display. `before_fx`: a layer's picture before its effects.
    Feed {
        id: u64,
        name: String,
        #[serde(default)]
        before_fx: bool,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
}

/// A text layer: what it says and how it looks. Drawn at the composition size whenever it changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
    pub text: String,
    /// "builtin:sans" / "builtin:mono", or a font file path with an optional "#index" (collections).
    pub font: String,
    /// Letter height as a fraction of the picture height.
    pub size: f32,
    pub color: [u8; 4],
    pub align: TextAlign,
    /// Dark outline width as a fraction of the letter height (0 = none).
    pub outline: f32,
    pub line_spacing: f32,
    /// Text in parts (a file's lines, sentences or words), shown one at a time: `text` is the part showing.
    pub parts: Vec<String>,
    pub part: usize,
    /// How text is cut into parts (from a file, or "Split").
    pub chunk_by: ChunkBy,
    /// Live text: the time of day, a countdown or a stopwatch, kept up to date (replaces the words).
    pub live: LiveText,
    /// When the stopwatch was (re)started, in app seconds (not saved: it starts again with the app).
    #[serde(skip)]
    pub live_start: f64,
    /// `ClockAt`: the place's name; `Headlines`: the news feed's address.
    pub live_source: String,
}

/// Text that keeps itself up to date.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum LiveText {
    #[default]
    Off,
    /// The time of day (HH:MM:SS).
    Clock,
    /// Time left until a time of day (seconds since midnight), then 00:00.
    Countdown(u32),
    /// Time since it was started (Restart sets it going from 0).
    Stopwatch,
    /// The last QR code video tracking read (`Composition::seen_code`).
    Code,
    /// The text video tracking reads in the picture (`Composition::seen_text`).
    Read,
    /// The time of day somewhere else: its offset from UTC in seconds (the place is `TextStyle::live_source`).
    ClockAt(i32),
    /// A news feed's headlines (`TextStyle::live_source`), each shown for this many seconds.
    Headlines(u32),
}

impl LiveText {
    /// The words at local time of day `tod` (seconds since midnight), `running` seconds since the stopwatch started.
    pub fn words(self, tod: u32, running: f64) -> Option<String> {
        let hms = |s: u64| if s >= 3600 { format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{:02}:{:02}", s / 60, s % 60) };
        match self {
            LiveText::Off => None,
            LiveText::Clock => Some(format!("{:02}:{:02}:{:02}", tod / 3600, tod / 60 % 60, tod % 60)),
            LiveText::Countdown(at) => Some(hms(if at > tod { (at - tod) as u64 } else { 0 })),
            LiveText::Stopwatch => Some(hms(running.max(0.0) as u64)),
            // These come from the project (`run_live_text`).
            LiveText::Code | LiveText::Read | LiveText::Headlines(_) => None,
            // (Given the time in UTC: `clock_at`.)
            LiveText::ClockAt(_) => None,
        }
    }
}

impl LiveText {
    /// `ClockAt`'s words at `utc` (seconds since midnight UTC).
    pub fn clock_at(utc: u32, offset: i32) -> String {
        let t = (utc as i64 + offset as i64).rem_euclid(86_400) as u32;
        format!("{:02}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60)
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        Self { text: "PHOSPHOR".into(), font: "builtin:sans".into(), size: 0.18, color: [255, 255, 255, 255], align: TextAlign::Center, outline: 0.0, line_spacing: 1.0, parts: Vec::new(), part: 0, chunk_by: ChunkBy::Line, live: LiveText::Off, live_start: 0.0, live_source: String::new() }
    }
}

/// How text is cut into parts shown one at a time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkBy {
    #[default]
    Line,
    Sentence,
    Word,
    /// Blocks separated by an empty line (verses).
    Paragraph,
}

impl ChunkBy {
    pub const ALL: [ChunkBy; 4] = [ChunkBy::Line, ChunkBy::Sentence, ChunkBy::Word, ChunkBy::Paragraph];

    pub fn name(self) -> &'static str {
        match self {
            ChunkBy::Line => "Lines",
            ChunkBy::Sentence => "Sentences",
            ChunkBy::Word => "Words",
            ChunkBy::Paragraph => "Paragraphs",
        }
    }

    /// The non-empty parts of `text`.
    pub fn split(self, text: &str) -> Vec<String> {
        let parts: Vec<String> = match self {
            ChunkBy::Line => text.lines().map(str::to_string).collect(),
            ChunkBy::Word => text.split_whitespace().map(str::to_string).collect(),
            ChunkBy::Paragraph => text.replace("\r\n", "\n").split("\n\n").map(str::to_string).collect(),
            ChunkBy::Sentence => {
                let mut out = Vec::new();
                let mut cur = String::new();
                for c in text.chars() {
                    cur.push(if c == '\n' { ' ' } else { c });
                    if matches!(c, '.' | '!' | '?') {
                        out.push(std::mem::take(&mut cur));
                    }
                }
                out.push(cur);
                out
            }
        };
        parts.into_iter().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect()
    }
}

impl TextStyle {
    /// Cuts `text` into parts and shows the first (no parts: shows `text` whole).
    pub fn set_parts(&mut self, text: &str, by: ChunkBy) {
        self.parts = by.split(text);
        self.part = 0;
        self.text = self.parts.first().cloned().unwrap_or_else(|| text.to_string());
    }

    /// The next (or previous) part, round at the ends. False when there are no parts.
    pub fn step_part(&mut self, forward: bool) -> bool {
        let n = self.parts.len();
        if n == 0 {
            return false;
        }
        self.part = if forward { (self.part + 1) % n } else { (self.part + n - 1) % n };
        self.text = self.parts[self.part].clone();
        true
    }
}

impl SourceSpec {
    /// For logs and reports: the label, and for a file where it is ("Movie clip.mov at /Volumes/SSD/clip.mov").
    pub fn describe(&self) -> String {
        match self {
            SourceSpec::Image { path: Some(p), .. } | SourceSpec::Movie { path: Some(p), .. } => format!("{} at {p}", self.label()),
            _ => self.label(),
        }
    }

    pub fn label(&self) -> String {
        match self {
            SourceSpec::None => "No source".into(),
            SourceSpec::Generator(id) => shaders::generator(id).map_or(id.clone(), |g| g.name.to_string()),
            SourceSpec::Image { name, .. } | SourceSpec::Movie { name, .. } | SourceSpec::Live { name, .. } => name.clone(),
            SourceSpec::Feed { name, .. } => format!("From: {name}"),
            SourceSpec::Text(t) => {
                let first = t.text.lines().next().unwrap_or("").trim();
                let short: String = first.chars().take(24).collect();
                format!("Text: {}{}", short, if first.chars().count() > 24 { "…" } else { "" })
            }
        }
    }
}

/// A clip slot: a source ready to be triggered onto its layer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Clip {
    pub id: u64,
    pub name: String,
    pub source: SourceSpec,
    pub generator_values: Vec<f32>,
    /// Modulation on the generator's parameters, put on the layer with the clip.
    pub mods: Vec<crate::ParamMod>,
    /// Normal / toggle / flash / piano.
    pub trigger: TriggerMode,
    /// The clip's own playback settings and effects (Resolume-style); None = it uses the layer's.
    pub own: Option<ClipSettings>,
    /// This clip plays without sound (wherever it's triggered).
    pub muted: bool,
    /// A picture chosen for its cell in the grid (a PNG path); empty = from the source.
    pub thumb: String,
}

/// What a clip remembers when it keeps its own settings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClipSettings {
    pub rate: f32,
    pub direction: crate::PlayDirection,
    pub loop_in: f32,
    pub loop_out: f32,
    pub loop_beats: f32,
    pub sync_beats: f32,
    pub effects: Vec<crate::EffectState>,
}

impl Clip {
    pub fn is_empty(&self) -> bool {
        self.source == SourceSpec::None
    }
}

#[cfg(test)]
mod text_tests {
    use super::*;

    #[test]
    fn text_moves_the_ready_made_ways() {
        let mut l = crate::LayerState::default();
        l.set_text_motion(TextMotion::ScrollLeft);
        assert_eq!(l.text_motion(), TextMotion::ScrollLeft);
        let inputs = |phase: f32| crate::control::ModInputs { beats: phase * 8.0, time: phase, ..Default::default() };
        let x = |l: &crate::LayerState, p: f32| crate::control::modulate(l.transform.x, -1.0, 1.0, l.transform.x_mod.as_ref(), &inputs(p));
        assert!(x(&l, 0.1) > x(&l, 0.6), "moves left");
        l.set_text_motion(TextMotion::Fade);
        assert!(l.transform.x_mod.is_none() && l.opacity_mod.is_some(), "one at a time");
        l.set_text_motion(TextMotion::Still);
        assert_eq!(l.text_motion(), TextMotion::Still);
        assert!(l.opacity_mod.is_none());
    }

    #[test]
    fn live_text_reads_the_time() {
        assert_eq!(LiveText::Clock.words(3 * 3600 + 4 * 60 + 5, 0.0).as_deref(), Some("03:04:05"));
        assert_eq!(LiveText::Countdown(18 * 3600).words(18 * 3600 - 75, 0.0).as_deref(), Some("01:15"));
        assert_eq!(LiveText::Countdown(18 * 3600).words(18 * 3600 + 5, 0.0).as_deref(), Some("00:00"), "stays at zero");
        assert_eq!(LiveText::Stopwatch.words(0, 3725.4).as_deref(), Some("01:02:05"));
        assert_eq!(LiveText::Off.words(0, 0.0), None);
        assert_eq!(LiveText::clock_at(23 * 3600, 2 * 3600), "01:00:00", "past midnight there");
        assert_eq!(LiveText::clock_at(3600, -5 * 3600), "20:00:00", "before midnight there");
    }

    #[test]
    fn text_in_parts_steps_round() {
        let mut t = TextStyle::default();
        let lyrics = "Hello there. How are you?\nFine!\n\nSecond verse";
        assert_eq!(ChunkBy::Sentence.split(lyrics), vec!["Hello there.", "How are you?", "Fine!", "Second verse"]);
        assert_eq!(ChunkBy::Paragraph.split(lyrics).len(), 2);
        assert_eq!(ChunkBy::Word.split(lyrics).len(), 8);
        t.set_parts(lyrics, ChunkBy::Line);
        assert_eq!(t.text, "Hello there. How are you?");
        assert!(t.step_part(true));
        assert_eq!(t.text, "Fine!");
        t.step_part(true);
        t.step_part(true);
        assert_eq!((t.part, t.text.as_str()), (0, "Hello there. How are you?"), "round at the end");
        t.step_part(false);
        assert_eq!(t.text, "Second verse");
    }
}

/// Ready-made movement for a text layer (or any layer): it sets the layer's position, size or opacity motion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextMotion {
    #[default]
    Still,
    /// Across from right to left, every 8 beats (a ticker).
    ScrollLeft,
    /// Up from the bottom, every 8 beats (credits).
    ScrollUp,
    /// A kick of size on every beat.
    Pulse,
    /// In and out over 4 beats.
    Fade,
}

impl TextMotion {
    pub const ALL: [TextMotion; 5] = [TextMotion::Still, TextMotion::ScrollLeft, TextMotion::ScrollUp, TextMotion::Pulse, TextMotion::Fade];

    pub fn name(self) -> &'static str {
        match self {
            TextMotion::Still => "Still",
            TextMotion::ScrollLeft => "Scroll left",
            TextMotion::ScrollUp => "Scroll up",
            TextMotion::Pulse => "Pulse on the beat",
            TextMotion::Fade => "Fade in and out",
        }
    }
}

impl crate::LayerState {
    /// Sets the layer moving one of the ready-made ways (Still takes those movements off again).
    pub fn set_text_motion(&mut self, m: TextMotion) {
        use crate::{ModSource, Modulation};
        let synced = |source, beats: f32, min: f32, max: f32| Some(Modulation { source, sync: true, rate: beats, range: true, min, max, amount: 1.0, ..Default::default() });
        let t = &mut self.transform;
        (t.x_mod, t.y_mod, t.scale_mod, self.opacity_mod) = (None, None, None, None);
        match m {
            TextMotion::Still => {}
            // X and Y run -1..1: from just off one side to just off the other.
            TextMotion::ScrollLeft => t.x_mod = synced(ModSource::LfoSaw, 8.0, 1.0, 0.0),
            TextMotion::ScrollUp => t.y_mod = synced(ModSource::LfoSaw, 8.0, 0.0, 1.0),
            TextMotion::Pulse => t.scale_mod = Some(Modulation { source: ModSource::Beat, invert: true, amount: 0.02, ..Default::default() }),
            TextMotion::Fade => self.opacity_mod = synced(ModSource::LfoSine, 4.0, 0.0, 1.0),
        }
    }

    /// Which ready-made movement the layer has now (Still when it's anything else).
    pub fn text_motion(&self) -> TextMotion {
        TextMotion::ALL.into_iter().find(|m| {
            let mut probe = self.clone();
            probe.set_text_motion(*m);
            (probe.transform.x_mod, probe.transform.y_mod, probe.transform.scale_mod, probe.opacity_mod) == (self.transform.x_mod, self.transform.y_mod, self.transform.scale_mod, self.opacity_mod)
        }).unwrap_or(TextMotion::Still)
    }
}
