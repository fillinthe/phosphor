//! One UI frame: input, actions, panels, windows.

use super::*;

impl PhosphorApp {
    pub(crate) fn ui_frame(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let wall = ctx.input(|i| i.time);
        // Rendering offline, the show runs on the render's clock (a frame at a time, never ahead of real time).
        let now = self.state.offline.as_ref().map_or(wall, |o| o.now());

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
            // (A follower's project comes from its leader.)
            if run.cfg.project.is_none() && run.cfg.follow.is_none() {
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
            let names: Vec<(u64, String)> = self.state.comp.modulators.iter().map(|m| (m.id, m.name.clone())).collect();
            d.insert_temp(egui::Id::new(crate::ui::MODULATORS), names);
            d.insert_temp(egui::Id::new(panels::MAP_MODE), self.state.map_mode);
            d.insert_temp(egui::Id::new(panels::LEARNING), self.state.control.learning.clone());
        });
        self.control_requests(&ctx);
        self.menu_actions(&ctx, now);
        self.fade_in_effects();
        self.shortcuts(&ctx);
        let busy = ctx.input(|i| i.pointer.any_down());
        if ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::PointerButton { .. } | egui::Event::Key { .. } | egui::Event::MouseWheel { .. }))) {
            self.state.touched(now);
        }
        self.state.history.track(&self.state.comp, now, busy);
        self.update_title(&ctx);
        #[cfg(not(target_arch = "wasm32"))]
        self.start_show(now);
        // A mapping surface asked for its 3D model file.
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(sid) = ctx.data_mut(|d| d.remove_temp::<u64>(egui::Id::new(crate::panels::PICK_MODEL))) {
            self.state.dialogs.pick_file(crate::dialogs::Purpose::Model3d(sid), Some(("3D model (OBJ)", &["obj"])));
        }
        #[cfg(not(target_arch = "wasm32"))]
        crate::panels::link_window(&ctx, &mut self.state);
        #[cfg(not(target_arch = "wasm32"))]
        crate::panels::ai_shader_window(&ctx, &mut self.state);
        // Something dropped onto a slice on the mapping canvas: a file or input gets a layer of its own there, a
        // layer is shown there.
        if let Some((sid, what)) = ctx.data_mut(|d| {
            let id = egui::Id::new(crate::panels::SLICE_DROP);
            let got = d.get_temp::<(u64, crate::panels::SliceDrop)>(id);
            d.remove::<(u64, crate::panels::SliceDrop)>(id);
            got
        }) {
            crate::panels::apply_slice_drop(&mut self.state.comp, sid, what);
        }
        // A slice asked for a picture or movie of its own.
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(sid) = ctx.data_mut(|d| d.remove_temp::<u64>(egui::Id::new(crate::panels::PICK_SLICE_FILE))) {
            let exts = crate::media::dialog_extensions();
            self.state.dialogs.pick_file(crate::dialogs::Purpose::SliceFile(sid), Some(("Picture or movie", &exts)));
        }
        // An output asked for a stage photo.
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(oid) = ctx.data_mut(|d| d.remove_temp::<u64>(egui::Id::new(crate::panels::PICK_STAGE_PHOTO))) {
            self.state.dialogs.pick_file(crate::dialogs::Purpose::StagePhoto(oid), Some(("Photo", &["jpg", "jpeg", "png", "heic", "tif", "tiff", "webp"])));
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.handle_dialogs_and_folders(now);
        #[cfg(not(target_arch = "wasm32"))]
        self.state.sync_backgrounds();
        self.state.run_live_text(now);
        // Eyedropper: one pixel of the program picture (read once, on the click).
        if let Some(uv) = crate::ui::eyedropper::take_request(&ctx)
            && let Some((w, h, px)) = self.state.engine.as_ref().and_then(|e| e.read_output())
            && let Some(rgb) = crate::ui::eyedropper::pixel_at(w, h, &px, uv)
        {
            crate::ui::eyedropper::deliver(&ctx, rgb);
        }
        self.state.run_multi_edit();
        self.state.run_timeline(now);
        self.state.run_computed(now);
        // Layer names for settings that pick a layer (mask layers), rebuilt only when they change.
        let id = egui::Id::new(crate::ui::LAYER_NAMES);
        let names = ctx.data(|d| d.get_temp::<std::sync::Arc<Vec<String>>>(id));
        if names.as_ref().is_none_or(|n| n.len() != self.state.comp.layers.len() || n.iter().zip(&self.state.comp.layers).any(|(a, l)| *a != l.name)) {
            let fresh: Vec<String> = self.state.comp.layers.iter().map(|l| l.name.clone()).collect();
            ctx.data_mut(|d| d.insert_temp(id, std::sync::Arc::new(fresh)));
        }
        if let Some(target) = ctx.data_mut(|d| {
            let id = egui::Id::new(crate::ui::KEY_REQUEST);
            let t = d.get_temp::<phosphor_engine::ControlTarget>(id);
            d.remove::<phosphor_engine::ControlTarget>(id);
            t
        }) {
            self.state.key_now(&target);
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.state.run_stream_bar(now);
        #[cfg(not(target_arch = "wasm32"))]
        {
            crate::media::video::sound::set_auto_level(self.state.comp.sound_auto_level);
            crate::media::video::sound::set_eq(self.state.comp.sound_eq);
            self.state.stream.mute_audio = self.state.comp.sound_muted;
        }
        self.remote_sync(now);
        self.sync_mapping_from_output();
        {
            let State { control, comp, clock, cue_run, ltc_out, .. } = &mut self.state;
            let heard = control.poll(comp);
            control.poll_clock(now);
            control.tick_clock_out(clock.beats(now), clock.bpm);
            #[cfg(not(target_arch = "wasm32"))]
            if !control.mtc_out_port.is_empty() || control.ltc_out_pair.is_some() || ltc_out.is_some() {
                // Time of day to the millisecond (the wall clock plus the local offset from UTC).
                let utc = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
                let tod = (utc + crate::calendar::local_now().1 as f64).rem_euclid(86_400.0);
                let (show, running) = (cue_run.clock.time(now), cue_run.clock.running());
                control.tick_mtc_out(show, running, tod);
                // LTC out: the same time as timecode audio on the chosen output pair.
                match control.ltc_out_pair {
                    None => *ltc_out = None,
                    Some(pair) => {
                        if ltc_out.is_none() {
                            *ltc_out = timecode_out::LtcOut::start(pair);
                        }
                        if let Some(l) = ltc_out {
                            l.set_pair(pair);
                            if control.mtc_time_of_day { l.tick(tod, true) } else { l.tick(show, running) }
                        }
                    }
                }
            }
            control.apply(comp, now as f32);
            control.feedback(comp, now);
            control.surface_osc(comp);
            control.sends(comp);
            control.publish_query(comp, now);
            if heard {
                self.state.touched(now);
            }
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
            // A follower's clips, fades and cues come from the leader (in the leader's time).
            #[cfg(not(target_arch = "wasm32"))]
            let leading = st.following.is_none();
            #[cfg(target_arch = "wasm32")]
            let leading = true;
            if leading {
                st.comp.expire_transitions(now as f32);
            }
            // Movies remember how they last played (twice a second is plenty).
            if now >= st.media_memory_at {
                st.media_memory_at = now + 0.5;
                st.comp.remember_media_settings();
            }
            if leading {
                st.run_autopilot(now);
                st.run_step_fires(now);
                st.run_auto_show(now);
                st.run_cues(now);
            }
            #[cfg(not(target_arch = "wasm32"))]
            st.poll_rhythm();
            #[cfg(not(target_arch = "wasm32"))]
            st.projectors.poll();
            st.run_play(now);
            st.update_now_playing(now);
        }
        if let Some(n) = self.bench.as_ref().map(|b| b.cfg.previews).filter(|n| *n > 0) {
            for (i, d) in phosphor_engine::shaders::all_effects().iter().cycle().take(n).enumerate() {
                let fx = phosphor_engine::EffectState { id: 8_000_000 + i as u64, effect: d.id.to_string(), enabled: true, values: d.defaults(), mix: 1.0, mods: Vec::new(), blend: Default::default() };
                self.state.preview_wants.push((format!("bench:{i}"), phosphor_engine::PreviewItem::Effect(fx), true));
            }
        }
        let due = match &self.state.offline {
            Some(o) => o.now() <= wall,
            None => self.state.render_due(now),
        };
        if due {
            self.render(now);
        }
        if self.state.offline.is_some() {
            self.state.run_offline();
            ctx.request_repaint();
        }
        if self.state.notice != self.logged_notice {
            if let Some(n) = &self.state.notice {
                log::info!("notice: {n}");
            }
            self.logged_notice = self.state.notice.clone();
        }
        if self.lock.active {
            self.locked_top_bar(ui);
        } else {
            self.top_bar(ui);
            self.status_line(ui);
        }
        rescue_windows(&mut self.dock, ctx.content_rect(), std::mem::take(&mut self.gather_windows));

        egui::CentralPanel::no_frame().show(ui, |ui| {
            let mut style = egui_dock::Style::from_egui(ui.style());
            style.tab_bar.fill_tab_bar = true;
            let mut viewer = Viewer { st: &mut self.state, detach: None };
            DockArea::new(&mut self.dock)
                .style(style)
                .show_close_buttons(!self.lock.active && !self.panels_locked)
                .draggable_tabs(!self.panels_locked)
                .tab_context_menus(!self.panels_locked)
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
            self.show_tab_anywhere(tab);
        }
        if let Some(src) = ctx.data_mut(|d| {
            let id = egui::Id::new(panels::PREVIEW_REQUEST);
            let s = d.get_temp::<panels::MonitorSource>(id);
            d.remove::<panels::MonitorSource>(id);
            s
        }) {
            self.state.new_monitor = Some(src);
        }
        if let Some(id) = ctx.data_mut(|d| d.remove_temp::<u64>(egui::Id::new(panels::SNAPSHOT_REQUEST))) {
            self.state.snapshot_source = Some(id);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(px) = self.state.snapshot_ready.take() {
            self.pending_snapshot = Some(px);
            self.state.dialogs.save_file(crate::dialogs::Purpose::SaveSnapshot, ("PNG", &["png"]), "Phosphor snapshot.png");
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
        #[cfg(not(target_arch = "wasm32"))]
        self.convert_window(&ctx);
        self.save_layout_window(&ctx);
        self.account_windows(&ctx);
        crate::support::bug_window(&ctx, &mut self.state);
        self.welcome_window(&ctx);
        self.templates_window(&ctx);
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
        // "Only selected" (Studio) lasts only while the Studio is on screen.
        let studio_at: f64 = ctx.data(|d| d.get_temp(egui::Id::new(crate::panels::STUDIO_SHOWN))).unwrap_or(f64::MIN);
        if now - studio_at > 0.5 {
            for o in self.state.comp.outputs.iter_mut().filter(|o| !o.mapping.solo.is_empty()) {
                o.mapping.solo.clear();
            }
        }
        self.output_windows(&ctx);
        self.finish_bench(&ctx, now);
        // A VJ app renders continuously. At "Display rate" every vsync; with a cap, ask for the repaint
        // just past the half-way point so a 120 Hz display lands on every other refresh (60 Hz: every one).
        match self.state.render_rate.fps() {
            None => ctx.request_repaint(),
            Some(fps) => ctx.request_repaint_after(std::time::Duration::from_secs_f64(1.0 / fps as f64 - 1.0 / 120.0 + 0.0005)),
        }
    }
}
