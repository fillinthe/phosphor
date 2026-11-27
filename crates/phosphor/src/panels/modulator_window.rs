//! A shared modulator in a window of its own (VDMX's LFO plugin, the owner's "an LFO window, not a right-click
//! menu"): a big live wave with its playhead (click or drag on it to scrub), pause, reverse and retrigger, its signal
//! settings, and every control that follows it with how much each one does. Opened from the Modulators panel; any
//! number can be open, and they stay open while other panels change.

use super::*;

/// Opens modulator `id` in its window (or keeps it open).
pub(crate) fn open_modulator_window(st: &mut State, id: u64) {
    if !st.mod_windows.contains(&id) {
        st.mod_windows.push(id);
    }
}

/// Every open modulator window (drawn by the app each frame, over the panels). Nothing to do while none is open.
pub(crate) fn modulator_windows(ctx: &egui::Context, st: &mut State) {
    if st.mod_windows.is_empty() {
        return;
    }
    let State { mod_windows, comp, .. } = st;
    mod_windows.retain(|id| comp.modulators.iter().any(|m| m.id == *id));
    for (k, id) in st.mod_windows.clone().into_iter().enumerate() {
        let name = st.comp.modulators.iter().find(|m| m.id == id).map_or_else(String::new, |m| m.name.clone());
        let mut open = true;
        let at = ctx.content_rect().center() + egui::vec2(28.0, 28.0) * k as f32;
        crate::ui::dialog(ctx, format!("{} {name}", icon::WAVE_SINE))
            .id(egui::Id::new(("modulator_window", id)))
            .default_pos(at)
            .default_width(480.0)
            .open(&mut open)
            .show(ctx, |ui| modulator_window(ui, st, id));
        if !open {
            st.mod_windows.retain(|x| *x != id);
        }
    }
}

fn modulator_window(ui: &mut egui::Ui, st: &mut State, id: u64) {
    let Some(i) = st.comp.modulators.iter().position(|m| m.id == id) else { return };
    let cycle = st.core.lock().mod_phases.cycle_of(id);
    let value = st.mod_inputs.shared.iter().find(|(m, _)| *m == id).map_or(0.0, |x| x.1);
    let mut scrub = None;
    let m = &mut st.comp.modulators[i];
    ui.horizontal(|ui| {
        crate::ui::link_handle(ui, ("window", id), crate::ui::LinkSource::Modulator { id, name: m.name.clone() }, "Drag onto any slider: it follows this modulator");
        crate::ui::text_field(ui, &mut m.name, "name", Some(140.0));
        ui.add(crate::ui::meter(value).text(format!("{value:.2}")).fill(accent().gamma_multiply(0.7)));
    });
    let wave = m.signal.link == 0 && m.signal.source.is_lfo() && m.sequence.is_none();
    if wave {
        let r = wave_view(ui, &m.signal, cycle, 150.0).on_hover_text("Click or drag to move it to that point of its cycle");
        scrub = crate::ui::scrub_at(&r);
        crate::ui::toolbar(ui, |ui| {
            crate::ui::icon_toggle(ui, &mut m.paused, icon::PAUSE).on_hover_text("Pause: it holds where it is (the controls following it hold too)");
            crate::ui::icon_toggle(ui, &mut m.reverse, icon::ARROWS_LEFT_RIGHT).on_hover_text("Reverse: its cycle runs backwards");
            if crate::ui::small_command(ui, label(icon::SKIP_BACK, "Retrigger")).on_hover_text("Back to the start of its cycle now (map it to a pad: right-click)").clicked() {
                scrub = Some(0.0);
            }
            if m.paused {
                hint(ui, "paused");
            }
        });
    } else {
        hint(ui, format!("Moves with: {}", m.signal.source.name()));
    }
    ui.separator();
    let mut sig = Some(m.signal);
    ui.push_id(("window_sig", id), |ui| modulation_editor_signal(ui, &mut sig, 0.0));
    if let Some(s) = sig {
        m.signal = s;
    }
    ui.separator();
    followed_by(ui, &mut st.comp, id);
    if let Some(at) = scrub {
        st.core.lock().mod_phases.set_cycle(id, at);
    }
}

/// "Followed by": every control following the modulator, each with how much (Amount, or the From–To it sweeps) and
/// invert; then the modulators chained from it.
fn followed_by(ui: &mut egui::Ui, comp: &mut phosphor_engine::Composition, id: u64) {
    crate::ui::heading_help(ui, format!("{} Followed by", icon::LINK), "Each control's own share of this modulator: Amount pushes the slider up (or down, below 0) from where it's set; Sweep runs it between From and To.");
    let chained: Vec<String> = comp.modulators.iter().filter(|m| m.chain.is_some_and(|c| c.from == id)).map(|m| format!("{} (modulator)", m.name)).collect();
    let mut n = 0;
    crate::ui::column_scroll(ui).id_salt(("followers", id)).max_height(260.0).show(ui, |ui| {
        comp.linked_modulations_mut(id, |what, m| {
            ui.push_id(("follower", n), |ui| follower_row(ui, &what, m));
            n += 1;
        });
        for c in &chained {
            ui.label(c);
        }
    });
    if n == 0 && chained.is_empty() {
        crate::ui::empty_note(ui, "Nothing follows it yet. Drag its link handle onto any slider.");
    }
}

/// One follower: its name, Amount (or From / To when it sweeps a range) and invert.
fn follower_row(ui: &mut egui::Ui, what: &str, m: &mut Modulation) {
    ui.horizontal(|ui| {
        let name_w = (ui.available_width() * 0.38).max(80.0);
        ui.add_sized([name_w, crate::ui::CONTROL_H], egui::Label::new(crate::ui::cut_to_width(ui, what, name_w)).truncate()).on_hover_text(what);
        let w = (ui.available_width() - 36.0).max(80.0);
        if m.range {
            Level::percent(&mut m.min, "From").default(0.0).width(w / 2.0 - 3.0).show(ui);
            Level::percent(&mut m.max, "To").default(1.0).width(w / 2.0 - 3.0).show(ui);
        } else {
            Level::range(&mut m.amount, -1.0, 1.0, "Amount").default(0.5).width(w).show(ui);
        }
        crate::ui::icon_toggle(ui, &mut m.invert, icon::ARROWS_DOWN_UP).on_hover_text("Invert (up = down)");
    });
}
