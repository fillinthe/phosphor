//! LED / DMX pixel mapping (MadMapper / Resolume Arena style): fixtures placed on the composition, each
//! pixel sampled from the program and sent as DMX channels over Art-Net or sACN. This file is the plan
//! (where each pixel samples, which channels it fills); the app samples and sends.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DmxProtocol {
    #[default]
    ArtNet,
    Sacn,
    /// A USB interface speaking the Enttec DMX USB Pro protocol (one universe; `target` = its serial port).
    UsbPro,
    /// A plain FTDI cable with no timing of its own (Enttec Open DMX…): the app makes the DMX signal.
    OpenDmx,
}

impl DmxProtocol {
    pub const ALL: [DmxProtocol; 4] = [DmxProtocol::ArtNet, DmxProtocol::Sacn, DmxProtocol::UsbPro, DmxProtocol::OpenDmx];

    pub fn name(self) -> &'static str {
        match self {
            DmxProtocol::ArtNet => "Art-Net",
            DmxProtocol::Sacn => "sACN (E1.31)",
            DmxProtocol::UsbPro => "USB (Enttec Pro)",
            DmxProtocol::OpenDmx => "USB (Open DMX)",
        }
    }
}

/// Channel order of one LED pixel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PixelOrder {
    #[default]
    Rgb,
    Grb,
    Brg,
    Rgbw,
    Grbw,
    /// One channel: brightness (dimmers, single-colour strips).
    Mono,
}

impl PixelOrder {
    pub const ALL: [PixelOrder; 6] = [PixelOrder::Rgb, PixelOrder::Grb, PixelOrder::Brg, PixelOrder::Rgbw, PixelOrder::Grbw, PixelOrder::Mono];

    pub fn name(self) -> &'static str {
        match self {
            PixelOrder::Rgb => "RGB",
            PixelOrder::Grb => "GRB",
            PixelOrder::Brg => "BRG",
            PixelOrder::Rgbw => "RGBW",
            PixelOrder::Grbw => "GRBW",
            PixelOrder::Mono => "Dimmer (1 ch)",
        }
    }

    pub fn channels(self) -> usize {
        match self {
            PixelOrder::Rgb | PixelOrder::Grb | PixelOrder::Brg => 3,
            PixelOrder::Rgbw | PixelOrder::Grbw => 4,
            PixelOrder::Mono => 1,
        }
    }

    /// The pixel's channel values for a colour (white = the common part of R, G and B).
    fn write(self, [r, g, b]: [u8; 3], out: &mut [u8]) {
        let w = r.min(g).min(b);
        match self {
            PixelOrder::Rgb => out.copy_from_slice(&[r, g, b]),
            PixelOrder::Grb => out.copy_from_slice(&[g, r, b]),
            PixelOrder::Brg => out.copy_from_slice(&[b, r, g]),
            PixelOrder::Rgbw => out.copy_from_slice(&[r - w, g - w, b - w, w]),
            PixelOrder::Grbw => out.copy_from_slice(&[g - w, r - w, b - w, w]),
            PixelOrder::Mono => out[0] = ((r as u32 * 54 + g as u32 * 183 + b as u32 * 19) >> 8) as u8,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FixtureShape {
    /// A line of pixels from `points[0]` to `points[1]`.
    #[default]
    Strip,
    /// `cols × rows` pixels over the quad `points` (TL, TR, BR, BL), wired row by row.
    Grid,
    /// A moving head: colour from the picture at `points[0]`, pan / tilt from its own position and movement.
    MovingHead,
    /// `cols` pixels round a circle: centre `points[0]`, the first pixel at `points[1]`.
    Ring,
    /// `cols` pixels along a curve: a Bézier from `points[0]` to `points[3]`, bent towards `points[1]` and `points[2]`.
    Curve,
}

impl FixtureShape {
    pub const ALL: [FixtureShape; 5] = [FixtureShape::Strip, FixtureShape::Curve, FixtureShape::Ring, FixtureShape::Grid, FixtureShape::MovingHead];

    pub fn name(self) -> &'static str {
        match self {
            FixtureShape::Strip => "Strip",
            FixtureShape::Grid => "Grid / matrix",
            FixtureShape::MovingHead => "Moving head",
            FixtureShape::Ring => "Ring",
            FixtureShape::Curve => "Curve",
        }
    }
}

/// A moving head's channels, in order (the common layouts; set the head to the matching mode).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeadLayout {
    /// Pan, pan fine, tilt, tilt fine, dimmer, red, green, blue.
    #[default]
    Rgb8,
    /// Pan, pan fine, tilt, tilt fine, dimmer, red, green, blue, white.
    Rgbw9,
    /// Pan, pan fine, tilt, tilt fine, shutter (open), dimmer, red, green, blue, white.
    ShutterRgbw10,
}

impl HeadLayout {
    pub const ALL: [HeadLayout; 3] = [HeadLayout::Rgb8, HeadLayout::Rgbw9, HeadLayout::ShutterRgbw10];

    pub fn name(self) -> &'static str {
        match self {
            HeadLayout::Rgb8 => "8 ch: pan/tilt 16-bit, dim, RGB",
            HeadLayout::Rgbw9 => "9 ch: pan/tilt 16-bit, dim, RGBW",
            HeadLayout::ShutterRgbw10 => "10 ch: pan/tilt 16-bit, shutter, dim, RGBW",
        }
    }

    pub fn channels(self) -> usize {
        match self {
            HeadLayout::Rgb8 => 8,
            HeadLayout::Rgbw9 => 9,
            HeadLayout::ShutterRgbw10 => 10,
        }
    }
}

/// How a moving head moves on its own, around its pan / tilt, timed in beats.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeadMotion {
    #[default]
    Still,
    Circle,
    FigureEight,
    /// Pan back and forth.
    Sweep,
}

impl HeadMotion {
    pub const ALL: [HeadMotion; 4] = [HeadMotion::Still, HeadMotion::Circle, HeadMotion::FigureEight, HeadMotion::Sweep];

    pub fn name(self) -> &'static str {
        match self {
            HeadMotion::Still => "Still",
            HeadMotion::Circle => "Circle",
            HeadMotion::FigureEight => "Figure 8",
            HeadMotion::Sweep => "Sweep",
        }
    }

    /// Offset from the centre (−1..1 on each axis) at phase `t` (0..1 = one cycle).
    fn offset(self, t: f64) -> (f64, f64) {
        let a = t * std::f64::consts::TAU;
        match self {
            HeadMotion::Still => (0.0, 0.0),
            HeadMotion::Circle => (a.cos(), a.sin()),
            HeadMotion::FigureEight => (a.sin(), (2.0 * a).sin() * 0.5),
            HeadMotion::Sweep => (a.sin(), 0.0),
        }
    }
}

/// A moving head channel after its layout (gobo, prism, zoom, strobe, focus…): a name and a level.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExtraChannel {
    pub name: String,
    /// 0..1, sent as 0–255.
    pub value: f32,
}

/// One LED strip, matrix or set of lights.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fixture {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    pub shape: FixtureShape,
    /// Placement on the composition (0..1, y down): strip = start, end; grid = TL, TR, BR, BL.
    pub points: [[f32; 2]; 4],
    /// Strip: number of pixels; grid: columns.
    pub cols: u32,
    pub rows: u32,
    /// Grid wiring: every other row runs backwards (the usual LED matrix zig-zag).
    pub zigzag: bool,
    /// Grid wiring: column by column (top to bottom) instead of row by row.
    #[serde(default)]
    pub by_columns: bool,
    pub order: PixelOrder,
    /// First universe (Art-Net counts from 0, sACN from 1) and channel (1–512). Long fixtures continue
    /// into the next universes without splitting a pixel.
    pub universe: u16,
    pub channel: u16,
    pub brightness: f32,
    /// LED gamma (2.2 makes fades look even on most strips; 1 = linear).
    pub gamma: f32,
    /// Moving heads: channel layout, where it points (0..1 of its pan / tilt range), its movement, how far it
    /// moves (0..1) and how many beats one cycle takes, and its dimmer.
    pub head: HeadLayout,
    pub pan: f32,
    pub tilt: f32,
    pub motion: HeadMotion,
    pub motion_size: f32,
    pub motion_beats: f32,
    pub dimmer: f32,
    /// Moving heads: channels after the layout's, in order (gobo, prism, zoom…), each a control of its own.
    pub extra: Vec<ExtraChannel>,
    /// Each pixel takes the average of its own patch of the picture (9 samples across its cell) instead of the one
    /// point at its centre: steadier on detailed or moving pictures.
    pub average: bool,
}

impl Default for Fixture {
    fn default() -> Self {
        Self {
            id: 0,
            name: "Strip".into(),
            enabled: true,
            shape: FixtureShape::Strip,
            points: [[0.1, 0.5], [0.9, 0.5], [0.9, 0.9], [0.1, 0.9]],
            cols: 60,
            rows: 8,
            zigzag: true,
            by_columns: false,
            order: PixelOrder::Rgb,
            universe: 0,
            channel: 1,
            brightness: 1.0,
            gamma: 1.0,
            head: HeadLayout::Rgb8,
            pan: 0.5,
            tilt: 0.5,
            motion: HeadMotion::Still,
            motion_size: 0.2,
            motion_beats: 8.0,
            dimmer: 1.0,
            extra: Vec::new(),
            average: false,
        }
    }
}

impl Fixture {
    /// Its controllable settings, as `ControlTarget::Fixture` params (all 0..1). Pan to motion size: moving heads;
    /// after these come its extra channels (5 = the first).
    pub const PARAMS: [&'static str; 5] = ["pan", "tilt", "dimmer", "motion size", "brightness"];

    /// Setting `param` (`PARAMS`), to read or set.
    pub fn param_mut(&mut self, param: u8) -> Option<&mut f32> {
        Some(match param {
            0 => &mut self.pan,
            1 => &mut self.tilt,
            2 => &mut self.dimmer,
            3 => &mut self.motion_size,
            4 => &mut self.brightness,
            n => &mut self.extra.get_mut(n as usize - 5)?.value,
        })
    }

    pub fn param(&self, param: u8) -> Option<f32> {
        Some(match param {
            0 => self.pan,
            1 => self.tilt,
            2 => self.dimmer,
            3 => self.motion_size,
            4 => self.brightness,
            n => self.extra.get(n as usize - 5)?.value,
        })
    }

    /// What `param` is called ("pan", or an extra channel's name).
    pub fn param_name(&self, param: u8) -> String {
        match Self::PARAMS.get(param as usize) {
            Some(n) => n.to_string(),
            None => self.extra.get(param as usize - 5).map_or_else(|| "?".into(), |e| if e.name.is_empty() { format!("channel {}", param - 4) } else { e.name.clone() }),
        }
    }

    /// The params that apply to this fixture (heads have all; strips and grids only brightness).
    pub fn params(&self) -> std::ops::Range<u8> {
        if self.shape == FixtureShape::MovingHead { 0..(5 + self.extra.len().min(200)) as u8 } else { 4..5 }
    }

    pub fn pixel_count(&self) -> usize {
        match self.shape {
            FixtureShape::Strip | FixtureShape::Ring | FixtureShape::Curve => self.cols.clamp(1, 4096) as usize,
            FixtureShape::Grid => (self.cols.clamp(1, 512) * self.rows.clamp(1, 512)) as usize,
            FixtureShape::MovingHead => 1,
        }
    }

    /// DMX channels one "pixel" takes (a moving head is one pixel with all its channels).
    fn channels_per_pixel(&self) -> usize {
        if self.shape == FixtureShape::MovingHead { (self.head.channels() + self.extra.len()).min(512) } else { self.order.channels() }
    }

    /// A moving head's pan and tilt (0..1) at `beats`: its centre plus its movement.
    pub fn head_position(&self, beats: f64) -> (f32, f32) {
        let t = if self.motion_beats > 0.0 { (beats / self.motion_beats as f64).rem_euclid(1.0) } else { 0.0 };
        let (dx, dy) = self.motion.offset(t);
        let s = self.motion_size.clamp(0.0, 1.0) as f64 * 0.5;
        ((self.pan as f64 + dx * s).clamp(0.0, 1.0) as f32, (self.tilt as f64 + dy * s).clamp(0.0, 1.0) as f32)
    }

    /// Where each pixel samples the composition, in wiring order.
    pub fn sample_points(&self) -> Vec<[f32; 2]> {
        let p = &self.points;
        let lerp = |a: [f32; 2], b: [f32; 2], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        // Pixel centres: n pixels split the line into n equal cells.
        let centre = |i: u32, n: u32| if n <= 1 { 0.5 } else { (i as f32 + 0.5) / n as f32 };
        match self.shape {
            FixtureShape::MovingHead => vec![p[0]],
            FixtureShape::Strip => {
                let n = self.cols.clamp(1, 4096);
                (0..n).map(|i| lerp(p[0], p[1], centre(i, n))).collect()
            }
            FixtureShape::Grid => {
                let (cols, rows) = (self.cols.clamp(1, 512), self.rows.clamp(1, 512));
                let at = |c: u32, r: u32| {
                    let t = centre(r, rows);
                    lerp(lerp(p[0], p[3], t), lerp(p[1], p[2], t), centre(c, cols))
                };
                let mut out = Vec::with_capacity((cols * rows) as usize);
                // Wired row by row (or column by column), every other one backwards with zig-zag.
                let (outer, inner) = if self.by_columns { (cols, rows) } else { (rows, cols) };
                for a in 0..outer {
                    for b in 0..inner {
                        let b = if self.zigzag && a % 2 == 1 { inner - 1 - b } else { b };
                        out.push(if self.by_columns { at(a, b) } else { at(b, a) });
                    }
                }
                out
            }
            FixtureShape::Ring => {
                let n = self.cols.clamp(1, 4096);
                let (cx, cy) = (p[0][0], p[0][1]);
                let (dx, dy) = (p[1][0] - cx, p[1][1] - cy);
                (0..n)
                    .map(|i| {
                        let a = std::f32::consts::TAU * i as f32 / n as f32;
                        let (s, c) = a.sin_cos();
                        [cx + dx * c - dy * s, cy + dx * s + dy * c]
                    })
                    .collect()
            }
            FixtureShape::Curve => {
                let n = self.cols.clamp(1, 4096);
                (0..n)
                    .map(|i| {
                        let t = centre(i, n);
                        let u = 1.0 - t;
                        let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                        [0, 1].map(|k| w[0] * p[0][k] + w[1] * p[1][k] + w[2] * p[2][k] + w[3] * p[3][k])
                    })
                    .collect()
            }
        }
    }

    /// Samples the picture takes for each pixel: 9 when averaging (not moving heads), else 1.
    pub fn taps(&self) -> usize {
        if self.average && self.shape != FixtureShape::MovingHead { 9 } else { 1 }
    }

    /// Every point the picture is sampled at, in wiring order: `taps()` per pixel, together. Averaging spreads a
    /// pixel's 3 × 3 samples across its cell (the step to its neighbours along the line, and as much across; a
    /// grid's column and row steps).
    pub fn sample_taps(&self) -> Vec<[f32; 2]> {
        let centres = self.sample_points();
        if self.taps() == 1 || centres.is_empty() {
            return centres;
        }
        let p = &self.points;
        let n = centres.len();
        let sub = |a: [f32; 2], b: [f32; 2]| [a[0] - b[0], a[1] - b[1]];
        let steps: Vec<([f32; 2], [f32; 2])> = if self.shape == FixtureShape::Grid {
            let (cols, rows) = (self.cols.clamp(1, 512) as f32, self.rows.clamp(1, 512) as f32);
            let along = sub(p[1], p[0]).map(|v| v / cols);
            let down = sub(p[3], p[0]).map(|v| v / rows);
            vec![(along, down); n]
        } else {
            (0..n)
                .map(|i| {
                    let (a, b) = (centres[i.saturating_sub(1)], centres[(i + 1).min(n - 1)]);
                    let span = ((i + 1).min(n - 1) - i.saturating_sub(1)).max(1) as f32;
                    let along = sub(b, a).map(|v| v / span);
                    (along, [-along[1], along[0]])
                })
                .collect()
        };
        let mut out = Vec::with_capacity(n * 9);
        for (c, (u, v)) in centres.iter().zip(steps) {
            for b in [-1.0f32, 0.0, 1.0] {
                for a in [-1.0f32, 0.0, 1.0] {
                    let (a, b) = (a / 3.0, b / 3.0);
                    out.push([(c[0] + u[0] * a + v[0] * b).clamp(0.0, 1.0), (c[1] + u[1] * a + v[1] * b).clamp(0.0, 1.0)]);
                }
            }
        }
        out
    }

    /// Writes this fixture's pixels (`colors`, one per sample point) into DMX universes. `beats` times a moving
    /// head's movement.
    pub fn write(&self, colors: &[[u8; 4]], universes: &mut BTreeMap<u16, [u8; 512]>, beats: f64) {
        let per = self.channels_per_pixel();
        let lut = gamma_table(self.brightness, self.gamma);
        if self.shape == FixtureShape::MovingHead {
            let Some(c) = colors.first() else { return };
            let mut ch = (self.channel.clamp(1, 512) - 1) as usize;
            let mut uni = self.universe;
            if ch + per > 512 {
                uni = uni.saturating_add(1);
                ch = 0;
            }
            let (pan, tilt) = self.head_position(beats);
            let (pan, tilt) = ((pan * 65535.0).round() as u16, (tilt * 65535.0).round() as u16);
            let [r, g, b] = [lut[c[0] as usize], lut[c[1] as usize], lut[c[2] as usize]];
            let w = r.min(g).min(b);
            let dim = (self.dimmer.clamp(0.0, 1.0) * 255.0).round() as u8;
            let mut v: Vec<u8> = vec![(pan >> 8) as u8, pan as u8, (tilt >> 8) as u8, tilt as u8];
            match self.head {
                HeadLayout::Rgb8 => v.extend([dim, r, g, b]),
                HeadLayout::Rgbw9 => v.extend([dim, r - w, g - w, b - w, w]),
                HeadLayout::ShutterRgbw10 => v.extend([255, dim, r - w, g - w, b - w, w]),
            }
            v.extend(self.extra.iter().map(|e| (e.value.clamp(0.0, 1.0) * 255.0).round() as u8));
            v.truncate(per);
            let data = universes.entry(uni).or_insert([0; 512]);
            data[ch..ch + per].copy_from_slice(&v);
            return;
        }
        let mut uni = self.universe;
        let mut ch = (self.channel.clamp(1, 512) - 1) as usize;
        for c in colors.iter().take(self.pixel_count()) {
            if ch + per > 512 {
                uni = uni.saturating_add(1);
                ch = 0;
            }
            let rgb = [lut[c[0] as usize], lut[c[1] as usize], lut[c[2] as usize]];
            let data = universes.entry(uni).or_insert([0; 512]);
            self.order.write(rgb, &mut data[ch..ch + per]);
            ch += per;
        }
    }

    /// Where the next fixture can start: (universe, channel) just after this one's last pixel.
    pub fn next_free(&self) -> (u16, u16) {
        let (uni, ch) = self.end();
        if ch >= 512 { (uni.saturating_add(1), 1) } else { (uni, ch as u16 + 1) }
    }

    /// (universe, channels used in it) of the last pixel, walking pixels exactly like `write`.
    fn end(&self) -> (u16, usize) {
        let per = self.channels_per_pixel();
        let mut uni = self.universe;
        let mut ch = (self.channel.clamp(1, 512) - 1) as usize;
        for _ in 0..self.pixel_count() {
            if ch + per > 512 {
                uni = uni.saturating_add(1);
                ch = 0;
            }
            ch += per;
        }
        (uni, ch)
    }

    /// "U0 ch 1 – U1 ch 30" for the UI.
    pub fn span(&self) -> String {
        let (uni, ch) = self.end();
        format!("U{} ch {} – U{uni} ch {ch}", self.universe, self.channel.max(1))
    }
}

fn gamma_table(brightness: f32, gamma: f32) -> [u8; 256] {
    let mut t = [0u8; 256];
    let g = gamma.clamp(0.2, 4.0);
    let b = brightness.clamp(0.0, 1.0);
    for (i, v) in t.iter_mut().enumerate() {
        *v = ((i as f32 / 255.0).powf(g) * b * 255.0).round() as u8;
    }
    t
}

/// The composition's LED / DMX output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DmxSettings {
    pub enabled: bool,
    pub protocol: DmxProtocol,
    /// Where to send: an IP address, or empty = broadcast (Art-Net) / multicast (sACN); for USB, the serial port.
    pub target: String,
    pub fps: f32,
    pub fixtures: Vec<Fixture>,
    /// LED / DMX master: every fixture's colour (and moving heads' dimmers) scaled by this (0..1).
    pub master: f32,
}

impl Default for DmxSettings {
    fn default() -> Self {
        Self { enabled: false, protocol: DmxProtocol::ArtNet, target: String::new(), fps: 40.0, fixtures: Vec::new(), master: 1.0 }
    }
}

impl DmxSettings {
    /// Every enabled fixture's sample points, in order (the engine samples these).
    pub fn sample_points(&self) -> Vec<[f32; 2]> {
        self.fixtures.iter().filter(|f| f.enabled).flat_map(|f| f.sample_taps()).collect()
    }

    /// A video wall: `across` × `down` LED tiles of `w` × `h` pixels covering the whole picture, wired one after
    /// another from `start` (universe, channel), each tile's own universes following the last. Returns their ids.
    pub fn add_video_wall(&mut self, across: u32, down: u32, w: u32, h: u32, start: (u16, u16), mut new_id: impl FnMut() -> u64) -> Vec<u64> {
        let (across, down) = (across.clamp(1, 32), down.clamp(1, 32));
        let mut at = start;
        let mut ids = Vec::new();
        for r in 0..down {
            for c in 0..across {
                let (x0, x1) = (c as f32 / across as f32, (c + 1) as f32 / across as f32);
                let (y0, y1) = (r as f32 / down as f32, (r + 1) as f32 / down as f32);
                let tile = Fixture {
                    id: new_id(),
                    name: format!("Tile {}", r * across + c + 1),
                    shape: FixtureShape::Grid,
                    points: [[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
                    cols: w.clamp(1, 512),
                    rows: h.clamp(1, 512),
                    zigzag: false,
                    universe: at.0,
                    channel: at.1,
                    ..Default::default()
                };
                // Each tile starts on a fresh universe after the one before (the way tiles are usually patched).
                let (uni, ch) = tile.next_free();
                at = if ch == 1 { (uni, 1) } else { (uni.saturating_add(1), 1) };
                ids.push(tile.id);
                self.fixtures.push(tile);
            }
        }
        ids
    }

    /// Fixtures from a table (a patch list from a spreadsheet): a header row naming the columns, then one fixture a
    /// row. Columns (any order, any case): name, shape (strip / grid / ring / curve), pixels (or cols / width), rows
    /// (or height), universe, channel, x1, y1 … x4, y4 (0..1 of the picture). `new_id` gives each one an id.
    /// Returns how many came in.
    pub fn import_csv(&mut self, text: &str, mut new_id: impl FnMut() -> u64) -> Result<usize, String> {
        let mut rows = text.lines().filter(|l| !l.trim().is_empty());
        let header: Vec<String> = rows.next().ok_or("empty table")?.split([',', ';', '\t']).map(|h| h.trim().to_lowercase()).collect();
        let col = |names: &[&str]| header.iter().position(|h| names.contains(&h.as_str()));
        let (name, shape, pixels, height) = (col(&["name"]), col(&["shape", "type"]), col(&["pixels", "cols", "width", "leds"]), col(&["rows", "height"]));
        let (universe, channel) = (col(&["universe"]), col(&["channel", "address", "start"]));
        let xy: Vec<(Option<usize>, Option<usize>)> = (1..=4).map(|k| (col(&[format!("x{k}").as_str()]), col(&[format!("y{k}").as_str()]))).collect();
        if pixels.is_none() && xy[0].0.is_none() {
            return Err("no \"pixels\" or \"x1\" column".into());
        }
        let mut n = 0;
        for row in rows {
            let f: Vec<&str> = row.split([',', ';', '\t']).map(str::trim).collect();
            let get = |c: Option<usize>| c.and_then(|c| f.get(c)).copied().filter(|v| !v.is_empty());
            let num = |c: Option<usize>| get(c).and_then(|v| v.parse::<f32>().ok());
            let mut fx = Fixture { id: new_id(), ..Default::default() };
            fx.name = get(name).map_or_else(|| format!("Fixture {}", self.fixtures.len() + 1), str::to_string);
            fx.shape = match get(shape).map(str::to_lowercase).as_deref() {
                Some("grid" | "matrix") => FixtureShape::Grid,
                Some("ring" | "circle") => FixtureShape::Ring,
                Some("curve" | "bezier") => FixtureShape::Curve,
                _ => FixtureShape::Strip,
            };
            if let Some(p) = num(pixels) {
                fx.cols = p.max(1.0) as u32;
            }
            if let Some(r) = num(height) {
                fx.rows = r.max(1.0) as u32;
            }
            if let Some(u) = num(universe) {
                fx.universe = u.max(0.0) as u16;
            }
            if let Some(c) = num(channel) {
                fx.channel = c.clamp(1.0, 512.0) as u16;
            }
            for (k, (x, y)) in xy.iter().enumerate() {
                if let (Some(x), Some(y)) = (num(*x), num(*y)) {
                    fx.points[k] = [x, y];
                }
            }
            self.fixtures.push(fx);
            n += 1;
        }
        Ok(n)
    }

    /// Turns sampled colours (in `sample_points` order) into DMX universes; `beats` (the show clock) times the
    /// moving heads' movements.
    pub fn universes(&self, colors: &[[u8; 4]], beats: f64) -> BTreeMap<u16, [u8; 512]> {
        let mut out = BTreeMap::new();
        let mut at = 0;
        let master = self.master.clamp(0.0, 1.0);
        for f in self.fixtures.iter().filter(|f| f.enabled) {
            let taps = f.taps();
            let n = f.pixel_count() * taps;
            let averaged: Vec<[u8; 4]>;
            let mut c = colors.get(at..(at + n).min(colors.len()));
            if taps > 1
                && let Some(all) = c
            {
                // Each pixel's samples, averaged.
                averaged = all.chunks(taps).map(|t| [0, 1, 2, 3].map(|k| (t.iter().map(|p| p[k] as u32).sum::<u32>() as f32 / t.len() as f32).round() as u8)).collect();
                c = Some(&averaged);
            }
            if let Some(c) = c {
                if master < 1.0 {
                    let dimmed: Vec<[u8; 4]> = c.iter().map(|p| [0, 1, 2, 3].map(|k| if k < 3 { (p[k] as f32 * master).round() as u8 } else { p[k] })).collect();
                    let f = Fixture { dimmer: f.dimmer * master, ..f.clone() };
                    f.write(&dimmed, &mut out, beats);
                } else {
                    f.write(c, &mut out, beats);
                }
            }
            at += n;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn averaging_pixels_read_their_whole_cell() {
        // A 2-pixel strip across the picture: each pixel's 9 samples stay inside its own half.
        let f = Fixture { cols: 2, points: [[0.0, 0.5], [1.0, 0.5], [0.0; 2], [0.0; 2]], average: true, order: PixelOrder::Rgb, ..Default::default() };
        let taps = f.sample_taps();
        assert_eq!(taps.len(), 18);
        assert!(taps[..9].iter().all(|p| p[0] < 0.5) && taps[9..].iter().all(|p| p[0] > 0.5), "{taps:?}");
        assert_eq!(taps[4], [0.25, 0.5], "the middle sample is the centre");
        // Half the first pixel's samples white, half black: it sends the average.
        let settings = DmxSettings { fixtures: vec![Fixture { gamma: 1.0, ..f.clone() }], ..Default::default() };
        let mut colors = vec![[0u8, 0, 0, 255]; 18];
        for c in &mut colors[..9].iter_mut().step_by(2) {
            *c = [255, 255, 255, 255];
        }
        let u = settings.universes(&colors, 0.0);
        let data = u.values().next().unwrap();
        assert_eq!(&data[..6], &[142, 142, 142, 0, 0, 0], "5 of 9 white = 142");
        // Moving heads never average.
        assert_eq!(Fixture { shape: FixtureShape::MovingHead, average: true, ..Default::default() }.taps(), 1);
    }

    #[test]
    fn a_video_wall_covers_the_picture_in_patched_tiles() {
        let mut d = DmxSettings::default();
        let mut next = 10;
        let ids = d.add_video_wall(2, 2, 16, 16, (0, 1), || {
            next += 1;
            next
        });
        assert_eq!(ids, vec![11, 12, 13, 14]);
        let t = &d.fixtures;
        assert_eq!((t[0].points[0], t[3].points[2]), ([0.0, 0.0], [1.0, 1.0]), "corner to corner");
        assert_eq!(t[1].points[0], [0.5, 0.0]);
        // 256 RGB pixels = 768 channels: two universes a tile (170 pixels fit in one), the next tile on a fresh one.
        assert_eq!(t.iter().map(|f| (f.universe, f.channel)).collect::<Vec<_>>(), vec![(0, 1), (2, 1), (4, 1), (6, 1)]);
    }

    use super::*;

    #[test]
    fn the_dmx_master_dims_everything() {
        let mut d = DmxSettings { fixtures: vec![Fixture { cols: 1, order: PixelOrder::Rgb, gamma: 1.0, ..Default::default() }], ..Default::default() };
        let full = d.universes(&[[200, 100, 50, 255]], 0.0);
        d.master = 0.5;
        let half = d.universes(&[[200, 100, 50, 255]], 0.0);
        assert_eq!(&full[&0][..3], &[200, 100, 50]);
        assert_eq!(&half[&0][..3], &[100, 50, 25]);
    }

    #[test]
    fn fixtures_come_in_from_a_table() {
        let mut d = DmxSettings::default();
        let mut id = 10;
        let csv = "Name;Shape;Pixels;Universe;Channel;X1;Y1;X2;Y2\nBar left;strip;60;0;1;0.1;0.2;0.1;0.8\nHalo;ring;24;1;1;0.5;0.5;0.6;0.5\n";
        assert_eq!(d.import_csv(csv, || { id += 1; id }), Ok(2));
        assert_eq!((d.fixtures[0].name.as_str(), d.fixtures[0].cols, d.fixtures[0].points[1]), ("Bar left", 60, [0.1, 0.8]));
        assert_eq!((d.fixtures[1].shape, d.fixtures[1].universe, d.fixtures[1].id), (FixtureShape::Ring, 1, 12));
        assert!(d.import_csv("a,b\n1,2", || 0).is_err());
    }

    #[test]
    fn rings_curves_and_column_wiring() {
        let ring = Fixture { shape: FixtureShape::Ring, cols: 4, points: [[0.5, 0.5], [0.7, 0.5], [0.0; 2], [0.0; 2]], ..Default::default() };
        let pts = ring.sample_points();
        assert_eq!(pts.len(), 4);
        assert!((pts[0][0] - 0.7).abs() < 1e-5 && (pts[1][1] - 0.7).abs() < 1e-5 && (pts[2][0] - 0.3).abs() < 1e-5, "{pts:?}");
        let curve = Fixture { shape: FixtureShape::Curve, cols: 3, points: [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]], ..Default::default() };
        let c = curve.sample_points();
        assert!((c[1][0] - 0.5).abs() < 1e-5 && c[1][1] > 0.7, "bends up in the middle: {c:?}");
        let mut grid = Fixture { shape: FixtureShape::Grid, cols: 2, rows: 3, points: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], by_columns: true, ..Default::default() };
        let g = grid.sample_points();
        assert!(g[0][0] < 0.5 && g[1][0] < 0.5 && g[2][0] < 0.5 && g[3][0] > 0.5, "first column first");
        grid.zigzag = true;
        let z = grid.sample_points();
        assert!(z[3][1] > z[5][1], "the second column runs back up");
    }

    #[test]
    fn strips_fill_universes_without_splitting_pixels() {
        let f = Fixture { cols: 200, order: PixelOrder::Rgb, universe: 3, channel: 1, ..Default::default() };
        let colors = vec![[255, 128, 0, 255]; 200];
        let mut u = BTreeMap::new();
        f.write(&colors, &mut u, 0.0);
        // 170 RGB pixels fit in 510 channels; the 171st starts universe 4.
        assert_eq!(&u[&3][0..3], &[255, 128, 0]);
        assert_eq!(&u[&3][510..512], &[0, 0]);
        assert_eq!(&u[&4][0..3], &[255, 128, 0]);
        assert_eq!(&u[&4][90..93], &[0, 0, 0], "30 pixels in universe 4");
        assert_eq!(f.span(), "U3 ch 1 – U4 ch 90");
        assert_eq!(f.next_free(), (4, 91));
    }

    #[test]
    fn grid_points_zigzag_and_orders() {
        let f = Fixture { shape: FixtureShape::Grid, cols: 2, rows: 2, points: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], ..Default::default() };
        assert_eq!(f.sample_points(), vec![[0.25, 0.25], [0.75, 0.25], [0.75, 0.75], [0.25, 0.75]]);
        let mut px = [0u8; 4];
        PixelOrder::Grbw.write([200, 100, 50], &mut px);
        assert_eq!(px, [50, 150, 0, 50]);
    }

    #[test]
    fn moving_head_writes_pan_tilt_dimmer_and_colour() {
        let f = Fixture { shape: FixtureShape::MovingHead, head: HeadLayout::ShutterRgbw10, universe: 1, channel: 11, pan: 0.5, tilt: 0.25, dimmer: 0.5, ..Default::default() };
        assert_eq!(f.pixel_count(), 1);
        assert_eq!(f.sample_points().len(), 1);
        let mut u = BTreeMap::new();
        f.write(&[[200, 100, 50, 255]], &mut u, 0.0);
        let d = &u[&1][10..20];
        // pan 0.5 → 32768 (0x8000), tilt 0.25 → 16384 (0x4000), shutter open, dimmer 128, RGBW.
        assert_eq!(d, &[0x80, 0x00, 0x40, 0x00, 255, 128, 150, 50, 0, 50]);
        assert_eq!(f.next_free(), (1, 21));
        // A circle moves the head around its centre over `motion_beats`.
        let c = Fixture { motion: HeadMotion::Circle, motion_size: 0.4, motion_beats: 4.0, ..f.clone() };
        let (p0, _) = c.head_position(0.0);
        let (_, t1) = c.head_position(1.0);
        let (p2, _) = c.head_position(2.0);
        assert!((p0 - 0.7).abs() < 1e-5 && (t1 - 0.45).abs() < 1e-5 && (p2 - 0.3).abs() < 1e-5, "{p0} {t1} {p2}");
    }
}
