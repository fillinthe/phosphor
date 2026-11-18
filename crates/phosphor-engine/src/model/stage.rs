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

    /// Puts every output back in one row, in order (Stage › Line up).
    pub fn line_up_stage(&mut self) {
        let canvas = (self.width, self.height);
        let mut x = 0.0;
        for o in &mut self.outputs {
            let w = o.stage.map_or_else(|| default_width(o.size(canvas)), |p| p.w);
            o.stage = Some(StagePlace { x, y: 0.0, w });
            x += w + STAGE_GAP;
        }
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
