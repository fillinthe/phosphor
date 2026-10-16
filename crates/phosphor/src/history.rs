//! Undo / redo for the project. A snapshot is taken once an edit settles (mouse up and nothing
//! changed for a moment), so one drag of a slider is one undo step.

use phosphor_engine::Composition;

const LIMIT: usize = 100;
/// Seconds an edit has to be still before it becomes an undo step.
const SETTLE: f64 = 0.4;

#[derive(Default)]
pub struct History {
    undo: Vec<Composition>,
    redo: Vec<Composition>,
    /// The project as of the last undo step.
    committed: Option<Composition>,
    changed_at: Option<f64>,
    /// When the project was last compared with `committed`.
    checked_at: f64,
}

/// Fields that change on their own (tempo following, transitions in flight) aren't edits.
fn settle_volatile(from: &Composition, into: &mut Composition) {
    into.bpm = from.bpm;
    for l in &mut into.layers {
        if let Some(src) = from.layers.iter().find(|s| s.id == l.id) {
            l.transition = src.transition.clone();
        }
    }
}

impl History {
    /// Call once per frame. `busy` = a mouse button is down (mid-drag).
    pub fn track(&mut self, comp: &Composition, now: f64, busy: bool) {
        let Some(committed) = self.committed.as_mut() else {
            self.committed = Some(comp.clone());
            return;
        };
        // Comparing two whole projects every frame adds up; ten times a second is plenty (SETTLE is longer).
        if now - self.checked_at < 0.1 {
            return;
        }
        self.checked_at = now;
        settle_volatile(comp, committed);
        if committed == comp {
            self.changed_at = None;
            return;
        }
        let since = *self.changed_at.get_or_insert(now);
        if busy || now - since < SETTLE {
            return;
        }
        let prev = std::mem::replace(committed, comp.clone());
        self.undo.push(prev);
        if self.undo.len() > LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.changed_at = None;
    }

    /// Steps back; returns the project to load.
    pub fn undo(&mut self, current: &Composition) -> Option<Composition> {
        // An edit that hasn't settled yet is undone first.
        let pending = self.committed.as_ref().is_some_and(|c| c != current);
        let target = if pending { self.committed.clone()? } else { self.undo.pop()? };
        self.redo.push(current.clone());
        self.committed = Some(target.clone());
        self.changed_at = None;
        Some(target)
    }

    pub fn redo(&mut self, current: &Composition) -> Option<Composition> {
        let target = self.redo.pop()?;
        self.undo.push(current.clone());
        self.committed = Some(target.clone());
        self.changed_at = None;
        Some(target)
    }

    pub fn can_undo(&self, current: &Composition) -> bool {
        !self.undo.is_empty() || self.committed.as_ref().is_some_and(|c| c != current)
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_settle_into_steps() {
        let mut h = History::default();
        let mut comp = Composition::default();
        h.track(&comp, 0.0, false);
        comp.master_fade = 0.5;
        h.track(&comp, 1.0, true); // dragging
        comp.master_fade = 0.3;
        h.track(&comp, 1.2, true);
        h.track(&comp, 2.0, false); // released and still
        comp.bpm = 133.0; // tempo following isn't an edit
        h.track(&comp, 3.0, false);
        h.track(&comp, 4.0, false);
        let back = h.undo(&comp).unwrap();
        assert_eq!(back.master_fade, 1.0);
        assert!(h.undo(&back).is_none());
        let fwd = h.redo(&back).unwrap();
        assert_eq!(fwd.master_fade, 0.3);
    }
}
