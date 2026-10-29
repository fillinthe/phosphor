//! Quick visuals: one entry point for Surprise / Shuffle / React / Fill / Instant Set, used by the Quick menu,
//! the command palette, the toolbar buttons and controllers (rules: `phosphor_engine::model::quick`).

use super::*;
use phosphor_engine::Dice;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quick {
    /// A random look on every layer.
    Surprise,
    /// A random look on the selected layer.
    SurpriseLayer,
    /// New values for the selected layer's generator and effects.
    Shuffle,
    /// Music-following motion on the selected layer.
    React,
    /// Fill the selected layer's empty clips with variations.
    Fill,
    /// Replace the layers with a ready-to-play set (undo brings them back).
    InstantSet,
    /// Take the motion off every layer (the look stays).
    Still,
}

impl Quick {
    pub const ALL: [Quick; 7] = [Quick::Surprise, Quick::SurpriseLayer, Quick::Shuffle, Quick::React, Quick::Still, Quick::Fill, Quick::InstantSet];

    /// Menu / palette wording (one place for both).
    pub const fn label(self) -> &'static str {
        match self {
            Quick::Surprise => "Surprise Me (all layers)",
            Quick::SurpriseLayer => "Surprise the Selected Layer",
            Quick::Shuffle => "Shuffle the Selected Layer",
            Quick::React => "Make It React to the Music",
            Quick::Fill => "Fill Empty Clips with Variations",
            Quick::InstantSet => "Instant Set (replace layers)",
            Quick::Still => "Stop the Motion (all layers)",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Quick::Surprise => icon::DICE_FIVE,
            Quick::SurpriseLayer => icon::DICE_TWO,
            Quick::Shuffle => icon::SHUFFLE,
            Quick::React => icon::WAVEFORM,
            Quick::Fill => icon::GRID_NINE,
            Quick::InstantSet => icon::ROCKET_LAUNCH,
            Quick::Still => icon::PAUSE,
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Quick::Surprise => "A random look on every layer: generators, effects, blends and motion that follows the music",
            Quick::SurpriseLayer => "A random look on the selected layer (movies and cameras keep playing, their effects change)",
            Quick::Shuffle => "New settings for the selected layer's generator and effects, near where they are",
            Quick::React => "Make the selected layer move with the music: kick pumps, beat-synced sweeps, colour drift",
            Quick::Fill => "Fill the selected layer's empty clip slots with generator variations",
            Quick::InstantSet => "Three layers of clips ready to play, column 1 launched. Outputs and mapping stay. Undo brings your layers back",
            Quick::Still => "Keep the look but take off all the motion (music-following and sweeps) on every layer",
        }
    }
}

/// A fresh seed every call.
pub(crate) fn dice() -> Dice {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let t = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    Dice::new(t ^ N.fetch_add(1, Ordering::Relaxed).wrapping_mul(0xA24B_AED4_963E_E407))
}

impl State {
    /// The selected layer (not a group or MASTER), else the top layer.
    pub(crate) fn quick_layer(&self) -> Option<u64> {
        self.selected.filter(|id| self.comp.layers.iter().any(|l| l.id == *id)).or_else(|| self.comp.layers.last().map(|l| l.id))
    }

    /// Runs a Quick action; layer actions go to the selected layer.
    pub(crate) fn quick(&mut self, q: Quick, now: f64) {
        self.quick_on(q, self.quick_layer(), now);
    }

    /// Runs a Quick action; layer actions go to `layer`.
    pub(crate) fn quick_on(&mut self, q: Quick, layer: Option<u64>, now: f64) {
        log::info!("quick: {q:?}");
        if q == Quick::Still {
            for l in &mut self.comp.layers {
                l.still();
            }
            self.notice = Some("Motion off: the look stays still (undo with ⌘Z)".into());
            return;
        }
        let mut d = dice();
        let now32 = now as f32;
        // Every Quick action adds motion that listens to the music (except Shuffle): make sure something is listening.
        if q != Quick::Shuffle && !self.audio.is_open() && !self.audio_declined {
            match self.audio.open(None) {
                Ok(()) => self.notice = Some("Listening to the default audio input (Audio panel to change it)".into()),
                // No input or no permission: the motion still runs from the LFOs; say so once.
                Err(e) => {
                    self.audio_declined = true;
                    self.notice = Some(format!("No audio input ({e}); music-following motion is off, beat-synced motion still runs"));
                }
            }
        }
        match q {
            Quick::Surprise => {
                self.comp.surprise(now32, &mut d);
                self.notice = Some("Surprise: a random look on every layer that moves with the sound. ⌘Z undoes it; Quick › Stop the Motion keeps it still".into());
            }
            Quick::InstantSet => {
                self.comp.instant_set(now32, &mut d);
                self.selected = self.comp.layers.last().map(|l| l.id);
                self.clip_page = 0;
                self.notice = Some("Instant set: hit columns 1–8 (keys 1–9), tap the tempo. Undo (⌘Z) brings your layers back.".into());
            }
            _ => {
                let Some(id) = layer else {
                    self.notice = Some("Add a layer first".into());
                    return;
                };
                match q {
                    Quick::SurpriseLayer => self.comp.surprise_layer(id, now32, &mut d),
                    Quick::Fill => {
                        let columns = self.comp.clip_columns;
                        let made = self.comp.fill_variations(id, columns, &mut d);
                        if made == 0 {
                            self.notice = Some("No empty clip slots on that layer (add a column, or clear some)".into());
                        }
                    }
                    _ => {
                        if let Some(l) = self.comp.layer_mut(id) {
                            if q == Quick::Shuffle {
                                l.shuffle(0.5, &mut d);
                            } else {
                                l.react(&mut d);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The Quick actions as menu buttons (top bar dropdown, clips toolbar). Returns the one clicked.
pub fn quick_menu(ui: &mut egui::Ui) -> Option<Quick> {
    let mut out = None;
    for q in Quick::ALL {
        if q == Quick::Fill {
            ui.separator();
        }
        if crate::ui::command(ui, format!("{} {}", q.icon(), q.label())).on_hover_text(q.about()).clicked() {
            out = Some(q);
            ui.close();
        }
    }
    out
}

/// Surprise / Shuffle / React / Fill for one layer, as a row of small buttons (Inspector, Layer Windows).
pub fn quick_row(ui: &mut egui::Ui) -> Option<Quick> {
    let mut out = None;
    ui.horizontal_wrapped(|ui| {
        for (q, text) in [(Quick::SurpriseLayer, "Surprise"), (Quick::Shuffle, "Shuffle"), (Quick::React, "React"), (Quick::Fill, "Fill clips")] {
            if crate::ui::small_command(ui, format!("{} {text}", q.icon())).on_hover_text(q.about()).clicked() {
                out = Some(q);
            }
        }
    });
    out
}

/// The Quick menu for layers (Layers panel): Surprise where it lands (the selected layer, else every layer),
/// Surprise several layers picked by hand, and every other Quick action.
pub(crate) fn layers_quick_menu(ui: &mut egui::Ui, st: &mut State) {
    use egui_phosphor::regular as icon;
    let now = ui.ctx().input(|i| i.time);
    let target = st.selected.filter(|id| st.comp.layers.iter().any(|l| l.id == *id));
    let where_ = target.map_or("all layers".to_string(), |id| crate::ui::layer_name(&st.comp.layers, id));
    if crate::ui::command(ui, format!("{} Surprise · {where_}", icon::DICE_FIVE)).on_hover_text(format!("{} (⌘R: all layers)", Quick::Surprise.about())).clicked() {
        match target {
            Some(id) => st.quick_on(Quick::SurpriseLayer, Some(id), now),
            None => st.quick(Quick::Surprise, now),
        }
        ui.close();
    }
    ui.menu_button(format!("{} Surprise which layers", icon::DICE_FIVE), |ui| {
        let key = egui::Id::new("surprise_picks");
        let mut picks: Vec<u64> = ui.ctx().data(|d| d.get_temp(key)).unwrap_or_default();
        for l in st.comp.layers.iter().rev() {
            let mut on = picks.contains(&l.id);
            if crate::ui::toggle(ui, &mut on, &l.name).changed() {
                if on { picks.push(l.id) } else { picks.retain(|x| *x != l.id) }
            }
        }
        picks.retain(|id| st.comp.layers.iter().any(|l| l.id == *id));
        let mut surprise: Vec<u64> = Vec::new();
        crate::ui::toolbar(ui, |ui| {
            if crate::ui::command_if(ui, !picks.is_empty(), format!("{} Surprise these", icon::DICE_FIVE)).clicked() {
                surprise = picks.clone();
                ui.close();
            }
            if crate::ui::command(ui, "All layers").clicked() {
                surprise = st.comp.layers.iter().map(|l| l.id).collect();
                ui.close();
            }
        });
        ui.ctx().data_mut(|d| d.insert_temp(key, picks));
        for id in surprise {
            st.quick_on(Quick::SurpriseLayer, Some(id), now);
        }
    });
    ui.separator();
    if let Some(q) = quick_menu(ui) {
        st.quick(q, now);
    }
}
