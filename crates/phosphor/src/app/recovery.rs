//! Crash recovery: snapshot, restore and the recovered banner.

use super::*;

impl PhosphorApp {
    /// Puts the show back the way it was before an unexpected quit (and says so).
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn restore(&mut self, s: crate::recovery::Snapshot) {
        let streaming = s.streaming;
        self.resume(s);
        let st = &mut self.state;
        self.recovered = Some(streaming);
        // Opted in: the crash report goes by itself (once: this runs once per crash).
        if st.bug.send_crashes {
            let report = crate::support::crash_diagnostics(st);
            let sent = st.bug.sent.clone();
            crate::crash_report::send(report, move |r| {
                *sent.lock().unwrap() = Some(r);
            });
        }
    }

    /// Back where it was when it last closed (a normal quit or a crash): outputs open on their screens, movies at
    /// the same place, the session and the audio input on. (The project itself comes back with the settings.)
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn resume(&mut self, s: crate::recovery::Snapshot) {
        let st = &mut self.state;
        st.comp.ensure_outputs();
        st.selected = s.selected.or(st.selected);
        for (layer, t) in &s.video_positions {
            st.media.seek(*layer, *t);
        }
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
            video_positions: st.media.positions(),
        }
    }

    pub(crate) fn recovered_banner(&mut self, ctx: &egui::Context) {
        let Some(was_streaming) = self.recovered else { return };
        egui::Area::new(egui::Id::new("recovered_banner")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 44.0)).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).fill(egui::Color32::from_rgb(20, 44, 32)).show(ui, |ui| {
                crate::ui::toolbar(ui, |ui| {
                    ui.label(egui::RichText::new(format!("{} Recovered after an unexpected quit", icon::LIFEBUOY)).color(theme::accent()).strong());
                    crate::ui::help(ui, "project, layout, outputs, session and audio were restored");
                    if was_streaming && crate::ui::command(ui, format!("{} Resume stream", icon::BROADCAST)).clicked() {
                        let (w, h) = (self.state.comp.width, self.state.comp.height);
                        let now = ui.ctx().input(|i| i.time);
                        if let Err(e) = self.state.stream.start(w, h, now) {
                            self.state.notice = Some(e);
                        }
                        self.recovered = None;
                    }
                    match self.state.bug.sent.lock().unwrap().as_ref() {
                        Some(Ok(())) => {
                            crate::ui::hint(ui, "a crash report was sent");
                        }
                        Some(Err(e)) => {
                            crate::ui::hint(ui, format!("the crash report wasn't sent ({e})"));
                        }
                        None => {}
                    }
                    // Opt-in crash report: the bug window, filled in, with the end of crash.log in its diagnostics.
                    if crate::ui::command(ui, format!("{} Report it", icon::BUG)).on_hover_text("Opens Report a Bug with what happened filled in; nothing is sent until you send it").clicked() {
                        let bug = &mut self.state.bug;
                        bug.open = true;
                        bug.include_diagnostics = true;
                        if bug.text.trim().is_empty() {
                            bug.text = "Phosphor quit unexpectedly. What I was doing just before: ".into();
                        }
                        self.recovered = None;
                    }
                    if crate::ui::small_command(ui, "OK").clicked() {
                        self.recovered = None;
                    }
                });
            });
        });
    }
}
