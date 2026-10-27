//! When preview stills (effect / generator tiles, clip pictures) are taken again. A still is taken once per tile;
//! the picture it's made from may still have been black then (the app starting, a movie opening), so all of them are
//! taken again a few times after the tiles change, and after that one tile a second, so they follow the picture
//! without costing much.

/// Seconds after the tiles change that every still is taken again.
const AGAIN_AFTER: [f64; 3] = [0.5, 1.5, 4.0];
/// Then one tile this often.
const ONE_EVERY: f64 = 1.0;

#[derive(Default)]
pub(crate) struct Stills {
    since: f64,
    round: usize,
    next: f64,
    turn: usize,
}

/// Which stills to take again this frame.
#[derive(Debug, PartialEq)]
pub(crate) enum Again {
    None,
    All,
    One(usize),
}

impl Stills {
    /// `changed` = the tiles asked for aren't the ones of last frame; `n` = how many there are.
    pub(crate) fn plan(&mut self, now: f64, changed: bool, n: usize) -> Again {
        if changed {
            (self.since, self.round, self.next) = (now, 0, now + AGAIN_AFTER[0]);
        }
        if n == 0 || now < self.next {
            return Again::None;
        }
        if self.round < AGAIN_AFTER.len() {
            self.round += 1;
            self.next = AGAIN_AFTER.get(self.round).map_or(now + ONE_EVERY, |t| self.since + t);
            return Again::All;
        }
        self.next = now + ONE_EVERY;
        self.turn = self.turn.wrapping_add(1);
        Again::One(self.turn % n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stills_are_taken_again_a_few_times_then_one_at_a_time() {
        let mut s = Stills::default();
        assert_eq!(s.plan(10.0, true, 4), Again::None, "just taken");
        assert_eq!(s.plan(10.6, false, 4), Again::All);
        assert_eq!(s.plan(10.7, false, 4), Again::None);
        assert_eq!(s.plan(11.6, false, 4), Again::All);
        assert_eq!(s.plan(14.1, false, 4), Again::All);
        assert_eq!(s.plan(14.2, false, 4), Again::None);
        let ones: Vec<Again> = (0..5).map(|i| s.plan(15.2 + i as f64, false, 4)).collect();
        assert_eq!(ones, [Again::One(1), Again::One(2), Again::One(3), Again::One(0), Again::One(1)]);
        // Scrolled: new tiles start over.
        assert_eq!(s.plan(30.0, true, 4), Again::None);
        assert_eq!(s.plan(30.5, false, 4), Again::All);
    }
}
