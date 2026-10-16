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
}

impl DmxProtocol {
    pub const ALL: [DmxProtocol; 2] = [DmxProtocol::ArtNet, DmxProtocol::Sacn];

    pub fn name(self) -> &'static str {
        match self {
            DmxProtocol::ArtNet => "Art-Net",
            DmxProtocol::Sacn => "sACN (E1.31)",
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
}

impl FixtureShape {
    pub const ALL: [FixtureShape; 2] = [FixtureShape::Strip, FixtureShape::Grid];

    pub fn name(self) -> &'static str {
        match self {
            FixtureShape::Strip => "Strip",
            FixtureShape::Grid => "Grid / matrix",
        }
    }
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
    pub order: PixelOrder,
    /// First universe (Art-Net counts from 0, sACN from 1) and channel (1–512). Long fixtures continue
    /// into the next universes without splitting a pixel.
    pub universe: u16,
    pub channel: u16,
    pub brightness: f32,
    /// LED gamma (2.2 makes fades look even on most strips; 1 = linear).
    pub gamma: f32,
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
            order: PixelOrder::Rgb,
            universe: 0,
            channel: 1,
            brightness: 1.0,
            gamma: 1.0,
        }
    }
}

impl Fixture {
    pub fn pixel_count(&self) -> usize {
        match self.shape {
            FixtureShape::Strip => self.cols.clamp(1, 4096) as usize,
            FixtureShape::Grid => (self.cols.clamp(1, 512) * self.rows.clamp(1, 512)) as usize,
        }
    }

    /// Where each pixel samples the composition, in wiring order.
    pub fn sample_points(&self) -> Vec<[f32; 2]> {
        let p = &self.points;
        let lerp = |a: [f32; 2], b: [f32; 2], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        // Pixel centres: n pixels split the line into n equal cells.
        let centre = |i: u32, n: u32| if n <= 1 { 0.5 } else { (i as f32 + 0.5) / n as f32 };
        match self.shape {
            FixtureShape::Strip => {
                let n = self.cols.clamp(1, 4096);
                (0..n).map(|i| lerp(p[0], p[1], centre(i, n))).collect()
            }
            FixtureShape::Grid => {
                let (cols, rows) = (self.cols.clamp(1, 512), self.rows.clamp(1, 512));
                let mut out = Vec::with_capacity((cols * rows) as usize);
                for r in 0..rows {
                    let t = centre(r, rows);
                    let (left, right) = (lerp(p[0], p[3], t), lerp(p[1], p[2], t));
                    for c in 0..cols {
                        let c = if self.zigzag && r % 2 == 1 { cols - 1 - c } else { c };
                        out.push(lerp(left, right, centre(c, cols)));
                    }
                }
                out
            }
        }
    }

    /// Writes this fixture's pixels (`colors`, one per sample point) into DMX universes.
    pub fn write(&self, colors: &[[u8; 4]], universes: &mut BTreeMap<u16, [u8; 512]>) {
        let per = self.order.channels();
        let lut = gamma_table(self.brightness, self.gamma);
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
        let per = self.order.channels();
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
    /// Where to send: an IP address, or empty = broadcast (Art-Net) / multicast (sACN).
    pub target: String,
    pub fps: f32,
    pub fixtures: Vec<Fixture>,
}

impl Default for DmxSettings {
    fn default() -> Self {
        Self { enabled: false, protocol: DmxProtocol::ArtNet, target: String::new(), fps: 40.0, fixtures: Vec::new() }
    }
}

impl DmxSettings {
    /// Every enabled fixture's sample points, in order (the engine samples these).
    pub fn sample_points(&self) -> Vec<[f32; 2]> {
        self.fixtures.iter().filter(|f| f.enabled).flat_map(|f| f.sample_points()).collect()
    }

    /// Turns sampled colours (in `sample_points` order) into DMX universes.
    pub fn universes(&self, colors: &[[u8; 4]]) -> BTreeMap<u16, [u8; 512]> {
        let mut out = BTreeMap::new();
        let mut at = 0;
        for f in self.fixtures.iter().filter(|f| f.enabled) {
            let n = f.pixel_count();
            if let Some(c) = colors.get(at..(at + n).min(colors.len())) {
                f.write(c, &mut out);
            }
            at += n;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_fill_universes_without_splitting_pixels() {
        let f = Fixture { cols: 200, order: PixelOrder::Rgb, universe: 3, channel: 1, ..Default::default() };
        let colors = vec![[255, 128, 0, 255]; 200];
        let mut u = BTreeMap::new();
        f.write(&colors, &mut u);
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
}
