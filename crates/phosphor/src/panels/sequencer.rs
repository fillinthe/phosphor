//! Step sequencer tracks in the Modulators panel (VDMX's Step Sequencer): each track is a shared modulator, so
//! any control can follow it (right-click a slider › Modulate › pick the track).

use phosphor_engine::{STEP_COUNTS, STEP_RATES, StepTrack};

use super::*;

/// One track's settings and steps. `beats` = the clock now (for the playing step), `seed` for Random.
pub(crate) fn track_editor(ui: &mut egui::Ui, track: &mut StepTrack, beats: f32, seed: u64) -> bool {
    let mut back_to_signal = false;
    crate::ui::field(ui, "Steps", |ui| {
        let mut len = track.len();
        let counts: Vec<(usize, String)> = STEP_COUNTS.iter().map(|&n| (n, n.to_string())).collect();
        if segmented(ui, &mut len, &counts, |_, r, n| {
            r.clone().on_hover_text(format!("{n} steps"));
        }) {
            track.resize(len);
        }
    });
    crate::ui::field(ui, "Each", |ui| {
        let mut rate = track.rate();
        let rates: Vec<(f32, String)> = STEP_RATES.iter().map(|&(v, n)| (v, n.to_string())).collect();
        if segmented(ui, &mut rate, &rates, |_, r, _| {
            r.clone().on_hover_text("How long each step lasts");
        }) {
            track.per_beat = rate;
        }
    });
    ui.horizontal_wrapped(|ui| {
        let mut colour = track.is_colour();
        if toggle(ui, &mut colour, "Colours").on_hover_text("A colour on each step, painted into colour settings (a solid colour, a tint…)").changed() {
            track.set_colour(colour);
        }
        if !colour {
            toggle(ui, &mut track.gate, "Gates").on_hover_text("On/off steps (pads) instead of levels");
        }
        if colour {
            toggle(ui, &mut track.glide, "Glide").on_hover_text("Fade from one step's colour to the next");
        } else if !track.gate {
            let mut index = track.choices >= 2;
            if toggle(ui, &mut index, "Choices").on_hover_text("Each step picks one of a few choices (a clip, blend mode, direction…) instead of a level").changed() {
                track.choices = if index { 4 } else { 0 };
            }
            if index {
                guarded!(ui, track.choices, egui::DragValue::new(&mut track.choices).range(2..=64)).on_hover_text("How many choices (match the control it drives: e.g. 4 directions)");
            } else {
                toggle(ui, &mut track.glide, "Glide").on_hover_text("Slide from one step to the next");
            }
        }
        if crate::ui::small_command(ui, icon::ARROW_LEFT).on_hover_text("Shift the pattern one step earlier").clicked() {
            track.shift(-1);
        }
        if crate::ui::small_command(ui, icon::ARROW_RIGHT).on_hover_text("Shift the pattern one step later").clicked() {
            track.shift(1);
        }
        if crate::ui::command(ui, label(icon::DICE_FIVE, "Random")).clicked() {
            track.randomize(seed);
        }
        if crate::ui::command(ui, "Clear").clicked() {
            track.steps.iter_mut().for_each(|s| *s = 0.0);
        }
        if crate::ui::small_command(ui, "Plain signal").on_hover_text("Back to an ordinary signal (LFO, audio…)").clicked() {
            back_to_signal = true;
        }
    });
    let playing = track.step_at(beats);
    if track.is_colour() {
        colour_steps(ui, &mut track.colours, playing);
        return back_to_signal;
    }
    if step_bars(ui, &mut track.steps, Some(playing), track.gate, if track.gate { 26.0 } else { 48.0 }) && track.choices >= 2 {
        // Drawn steps land on whole choices, so the bars show what will be picked.
        let n = track.choices as usize;
        track.steps.iter_mut().for_each(|s| *s = phosphor_engine::pick_unit(phosphor_engine::pick(*s, n), n));
    }
    back_to_signal
}

/// A colour track's steps: a colour button each (the playing one outlined).
fn colour_steps(ui: &mut egui::Ui, colours: &mut [[f32; 3]], playing: usize) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for (i, c) in colours.iter_mut().enumerate() {
            let r = ui.color_edit_button_rgb(c);
            if i == playing {
                ui.painter().rect_stroke(r.rect.expand(1.5), 2.0, Stroke::new(1.5, Color32::WHITE), StrokeKind::Outside);
            }
        }
    });
}

/// A new step track on the beat: four on the floor as gates.
pub(crate) fn new_track() -> StepTrack {
    StepTrack { gate: true, steps: [1.0, 0.0, 0.0, 0.0].repeat(4), ..Default::default() }
}

/// The colour settings a colour track paints: each picked one with a remove button, and a picker to add one.
/// `reds` = every colour setting (its red), named.
pub(crate) fn colour_targets(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug + Copy, reds: &[(ControlTarget, String)], paint: &mut Vec<ControlTarget>) {
    let mut remove = None;
    for (i, t) in paint.iter().enumerate() {
        crate::ui::toolbar(ui, |ui| {
            ui.label(reds.iter().find(|(r, _)| r == t).map_or("(removed)", |(_, n)| n.as_str()));
            if crate::ui::small_command(ui, icon::X).on_hover_text("Stop painting this one").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        paint.remove(i);
    }
    if reds.is_empty() {
        hint(ui, "Nothing with a colour yet: add a Solid colour layer, or an effect with a colour (Tint, Colorize…).");
    } else if let Some(t) = target_picker(ui, (id, "add"), reds, None, 220.0) {
        paint.push(t);
    }
}
