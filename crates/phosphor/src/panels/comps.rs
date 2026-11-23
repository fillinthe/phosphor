//! Comps in the UI (`phosphor_engine::Comp`): the Inspector for a selected comp and "Move to comp" for a layer. Comps
//! show as bands in Layers / Clips (and strips in the Mixer), not as tabs; new layers go to the open comp, which
//! follows the selection when it moves (`follow_selection_comp`) or a comp's name is clicked (`pick_comp`).

use phosphor_engine::{Comp, Composition};

use super::*;

/// The comps a layer of the open comp could move to (empty with one comp).
pub(crate) fn other_comps(comp: &Composition) -> Vec<(u64, String)> {
    let open = comp.open_comp();
    comp.comps.iter().filter(|c| Some(c.id) != open).map(|c| (c.id, c.name.clone())).collect()
}

/// "Move to comp" for a layer's menu (`others` from `other_comps`; nothing with one comp). Returns the comp picked.
pub(crate) fn move_to_comp_menu(ui: &mut egui::Ui, others: &[(u64, String)]) -> Option<u64> {
    if others.is_empty() {
        return None;
    }
    let mut picked = None;
    ui.menu_button(label(icon::FRAME_CORNERS, "Move to comp"), |ui| {
        for (id, name) in others {
            if crate::ui::command(ui, name).clicked() {
                picked = Some(*id);
                ui.close();
            }
        }
    });
    picked
}

/// Inspector for a selected comp: on/off, outputs it shows on, name, size and fit, opacity, blend, master mix and
/// its effects. Returns an effect-chain action and an output switched on/off.
pub(crate) fn comp_inspector(ui: &mut egui::Ui, c: &mut Comp, thumb: Option<egui::TextureId>, master: (u32, u32), clipboard_len: usize, outs: &[(u64, String, bool)]) -> (Option<FxAction>, Option<(u64, bool)>) {
    let mut fx_action = None;
    let mut send = None;
    crate::ui::column_scroll(ui).auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().slider_width = (ui.available_width() - 210.0).max(80.0);
        let (cw, ch) = c.size(master);
        let w = ui.available_width().min(360.0);
        thumbnail(ui, thumb, egui::vec2(w, w * ch as f32 / cw.max(1) as f32), c.enabled);
        let what = "A comp is a canvas of its own: its layers mix at its size, then its effects run, then it mixes into the master like a layer. The screen button sends it to outputs of its own.";
        crate::ui::name_row(ui, &mut c.name, what, |ui| {
            let on = toggle(ui, &mut c.enabled, "ON");
            mapped_badge(ui, &on, &ControlTarget::LayerEnabled(c.id));
            on.right_menu(|ui| learn_items(ui, &ControlTarget::LayerEnabled(c.id)));
            ui.label(icon::FRAME_CORNERS);
            send = outputs_menu(ui, outs).or(send);
        });
        crate::ui::toolbar(ui, |ui| {
            let mut same = c.width == 0;
            if toggle(ui, &mut same, "Master size").on_hover_text(format!("{}×{}", master.0, master.1)).changed() {
                (c.width, c.height) = if same { (0, 0) } else { master };
            }
            if !same {
                ui.add(egui::DragValue::new(&mut c.width).range(16..=8192).suffix(" w"));
                ui.label("×");
                ui.add(egui::DragValue::new(&mut c.height).range(16..=8192).suffix(" h"));
            }
        });
        if c.width != 0 {
            crate::ui::field(ui, "In the master", |ui| {
                choice_buttons(ui, &mut c.fit, |_, _| {});
            });
        }
        ui.horizontal(|ui| {
            Level::percent(&mut c.opacity, "Opacity").modulation(&mut c.opacity_mod).target(ControlTarget::LayerOpacity(c.id)).show(ui);
        });
        crate::ui::field(ui, "Blend", |ui| {
            choice_combo(ui, ("comp_blend", c.id), &mut c.blend, 120.0, false);
            if let Some(r) = fx_badge(ui, &c.effects, &mut c.fx_bypass) {
                map_menu(ui, &r, ControlTarget::LayerFxBypass(c.id));
            }
        });
        toggle(ui, &mut c.in_mix, "In the master mix").on_hover_text("Off: the comp shows only on outputs, slices and Feed sources that pick it");
        ui.separator();
        fx_action = effects_editor(ui, &mut c.effects, clipboard_len);
    });
    (fx_action, send)
}

/// New layers go into the comp of what's selected (a layer, a group, the comp itself): with every comp in one list
/// there are no tabs to say which is "open". Only when the selection moves (to something else, or the selected layer
/// to another comp): `last` is what it pointed at the frame before. A comp opened another way while the selection
/// stays put ("Only", a new Designer comp, a comp's name in the clip grid) stays open; this used to run every frame
/// and put the open comp back on the selected layer's at once, so new layers landed in the old comp.
pub(crate) fn follow_selection_comp(comp: &mut Composition, selected: Option<u64>, last: &mut Option<(u64, Option<u64>)>) {
    let Some(sel) = selected else {
        *last = None;
        return;
    };
    let target = if comp.comp(sel).is_some() {
        Some(sel)
    } else if let Some(l) = comp.layers.iter().find(|l| l.id == sel) {
        comp.comp_of(l)
    } else if let Some(l) = comp.layers.iter().find(|l| l.parent == Some(sel)) {
        comp.comp_of(l)
    } else {
        None
    };
    // The same thing still selected counts as a move only if it went from one comp to another: a project getting
    // its first comps (None → "Main", e.g. a new Designer comp) isn't the selected layer moving.
    let moved = match *last {
        Some((was, from)) if was == sel => from.is_some() && from != target,
        _ => true,
    };
    *last = Some((sel, target));
    if moved && target.is_some() && target != comp.open_comp {
        comp.open_comp = target;
    }
}

/// Opens comp `id` from a click on it (the clip grid's comp name): it's selected too, so the Inspector shows it and
/// the open comp follows the selection from there.
pub(crate) fn pick_comp(st: &mut State, id: u64) {
    if st.comp.comp(id).is_some() {
        st.comp.open_comp = Some(id);
        st.selected = Some(id);
    }
}

/// A new layer on top of comp `id` (its "+ Layer"), whichever comp was open. Returns the layer's id.
pub(crate) fn add_layer_in(st: &mut State, id: u64) -> Option<u64> {
    st.comp.comp(id)?;
    st.comp.open_comp = Some(id);
    Some(st.add_layer())
}

#[cfg(test)]
mod tests {
    use phosphor_engine::Composition;

    /// One frame of the app: the open comp follows the selection (`frame.rs`).
    fn frame(c: &mut Composition, selected: Option<u64>, last: &mut Option<(u64, Option<u64>)>) {
        super::follow_selection_comp(c, selected, last);
    }

    /// Selecting a layer in Comp 2 makes new layers go to Comp 2 (no tabs to pick it with any more).
    #[test]
    fn new_layers_go_to_the_selected_layers_comp() {
        let mut c = Composition::default();
        let mut last = None;
        let a = c.add_layer().id;
        let side = c.add_comp("Side");
        let b = c.add_layer().id;
        let main = c.comps[0].id;
        assert_eq!(c.layer(b).and_then(|l| l.comp), Some(side), "a new comp opens");
        frame(&mut c, Some(a), &mut last);
        assert_eq!(c.open_comp, Some(main));
        let n = c.add_layer().id;
        assert_eq!(c.comp_of(c.layer(n).unwrap()), Some(main), "the new layer lands with the selected one");
        frame(&mut c, Some(side), &mut last);
        assert_eq!(c.open_comp, Some(side), "selecting the comp itself");
    }

    /// "Only" on comp B while a layer of comp A is still selected: B stays open over the next frames, so new layers go
    /// into B (they used to land in A, which "Only" had just switched off).
    #[test]
    fn show_only_keeps_new_layers_in_that_comp() {
        let mut c = Composition::default();
        let mut last = None;
        let comp_a = c.add_comp("A");
        let a = c.add_layer().id;
        let b = c.add_comp("B");
        frame(&mut c, Some(a), &mut last);
        assert_eq!(c.open_comp(), Some(comp_a), "the selected layer's comp is open");
        c.show_only_comp(b);
        for _ in 0..3 {
            frame(&mut c, Some(a), &mut last);
        }
        assert_eq!(c.open_comp(), Some(b), "B stays open while the selection stands still");
        let n = c.add_layer().id;
        assert_eq!(c.comp_of(c.layer(n).unwrap()), Some(b));
        assert!(c.comp(b).unwrap().enabled && !c.comps.iter().filter(|x| x.id != b).any(|x| x.enabled), "only B shows");
        // Selecting something in A again opens A (the selection moved).
        frame(&mut c, Some(n), &mut last);
        frame(&mut c, Some(a), &mut last);
        assert_eq!(c.open_comp(), Some(comp_a));
    }

    /// A new Designer comp (opened without selecting it) keeps the open comp: new layers go into it.
    #[test]
    fn a_new_designer_comp_keeps_new_layers() {
        let mut c = Composition::default();
        let mut last = None;
        let a = c.add_layer().id;
        frame(&mut c, Some(a), &mut last);
        let d = c.add_comp("Designer");
        c.start_network(d);
        frame(&mut c, Some(a), &mut last);
        frame(&mut c, Some(a), &mut last);
        assert_eq!(c.open_comp(), Some(d));
        let n = c.add_layer().id;
        assert_eq!(c.comp_of(c.layer(n).unwrap()), Some(d), "the new layer is in the Designer comp");
        // Comps already there: a layer of the first selected, then another Designer comp.
        frame(&mut c, Some(a), &mut last);
        let d2 = c.add_comp("Designer");
        c.start_network(d2);
        frame(&mut c, Some(a), &mut last);
        assert_eq!(c.open_comp(), Some(d2));
        let n2 = c.add_layer().id;
        assert_eq!(c.comp_of(c.layer(n2).unwrap()), Some(d2));
    }

    /// The selected layer moved to another comp: the open comp goes with it (the selection's comp changed).
    #[test]
    fn a_selected_layer_moved_to_another_comp_takes_the_open_comp_along() {
        let mut c = Composition::default();
        let mut last = None;
        let a = c.add_layer().id;
        let side = c.add_comp("Side");
        frame(&mut c, Some(a), &mut last);
        assert_ne!(c.open_comp(), Some(side));
        c.move_to_comp(a, side);
        frame(&mut c, Some(a), &mut last);
        assert_eq!(c.open_comp(), Some(side));
    }

    /// Nothing selected, then the same layer again: that counts as a move (it follows).
    #[test]
    fn reselecting_after_nothing_follows_again() {
        let mut c = Composition::default();
        let mut last = None;
        let a = c.add_layer().id;
        let side = c.add_comp("Side");
        let main = c.comps[0].id;
        frame(&mut c, Some(a), &mut last);
        c.open_comp = Some(side);
        frame(&mut c, None, &mut last);
        assert_eq!(c.open_comp(), Some(side));
        frame(&mut c, Some(a), &mut last);
        assert_eq!(c.open_comp(), Some(main));
    }
}
