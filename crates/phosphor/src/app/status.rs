//! The status line along the bottom: what's going out and coming in, whichever area is open. Outputs (green dot =
//! window open), streaming / recording, the phone session, and IN / OUT activity lights (media, live inputs, NDI,
//! Syphon / Spout, sound, MIDI, OSC, DMX).
//! Clicking the outputs opens the Outputs area; Library opens or closes the Library column.

use eframe::egui::{self, Color32, RichText};
use egui_phosphor::regular as icon;

use super::PhosphorApp;
use crate::theme;

const LIVE: Color32 = crate::theme::RECORD;

impl PhosphorApp {
    pub(crate) fn status_line(&mut self, ui: &mut egui::Ui) {
        let now = ui.ctx().input(|i| i.time);
        let mut open_outputs = false;
        let mut unmute = false;
        let mut web_toggle = false;
        let mut open_tab = None;
        let mut lift_blackout = false;
        egui::Panel::bottom("status_line").show(ui, |ui| {
            crate::ui::toolbar(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let st = &self.state;
                let small = |t: String, c: Color32| RichText::new(t).small().color(c);
                // Outputs.
                let outs: Vec<(String, bool)> = st.comp.outputs.iter().filter(|o| o.enabled).map(|o| (o.name.clone(), st.output_window_open(o.id))).collect();
                let r = ui
                    .horizontal(|ui| {
                        ui.label(small(icon::PROJECTOR_SCREEN.to_string(), crate::theme::weak()));
                        for (name, open) in &outs {
                            let dot = if *open { theme::accent() } else { crate::theme::faint() };
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
                    // (Filled badges: what's going out live must read at a glance.)
                    if crate::ui::state_badge(ui, format!("{} {what} {:02}:{:02}", icon::RECORD, t / 60, t % 60), LIVE).on_hover_text("Streaming or recording: click to open its panel").clicked() {
                        open_tab = Some(crate::panels::Tab::Stream);
                    }
                }
                if st.comp.blackout > 0.0
                    && crate::ui::state_badge(ui, format!("{} BLACKOUT", icon::MOON), LIVE).on_hover_text("Every output is fading or faded to black: click to bring the show back").clicked()
                {
                    lift_blackout = true;
                }
                if st.comp.sound_muted
                    && ui.add(egui::Label::new(small(format!("{} Sound muted", icon::SPEAKER_SIMPLE_X), crate::theme::WARN)).sense(egui::Sense::click())).on_hover_text("All sound is off: click to turn it back on").clicked()
                {
                    unmute = true;
                }
                #[cfg(not(target_arch = "wasm32"))]
                if st.net.running() {
                    let n = st.net.peers().len();
                    ui.label(small(format!("{} Session · {n} connected", icon::DEVICE_MOBILE), crate::theme::weak()));
                }
                web_toggle = super::web_badge(ui, st, now);
                // What's coming in and going out: each light is on while something keeps flowing.
                use crate::ui::activity::{Flow, heading, light};
                ui.separator();
                heading(ui, "IN", "What's coming in (lit while it keeps arriving)");
                if light(ui, "Media", Flow::Count(st.core.lock().media.flow.movies), "Movies and clips: frames being played").clicked() {
                    open_tab = Some(crate::panels::Tab::Media);
                }
                let open = st.core.lock().media.open_inputs();
                if open.live {
                    if light(ui, "Live", Flow::Count(st.core.lock().media.flow.live), "Cameras, screens and network streams: frames arriving").clicked() {
                        open_tab = Some(crate::panels::Tab::Inputs);
                    }
                }
                if open.ndi {
                    if light(ui, "NDI", Flow::Count(st.core.lock().media.flow.ndi), "NDI sources on layers: frames arriving").clicked() {
                        open_tab = Some(crate::panels::Tab::Inputs);
                    }
                }
                if open.syphon {
                    if light(ui, if cfg!(windows) { "Spout" } else { "Syphon" }, Flow::Count(st.core.lock().media.flow.syphon), "Other apps' pictures on layers: frames arriving").clicked() {
                        open_tab = Some(crate::panels::Tab::Inputs);
                    }
                }
                if st.audio.is_open() {
                    if light(ui, "Sound", Flow::On(st.audio.analysis().level > 0.01), "Sound in (the input chosen under Audio): lit while it isn't silent").clicked() {
                        open_tab = Some(crate::panels::Tab::Audio);
                    }
                }
                // Controllers.
                let fresh = |t: web_time::Instant| t.elapsed().as_secs_f32() < 1.5;
                let (midi, osc) = {
                    let a = st.control.activity.lock().unwrap();
                    (a.midi_seen.values().any(|t| fresh(*t)), a.osc.values().any(|s| fresh(s.last_seen)))
                };
                if light(ui, "MIDI", Flow::On(midi), "MIDI from controllers").clicked() {
                    open_tab = Some(crate::panels::Tab::Control);
                }
                if light(ui, "OSC", Flow::On(osc), "OSC messages").clicked() {
                    open_tab = Some(crate::panels::Tab::Control);
                }
                if st.control.dmx_in() {
                    if light(ui, "DMX", Flow::Count(st.control.dmx_packets()), "DMX in (Art-Net / sACN) from a lighting desk").clicked() {
                        open_tab = Some(crate::panels::Tab::Control);
                    }
                }
                // Out: only what's switched on.
                let mut outs: Vec<(&str, crate::panels::Tab, Flow, String)> = Vec::new();
                #[cfg(any(target_os = "macos", windows))]
                if st.comp.ndi.enabled {
                    outs.push(("NDI", crate::panels::Tab::Output, Flow::On(st.ndi.connections() > 0), format!("NDI out: {} watching", st.ndi.connections())));
                }
                #[cfg(target_os = "macos")]
                if st.comp.share_out() {
                    let n = st.core.lock().syphon.clients();
                    outs.push(("Syphon", crate::panels::Tab::Output, Flow::On(n > 0), format!("Syphon out: {n} app(s) taking a picture")));
                }
                #[cfg(windows)]
                if st.comp.share_out() {
                    outs.push(("Spout", crate::panels::Tab::Output, Flow::On(true), "Spout out".to_string()));
                }
                #[cfg(not(target_arch = "wasm32"))]
                if st.comp.dmx.enabled {
                    outs.push(("DMX", crate::panels::Tab::Leds, Flow::Count(st.dmx.packets), "DMX out to the lights (Art-Net / sACN / USB)".to_string()));
                }
                #[cfg(not(target_arch = "wasm32"))]
                if st.comp.lasers.armed {
                    let n = st.lasers.sending();
                    outs.push(("Lasers", crate::panels::Tab::Lasers, Flow::On(n > 0), format!("Lasers armed: {n} sending")));
                }
                if !outs.is_empty() {
                    ui.separator();
                    heading(ui, "OUT", "What's going out (lit while something takes it)");
                    for (name, tab, flow, tip) in outs {
                        if light(ui, name, flow, &tip).clicked() {
                            open_tab = Some(tab);
                        }
                    }
                }
                // Right: the side-column toggles (areas), then frame rate and load.
                let s = self.state.stats.latest;
                // CPU as a share of the whole computer (Activity Monitor's figure counts one core as 100%, so 78% on a
                // 10-core Mac is about 8%: it read as if the Mac was nearly full; owner, 2026-11-14).
                let cores = crate::panels::cores() as f64;
                let whole = s.cpu_percent / cores;
                let perf = format!("{:.0} fps · engine {:.1} ms · {:.0} MB", s.fps, s.render_ms, s.memory_mb);
                let slow = s.fps > 0.0 && s.fps < 50.0;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let open = super::areas::library_open(&self.dock);
                    if crate::ui::pick_item(ui, open, RichText::new(format!("{} Library", icon::BOOKS)).small()).on_hover_text("Media, effects and presets in a column on the left (drag it anywhere once it's open)").clicked() {
                        self.toggle_library();
                    }
                    ui.separator();
                    let cpu_color = if whole >= 70.0 { theme::DANGER } else if whole >= 40.0 { theme::WARN } else { crate::theme::weak() };
                    ui.add_sized([70.0, 16.0], egui::Label::new(RichText::new(format!("CPU {whole:>3.0}%")).small().monospace().color(cpu_color)))
                        .on_hover_text(format!("Phosphor's share of the whole computer ({cores:.0} cores). Counted per core, as Activity Monitor does: {:.0}%", s.cpu_percent));
                    ui.label(RichText::new(perf).small().monospace().color(if slow { theme::WARN } else { crate::theme::weak() }))
                        .on_hover_text("Frame rate, the engine's time per frame, memory");
                });
            });
        });
        if lift_blackout {
            self.state.comp.blackout_on = false;
        }
        if let Some(tab) = open_tab {
            self.show_tab_anywhere(tab);
        }
        if web_toggle {
            let on = self.state.comp.web_control_off;
            self.state.set_web_control(on);
        }
        if unmute {
            self.state.comp.sound_muted = false;
        }
        if open_outputs {
            self.switch_workspace(1);
        }
    }
}
