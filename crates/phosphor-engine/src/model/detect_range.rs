//! Detect range (VDMX): learn a whole row at once. Move the first and the last knob (or send the first and last OSC
//! address, press the first and last pad) and the inputs in between are worked out, so eight faders or a clip row
//! map in one go.

use super::ControlSource;

/// `n` evenly spaced whole numbers from `a` to `b` (None when they don't fall on whole steps, or `a` = `b`).
fn steps(a: i64, b: i64, n: usize) -> Option<Vec<i64>> {
    if n < 2 || a == b {
        return None;
    }
    let span = b - a;
    let gaps = n as i64 - 1;
    (span % gaps == 0).then(|| (0..n as i64).map(|k| a + k * span / gaps).collect())
}

/// Two OSC addresses that differ only by one number in the same place ("/1/fader1", "/1/fader8"): what comes
/// before it, both numbers, and what comes after.
fn number_between<'a>(a: &'a str, b: &str) -> Option<(&'a str, i64, i64, &'a str)> {
    if a == b {
        return None;
    }
    // (One can run on past the other: "/fader1" and "/fader12".)
    let at = a.bytes().zip(b.bytes()).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
    // Back to the start of the number the difference is in.
    let start = a[..at].rfind(|c: char| !c.is_ascii_digit()).map_or(0, |i| i + 1);
    let digits = |s: &str| s[start..].find(|c: char| !c.is_ascii_digit()).map_or(s.len(), |i| start + i);
    let (ea, eb) = (digits(a), digits(b));
    let (na, nb) = (a[start..ea].parse().ok()?, b.get(start..eb)?.parse().ok()?);
    (a[ea..] == b[eb..]).then_some((&a[..start], na, nb, &a[ea..]))
}

impl ControlSource {
    /// The `n` inputs from `first` to `last`, evenly spaced (CC 20 … CC 27, note 36 … 43, /fader1 … /fader8, DMX
    /// channels, Stream Deck keys). None when the two aren't the same kind on the same channel, or the gap doesn't
    /// split into `n - 1` whole steps.
    pub fn row(first: &ControlSource, last: &ControlSource, n: usize) -> Option<Vec<ControlSource>> {
        use ControlSource as S;
        Some(match (first, last) {
            (S::MidiCc { channel, cc: a }, S::MidiCc { channel: c2, cc: b }) if channel == c2 => {
                steps(*a as i64, *b as i64, n)?.into_iter().map(|cc| S::MidiCc { channel: *channel, cc: cc as u8 }).collect()
            }
            (S::MidiNote { channel, note: a }, S::MidiNote { channel: c2, note: b }) if channel == c2 => {
                steps(*a as i64, *b as i64, n)?.into_iter().map(|note| S::MidiNote { channel: *channel, note: note as u8 }).collect()
            }
            (S::Dmx { universe, channel: a }, S::Dmx { universe: u2, channel: b }) if universe == u2 => {
                steps(*a as i64, *b as i64, n)?.into_iter().map(|channel| S::Dmx { universe: *universe, channel: channel as u16 }).collect()
            }
            (S::StreamDeck { key: a }, S::StreamDeck { key: b }) => steps(*a as i64, *b as i64, n)?.into_iter().map(|key| S::StreamDeck { key: key as u8 }).collect(),
            (S::Arduino { pin: a, analog }, S::Arduino { pin: b, analog: an2 }) if analog == an2 => {
                steps(*a as i64, *b as i64, n)?.into_iter().map(|pin| S::Arduino { pin: pin as u8, analog: *analog }).collect()
            }
            (S::Osc { address: a }, S::Osc { address: b }) => {
                let (before, na, nb, after) = number_between(a, b)?;
                steps(na, nb, n)?.into_iter().map(|k| S::Osc { address: format!("{before}{k}{after}") }).collect()
            }
            _ => return None,
        })
    }
}

/// A row of controls to learn in one go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// The clip grid's columns (fire a column).
    Columns,
    /// One layer's clips, left to right.
    Clips(u64),
    /// The open comp's layers' opacity, bottom to top.
    Opacity,
    /// The dials of a layer, group, clip or MASTER.
    Dials(u64),
}

impl super::Composition {
    /// The first `n` controls of a row (fewer when the row is shorter: three layers have three opacities).
    pub fn row_targets(&self, row: Row, n: usize) -> Vec<super::ControlTarget> {
        use super::ControlTarget as T;
        match row {
            Row::Columns => (0..n).map(T::TriggerColumn).collect(),
            Row::Clips(layer) => (0..n).map(|slot| T::TriggerClip { layer, slot }).collect(),
            Row::Opacity => self.layers.iter().filter(|l| self.in_open_comp(l) && !l.designer).take(n).map(|l| T::LayerOpacity(l.id)).collect(),
            Row::Dials(owner) => (0..self.dials(owner).map_or(0, Vec::len).min(n)).map(|i| T::Macro { owner, index: i as u8 }).collect(),
        }
    }

    /// Maps a row in one go: the inputs from `first` to `last` onto the row's first controls, one each (replacing what
    /// those controls were mapped to). Returns how many were mapped, or what's wrong.
    pub fn learn_row(&mut self, row: Row, n: usize, first: &ControlSource, last: &ControlSource) -> Result<usize, String> {
        let targets = self.row_targets(row, n);
        if targets.len() < 2 {
            return Err("That row has fewer than two controls to map.".into());
        }
        let Some(sources) = ControlSource::row(first, last, targets.len()) else {
            return Err(format!("{} and {} don't make a row of {}: use two inputs of the same kind, evenly spaced.", first.label(), last.label(), targets.len()));
        };
        self.control_maps.retain(|m| !targets.contains(&m.target));
        let n = targets.len();
        self.control_maps.extend(sources.into_iter().zip(targets).map(|(s, t)| super::ControlMap::new(s, t)));
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_is_worked_out_from_its_ends() {
        let cc = |cc| ControlSource::MidiCc { channel: 0, cc };
        assert_eq!(ControlSource::row(&cc(20), &cc(27), 8), Some((20..=27).map(cc).collect()));
        // Every other CC (some controllers number their knobs in twos), and a row going down.
        assert_eq!(ControlSource::row(&cc(10), &cc(16), 4), Some(vec![cc(10), cc(12), cc(14), cc(16)]));
        assert_eq!(ControlSource::row(&cc(7), &cc(0), 8), Some((0..=7).rev().map(cc).collect()));
        assert_eq!(ControlSource::row(&cc(20), &cc(26), 8), None, "7 apart doesn't split into 7 whole steps");
        assert_eq!(ControlSource::row(&cc(20), &ControlSource::MidiCc { channel: 1, cc: 27 }, 8), None, "another channel");
        assert_eq!(ControlSource::row(&cc(20), &ControlSource::MidiNote { channel: 0, note: 27 }, 8), None, "another kind");
        let osc = |a: &str| ControlSource::Osc { address: a.into() };
        assert_eq!(ControlSource::row(&osc("/1/fader1"), &osc("/1/fader8"), 8), Some((1..=8).map(|k| osc(&format!("/1/fader{k}"))).collect()));
        assert_eq!(ControlSource::row(&osc("/layer/9/opacity"), &osc("/layer/12/opacity"), 4), Some((9..=12).map(|k| osc(&format!("/layer/{k}/opacity"))).collect()));
        assert_eq!(ControlSource::row(&osc("/fader1"), &osc("/fader12"), 12), Some((1..=12).map(|k| osc(&format!("/fader{k}"))).collect()));
        assert_eq!(ControlSource::row(&osc("/a/fader1"), &osc("/b/fader8"), 8), None, "differ by more than a number");
        assert_eq!(ControlSource::row(&osc("/x1"), &osc("/x1"), 8), None, "the same address twice");
    }

    #[test]
    fn learning_a_row_maps_each_control_once() {
        let mut comp = super::super::Composition::default();
        let ids: Vec<u64> = (0..3).map(|_| comp.add_layer().id).collect();
        let cc = |cc| ControlSource::MidiCc { channel: 0, cc };
        // Only three layers: their opacities take the first three knobs.
        assert_eq!(comp.learn_row(Row::Opacity, 8, &cc(1), &cc(3)), Ok(3));
        assert_eq!(comp.control_maps.len(), 3);
        assert_eq!(comp.control_maps[2].target, super::super::ControlTarget::LayerOpacity(ids[2]));
        assert_eq!(comp.control_maps[2].source, cc(3));
        // Again with other knobs: replaced, not doubled.
        assert_eq!(comp.learn_row(Row::Opacity, 8, &cc(11), &cc(13)), Ok(3));
        assert_eq!(comp.control_maps.len(), 3);
        assert!(comp.learn_row(Row::Columns, 8, &cc(1), &cc(4)).is_err(), "8 columns from 4 knobs: no whole steps");
        assert_eq!(comp.learn_row(Row::Columns, 8, &ControlSource::MidiNote { channel: 0, note: 36 }, &ControlSource::MidiNote { channel: 0, note: 43 }), Ok(8));
        assert!(comp.learn_row(Row::Dials(ids[0]), 8, &cc(1), &cc(8)).is_err(), "no dials yet");
    }
}
