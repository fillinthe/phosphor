//! One Inspector for everything: it shows whatever was selected last — a layer (and its clip), a mapping slice or
//! mask (the Studio's properties), or a Control Surface widget — and a row on top switches between the ones that
//! are selected. Which came last is found by watching each selection change, so nothing that selects has to know.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum Focus {
    #[default]
    Layer,
    Slice,
    Widget,
}

const KEY: &str = "inspector_focus";

/// What the Inspector should show this frame (and the row to switch, when more than one kind is selected).
pub(crate) fn focus(ui: &mut egui::Ui, st: &State) -> Focus {
    let ctx = ui.ctx().clone();
    let id = egui::Id::new(KEY);
    let now = (st.selected, st.selected_surface, super::selected_widget(&ctx));
    let (mut focus, last): (Focus, (Option<u64>, Option<u64>, Option<u64>)) = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
    // The selection that changed decides (several at once: the slice, then the widget, then the layer).
    if now.0 != last.0 && now.0.is_some() {
        focus = Focus::Layer;
    }
    if now.2 != last.2 && now.2.is_some() {
        focus = Focus::Widget;
    }
    if now.1 != last.1 && now.1.is_some() {
        focus = Focus::Slice;
    }
    // What can be shown: anything selected (a layer always can).
    let slice = now.1.is_some_and(|s| st.comp.slice(s).is_some() || st.comp.outputs.iter().any(|o| o.mapping.masks.iter().any(|m| m.id == s)));
    let widget = now.2.is_some_and(|w| st.comp.surface.widget(w).is_some());
    if (focus == Focus::Slice && !slice) || (focus == Focus::Widget && !widget) {
        focus = Focus::Layer;
    }
    if slice || widget {
        let mut options = vec![(Focus::Layer, label(icon::STACK, "Layer"))];
        if slice {
            options.push((Focus::Slice, label(icon::POLYGON, "Slice")));
        }
        if widget {
            options.push((Focus::Widget, label(icon::SLIDERS_HORIZONTAL, "Widget")));
        }
        segmented(ui, &mut focus, &options, |_, _, _| {});
        ui.add_space(2.0);
    }
    ctx.data_mut(|d| d.insert_temp(id, (focus, now)));
    focus
}
