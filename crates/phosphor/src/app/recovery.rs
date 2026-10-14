//! Crash recovery: snapshot, restore and the recovered banner.

use super::*;

impl PhosphorApp {
    /// Puts the show back the way it was before an unexpected quit.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn restore(&mut self, s: crate::recovery::Snapshot) {
        let st = &mut self.state;
        st.comp.ensure_outputs();
        st.selected = s.selected.or(st.selected);
        {
            let mut map = st.output.lock().unwrap();
            for w in s.outputs.iter().filter(|w| w.open) {
                let e = map.entry(w.id).or_default();
                e.open = true;
                if let Some(d) = w.display.and_then(|id| st.displays.iter().find(|d| d.id == id)) {
                    e.display = Some(d.id);
                    if w.fullscreen {
                        e.place = Some((d.clone(), 0, 0));
                    }
                }
            }
        }
        if s.session && let Err(e) = st.net.start() {
            st.notice = Some(e);
        }
        if s.audio_open {
            let _ = st.audio.open(s.audio_device.as_deref());
        }
        self.recovered = Some(s.streaming);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn snapshot(&self) -> crate::recovery::Snapshot {
        let st = &self.state;
        let outputs = st
            .output
            .lock()
            .unwrap()
            .iter()
            .map(|(id, o)| crate::recovery::OutputWindowState { id: *id, open: o.open, display: o.display, fullscreen: o.fullscreen })
            .collect();
        crate::recovery::Snapshot {
            comp: Some(st.comp.clone()),
            dock: to_text(&self.dock).map(serde_json::Value::String),
            detached: self.detached.clone(),
            outputs,
            session: st.net.running(),
            audio_open: st.audio.is_open() && !st.audio.is_network(),
            audio_device: st.audio.current().map(str::to_string),
            streaming: st.stream.running(),
            selected: st.selected,
        }
    }

    pub(crate) fn recovered_banner(&mut self, ctx: &egui::Context) {
        let Some(was_streaming) = self.recovered else { return };
        egui::Area::new(egui::Id::new("recovered_banner")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 44.0)).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).fill(egui::Color32::from_rgb(20, 44, 32)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("{} Recovered after an unexpected quit", icon::LIFEBUOY)).color(theme::ACCENT).strong());
                    ui.weak("project, layout, outputs, session and audio were restored");
                    if was_streaming && ui.button(format!("{} Resume stream", icon::BROADCAST)).clicked() {
                        let (w, h) = (self.state.comp.width, self.state.comp.height);
                        let now = ui.ctx().input(|i| i.time);
                        if let Err(e) = self.state.stream.start(w, h, now) {
                            self.state.notice = Some(e);
                        }
                        self.recovered = None;
                    }
                    if ui.small_button("OK").clicked() {
                        self.recovered = None;
                    }
                });
            });
        });
    }
}
