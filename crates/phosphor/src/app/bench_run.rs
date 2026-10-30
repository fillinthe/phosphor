//! Bench mode: stats summary, snapshot, export test.

use super::*;

impl PhosphorApp {
    pub(crate) fn finish_bench(&mut self, ctx: &egui::Context, now: f64) {
        let Some(run) = &mut self.bench else { return };
        // After the project is final (bench rebuilds it once ISF shaders are in).
        if run.rebuilt
            && let Some(want) = run.cfg.send_output.take()
            && let Some(d) = crate::screens::displays().into_iter().find(|d| d.name.to_lowercase().contains(&want.to_lowercase()))
        {
            self.state.send_output_to(d);
            return;
        }
        let started = *run.started.get_or_insert(now);
        // --window: macOS may restore the window to its last (full) size, so set it explicitly once.
        if let Some((w, h)) = run.cfg.window.take() {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
        }
        // (After any --quick actions: they can make what's selected, like slicefx's slices.)
        if run.cfg.quick.is_none()
            && let Some(id) = run.cfg.select.take_if(|_| now - started >= 0.5)
        {
            self.state.selected = Some(id);
            // (A slice's or mask's id selects it on the mapping canvas.)
            if self.state.comp.outputs.iter().any(|o| o.mapping.surfaces.iter().any(|s| s.id == id) || o.mapping.masks.iter().any(|m| m.id == id)) {
                self.state.selected_surface = Some(id);
            }
        }
        // --tab all: every panel in turn, so one that panics when drawn fails the bench.
        if run.cfg.tab.as_deref() == Some("all") {
            let due = ((now - started) / 0.2) as usize;
            if run.tabs_shown <= due && run.tabs_shown < Tab::ALL.len() {
                if run.tabs_shown == 0 {
                    println!("TAB_TOTAL {}", Tab::ALL.len());
                }
                let tab = Tab::ALL[run.tabs_shown];
                run.tabs_shown += 1;
                println!("TAB {}", tab.title());
                self.show_tab(tab);
                return;
            }
        }
        if let Some(col) = run.cfg.trigger_column.take_if(|_| now - started >= run.cfg.trigger_at) {
            self.state.trigger(Trigger::Column(col), now);
            return;
        }
        if now - started < run.cfg.seconds {
            return;
        }
        // --ui-shot: ask egui for a picture of the window, and finish once it arrives.
        if let Some(path) = run.cfg.ui_shot.clone() {
            if !run.shot_requested {
                run.shot_requested = true;
                // A covered window isn't painted on macOS, so the picture would never come: come to the front first.
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                return;
            }
            let shot = ctx.input(|i| i.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }));
            let Some(image) = shot else { return };
            let [w, h] = image.size;
            let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
            match image::save_buffer(&path, &rgba, w as u32, h as u32, image::ColorType::Rgba8) {
                Ok(()) => println!("UI_SHOT {path}"),
                Err(e) => println!("UI_SHOT failed: {e}"),
            }
            run.cfg.ui_shot = None;
        }
        // Drop the first sample: it includes startup and pipeline warm-up.
        let steady: Vec<_> = run.samples.iter().skip(1).copied().collect();
        let n = steady.len().max(1) as f64;
        let avg = |f: fn(&Snapshot) -> f64| steady.iter().map(f).sum::<f64>() / n;
        let summary = Snapshot {
            fps: avg(|s| s.fps),
            frame_ms: avg(|s| s.frame_ms),
            render_ms: avg(|s| s.render_ms),
            cpu_percent: avg(|s| s.cpu_percent),
            memory_mb: avg(|s| s.memory_mb),
            passes: steady.last().map_or(0, |s| s.passes),
            audio_level: avg(|s| s.audio_level),
            gpu_ms: avg(|s| s.gpu_ms),
            worst_ms: steady.iter().map(|s| s.worst_ms).fold(0.0, f64::max),
        };
        // CPU per frame (ms of one core) is steadier than the percentage when the frame rate varies.
        println!("SUMMARY {} cpu_ms_per_frame={:.2}", summary.to_json(), if summary.fps > 0.0 { summary.cpu_percent * 10.0 / summary.fps } else { 0.0 });
        println!("AUDIO {}", self.state.audio.debug_line());
        #[cfg(target_os = "macos")]
        if self.state.vcam.on {
            println!("VCAM connected={} status={:?}", self.state.vcam.connected(), crate::virtual_camera::status());
        }
        #[cfg(not(target_arch = "wasm32"))]
        if std::env::var_os("PHOSPHOR_STARTER").is_some() {
            let rows: Vec<String> = self.state.comp.layers.iter().map(|l| format!("{}={} playing={:?}", l.name, l.clips.iter().filter(|c| !c.is_empty()).count(), l.source.label())).collect();
            println!("STARTER {} credits={}", rows.join(" "), self.state.comp.media_bin.iter().filter(|m| m.credit.is_some()).count());
        }
        for l in &self.state.comp.layers {
            if let phosphor_engine::SourceSpec::Text(t) = &l.source
                && t.live != phosphor_engine::LiveText::Off
            {
                println!("LIVETEXT {:?} source={:?} text={:?}", t.live, t.live_source, t.text);
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        if !self.state.comp.live_data.readings().is_empty() {
            let vals: Vec<String> = self.state.comp.live_data.readings().iter().map(|n| format!("{n}={:?}", self.state.comp.hand(n))).collect();
            println!("LIVEDATA {} problems={:?}", vals.join(" "), self.state.feeds.problems);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(line) = self.state.online.bench_line() {
            let credits = self.state.comp.media_bin.iter().filter(|m| m.credit.is_some()).count();
            let layer = self.state.comp.layers.last().map(|l| l.source.label()).unwrap_or_default();
            let font = self.state.comp.layers.iter().find_map(|l| if let phosphor_engine::SourceSpec::Text(t) = &l.source { Some(crate::text::font_name(&t.font)) } else { None }).unwrap_or_default();
            let fx: Vec<String> = self.state.comp.layers.last().map(|l| l.effects.iter().map(|e| e.effect.clone()).collect()).unwrap_or_default();
            println!("{line} credited_files={credits} top_layer={layer} top_effects={fx:?} text_font={font:?} notice={:?}", self.state.notice);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(s) = self.state.link_import.status() {
            let layer = self.state.comp.layers.last().map(|l| l.source.label()).unwrap_or_default();
            println!("LINK {s:?} layer_source={layer} media_bin={}", self.state.comp.media_bin.len());
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(s) = self.state.ai_shader.status() {
            let last = self.state.editor.shaders.last().map(|s| (s.slug.clone(), s.error.is_none())).unwrap_or_default();
            println!("AI {s:?} newest_shader={} compiles={}", last.0, last.1);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(l) = &self.state.following {
            let show = l.show_time().unwrap_or(f64::NAN);
            println!(
                "FOLLOW {} projects={} layers={} rtt_ms={:.1} show={:.3} beats={:.3} bpm={} wall={:.3} pos={:?}",
                l.status,
                l.projects,
                self.state.comp.layers.len(),
                l.rtt().unwrap_or(f64::NAN) * 1000.0,
                show,
                self.state.clock.beats(now),
                self.state.clock.bpm,
                web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64()),
                self.state.media.positions()
            );
        }
        if self.state.net.followers.load(std::sync::atomic::Ordering::Relaxed) > 0 {
            println!(
                "LEADER followers={} beats={:.3} now={now:.3} bpm={} wall={:.3} pos={:?}",
                self.state.net.followers.load(std::sync::atomic::Ordering::Relaxed),
                self.state.clock.beats(now),
                self.state.clock.bpm,
                web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64()),
                self.state.media.positions()
            );
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.state.scanner.status != crate::scanner::Status::Idle {
            println!("SCAN {:?}", self.state.scanner.status);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.state.led_scan.status != crate::led_scan::Status::Idle {
            println!("LEDSCAN {:?} dmx_packets={}", self.state.led_scan.status, self.state.dmx.packets);
        }
        for l in self.state.comp.layers.iter().filter(|l| l.collection.is_some()) {
            let file = match &l.source {
                phosphor_engine::SourceSpec::Movie { name, .. } | phosphor_engine::SourceSpec::Image { name, .. } => name.clone(),
                _ => "-".into(),
            };
            let c = l.collection.clone().unwrap_or_default();
            println!("COLLECTION {c}: {} files, layer \"{}\" showing {file}", self.state.comp.collection_items(&c).len(), l.name);
        }
        if self.state.net.running() {
            println!("WEB control={} master={:.2} last={:?}", if self.state.comp.web_control_off { "off" } else { "on" }, self.state.comp.master_fade, self.state.web_activity.as_ref().map(|(_, w)| w.as_str()));
            for l in &self.state.web_log {
                println!("WEBLOG {} {} | {}", l.wall, l.who, l.what);
            }
        }
        if !self.state.comp.modulators.is_empty() {
            let vals: Vec<String> = self.state.comp.modulators.iter().map(|m| format!("{}={:.2}", m.name, self.state.mod_inputs.shared.iter().find(|(id, _)| *id == m.id).map_or(0.0, |x| x.1))).collect();
            println!("MODULATORS {}", vals.join(" "));
        }
        if let Some(e) = &self.state.engine {
            println!("GPU timing: {}", e.gpu_timing_status());
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.state.comp.tracking.enabled {
            let vals: Vec<String> = self.state.comp.tracking.outputs().iter().map(|n| format!("{}={:.3}", n.trim_start_matches("Track "), self.state.comp.hand(n).unwrap_or(-1.0))).collect();
            println!("TRACK {} · {}", self.state.tracker.status, vals.join(" "));
        }
        if self.state.comp.auto_show.enabled {
            let reacting = self.state.comp.layers.iter().filter(|l| !l.generator_mods.is_empty() || l.effects.iter().any(|f| !f.mods.is_empty())).count();
            println!("AUTOSHOW columns fired: {} · layers reacting: {reacting}/{}", self.state.auto_run.fired, self.state.comp.layers.len());
        }
        let causes: Vec<String> = ctx.repaint_causes().iter().map(|c| c.to_string()).collect();
        println!("UI_FPS {:.1} repaint causes: {}", self.ui_frames as f64 / run.cfg.seconds.max(1.0), causes.join(" | "));
        println!("LAYERS {:?}", self.state.comp.layers.iter().map(|l| (l.name.clone(), l.enabled, l.opacity, l.source.label())).collect::<Vec<_>>());
        for (layer, e) in &self.state.media.errors {
            println!("LAYER_ERROR {layer}: {e}");
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(dir) = run.cfg.export.clone() {
            let req = crate::export::Request {
                comp: self.state.comp.clone(),
                name: "Bench Show".into(),
                dest: dir.into(),
                zip: true,
                lock: None,
                autostart: false,
                screens: Vec::new(),
                layout: None,
            };
            let p = crate::export::start(req);
            for _ in 0..1200 {
                if p.lock().unwrap().done {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let p = p.lock().unwrap().clone();
            println!("EXPORT app={:?} zip={:?} media={} bytes={} error={:?}", p.output, p.zip, p.media_files, p.bytes, p.error);
        }
        let an = self.state.audio.analysis();
        println!("BEAT bpm {:.1} confidence {:.2} clock {:.1} follow {} downbeat shifts {}", an.bpm, an.confidence, self.state.clock.bpm, self.state.clock.follow_audio, self.state.clock.downbeat.shifts);
        // Bar phase check for a clip cut at a known tempo (PHOSPHOR_BENCH_BPM): the clip's own bar phase vs the clock's.
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(bpm) = std::env::var("PHOSPHOR_BENCH_BPM").map(|v| v.parse::<f64>().unwrap_or(0.0))
            && bpm > 0.0
            && let Some(l) = self.state.comp.layers.iter().find(|l| crate::app::beat_match::movie_path(&l.source).is_some())
            && let Some((pos, _)) = self.state.media.position(l.id)
        {
            let clip_phase = (pos * bpm / 60.0).rem_euclid(4.0);
            let clock_phase = self.state.clock.beats(now).rem_euclid(4.0);
            println!("BARPHASE clip {clip_phase:.2} clock {clock_phase:.2} diff {:.2} beats", (clock_phase - clip_phase).rem_euclid(4.0));
        }
        println!("CONTROL {}", self.state.control.osc_status);
        #[cfg(not(target_arch = "wasm32"))]
        println!("PLAYERS me={:?} others={:?}", crate::control::discovery::identity().0, crate::control::discovery::players());
        #[cfg(windows)]
        println!("SPOUT on={} running={} name={:?} error={:?}", self.state.comp.spout, self.state.spout.running(), self.state.spout.name(), self.state.spout.error);
        #[cfg(any(target_os = "macos", windows))]
        if run.cfg.ndi {
            println!("NDI_SOURCES {:?}", crate::ndi::sources());
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (rate, played) = crate::media::video::sound::stats();
            println!("SOUND output {rate} Hz, {played} sample frames played");
        }
        for l in &self.state.comp.layers {
            if let Some(info) = self.state.media.video_info(l.id) {
                println!("VIDEO {} ({:?}): {info}", l.name, l.direction);
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        println!("ISF {} loaded, {} skipped, reloaded {:?}", self.state.isf.loaded, self.state.isf.skipped, self.state.isf.reloaded);
        #[cfg(not(target_arch = "wasm32"))]
        if !self.state.comp.timeline.tracks.is_empty() || !self.state.comp.timeline.events.is_empty() || self.state.comp.timeline.sound.is_some() {
            let sound = self.state.timeline_sound.as_ref().and_then(|s| s.position());
            println!("TIMELINE pos={:.2} playing={} sound_at={:?}", self.state.timeline_pos, self.state.timeline_play.is_some(), sound.map(|p| (p * 100.0).round() / 100.0));
        }
        println!("MEDIA {} files in bin, {} thumbnails pending", self.state.comp.media_bin.len(), self.state.comp.media_bin.iter().filter(|m| self.state.thumbs.is_pending(&m.path)).count());
        if self.state.stream.running() {
            let h = self.state.stream.history();
            let kbps: Vec<String> = h.iter().map(|s| format!("{:.0}", s.kbps)).collect();
            println!("STREAM_HEALTH {:?} samples={} kbps=[{}] last_speed={:.2}", self.state.stream.health(), h.len(), kbps.join(","), h.back().map_or(0.0, |s| s.speed));
            self.state.stream.stop_and_wait();
            println!("RECORDED {}", self.state.stream.settings.file_path);
        }
        for r in self.state.recorders.iter_mut().filter(|r| r.running()) {
            r.stop_and_wait();
            println!("RECORDED {}", r.settings.file_path);
        }
        if let (Some(path), Some(engine)) = (&run.cfg.snapshot, &self.state.engine)
            && let Some((w, h, pixels)) = engine.read_output()
        {
            match image::save_buffer(path, &pixels, w, h, image::ColorType::Rgba8) {
                Ok(()) => println!("SNAPSHOT {path}"),
                Err(e) => eprintln!("snapshot failed: {e}"),
            }
            // Outputs that differ from the program (mapping, masks, colour), as <name>-out<N>.png.
            for (i, o) in self.state.comp.outputs.iter().enumerate().filter(|(_, o)| o.mapping.active() || !o.mapping.masks.is_empty() || !o.color.is_neutral() || o.program.is_some()) {
                if let Some((w, h, px)) = engine.read_output_of(o.id) {
                    let p = path.trim_end_matches(".png").to_string() + &format!("-out{}.png", i + 1);
                    if image::save_buffer(&p, &px, w, h, image::ColorType::Rgba8).is_ok() {
                        println!("SNAPSHOT {p}");
                    }
                }
            }
        }
        self.bench = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// Bench `--quick a,b`: Quick actions on the project the bench runs.
    pub(crate) fn bench_quick(&mut self, ctx: &egui::Context, names: &str, now: f64) {
        use super::Quick as Q;
        for name in names.split(',').filter(|n| !n.is_empty()) {
            if name == "update" {
                // Updater end to end against PHOSPHOR_UPDATE_URL: check, install, print UPDATE_READY.
                self.state.updater.test = true;
                self.state.updater.window = true;
                self.state.updater.check(ctx);
                continue;
            }
            #[cfg(not(target_arch = "wasm32"))]
            if name == "beat" {
                // Visual beat of the first movie layer, scanned right here (prints RHYTHM …).
                let path = self.state.comp.layers.iter().find_map(|l| crate::app::beat_match::movie_path(&l.source));
                match path.as_deref().map(|p| (p, crate::media::rhythm::analyze(p, 120.0))) {
                    Some((p, Ok(r))) => println!("RHYTHM {p}: {:.1} bpm period {:.3}s phase {:.3}s duration {:.2}s confidence {:.2} {} -> loop_in {:.3} beats {}", r.bpm, r.period, r.phase, r.duration, r.confidence, if r.steady() { "STEADY" } else { "NOT STEADY (left as is)" }, crate::media::rhythm::settings_for(&r, 1.0, self.state.comp.bpm).0, crate::media::rhythm::settings_for(&r, 1.0, self.state.comp.bpm).1),
                    Some((p, Err(e))) => println!("RHYTHM {p}: {e}"),
                    None => println!("RHYTHM: no movie layer"),
                }
                continue;
            }
            #[cfg(not(target_arch = "wasm32"))]
            if name == "beatmatch" {
                if let Some(id) = self.state.comp.layers.iter().find(|l| crate::app::beat_match::movie_path(&l.source).is_some()).map(|l| l.id) {
                    self.state.match_beat(crate::media::rhythm::MatchTarget::Layer(id));
                }
                continue;
            }
            if name == "lock" {
                self.lock_now(String::new());
                continue;
            }
            if name == "bug" || name == "bugout" {
                self.state.bug.open = true;
                self.state.bug.detached = name == "bugout";
                self.state.bug.text = "Example: dragging a movie onto the Clips grid shows a black cell.".into();
                continue;
            }
            #[cfg(any(target_os = "macos", windows))]
            if name == "login" {
                println!("QUICK login: {:?}", crate::login_item::set(true));
                continue;
            }
            if name == "twowindows" {
                // Two inverted masks on the first output: only two windows of the picture show.
                self.state.comp.ensure_outputs();
                let (a, b) = (self.state.comp.new_id(), self.state.comp.new_id());
                if let Some(o) = self.state.comp.outputs.first_mut() {
                    for (id, x) in [(a, 0.1), (b, 0.6)] {
                        let m = o.mapping.add_mask(id);
                        m.points = vec![[x, 0.1], [x + 0.2, 0.1], [x + 0.2, 0.3], [x, 0.3]];
                        m.invert = true;
                    }
                    o.mapping.enabled = true;
                    println!("QUICK twowindows");
                }
                continue;
            }
            if name == "cornerpin" {
                // The top layer pinned into a trapezoid (narrow at the top).
                if let Some(l) = self.state.comp.layers.last_mut() {
                    l.transform.corners = Some([[0.4, 0.1], [0.6, 0.1], [0.9, 0.9], [0.1, 0.9]]);
                    println!("QUICK cornerpin: {}", l.name);
                }
                continue;
            }
            if name == "slicefx" {
                // Output 1: the left half of the picture on the left, and the same half again on the right through
                // Invert (a slice effect), mirrored. Compare the -out1.png halves.
                self.state.comp.ensure_outputs();
                let (a, b) = (self.state.comp.new_id(), self.state.comp.new_id());
                let fx = self.state.comp.new_effect("invert");
                if let Some(o) = self.state.comp.outputs.first_mut() {
                    for (id, x) in [(a, 0.0), (b, 0.5)] {
                        let s = o.mapping.add_surface(id);
                        s.corners = [[x, 0.0], [x + 0.5, 0.0], [x + 0.5, 1.0], [x, 1.0]];
                        s.input = [0.0, 0.0, 0.5, 1.0];
                    }
                    o.mapping.surfaces[1].flip = [true, false];
                    o.mapping.surfaces[1].effects.extend(fx);
                    o.mapping.enabled = true;
                    println!("QUICK slicefx: {} effect(s) on slice {b}", o.mapping.surfaces[1].effects.len());
                }
                continue;
            }
            #[cfg(not(target_arch = "wasm32"))]
            if name == "scan" {
                // Stage scanner on output 1 with the output itself as the camera (a SCAN line at the end).
                self.state.comp.ensure_outputs();
                if let Some(o) = self.state.comp.outputs.first() {
                    let size = o.size((self.state.comp.width, self.state.comp.height));
                    self.state.scanner.settle = 0.05;
                    self.state.scanner.start(o.id, size, o.id, size.0 as f32 / size.1.max(1) as f32);
                    println!("QUICK scan: output {} at {}x{}", o.id, size.0, size.1);
                }
                continue;
            }
            #[cfg(not(target_arch = "wasm32"))]
            if name == "ledscan" {
                // LED scanner on a 10-light strip, the first layer standing in for the camera, Art-Net to this Mac
                // only (a LEDSCAN line at the end: it runs through every picture; a test picture finds no lights).
                let id = self.state.comp.new_id();
                let d = &mut self.state.comp.dmx;
                d.target = "127.0.0.1".into();
                d.fixtures.push(phosphor_engine::Fixture { id, name: "Scan test".into(), cols: 10, ..Default::default() });
                let camera = self.state.comp.layers.first().map_or(0, |l| l.id);
                self.state.led_scan.settle = 0.05;
                let aspect = self.state.comp.width as f32 / self.state.comp.height.max(1) as f32;
                match self.state.led_scan.start(&self.state.comp.dmx, Some(id), camera, aspect) {
                    Ok(()) => self.state.comp.dmx.enabled = true,
                    Err(e) => println!("QUICK ledscan failed: {e}"),
                }
                println!("QUICK ledscan: layer {camera} as the camera");
                continue;
            }
            if name == "tracking" {
                // Video tracking on the mix: faces, hands and body (TRACK line at the end).
                let t = &mut self.state.comp.tracking;
                (t.enabled, t.faces, t.hands, t.body, t.smoothing) = (true, true, true, true, 0.0);
                continue;
            }
            if name == "bezier" {
                // A selected circle mask on the first output with bezier handles on two points (one pulled out).
                self.state.comp.ensure_outputs();
                let id = self.state.comp.new_id();
                if let Some(o) = self.state.comp.outputs.first_mut() {
                    let m = o.mapping.add_mask_shape(id, phosphor_engine::mapping::MaskShape::Rectangle, 16.0 / 9.0);
                    m.set_handle(0, Some([0.0, -0.12]));
                    let h = m.auto_handle(2);
                    m.set_handle(2, Some(h));
                    println!("QUICK bezier: outline {} points", m.outline().len());
                    o.mapping.enabled = true;
                }
                self.state.selected_surface = Some(id);
                continue;
            }
            if name == "slicegroups" {
                // Four slices on the first output: two in "Wall", one of them also in "Window" inside it.
                self.state.comp.ensure_outputs();
                let ids: Vec<u64> = (0..4).map(|_| self.state.comp.new_id()).collect();
                let (outer, inner) = (self.state.comp.new_id(), self.state.comp.new_id());
                if let Some(o) = self.state.comp.outputs.first_mut() {
                    for id in &ids {
                        o.mapping.add_surface(*id);
                    }
                    o.mapping.enabled = true;
                    o.mapping.group_slices(outer, &ids[..2]);
                    o.mapping.group_slices(inner, &ids[1..2]);
                    o.mapping.groups.iter_mut().for_each(|g| g.name = if g.id == outer { "Wall".into() } else { "Window".into() });
                    println!("QUICK slicegroups: {} groups, Wall has {:?}", o.mapping.groups.len(), o.mapping.group_members(outer));
                }
                continue;
            }
            if let Some(b) = name.strip_prefix("sliceblend=") {
                use phosphor_engine::mapping::SliceBlend;
                if let (Some(blend), Some(o)) = (SliceBlend::ALL.into_iter().find(|x| x.name().eq_ignore_ascii_case(b)), self.state.comp.outputs.first_mut()) {
                    o.mapping.surfaces.iter_mut().for_each(|s| s.blend = blend);
                    println!("QUICK sliceblend: {}", blend.name());
                }
                continue;
            }
            if let Some(path) = name.strip_prefix("stagephoto=") {
                if let Some(o) = self.state.comp.outputs.first_mut() {
                    o.stage_photo = path.to_string();
                    o.stage_photo_opacity = 1.0;
                    println!("QUICK stagephoto: {path}");
                }
                continue;
            }
            if let Some(pat) = name.strip_prefix("pattern=") {
                // A test pattern on the first output (Grid, Card, Bars, White, Checker, Ramp), mapping off.
                use phosphor_engine::Choice;
                let pattern = phosphor_engine::TestPattern::ALL.into_iter().find(|p| format!("{p:?}").eq_ignore_ascii_case(pat) || p.label().eq_ignore_ascii_case(pat));
                if let (Some(p), Some(o)) = (pattern, self.state.comp.outputs.first_mut()) {
                    o.grid.on = true;
                    o.grid.pattern = p;
                    o.mapping.enabled = false;
                    println!("QUICK pattern: {}", p.label());
                }
                continue;
            }
            if name == "previews" {
                // Preview windows: the top layer, a group of the bottom layer, and output 1 after mapping.
                use crate::panels::MonitorSource;
                let ids: Vec<u64> = self.state.comp.layers.iter().map(|l| l.id).collect();
                if let Some(&top) = ids.last() {
                    self.add_monitor_with(MonitorSource::Layer(top));
                }
                if let Some(g) = ids.first().and_then(|&b| self.state.comp.create_group(&[b])) {
                    self.add_monitor_with(MonitorSource::Group(g));
                }
                if let Some(o) = self.state.comp.outputs.first().map(|o| o.id) {
                    self.add_monitor_with(MonitorSource::OneOutput(o));
                }
                println!("QUICK previews: layer, group, output");
                continue;
            }
            if name == "seq" {
                // Two step-sequencer tracks: a gate on the beat and a 16-step level line.
                use phosphor_engine::{ModSource, Modulation, Modulator, StepTrack};
                for (k, gate) in [(1, true), (2, false)] {
                    let id = self.state.comp.new_id();
                    let sequence = StepTrack { gate, steps: if gate { [1.0, 0.0, 0.0, 0.0].repeat(4) } else { (0..16).map(|i| i as f32 / 15.0).collect() }, ..Default::default() };
                    self.state.comp.modulators.push(Modulator { id, name: format!("Track {k}"), signal: Modulation { source: ModSource::Steps, ..Default::default() }, sequence: Some(sequence), ..Default::default() });
                }
                println!("QUICK seq: two step tracks");
                continue;
            }
            if name == "mods" {
                // Two shared modulators, the second gated by the first, and the first layer's generator speed following it.
                use phosphor_engine::{ChainMode, ModChain, ModSource, Modulation, Modulator};
                let (a, b) = (self.state.comp.new_id(), self.state.comp.new_id());
                self.state.comp.modulators.push(Modulator { id: a, name: "Gate".into(), signal: Modulation { source: ModSource::LfoSquare, sync: true, rate: 2.0, ..Default::default() }, chain: None, value: 0.0, looper: Default::default(), sequence: None, band: None, designer: false, data: None, event: None, live: Default::default() });
                self.state.comp.modulators.push(Modulator { id: b, name: "Swell".into(), signal: Modulation { source: ModSource::LfoSine, sync: true, rate: 1.0, ..Default::default() }, chain: Some(ModChain { from: a, mode: ChainMode::Multiply, amount: 1.0 }), value: 0.0, looper: Default::default(), sequence: None, band: None, designer: false, data: None, event: None, live: Default::default() });
                if let Some(l) = self.state.comp.layers.first_mut() {
                    l.generator_mods.retain(|m| m.index != 0);
                    l.generator_mods.push(phosphor_engine::ParamMod { index: 0, m: Modulation { link: b, range: true, min: 0.0, max: 1.0, ..Default::default() } });
                }
                println!("QUICK mods: Gate into Swell (multiply), layer 1 param 0 follows Swell");
                continue;
            }
            if name == "calibrate" {
                // Pins from a "real" projector placement, solved from the model surface's current settings.
                let aspect = self.state.comp.width as f32 / self.state.comp.height.max(1) as f32;
                for o in &mut self.state.comp.outputs {
                    for s in &mut o.mapping.surfaces {
                        let Some(m) = &mut s.model else { continue };
                        let truth = phosphor_engine::mapping::Model3d { yaw: -0.07, pitch: 0.1, roll: 0.02, distance: 4.0, fov: 28.0, center: [0.42, 0.55], ..m.clone() };
                        let mesh = phosphor_engine::mapping::obj_mesh(&m.path);
                        if let Ok(mesh) = mesh.as_ref() {
                            m.pins = mesh.verts.iter().take(8).filter_map(|v| truth.project_point(*v, aspect).map(|(xy, _)| phosphor_engine::mapping::Pin { model: *v, at: xy })).collect();
                        }
                        m.calibrating = true;
                        let err = m.solve(aspect);
                        println!("QUICK calibrate: {} pins, rms {:?}; solved yaw {:.3} pitch {:.3} roll {:.3} dist {:.2} fov {:.1} centre {:.3},{:.3} (truth -0.070 0.100 0.020 4.00 28.0 0.420,0.550)", m.pins.len(), err, m.yaw, m.pitch, m.roll, m.distance, m.fov, m.center[0], m.center[1]);
                    }
                }
                continue;
            }
            if name == "model" {
                // A 3D model surface (/tmp/house.obj) on the first output, turned so three sides show.
                let oid = self.state.comp.outputs.first().map(|o| o.id).unwrap_or_else(|| self.state.comp.add_output());
                let sid = self.state.comp.next_id;
                self.state.comp.next_id += 1;
                if let Some(o) = self.state.comp.outputs.iter_mut().find(|o| o.id == oid) {
                    o.mapping.enabled = true;
                    let s = o.mapping.add_surface(sid);
                    s.name = "House".into();
                    s.model = Some(phosphor_engine::mapping::Model3d { path: "/tmp/house.obj".into(), yaw: 0.1, pitch: 0.06, distance: 3.2, ..Default::default() });
                }
                self.state.selected_surface = Some(sid);
                println!("QUICK model: /tmp/house.obj on output {oid}");
                continue;
            }
            if name == "head" {
                // A moving head circling every 2 beats, colour from the centre of the picture, Art-Net to this Mac.
                let d = &mut self.state.comp.dmx;
                d.enabled = true;
                d.protocol = phosphor_engine::DmxProtocol::ArtNet;
                d.target = "127.0.0.1".into();
                d.fps = 30.0;
                let id = self.state.comp.next_id;
                self.state.comp.next_id += 1;
                self.state.comp.dmx.fixtures.push(phosphor_engine::Fixture {
                    id,
                    name: "Head".into(),
                    shape: phosphor_engine::FixtureShape::MovingHead,
                    points: [[0.5, 0.5], [0.5, 0.5], [0.5, 0.5], [0.5, 0.5]],
                    motion: phosphor_engine::HeadMotion::Circle,
                    motion_size: 0.5,
                    motion_beats: 2.0,
                    universe: 0,
                    channel: 1,
                    ..Default::default()
                });
                println!("QUICK head: moving head on universe 0 ch 1, Art-Net to 127.0.0.1");
                continue;
            }
            if name == "line" {
                // A line slice on the first output (added if there is none), for a look at the mapping canvas.
                let oid = self.state.comp.outputs.first().map(|o| o.id).unwrap_or_else(|| self.state.comp.add_output());
                if let Some(o) = self.state.comp.outputs.iter_mut().find(|o| o.id == oid) {
                    let sid = self.state.comp.next_id;
                    self.state.comp.next_id += 1;
                    let s = o.mapping.add_surface(sid);
                    s.name = "Line".into();
                    s.set_line(true);
                    if let Some(l) = &mut s.line {
                        l.points = vec![[0.1, 0.25], [0.45, 0.7], [0.9, 0.3]];
                        l.width = 0.08;
                    }
                }
                println!("QUICK line: line slice on output {oid}");
                continue;
            }
            if name == "welcome" {
                self.welcome_open = true;
                continue;
            }
            if name == "designer" {
                // A comp as a network: Plasma → an effect → Output, in the Designer area.
                use phosphor_engine::NodeKind;
                let comp = &mut self.state.comp;
                let cid = comp.add_comp("Designer");
                comp.start_network(cid);
                let mut net = comp.comp(cid).unwrap().network.clone().unwrap();
                let (pic, out) = (net.nodes[0].id, net.nodes[1].id);
                let fx = net.add(NodeKind::Effect, [560.0, 60.0]);
                net.nodes.iter_mut().find(|n| n.id == out).unwrap().pos = [820.0, 60.0];
                // A box of boxes passes the picture through (Input → Output inside) between the two.
                let group = net.add(NodeKind::Group, [300.0, 60.0]);
                net.connect(pic, group, 0);
                net.connect(group, fx, 0);
                net.connect(fx, out, 0);
                comp.comp_mut(cid).unwrap().network = Some(net);
                comp.sync_network(cid);
                let net = comp.comp(cid).unwrap().network.clone().unwrap();
                let pic_layer = net.node(pic).unwrap().parts[0];
                let fx_layer = net.node(fx).unwrap().parts[0];
                comp.layer_mut(pic_layer).unwrap().set_generator("plasma");
                let effect = comp.new_effect("kaleido").or_else(|| phosphor_engine::shaders::all_effects().first().and_then(|d| comp.new_effect(d.id)));
                if let (Some(e), Some(l)) = (effect, comp.layer_mut(fx_layer)) {
                    l.effects = vec![e];
                }
                // An LFO Number box into the effect's first setting.
                let mut net2 = comp.comp(cid).unwrap().network.clone().unwrap();
                let lfo = net2.add(NodeKind::Number, [300.0, 420.0]);
                comp.comp_mut(cid).unwrap().network = Some(net2);
                comp.sync_network(cid);
                let wired = comp.connect_boxes(cid, &[], lfo, fx, 1);
                comp.sync_network(cid);
                let net = comp.comp(cid).unwrap().network.clone().unwrap();
                println!("QUICK designer: {} boxes, shown={:?}, number wired={wired}", net.nodes.len(), net.shown());
                self.switch_workspace(3);
                continue;
            }
            if name == "data" {
                // Designer phase 5: a Clock → a Text box ("It's {raw}") → Output; a Timer into an effect; an Event.
                use phosphor_engine::NodeKind;
                let comp = &mut self.state.comp;
                let cid = comp.add_comp("Data");
                comp.start_network(cid);
                let mut net = comp.comp(cid).unwrap().network.clone().unwrap();
                let (pic, out) = (net.nodes[0].id, net.nodes[1].id);
                net.nodes.iter_mut().find(|n| n.id == out).unwrap().pos = [820.0, 60.0];
                net.nodes.iter_mut().find(|n| n.id == pic).unwrap().pos = [300.0, 60.0];
                let clock = net.add(NodeKind::Number, [40.0, 60.0]);
                let timer = net.add(NodeKind::Number, [40.0, 420.0]);
                let fx = net.add(NodeKind::Effect, [560.0, 60.0]);
                let event = net.add(NodeKind::Number, [300.0, 520.0]);
                net.disconnect(out, 0);
                comp.comp_mut(cid).unwrap().network = Some(net);
                comp.sync_network(cid);
                let net = comp.comp(cid).unwrap().network.clone().unwrap();
                let part = |id: u32| net.node(id).unwrap().parts[0];
                comp.layer_mut(part(pic)).unwrap().source = phosphor_engine::SourceSpec::Text(phosphor_engine::TextStyle { text: "Hello".into(), live_source: "It's {raw}".into(), ..crate::text::default_style() });
                let e = comp.new_effect("zoom_rotate").or_else(|| comp.new_effect("invert")).unwrap();
                comp.layer_mut(part(fx)).unwrap().effects = vec![e];
                for (id, data, ev) in [(clock, Some(phosphor_engine::DataKind::Clock), false), (timer, Some(phosphor_engine::DataKind::Timer { seconds: 8.0 }), false), (event, None, true)] {
                    if let Some(m) = comp.modulators.iter_mut().find(|m| m.id == part(id)) {
                        m.signal.source = phosphor_engine::ModSource::Hand;
                        m.data = data.map(|kind| phosphor_engine::DataFeed { kind, ..Default::default() });
                        if ev {
                            m.signal.source = phosphor_engine::ModSource::LfoSine;
                            m.signal.rate = 0.5;
                            m.event = Some(phosphor_engine::Trigger { threshold: 0.5, target: Some(phosphor_engine::ControlTarget::MasterFade), value: 1.0, both_ways: false, send: None });
                        }
                    }
                }
                let mut wired = comp.connect_boxes(cid, &[], pic, fx, 0);
                wired &= comp.connect_boxes(cid, &[], fx, out, 0);
                wired &= comp.connect_boxes(cid, &[], clock, pic, 0);
                wired &= comp.connect_boxes(cid, &[], timer, fx, 1);
                comp.sync_network(cid);
                let net = comp.comp(cid).unwrap().network.clone().unwrap();
                println!("QUICK data: {} boxes, shown={:?}, wired={wired}", net.nodes.len(), net.shown());
                self.switch_workspace(3);
                continue;
            }
            if name == "points" {
                // Designer phase 4: a sphere of points, turned by an LFO, drawn as dots coloured by a plasma.
                use phosphor_engine::NodeKind;
                let comp = &mut self.state.comp;
                let cid = comp.add_comp("Points");
                comp.start_network(cid);
                let mut net = comp.comp(cid).unwrap().network.clone().unwrap();
                let (pic, out) = (net.nodes[0].id, net.nodes[1].id);
                net.nodes.iter_mut().find(|n| n.id == pic).unwrap().pos = [40.0, 420.0];
                net.nodes.iter_mut().find(|n| n.id == out).unwrap().pos = [1080.0, 60.0];
                let shape = net.add(NodeKind::Picture, [40.0, 60.0]);
                let mv = net.add(NodeKind::Effect, [300.0, 60.0]);
                let render = net.add(NodeKind::Effect, [560.0, 60.0]);
                let lfo = net.add(NodeKind::Number, [820.0, 420.0]);
                net.disconnect(out, 0);
                comp.comp_mut(cid).unwrap().network = Some(net);
                comp.sync_network(cid);
                let net = comp.comp(cid).unwrap().network.clone().unwrap();
                let part = |id: u32| net.node(id).unwrap().parts[0];
                comp.layer_mut(part(pic)).unwrap().set_generator("plasma");
                // PHOSPHOR_POINTS_MODEL=file.obj: the model's vertices instead of the sphere, read in order.
                let model = std::env::var("PHOSPHOR_POINTS_MODEL").ok();
                match &model {
                    Some(path) => comp.layer_mut(part(shape)).unwrap().source = crate::models::source(path),
                    None => comp.layer_mut(part(shape)).unwrap().set_generator("pts_shape"),
                }
                let e = comp.new_effect("pts_move").unwrap();
                comp.layer_mut(part(mv)).unwrap().effects = vec![e];
                let render_id = match std::env::var("PHOSPHOR_POINTS_MODE").as_deref() {
                    Ok("lines") => "pts_lines",
                    Ok("mesh") => "pts_mesh",
                    _ => "pts_dots",
                };
                let mut e = comp.new_effect(render_id).unwrap();
                if model.is_some() {
                    e.values[1] = 6.0;
                }
                if render_id == "pts_mesh" {
                    e.values[10] = 0.8;
                }
                comp.layer_mut(part(render)).unwrap().effects = vec![e];
                let mut wired = comp.connect_boxes(cid, &[], shape, mv, 0);
                wired &= comp.connect_boxes(cid, &[], mv, render, 0);
                wired &= comp.connect_boxes(cid, &[], pic, render, 1);
                wired &= comp.connect_boxes(cid, &[], render, out, 0);
                // The LFO turns the shape (Move's Turn Y: input 1 + 4).
                wired &= comp.connect_boxes(cid, &[], lfo, mv, 5);
                // PHOSPHOR_POINTS_LIGHT=1: a warm Light box from the upper right wired into the render's Light input.
                if std::env::var("PHOSPHOR_POINTS_LIGHT").is_ok() {
                    let mut net = comp.comp(cid).unwrap().network.clone().unwrap();
                    let light = net.add(NodeKind::Effect, [1080.0, 220.0]);
                    comp.comp_mut(cid).unwrap().network = Some(net);
                    comp.sync_network(cid);
                    let lp = comp.comp(cid).unwrap().network.as_ref().unwrap().node(light).unwrap().parts[0];
                    let mut e = comp.new_effect(phosphor_engine::shaders::POINTS_LIGHT).unwrap();
                    e.values[0] = 0.7;
                    e.values[3] = 0.6;
                    comp.layer_mut(lp).unwrap().effects = vec![e];
                    let at = comp.node_inputs(comp.comp(cid).unwrap().network.as_ref().unwrap().node(render).unwrap()).len() - 1;
                    wired &= comp.connect_boxes(cid, &[], light, render, at);
                }
                comp.sync_network(cid);
                if let Some(m) = comp.modulators.iter_mut().find(|m| m.designer) {
                    m.signal.source = phosphor_engine::ModSource::LfoSaw;
                    m.signal.rate = 0.1;
                }
                let net = comp.comp(cid).unwrap().network.clone().unwrap();
                println!("QUICK points: {} boxes, shown={:?}, wired={wired}", net.nodes.len(), net.shown());
                self.switch_workspace(3);
                continue;
            }
            if let Some(cat) = name.strip_prefix("explorer:") {
                // The Effects panel on one category (checking its preview tiles).
                self.state.explorer.category = cat.to_string();
                println!("QUICK explorer: {cat}");
                continue;
            }
            if name == "import_new" || name == "import_add" {
                // After --import: bring it in as the buttons do (.mad: a new project, or added to the open one;
                // .avc / .vdmx5: every deck / page with media, as a new project).
                #[cfg(not(target_arch = "wasm32"))]
                if self.state.mad_import.is_some() {
                    self.state.finish_mad_import(name == "import_new");
                } else {
                    self.state.finish_avc_import();
                }
                let outs: Vec<String> = self.state.comp.outputs.iter().map(|o| format!("{}:{}", o.name, o.mapping.surfaces.len())).collect();
                let layers: Vec<String> = self.state.comp.layers.iter().map(|l| format!("{}[{}]", l.name, l.effects.iter().map(|e| e.effect.as_str()).collect::<Vec<_>>().join(","))).collect();
                let master: Vec<&str> = self.state.comp.master_effects.iter().map(|e| e.effect.as_str()).collect();
                println!("QUICK {name}: layers={layers:?} master={master:?} outputs={outs:?} notice={:?}", self.state.notice);
                continue;
            }
            if name == "panels" {
                // A panel of your own (with a fader on it) and the panel browser.
                let page = self.state.comp.surface.add_page();
                self.state.comp.surface.add(phosphor_engine::WidgetKind::Fader);
                self.state.comp.surface.page = 0;
                self.open_my_panel(page);
                self.state.panel_browser = true;
                println!("QUICK panels: panel {} open, browser open", page + 1);
                continue;
            }
            if name == "baroff" {
                // Knock the bar two beats off, so the downbeat tracker has something to fix.
                let b = self.state.clock.beats(now);
                self.state.clock.set_beats(now, b + 2.0);
                println!("QUICK baroff: bar moved two beats");
                continue;
            }
            if name == "autoshow" {
                self.state.comp.auto_show.enabled = true;
                self.state.comp.auto_show.bars = 1;
                println!("QUICK autoshow: on, every bar");
                continue;
            }
            #[cfg(target_os = "macos")]
            if name == "vcam" {
                // Virtual camera on: installs the extension (macOS answers later: the VCAM line at the end).
                self.state.vcam.on = true;
                crate::virtual_camera::activate();
                println!("QUICK vcam: asked macOS to install the camera");
                continue;
            }
            #[cfg(target_os = "macos")]
            if name == "menutest" {
                crate::menu_test::schedule(1.0);
                println!("QUICK menutest: a menu opens in 1 s for 3 s");
                continue;
            }
            if name == "clipsound" {
                self.state.audio.use_clips();
                println!("QUICK clipsound: analysing the clip mix");
                continue;
            }
            if name == "transfx" {
                // A 4 s "Through an effect" (pixelate) fade from the first layer's source to a tunnel.
                let t = now as f32;
                if let Some(l) = self.state.comp.layers.first_mut() {
                    l.transition_kind = phosphor_engine::TransitionKind::Effect;
                    l.transition_effect = "pixelate".into();
                    l.start_transition(t, 4.0);
                    l.set_generator("tunnel");
                }
                println!("QUICK transfx: pixelate fade started");
                continue;
            }
            if let Some(file) = name.strip_prefix("trans=") {
                // An 8 s shader transition (ISF file name, e.g. trans=Doorway) from the first layer's source to a
                // tunnel; crossfades until the ISF library has loaded and compiled it.
                let t = now as f32;
                let id = format!("{}{}", phosphor_engine::shaders::ISF_TRANSITION, phosphor_engine::custom::slug(file));
                if let Some(l) = self.state.comp.layers.first_mut() {
                    l.transition_kind = phosphor_engine::TransitionKind::Shader;
                    l.transition_effect = id.clone();
                    l.start_transition(t, 8.0);
                    l.set_generator("tunnel");
                }
                println!("QUICK trans: {id}");
                continue;
            }
            if name == "play" {
                // Audience play on with short turns and a rest, for checking the play page in a browser.
                self.state.comp.play.enabled = true;
                self.state.comp.play.turn_secs = 10;
                self.state.comp.play.rest_secs = 15;
                println!("QUICK play: on, 10 s turns, 15 s rest");
                continue;
            }
            if let Some(n) = name.strip_prefix("template").and_then(|n| n.parse::<usize>().ok()) {
                crate::explorer::load_user_shaders(&mut self.state);
                println!("QUICK template{n}: {:?}", crate::explorer::bench_template(&mut self.state, n));
                continue;
            }
            #[cfg(target_os = "macos")]
            if name == "ffgl" {
                println!("QUICK ffgl: {:?}", crate::ffgl::bench_apply(&mut self.state));
                continue;
            }
            if name == "timeline" {
                // Play the project's timeline from the start.
                self.state.timeline_seek(0.0, now);
                self.state.timeline_toggle(now);
                println!("QUICK timeline: {} events", self.state.comp.timeline.events.len());
                continue;
            }
            #[cfg(not(target_arch = "wasm32"))]
            if name == "starter" {
                // A starter show (PHOSPHOR_STARTER, default Club): STARTER line at the end with each layer's clips.
                let mood = std::env::var("PHOSPHOR_STARTER").unwrap_or_else(|_| "Club".into());
                println!("QUICK starter {mood}: {}", crate::panels::bench_starter(&mut self.state, &mood, ctx));
                continue;
            }
            #[cfg(not(target_arch = "wasm32"))]
            if name == "cityclock" {
                // The top text layer becomes a clock in PHOSPHOR_CITY (default Tokyo), looked up like the Inspector does.
                let city = std::env::var("PHOSPHOR_CITY").unwrap_or_else(|_| "Tokyo".into());
                if let Some(l) = self.state.comp.layers.iter().rev().find(|l| matches!(l.source, phosphor_engine::SourceSpec::Text(_))) {
                    self.state.feeds.find_zone(l.id, &city);
                }
                println!("QUICK cityclock: {city}");
                continue;
            }
            #[cfg(not(target_arch = "wasm32"))]
            if name == "lights" {
                // A real light…: the window open on PHOSPHOR_LIGHT_QUERY (default "intimidator spot"), its first match
                // looked at (with --ui-shot: a picture of the window).
                let q = std::env::var("PHOSPHOR_LIGHT_QUERY").unwrap_or_else(|_| "intimidator spot".into());
                crate::panels::open_light_window(ctx);
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("light_query"), q.clone()));
                self.state.ofl.bench_pick = Some(q);
                println!("QUICK lights");
                continue;
            }
            #[cfg(not(target_arch = "wasm32"))]
            if name == "lasers" {
                // The Laser Designer with the starter look on a projector that has no box (nothing is sent); a LASER line.
                let comp = &mut self.state.comp;
                let mut look = phosphor_engine::LaserLook::starter(|| comp.new_id());
                // Words under it, and PHOSPHOR_LASER_FILE (an .ild) on top when given.
                let words = phosphor_engine::LaserShape { id: comp.new_id(), kind: phosphor_engine::LaserShapeKind::Text, text: "PHOSPHOR".into(), y: -0.75, size: 0.45, color: [1.0, 0.2, 0.8], ..Default::default() };
                look.shapes.push(words);
                if let Ok(file) = std::env::var("PHOSPHOR_LASER_FILE") {
                    look.shapes.push(phosphor_engine::LaserShape { id: comp.new_id(), kind: phosphor_engine::LaserShapeKind::File, file, size: 0.3, x: 0.6, y: 0.6, count: 30.0, ..Default::default() });
                }
                // PHOSPHOR_LASER_TRACE=1: the mix traced too (its picture arrives once the app renders).
                if std::env::var_os("PHOSPHOR_LASER_TRACE").is_some() {
                    look.shapes.push(phosphor_engine::LaserShape { id: comp.new_id(), kind: phosphor_engine::LaserShapeKind::Trace, size: 1.0, count: 12.0, shape: 0.5, ..Default::default() });
                }
                // PHOSPHOR_LASER_ONION=picture: it as the look's onion skin, traced, and a drawing (picked) over it.
                if let Ok(onion) = std::env::var("PHOSPHOR_LASER_ONION") {
                    look.shapes.retain(|s| s.kind == phosphor_engine::LaserShapeKind::Text);
                    look.shapes.push(phosphor_engine::LaserShape { id: comp.new_id(), kind: phosphor_engine::LaserShapeKind::Drawing, size: 1.0, color: [1.0, 0.9, 0.0], points: vec![[-0.8, 0.6], [0.0, 0.85], [0.8, 0.6]], ..Default::default() });
                    look.shapes.push(phosphor_engine::LaserShape { id: comp.new_id(), kind: phosphor_engine::LaserShapeKind::Trace, image: onion.clone(), size: 1.0, count: 12.0, shape: 0.5, ..Default::default() });
                    look.shapes.rotate_right(2);
                    look.onion = onion;
                }
                // PHOSPHOR_LASER_TEST=1: the projector shows the test pattern.
                let test_pattern = std::env::var_os("PHOSPHOR_LASER_TEST").is_some();
                let id = comp.new_id();
                comp.lasers.projectors.push(phosphor_engine::LaserProjector { id, look: look.id, test_pattern, ..Default::default() });
                comp.lasers.looks.push(look);
                comp.lasers.projectors[0].floor = -0.6;
                // PHOSPHOR_LASER_TIMELINE=1: a second look, and the timeline switching between them (0 s, 20 s, 40 s).
                if std::env::var_os("PHOSPHOR_LASER_TIMELINE").is_some() {
                    let mut second = phosphor_engine::LaserLook::starter(|| comp.new_id());
                    second.name = "Finale".into();
                    let (pid, first, other) = (comp.lasers.projectors[0].id, comp.lasers.looks[0].id, second.id);
                    comp.lasers.looks.push(second);
                    for (t, look) in [(0.0, first), (20.0, other), (40.0, first)] {
                        comp.timeline.set_look(pid, t, look);
                    }
                    // Two cues: one per look.
                    for (name, look) in [("Verse", first), ("Finale", other)] {
                        comp.lasers.projectors[0].look = look;
                        let cue = comp.record_cue(name.into());
                        comp.cues.cues.push(cue);
                    }
                    comp.lasers.projectors[0].look = first;
                    // Its clip row: the two looks in columns 1 and 3; and a Control Surface button per look.
                    comp.lasers.projectors[0].clips = vec![first, 0, other];
                    crate::panels::add_laser_buttons(comp);
                }
                // PHOSPHOR_LASER_ZONES=1: two zones (the right one keystoned, showing the test pattern's look) and a
                // dark area.
                if std::env::var_os("PHOSPHOR_LASER_ZONES").is_some() {
                    let (a, b) = (comp.new_id(), comp.new_id());
                    let p = &mut comp.lasers.projectors[0];
                    p.zones = vec![
                        phosphor_engine::LaserZone { id: a, name: "Left".into(), corners: [[-1.0, -0.6], [-0.05, -0.6], [-0.05, 0.9], [-1.0, 0.9]], ..Default::default() },
                        phosphor_engine::LaserZone { id: b, name: "Wall".into(), brightness: 0.6, corners: [[0.05, -0.6], [1.0, -0.5], [0.85, 0.8], [0.15, 0.95]], ..Default::default() },
                    ];
                    p.dark = vec![phosphor_engine::LaserArea { left: 0.3, bottom: 0.0, right: 0.6, top: 0.3 }];
                }
                let p = &comp.lasers.projectors[0];
                let frame = crate::lasers::frame_for(&comp.lasers, p, 1.0, &self.state.mod_inputs, 1.0, &Default::default());
                println!("LASER points={} lit={} budget={}", frame.len(), frame.iter().filter(|q| q.lit()).count(), p.pps / crate::lasers::FPS);
                continue;
            }
            #[cfg(not(target_arch = "wasm32"))]
            if name == "livedata" {
                // Live data: weather in Reykjavik, the ISS and earthquakes on (a LIVEDATA line at the end).
                let d = &mut self.state.comp.live_data;
                (d.weather, d.place, d.latitude, d.longitude, d.iss, d.quakes) = (true, "Reykjavik".into(), 64.135, -21.895, true, true);
                println!("QUICK livedata");
                continue;
            }
            #[cfg(not(target_arch = "wasm32"))]
            if name == "online" {
                // Library › Online: PHOSPHOR_ONLINE_QUERY searched in every library, the first picture downloaded
                // onto a new layer (PHOSPHOR_ONLINE_PICK=video: an Internet Archive film; an ONLINE line at the end).
                let q = std::env::var("PHOSPHOR_ONLINE_QUERY").unwrap_or_else(|_| "aurora".into());
                self.state.online.bench_search(&q);
                println!("QUICK online: {q}");
                continue;
            }
            #[cfg(not(target_arch = "wasm32"))]
            if name == "link" {
                // From a link: PHOSPHOR_LINK_URL downloaded onto the top layer (a LINK line at the end).
                let url = std::env::var("PHOSPHOR_LINK_URL").unwrap_or_default();
                if self.state.comp.layers.is_empty() {
                    self.state.comp.add_layer();
                }
                self.state.link_import.bench_download(&url);
                println!("QUICK link: {url}");
                continue;
            }
            #[cfg(not(target_arch = "wasm32"))]
            if name == "ai" {
                // Describe a shader: PHOSPHOR_AI_PROMPT asked of PHOSPHOR_AI_URL (a local test server) or the AI.
                let prompt = std::env::var("PHOSPHOR_AI_PROMPT").unwrap_or_else(|_| "slow purple plasma".into());
                crate::explorer::load_user_shaders(&mut self.state);
                self.state.ai_shader.bench_ask(&prompt);
                println!("QUICK ai: {prompt}");
                continue;
            }
            if name == "patch" {
                crate::explorer::load_user_shaders(&mut self.state);
                println!("QUICK patch: {:?}", crate::explorer::bench_patch(&mut self.state));
                continue;
            }
            if name == "shadertoy" {
                crate::explorer::open_shadertoy(&mut self.state);
                println!("QUICK shadertoy: import window open");
                continue;
            }
            if name == "grid" {
                // The alignment grid on every output (seen in the Outputs area's strip).
                // Two outputs, so the Stage view has something to tile.
                while self.state.comp.outputs.len() < 2 {
                    self.state.comp.add_output();
                }
                for o in &mut self.state.comp.outputs {
                    o.grid.on = true;
                }
                println!("QUICK grid: on for {} output(s)", self.state.comp.outputs.len());
                continue;
            }
            if let Some(folder) = name.strip_prefix("collection=") {
                // A collection watching `folder`, shown in the Media panel, playing on a new layer (in order).
                let c = self.state.comp.create_collection("Show A");
                self.state.comp.add_collection_folder(&c, folder);
                #[cfg(not(target_arch = "wasm32"))]
                self.state.folders.add(folder.to_string());
                self.state.media_view.show_collection(c.clone());
                let id = self.state.add_layer();
                if let Some(l) = self.state.comp.layer_mut(id) {
                    l.collection = Some(c.clone());
                    l.autopilot = phosphor_engine::Autopilot::Next;
                    l.autopilot_beats = 4.0;
                    l.name = c.clone();
                }
                self.state.selected = Some(id);
                println!("QUICK collection: {c} watching {folder}, playing on layer {id}");
                continue;
            }
            if name == "cues" {
                // Three cues; cue 1 follows on after 1 s; random never repeats the cue that's on; the countdown.
                for k in 1..=3 {
                    let cue = self.state.comp.record_cue(format!("Cue {k}"));
                    self.state.comp.cues.cues.push(cue);
                }
                self.state.comp.cues.cues[0].fade = 0.0;
                self.state.comp.cues.cues[0].follow = 1.0;
                self.state.go_cue(0, now);
                self.state.run_cues(now);
                let countdown = self.state.next_cue_in(now + 0.25);
                let mut picks = Vec::new();
                for _ in 0..12 {
                    let before = self.state.cue_run.current;
                    self.state.cue_random(now);
                    picks.push((before, self.state.cue_run.current));
                }
                let repeats = picks.iter().filter(|(a, b)| a == b).count();
                println!("CUES countdown={countdown:?} random_repeats={repeats} picks={:?}", picks.iter().map(|p| p.1.unwrap_or(99)).collect::<Vec<_>>());
                continue;
            }
            if name == "projects" {
                // The Projects window open, for pictures.
                self.state.projects.open = true;
                println!("QUICK projects open");
                continue;
            }
            if name == "mapmode" {
                // Map mode on (every mappable control shows its learn layer), for pictures.
                self.state.map_mode = true;
                println!("QUICK mapmode on");
                continue;
            }
            if name == "takeover" {
                // Web control off: phones may only watch.
                self.state.set_web_control(false);
                println!("QUICK takeover: web control off");
                continue;
            }
            #[cfg(not(target_arch = "wasm32"))]
            if name == "qr" {
                // Audience play: put the QR code layer on the output.
                self.state.toggle_play_qr(now);
                println!("QUICK qr: {}", self.state.comp.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>().join(", "));
                continue;
            }
            let q = match name {
                "surprise" => Q::Surprise,
                "layer" => Q::SurpriseLayer,
                "shuffle" => Q::Shuffle,
                "react" => Q::React,
                "fill" => Q::Fill,
                "instant" => Q::InstantSet,
                "still" => Q::Still,
                other => {
                    eprintln!("--quick: unknown action {other}");
                    continue;
                }
            };
            self.state.quick(q, now);
            println!("QUICK {name}: {}", self.state.comp.layers.iter().map(|l| format!("{} [{}]", l.name, l.effects.iter().map(|f| f.effect.as_str()).collect::<Vec<_>>().join("+"))).collect::<Vec<_>>().join(", "));
        }
    }
}
