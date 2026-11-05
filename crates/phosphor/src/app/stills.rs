//! When preview stills (effect / generator tiles, clip pictures) are taken again. A still is taken once per tile;
//! the picture it's made from may still have been black then (the app starting, a movie opening), so all of them are
//! taken again a few times after the tiles (or the picture they're made from) change. After that a still stays as
//! it is: one picture per tile, nothing drawn until the mouse is over it (the owner's choice: no flicker, no GPU work).

/// Seconds after the tiles change that every still is taken again.
const AGAIN_AFTER: [f64; 3] = [0.5, 1.5, 4.0];

#[derive(Default)]
pub(crate) struct Stills {
    since: f64,
    round: usize,
    next: f64,
    /// The layer (or MASTER) the stills were made from.
    pub(crate) source: u64,
}

/// Which stills to take again this frame.
#[derive(Debug, PartialEq)]
pub(crate) enum Again {
    None,
    All,
}

impl Stills {
    /// `changed` = the tiles asked for aren't the ones of last frame; `source` = the layer they're made from;
    /// `n` = how many there are.
    pub(crate) fn plan(&mut self, now: f64, changed: bool, source: u64, n: usize) -> Again {
        if changed || source != self.source {
            (self.since, self.round, self.next, self.source) = (now, 0, now + AGAIN_AFTER[0], source);
        }
        if n == 0 || self.round >= AGAIN_AFTER.len() || now < self.next {
            return Again::None;
        }
        self.round += 1;
        self.next = AGAIN_AFTER.get(self.round).map_or(f64::INFINITY, |t| self.since + t);
        Again::All
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stills_are_taken_again_a_few_times_then_kept() {
        let mut s = Stills::default();
        assert_eq!(s.plan(10.0, true, 0, 4), Again::None, "just taken");
        assert_eq!(s.plan(10.6, false, 0, 4), Again::All);
        assert_eq!(s.plan(10.7, false, 0, 4), Again::None);
        assert_eq!(s.plan(11.6, false, 0, 4), Again::All);
        assert_eq!(s.plan(14.1, false, 0, 4), Again::All);
        // Then the stills stay: nothing is drawn again however long the list is open.
        assert!((0..50).all(|i| s.plan(14.2 + i as f64, false, 0, 4) == Again::None));
        // Scrolled: new tiles start over.
        assert_eq!(s.plan(70.0, true, 0, 4), Again::None);
        assert_eq!(s.plan(70.5, false, 0, 4), Again::All);
        // Another layer selected: the stills are made from its picture, so they're taken again.
        assert_eq!(s.plan(80.0, false, 7, 4), Again::None);
        assert_eq!(s.plan(80.5, false, 7, 4), Again::All);
    }
}
