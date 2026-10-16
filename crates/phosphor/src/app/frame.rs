//! One UI frame: input, actions, panels, windows.

use super::*;

impl PhosphorApp {
    pub(crate) fn ui_frame(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);

        if !self.lock.active {
            self.handle_dropped_files(&ctx);
        }
        self.state.clip_cells.clear();
        self.state.comp.ensure_outputs();
        self.state.comp.sync_media_bin();
        self.state.thumbs.poll(&ctx);
        self.account_events(&ctx);
        self.state.updater.tick(&ctx, now);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(engine) = self.state.engine.as_mut() {
            self.state.isf.poll(engine);
        }
        crate::explorer::load_user_shaders(&mut self.state);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(run) = &mut self.bench
            && !run.rebuilt
            && !self.state.isf.scanning()
        {
            // Effects named on the command line may be ISF shaders that only exist now.
            run.rebuilt = true;
            if run.cfg.project.is_none() {
                let bin = std::mem::take(&mut self.state.comp.media_bin);
                let ndi = self.state.comp.ndi.clone();
                self.state.comp = run.cfg.composition();
                self.state.comp.media_bin = bin;
                self.state.comp.ndi = ndi;
            }
            run.started = None;
            run.samples.clear();
            let quick = run.cfg.quick.take().unwrap_or_default();
            self.bench_quick(&ctx, &quick, now);
        }
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new("phosphor_mod_inputs"), self.state.mod_inputs);
            d.insert_temp(egui::Id::new(panels::MAP_MODE), self.state.map_mode);
            d.insert_temp(egui::Id::new(panels::LEARNING), self.state.control.learning.clone());
        });
        self.control_requests(&ctx);
        self.menu_actions(&ctx, now);
        self.fade_in_effects();
        self.shortcuts(&ctx);
        let busy = ctx.input(|i| i.pointer.any_down());
        self.state.history.track(&self.state.comp, now, busy);
        self.update_title(&ctx);
        #[cfg(not(target_arch = "wasm32"))]
        self.start_show(now);
        #[cfg(not(target_arch = "wasm32"))]
        self.handle_dialogs_and_folders(now);
        self.remote_sync(now);
        self.sync_mapping_from_output();
        {
            let State { control, comp, .. } = &mut self.state;
            control.poll(comp);
            control.poll_clock(now);
            control.apply(comp, now as f32);
            control.feedback(comp, now);
            control.publish_query(comp, now);
        }
        for d in std::mem::take(&mut self.state.control.triggers) {
            self.state.run_deferred(d, now);
        }
        {
            let st = &mut self.state;
            st.clock.follow_midi_clock(now, &st.control.midi_clock);
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(link) = &mut st.link {
                link.sync(&mut st.clock, now);
            }
            st.comp.bpm = st.clock.bpm;
            let beat = st.clock.tick(now);
            let fire_now = match st.comp.launch.timing {
                phosphor_engine::LaunchTiming::Now => true,
                phosphor_engine::LaunchTiming::Beat => beat,
                phosphor_engine::LaunchTiming::Bar => beat && st.clock.beat_in_bar(now) == 0,
            };
            if fire_now {
                for t in std::mem::take(&mut st.pending_triggers) {
                    st.fire(t, now as f32);
                }
            }
            st.comp.expire_transitions(now as f32);
            st.run_autopilot(now);
            st.run_cues(now);
            st.run_play(now);
            st.update_now_playing(now);
        }
        if let Some(n) = self.bench.as_ref().map(|b| b.cfg.previews).filter(|n| *n > 0) {
            for (i, d) in phosphor_engine::shaders::all_effects().iter().cycle().take(n).enumerate() {
                let fx = phosphor_engine::EffectState { id: 8_000_000 + i as u64, effect: d.id.to_string(), enabled: true, values: d.defaults(), mix: 1.0, mods: Vec::new() };
                self.state.preview_wants.push((format!("bench:{i}"), phosphor_engine::PreviewItem::Effect(fx), true));
            }
        }
        self.render(now);
        if self.lock.active {
            self.locked_top_bar(ui);
        } else {
            self.top_bar(ui);
        }
        rescue_windows(&mut self.dock, ctx.content_rect(), std::mem::take(&mut self.gather_windows));

        egui::CentralPanel::no_frame().show(ui, |ui| {
            let mut style = egui_dock::Style::from_egui(ui.style());
            style.tab_bar.fill_tab_bar = true;
            let mut viewer = Viewer { st: &mut self.state, detach: None };
            DockArea::new(&mut self.dock)
                .style(style)
                .show_close_buttons(!self.lock.active)
                .show_leaf_close_all_buttons(false)
                .show_leaf_collapse_buttons(false)
                .show_inside(ui, &mut viewer);
            if let Some(tab) = viewer.detach {
                self.detach(tab);
            }
        });
        self.detached_windows(&ctx);
        crate::explorer::fx_browser_window(&ctx, &mut self.state);
        if !self.lock.active
            && let Some(cmd) = crate::explorer::palette(&ctx, &mut self.state)
        {
            self.run_command(cmd, now);
        }
        self.lock_window(&ctx);
        if let Some(tab) = self.state.open_tab.take() {
            self.show_tab(tab);
        }
        if let Some(src) = self.state.new_monitor.take() {
            self.add_monitor_with(src);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if std::mem::take(&mut self.state.snapshot_requested) {
            self.save_snapshot();
        }
        self.learning_banner(&ctx);
        self.export_window(&ctx);
        self.save_layout_window(&ctx);
        self.account_windows(&ctx);
        crate::support::bug_window(&ctx, &mut self.state);
        crate::update::update_window(&ctx, &mut self.state.updater);
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.import_windows(&ctx);
            self.state.poll_relink();
        }
        self.recovered_banner(&ctx);

        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let rect = ctx.content_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop")));
            painter.rect_filled(rect, 0.0, Color32::from_black_alpha(160));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Drop images or movies to add them as layers",
                egui::FontId::proportional(22.0),
                Color32::WHITE,
            );
        }

        {
            let mut map = self.state.output.lock().unwrap();
            for o in &self.state.comp.outputs {
                if let Some(out) = map.get_mut(&o.id)
                    && !out.mapping_changed
                {
                    out.mapping = o.mapping.clone();
                    out.selected_surface = self.state.selected_surface;
                }
            }
        }
        self.output_windows(&ctx);
        self.finish_bench(&ctx, now);
        // A VJ app renders continuously; the GPU frame rate follows the display's vsync.
        ctx.request_repaint();
    }
}
