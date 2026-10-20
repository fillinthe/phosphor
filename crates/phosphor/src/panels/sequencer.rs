//! Step sequencer tracks in the Modulators panel (VDMX's Step Sequencer): each track is a shared modulator, so
//! any control can follow it (right-click a slider › Modulate › pick the track).

use phosphor_engine::{STEP_COUNTS, STEP_RATES, StepTrack};

use super::*;

/// One track's settings and steps. `beats` = the clock now (for the playing step), `seed` for Random.
pub(crate) fn track_editor(ui: &mut egui::Ui, track: &mut StepTrack, beats: f32, seed: u64) -> bool {
    let mut back_to_signal = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Steps").small());
        let mut len = track.len();
        let counts: Vec<(usize, String)> = STEP_COUNTS.iter().map(|&n| (n, n.to_string())).collect();
        if segmented(ui, &mut len, &counts, |_, r, n| {
            r.clone().on_hover_text(format!("{n} steps"));
        }) {
            track.resize(len);
        }
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Each").small());
        let mut rate = track.rate();
        let rates: Vec<(f32, String)> = STEP_RATES.iter().map(|&(v, n)| (v, n.to_string())).collect();
        if segmented(ui, &mut rate, &rates, |_, r, _| {
            r.clone().on_hover_text("How long each step lasts");
        }) {
            track.per_beat = rate;
        }
    });
    ui.horizontal_wrapped(|ui| {
        toggle(ui, &mut track.gate, "Gates").on_hover_text("On/off steps (pads) instead of levels");
        if !track.gate {
            toggle(ui, &mut track.glide, "Glide").on_hover_text("Slide from one step to the next");
        }
        if ui.small_button(icon::ARROW_LEFT).on_hover_text("Shift the pattern one step earlier").clicked() {
            track.shift(-1);
        }
        if ui.small_button(icon::ARROW_RIGHT).on_hover_text("Shift the pattern one step later").clicked() {
            track.shift(1);
        }
        if ui.button(label(icon::DICE_FIVE, "Random")).clicked() {
            track.randomize(seed);
        }
        if ui.button("Clear").clicked() {
            track.steps.iter_mut().for_each(|s| *s = 0.0);
        }
        if ui.small_button("Plain signal").on_hover_text("Back to an ordinary signal (LFO, audio…)").clicked() {
            back_to_signal = true;
        }
    });
    let playing = track.step_at(beats);
    step_bars(ui, &mut track.steps, Some(playing), track.gate, if track.gate { 26.0 } else { 48.0 });
    back_to_signal
}

/// A new step track on the beat: four on the floor as gates.
pub(crate) fn new_track() -> StepTrack {
    StepTrack { gate: true, steps: [1.0, 0.0, 0.0, 0.0].repeat(4), ..Default::default() }
}
