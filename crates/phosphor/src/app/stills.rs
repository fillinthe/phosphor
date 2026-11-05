//! When preview stills (effect / generator tiles, clip pictures) are taken again. A still is taken once per tile
//! and kept: nothing is drawn again unless the mouse is over the tile or it's the picked one (the owner's choice:
//! tiles that redraw on their own look like they're changing). The one exception: when the picture the stills are
//! made from changes (another layer selected, a clip launched on it, its picture arriving), every still is taken
//! again once, a moment later so a movie that was still opening has a frame.

/// Seconds after the source picture changes that every still is taken again.
const AFTER_SOURCE_CHANGE: f64 = 1.0;

#[derive(Default)]
pub(crate) struct Stills {
    /// What the stills were made from (`source` of `plan`).
    source: u64,
    /// When every still is taken again, if a change is pending.
    next: Option<f64>,
}

/// Which stills to take again this frame.
#[derive(Debug, PartialEq)]
pub(crate) enum Again {
    None,
    All,
}

impl Stills {
    /// `changed` = the tiles asked for aren't the ones of last frame (new ones draw fresh anyway); `source` = a key
    /// for the picture they're made from (the layer, what it plays, whether it has a picture); `n` = how many there are.
    pub(crate) fn plan(&mut self, now: f64, changed: bool, source: u64, n: usize) -> Again {
        if source != self.source {
            self.source = source;
            self.next = Some(now + AFTER_SOURCE_CHANGE);
        }
        if changed {
            self.next = None;
        }
        match self.next {
            Some(t) if n > 0 && now >= t => {
                self.next = None;
                Again::All
            }
            _ => Again::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stills_are_kept_and_only_taken_again_when_the_source_picture_changes() {
        let mut s = Stills::default();
        // The tiles appear: drawn once (as new tiles), never again on their own.
        assert_eq!(s.plan(10.0, true, 1, 4), Again::None);
        assert!((0..100).all(|i| s.plan(10.1 + i as f64, false, 1, 4) == Again::None));
        // Another layer selected (or its clip changed): every still again, a moment later.
        assert_eq!(s.plan(120.0, false, 2, 4), Again::None);
        assert_eq!(s.plan(120.5, false, 2, 4), Again::None);
        assert_eq!(s.plan(121.1, false, 2, 4), Again::All);
        assert_eq!(s.plan(121.2, false, 2, 4), Again::None);
        assert!((0..100).all(|i| s.plan(122.0 + i as f64, false, 2, 4) == Again::None));
        // A source change followed by new tiles: the new tiles draw fresh, so no second round.
        assert_eq!(s.plan(300.0, false, 3, 4), Again::None);
        assert_eq!(s.plan(300.2, true, 3, 4), Again::None);
        assert_eq!(s.plan(301.5, false, 3, 4), Again::None);
    }
}
