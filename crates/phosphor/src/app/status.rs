//! The status line along the bottom: what's going out and coming in, whichever area is open. Outputs (green dot =
//! window open), streaming / recording, the phone session, Syphon / Spout / NDI, MIDI and OSC activity.
//! Clicking the outputs opens the Outputs area.

use eframe::egui::{self, Color32, RichText};
use egui_phosphor::regular as icon;

use super::PhosphorApp;
use crate::theme;

const LIVE: Color32 = Color32::from_rgb(235, 80, 70);

impl PhosphorApp {
    pub(crate) fn status_line(&mut self, ui: &mut egui::Ui) {
        let now = ui.ctx().input(|i| i.time);
        let mut open_outputs = false;
        egui::Panel::bottom("status_line").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let st = &self.state;
                let small = |t: String, c: Color32| RichText::new(t).small().color(c);
                // Outputs.
                let outs: Vec<(String, bool)> = st.comp.outputs.iter().filter(|o| o.enabled).map(|o| (o.name.clone(), st.output_window_open(o.id))).collect();
                let r = ui
                    .horizontal(|ui| {
                        ui.label(small(icon::PROJECTOR_SCREEN.to_string(), Color32::GRAY));
                        for (name, open) in &outs {
                            let dot = if *open { theme::ACCENT } else { Color32::from_gray(90) };
                            ui.label(small(format!("{} {name}", icon::CIRCLE), dot));
                        }
                    })
                    .response
                    .interact(egui::Sense::click());
                if r.on_hover_text("Outputs (green = its window is open). Click to open the Outputs area.").clicked() {
                    open_outputs = true;
                }
                ui.separator();
                // Streaming / recording.
                if st.stream.running() {
                    let t = st.stream.elapsed(now) as u32;
                    let what = if st.stream.settings.destination == crate::stream::Destination::File { "REC" } else { "LIVE" };
                    ui.label(small(format!("{} {what} {:02}:{:02}", icon::RECORD, t / 60, t % 60), LIVE));
                }
                #[cfg(not(target_arch = "wasm32"))]
                if st.net.running() {
                    let n = st.net.peers().len();
                    ui.label(small(format!("{} Session · {n} connected", icon::DEVICE_MOBILE), Color32::GRAY));
                }
                if st.comp.share_out() {
                    ui.label(small(format!("{} {}", icon::SHARE_NETWORK, if cfg!(windows) { "Spout" } else { "Syphon" }), Color32::GRAY));
                }
                #[cfg(any(target_os = "macos", windows))]
                if st.comp.ndi.enabled {
                    ui.label(small(format!("{} NDI · {} watching", icon::BROADCAST, st.ndi.connections()), Color32::GRAY));
                }
                // Controllers: lit while something arrived in the last second and a half.
                let fresh = |t: std::time::Instant| t.elapsed().as_secs_f32() < 1.5;
                let (midi, osc) = {
                    let a = st.control.activity.lock().unwrap();
                    (a.midi_seen.values().any(|t| fresh(*t)), a.osc.values().any(|s| fresh(s.last_seen)))
                };
                for (on, name) in [(midi, "MIDI"), (osc, "OSC")] {
                    ui.label(small(format!("{} {name}", icon::CIRCLE), if on { theme::ACCENT } else { Color32::from_gray(70) }));
                }
            });
        });
        if open_outputs {
            self.switch_workspace(1);
        }
    }
}
