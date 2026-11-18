//! The Stage layout: every output where it really is in the venue (a center screen, two wings, an LED wall…), on one
//! big canvas, with a picture of the venue behind them if you like. Only for seeing and planning the show: nothing
//! here changes what an output shows.

use serde::{Deserialize, Serialize};

/// Where something sits on the Stage layout: its top-left corner and its width, in stage units (y down). Its height
/// follows its own picture's shape.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct StagePlace {
    pub x: f32,
    pub y: f32,
    pub w: f32,
}

impl StagePlace {
    /// (x, y, w, h) for a picture `width` × `height`.
    pub fn rect(&self, width: u32, height: u32) -> [f32; 4] {
        [self.x, self.y, self.w, self.w * height.max(1) as f32 / width.max(1) as f32]
    }
}

/// The venue picture behind the outputs ("" = none), where it sits and how strongly it shows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StageLayout {
    pub backdrop: String,
    pub backdrop_place: Option<StagePlace>,
    pub backdrop_opacity: f32,
}

impl Default for StageLayout {
    fn default() -> Self {
        Self { backdrop: String::new(), backdrop_place: None, backdrop_opacity: 0.6 }
    }
}

/// A new output's height on the stage (a 16:9 one is 4 wide), and the gap between outputs placed in a row.
pub const STAGE_HEIGHT: f32 = 2.25;
const STAGE_GAP: f32 = 0.25;

impl crate::Composition {
    /// Every output's place on the stage: as placed, or (never placed yet) in a row to the right of the placed ones,
    /// all the same height.
    pub fn stage_places(&self) -> Vec<(u64, StagePlace)> {
        let canvas = (self.width, self.height);
        let mut right = self.outputs.iter().filter_map(|o| o.stage.map(|p| p.x + p.w)).fold(f32::MIN, f32::max);
        let top = self.outputs.iter().filter_map(|o| o.stage.map(|p| p.y)).fold(f32::MAX, f32::min);
        let (mut x, y) = if right == f32::MIN { (0.0, 0.0) } else { (right + STAGE_GAP, top) };
        self.outputs
            .iter()
            .map(|o| {
                let place = o.stage.unwrap_or_else(|| {
                    let p = StagePlace { x, y, w: default_width(o.size(canvas)) };
                    x += p.w + STAGE_GAP;
                    right = x;
                    p
                });
                (o.id, place)
            })
            .collect()
    }

    /// Puts every output back in one row, in order, and the fixtures in a row under them (Stage › Line up).
    pub fn line_up_stage(&mut self) {
        let canvas = (self.width, self.height);
        let mut x = 0.0;
        for o in &mut self.outputs {
            let w = o.stage.map_or_else(|| default_width(o.size(canvas)), |p| p.w);
            o.stage = Some(StagePlace { x, y: 0.0, w });
            x += w + STAGE_GAP;
        }
        // The LED fixtures back in their row under them.
        for f in &mut self.dmx.fixtures {
            f.stage = None;
        }
    }
}

/// A fixture's lights as the Stage draws them: each pixel's place inside its box (0..1, y down) and the box's shape
/// (width / height), from where it samples a picture `aspect` wide for 1 high. A straight strip gets a little height.
pub fn fixture_dots(f: &crate::Fixture, aspect: f32) -> (Vec<[f32; 2]>, f32) {
    let pts: Vec<[f32; 2]> = f.sample_points().into_iter().map(|[x, y]| [x * aspect, y]).collect();
    let b = pts.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])]);
    if pts.is_empty() {
        return (Vec::new(), 1.0);
    }
    let long = (b[2] - b[0]).max(b[3] - b[1]).max(0.02);
    let pad = edge_room(long, pts.len());
    let (x0, y0, w, h) = (b[0] - pad, b[1] - pad, b[2] - b[0] + pad * 2.0, b[3] - b[1] + pad * 2.0);
    (pts.iter().map(|p| [(p[0] - x0) / w, (p[1] - y0) / h]).collect(), w / h)
}

/// Room round a fixture's outer lights: half the spacing of `n` lights along `long`, at least a sliver of it.
fn edge_room(long: f32, n: usize) -> f32 {
    (long / n.saturating_sub(1).max(1) as f32 * 0.5).max(long * 0.03)
}

/// Its natural width on the stage: true to scale with the outputs (as wide as its box on the picture when the
/// picture is the usual height).
fn fixture_width(f: &crate::Fixture, aspect: f32) -> f32 {
    let pts = f.sample_points();
    let (lo, hi) = pts.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[0] * aspect), hi.max(p[0] * aspect)));
    let (lo_y, hi_y) = pts.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[1]), hi.max(p[1])));
    let long = (hi - lo).max(hi_y - lo_y).max(0.02);
    let pad = edge_room(long, pts.len());
    ((hi - lo + pad * 2.0) * STAGE_HEIGHT).clamp(0.05, 40.0)
}

impl crate::Composition {
    /// Every LED / DMX fixture's place on the stage: as placed, or (never placed yet) in a row under the outputs.
    pub fn fixture_places(&self) -> Vec<(u64, StagePlace)> {
        let canvas = (self.width, self.height);
        let aspect = canvas.0.max(1) as f32 / canvas.1.max(1) as f32;
        let outs = self.stage_places();
        let rects: Vec<[f32; 4]> = self.outputs.iter().zip(&outs).map(|(o, (_, p))| {
            let (w, h) = o.size(canvas);
            p.rect(w, h)
        }).collect();
        let placed = self.dmx.fixtures.iter().filter_map(|f| f.stage.map(|p| (p, fixture_dots(f, aspect).1)));
        let bottom = rects.iter().map(|r| r[1] + r[3]).chain(placed.map(|(p, a)| p.y + p.w / a)).fold(0.0f32, f32::max);
        let mut x = rects.iter().map(|r| r[0]).fold(f32::MAX, f32::min);
        if x == f32::MAX {
            x = 0.0;
        }
        let y = bottom + STAGE_GAP * 2.0;
        self.dmx
            .fixtures
            .iter()
            .map(|f| {
                let place = f.stage.unwrap_or_else(|| {
                    let p = StagePlace { x, y, w: fixture_width(f, aspect) };
                    x += p.w + STAGE_GAP;
                    p
                });
                (f.id, place)
            })
            .collect()
    }
}

/// The width that gives a `w` × `h` picture the usual height.
fn default_width((w, h): (u32, u32)) -> f32 {
    STAGE_HEIGHT * w.max(1) as f32 / h.max(1) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Composition, OutputConfig};

    #[test]
    fn fixtures_line_up_under_the_outputs_and_read_back_what_was_sent() {
        let mut c = Composition { width: 1920, height: 1080, ..Default::default() };
        c.outputs = vec![OutputConfig { id: 1, width: 1920, height: 1080, ..Default::default() }];
        let strip = crate::Fixture { id: 7, cols: 4, points: [[0.0, 0.5], [1.0, 0.5], [0.0; 2], [0.0; 2]], universe: 1, channel: 1, ..Default::default() };
        c.dmx.fixtures = vec![strip.clone()];
        let p = c.fixture_places();
        assert!(p[0].1.y > STAGE_HEIGHT, "under the output");
        assert!((p[0].1.w - 4.0).abs() < 1e-3, "a strip the picture's width is the output's width");
        let (dots, shape) = fixture_dots(&strip, 16.0 / 9.0);
        assert_eq!(dots.len(), 4);
        assert!((shape - 4.0).abs() < 1e-3, "four lights in a row: a box four lights long, one high");
        assert!(dots.iter().all(|d| (0.0..=1.0).contains(&d[0]) && (0.0..=1.0).contains(&d[1])));
        let mut u = std::collections::BTreeMap::new();
        strip.write(&[[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [9, 9, 9, 255]], &mut u, 0.0);
        assert_eq!(strip.read(&u), vec![[255, 0, 0], [0, 255, 0], [0, 0, 255], [9, 9, 9]]);
        let grbw = crate::Fixture { order: crate::PixelOrder::Grbw, ..strip };
        let mut u = std::collections::BTreeMap::new();
        grbw.write(&[[200, 100, 50, 255]; 4], &mut u, 0.0);
        assert_eq!(grbw.read(&u)[0], [200, 100, 50]);
    }

    #[test]
    fn unplaced_outputs_line_up_after_the_placed_ones() {
        let mut c = Composition::default();
        c.outputs = vec![
            OutputConfig { id: 1, stage: Some(StagePlace { x: 2.0, y: 1.0, w: 6.0 }), ..Default::default() },
            OutputConfig { id: 2, width: 1920, height: 1080, ..Default::default() },
            OutputConfig { id: 3, width: 1080, height: 1920, ..Default::default() },
        ];
        let p = c.stage_places();
        assert_eq!(p[0].1, StagePlace { x: 2.0, y: 1.0, w: 6.0 });
        assert_eq!(p[1].1, StagePlace { x: 8.25, y: 1.0, w: 4.0 }, "16:9 at the usual height");
        assert_eq!(p[2].1.x, 8.25 + 4.0 + 0.25);
        assert!((p[2].1.rect(1080, 1920)[3] - STAGE_HEIGHT).abs() < 1e-4, "a portrait one just as tall");
        c.line_up_stage();
        let xs: Vec<f32> = c.outputs.iter().map(|o| o.stage.unwrap().x).collect();
        assert_eq!(xs, [0.0, 6.25, 6.25 + 4.0 + 0.25]);
        assert_eq!(StagePlace { x: 0.0, y: 0.0, w: 4.0 }.rect(1920, 1080), [0.0, 0.0, 4.0, 2.25]);
    }
}
