//! A movie layer's playhead (with its loop marks) and speed: the three things you reach for most, with opacity. Shown
//! in the Inspector and on the layer cards (Clips, Layers). Cards don't hold the players: the app publishes every
//! movie layer's position once a frame (`publish`) and carries out the seeks they ask for.

use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui;
use phosphor_engine::{ControlTarget, LayerState, SourceSpec};

use crate::app::State;
use crate::ui::{Level, scrub_bar};

const POSITIONS: &str = "phosphor_movie_positions";
const SEEK_REQUEST: &str = "phosphor_movie_seek";

/// The playhead with the loop section's marks (not while it loops by beats). Returns a seek (seconds).
pub(crate) fn loop_bar(ui: &mut egui::Ui, layer: &mut LayerState, pos: f64, dur: f64, height: f32) -> Option<f64> {
    let (mut lo, mut hi) = (layer.loop_in, layer.loop_out);
    let seek = scrub_bar(ui, pos, dur, (layer.loop_beats <= 0.0).then_some((&mut lo, &mut hi)), height);
    if (lo, hi) != (layer.loop_in, layer.loop_out) {
        layer.loop_in = lo.min(hi - 0.01).max(0.0);
        layer.loop_out = hi.max(lo + 0.01).min(1.0);
    }
    seek
}

/// The speed slider (hidden while the movie follows the tempo: its speed is worked out then).
pub(crate) fn speed_level(ui: &mut egui::Ui, layer: &mut LayerState, width: f32) {
    if layer.sync_beats <= 0.0 {
        Level::range(&mut layer.rate, 0.1, 4.0, "Speed").log().default(1.0).width(width).target(ControlTarget::LayerSpeed(layer.id)).show(ui);
    }
}

/// A movie layer's playhead (slim) for a layer card, `width` wide; nothing for other sources or before it opens.
pub(crate) fn card_bar(ui: &mut egui::Ui, layer: &mut LayerState, width: f32) {
    if !matches!(layer.source, SourceSpec::Movie { .. }) {
        return;
    }
    let Some((pos, dur)) = ui.ctx().data(|d| d.get_temp::<Arc<HashMap<u64, (f64, f64)>>>(egui::Id::new(POSITIONS))).and_then(|p| p.get(&layer.id).copied()) else { return };
    ui.allocate_ui(egui::vec2(width, 12.0), |ui| {
        if let Some(t) = loop_bar(ui, layer, pos, dur, 12.0) {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(SEEK_REQUEST), (layer.id, t)));
        }
    });
}

/// Once a frame: every movie layer's position for the cards, and the seek one asked for.
pub(crate) fn publish(ctx: &egui::Context, st: &mut State) {
    let positions: HashMap<u64, (f64, f64)> = st.comp.layers.iter().filter(|l| matches!(l.source, SourceSpec::Movie { .. })).filter_map(|l| Some((l.id, st.media.position(l.id)?))).collect();
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(POSITIONS), Arc::new(positions)));
    if let Some((layer, t)) = ctx.data_mut(|d| d.remove_temp::<(u64, f64)>(egui::Id::new(SEEK_REQUEST))) {
        st.media.seek(layer, t);
    }
}
