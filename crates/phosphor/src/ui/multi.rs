//! Selecting several things on a canvas (slices, surface widgets): Shift / Cmd-click adds or removes one, a drag on
//! empty space draws a band that selects everything it touches. The set lives in temp data under a key of yours;
//! the "primary" (the one whose Inspector shows) is yours to keep.

use super::*;

/// A multi-selection kept under one key. `primary` is always part of it (or is the whole selection on its own).
#[derive(Clone, Copy)]
pub(crate) struct Selection(egui::Id);

impl Selection {
    pub(crate) fn new(key: impl std::hash::Hash + std::fmt::Debug) -> Self {
        Selection(egui::Id::new(key))
    }

    /// Everything selected: the set, or just `primary` when nothing else is.
    pub(crate) fn all(self, ctx: &egui::Context, primary: Option<u64>) -> Vec<u64> {
        let set: Vec<u64> = ctx.data(|d| d.get_temp(self.0)).unwrap_or_default();
        match primary {
            Some(p) if set.contains(&p) => set,
            Some(p) => vec![p],
            None => Vec::new(),
        }
    }

    /// A click on `id`: with `extend` (Shift or Cmd) it joins (or leaves) the selection, else it's the only one selected.
    pub(crate) fn click(self, ctx: &egui::Context, primary: &mut Option<u64>, id: u64, extend: bool) {
        let mut set = self.all(ctx, *primary);
        if extend {
            if let Some(i) = set.iter().position(|s| *s == id) {
                set.remove(i);
                *primary = set.last().copied();
            } else {
                set.push(id);
                *primary = Some(id);
            }
        } else {
            set = vec![id];
            *primary = Some(id);
        }
        ctx.data_mut(|d| d.insert_temp(self.0, set));
    }

    /// Selects exactly `ids`: the last one becomes the primary.
    pub(crate) fn set(self, ctx: &egui::Context, primary: &mut Option<u64>, ids: Vec<u64>) {
        *primary = ids.last().copied();
        ctx.data_mut(|d| d.insert_temp(self.0, ids));
    }

    /// Nothing selected.
    pub(crate) fn clear(self, ctx: &egui::Context, primary: &mut Option<u64>) {
        self.set(ctx, primary, Vec::new());
    }
}

/// Shift or Cmd held: a click adds to the selection instead of replacing it.
pub(crate) fn extending(ui: &egui::Ui) -> bool {
    ui.input(|i| i.modifiers.shift || i.modifiers.command)
}

/// A band dragged out on empty canvas (`bg` senses drag). Drawn while it's dragged; returns the band once the
/// button is let go, for the caller to select what it touches. Scales with the canvas it's on.
pub(crate) fn rubber_band(ui: &mut egui::Ui, bg: &egui::Response, key: impl std::hash::Hash + std::fmt::Debug) -> Option<egui::Rect> {
    let id = egui::Id::new(key);
    let pointer = ui.ctx().input(|i| i.pointer.interact_pos());
    if bg.drag_started_by(egui::PointerButton::Primary)
        && let Some(p) = pointer
    {
        ui.ctx().data_mut(|d| d.insert_temp(id, p));
    }
    let start: Option<egui::Pos2> = ui.ctx().data(|d| d.get_temp(id));
    let (Some(start), Some(p)) = (start, pointer) else { return None };
    let band = egui::Rect::from_two_pos(start, p);
    if bg.dragged_by(egui::PointerButton::Primary) {
        let c = accent();
        ui.painter().rect(band, 0.0, c.gamma_multiply(0.12), Stroke::new(1.0, c.gamma_multiply(0.7)), StrokeKind::Inside);
        return None;
    }
    if bg.drag_stopped_by(egui::PointerButton::Primary) {
        ui.ctx().data_mut(|d| d.remove_temp::<egui::Pos2>(id));
        return Some(band);
    }
    if !ui.input(|i| i.pointer.primary_down()) {
        ui.ctx().data_mut(|d| d.remove_temp::<egui::Pos2>(id));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selection_grows_with_shift_and_keeps_its_primary() {
        let ctx = egui::Context::default();
        let sel = Selection::new("test_sel");
        let mut primary = None;
        assert!(sel.all(&ctx, primary).is_empty());
        sel.click(&ctx, &mut primary, 1, false);
        sel.click(&ctx, &mut primary, 2, true);
        sel.click(&ctx, &mut primary, 3, true);
        assert_eq!((sel.all(&ctx, primary), primary), (vec![1, 2, 3], Some(3)));
        sel.click(&ctx, &mut primary, 3, true);
        assert_eq!((sel.all(&ctx, primary), primary), (vec![1, 2], Some(2)), "shift-click again leaves");
        sel.click(&ctx, &mut primary, 9, false);
        assert_eq!(sel.all(&ctx, primary), vec![9], "a plain click selects one");
        primary = Some(4);
        assert_eq!(sel.all(&ctx, primary), vec![4], "a primary picked elsewhere is the selection");
        sel.clear(&ctx, &mut primary);
        assert_eq!((sel.all(&ctx, primary), primary), (Vec::new(), None));
    }
}
