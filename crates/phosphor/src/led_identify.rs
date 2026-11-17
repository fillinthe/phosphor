//! Finding which string is which on a big rig: one fixture lit white while every other light is dark (Flash), or one
//! pixel running along it from its first pixel to its last (Chase: which end is the start, and which way it runs).
//! Goes out instead of the picture while on, like the LED scanner's pictures.

use std::collections::BTreeMap;

use phosphor_engine::{DmxSettings, Fixture, FixtureShape};

/// Pixels a second the chase runs at.
const CHASE_SPEED: f64 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Flash,
    Chase,
}

#[derive(Default)]
pub struct Identify {
    /// The fixture being shown, how, and since when (the render's clock: set by its first frame).
    on: Option<(u64, Mode, Option<f64>)>,
}

impl Identify {
    pub fn showing(&self) -> Option<(u64, Mode)> {
        self.on.map(|(f, m, _)| (f, m))
    }

    /// Shows `fixture` this way, or stops when it already is.
    pub fn toggle(&mut self, fixture: u64, mode: Mode) {
        self.on = if self.showing() == Some((fixture, mode)) { None } else { Some((fixture, mode, None)) };
    }

    /// What goes out now instead of the picture (None = nothing being identified). A fixture that was deleted stops it.
    pub fn universes(&mut self, dmx: &DmxSettings, now: f64) -> Option<BTreeMap<u16, [u8; 512]>> {
        let (id, mode, since) = self.on?;
        if !dmx.fixtures.iter().any(|f| f.id == id) {
            self.on = None;
            return None;
        }
        let since = since.unwrap_or(now);
        self.on = Some((id, mode, Some(since)));
        Some(frame(dmx, id, mode, now - since))
    }
}

/// Every light dark but `id`: all of it white (Flash) or the one pixel the chase has reached after `secs` (Chase).
pub fn frame(dmx: &DmxSettings, id: u64, mode: Mode, secs: f64) -> BTreeMap<u16, [u8; 512]> {
    let mut out = BTreeMap::new();
    // Dark first, then the lit one on top (two fixtures may share channels, like overlapping universes).
    let mut lights: Vec<&Fixture> = dmx.fixtures.iter().filter(|f| f.enabled && f.shape != FixtureShape::MovingHead).collect();
    lights.sort_by_key(|f| f.id == id);
    for f in lights {
        let n = f.pixel_count();
        let lit = |i: usize| {
            f.id == id
                && match mode {
                    Mode::Flash => true,
                    Mode::Chase => n > 0 && i == (secs.max(0.0) * CHASE_SPEED) as usize % n,
                }
        };
        let colors: Vec<[u8; 4]> = (0..n).map(|i| if lit(i) { [255; 4] } else { [0, 0, 0, 255] }).collect();
        // Full white whatever its brightness and gamma (a dim string is harder to spot across a venue).
        Fixture { brightness: 1.0, gamma: 1.0, positions: Vec::new(), ..f.clone() }.write(&colors, &mut out, 0.0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig() -> DmxSettings {
        let a = Fixture { id: 1, cols: 10, universe: 1, channel: 1, ..Default::default() };
        let b = Fixture { id: 2, cols: 10, universe: 1, channel: 31, ..Default::default() };
        DmxSettings { fixtures: vec![a, b], ..Default::default() }
    }

    #[test]
    fn flash_lights_one_string_only() {
        let u = frame(&rig(), 2, Mode::Flash, 0.0);
        assert!(u[&1][..30].iter().all(|v| *v == 0), "the other string is dark");
        assert!(u[&1][30..60].iter().all(|v| *v == 255));
    }

    #[test]
    fn the_chase_runs_from_the_first_pixel() {
        let at = |secs: f64| frame(&rig(), 1, Mode::Chase, secs)[&1][..30].iter().position(|v| *v == 255);
        assert_eq!(at(0.0), Some(0));
        assert_eq!(at(0.5), Some(6 * 3), "12 pixels a second: pixel 6 after half a second");
        assert_eq!(at(1.0), Some(2 * 3), "then around again");
    }

    #[test]
    fn a_deleted_fixture_stops_it() {
        let mut id = Identify::default();
        id.toggle(9, Mode::Flash);
        assert!(id.universes(&rig(), 1.0).is_none());
        assert!(id.showing().is_none());
        id.toggle(1, Mode::Chase);
        assert!(id.universes(&rig(), 5.0).is_some_and(|u| u[&1][0] == 255), "the chase starts at its first pixel whenever it's pressed");
        id.toggle(1, Mode::Flash);
        assert_eq!(id.showing(), Some((1, Mode::Flash)), "the other button switches over");
        id.toggle(1, Mode::Flash);
        assert!(id.showing().is_none(), "pressed again: off");
    }
}
