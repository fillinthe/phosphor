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
        if let Some(id) = run.cfg.select.take_if(|_| now - started >= 0.5) {
            self.state.selected = Some(id);
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
        };
        // CPU per frame (ms of one core) is steadier than the percentage when the frame rate varies.
        println!("SUMMARY {} cpu_ms_per_frame={:.2}", summary.to_json(), if summary.fps > 0.0 { summary.cpu_percent * 10.0 / summary.fps } else { 0.0 });
        println!("AUDIO {}", self.state.audio.debug_line());
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
        println!("MEDIA {} files in bin, {} thumbnails pending", self.state.comp.media_bin.len(), self.state.comp.media_bin.iter().filter(|m| self.state.thumbs.is_pending(&m.path)).count());
        if self.state.stream.running() {
            let h = self.state.stream.history();
            let kbps: Vec<String> = h.iter().map(|s| format!("{:.0}", s.kbps)).collect();
            println!("STREAM_HEALTH {:?} samples={} kbps=[{}] last_speed={:.2}", self.state.stream.health(), h.len(), kbps.join(","), h.back().map_or(0.0, |s| s.speed));
            self.state.stream.stop_and_wait();
            println!("RECORDED {}", self.state.stream.settings.file_path);
        }
        if let (Some(path), Some(engine)) = (&run.cfg.snapshot, &self.state.engine)
            && let Some((w, h, pixels)) = engine.read_output()
        {
            match image::save_buffer(path, &pixels, w, h, image::ColorType::Rgba8) {
                Ok(()) => println!("SNAPSHOT {path}"),
                Err(e) => eprintln!("snapshot failed: {e}"),
            }
            // Outputs that differ from the program (mapping, masks, colour), as <name>-out<N>.png.
            for (i, o) in self.state.comp.outputs.iter().enumerate().filter(|(_, o)| o.mapping.active() || !o.mapping.masks.is_empty() || !o.color.is_neutral()) {
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
            if name == "mods" {
                // Two shared modulators, the second gated by the first, and the first layer's generator speed following it.
                use phosphor_engine::{ChainMode, ModChain, ModSource, Modulation, Modulator};
                let (a, b) = (self.state.comp.new_id(), self.state.comp.new_id());
                self.state.comp.modulators.push(Modulator { id: a, name: "Gate".into(), signal: Modulation { source: ModSource::LfoSquare, sync: true, rate: 2.0, ..Default::default() }, chain: None, value: 0.0 });
                self.state.comp.modulators.push(Modulator { id: b, name: "Swell".into(), signal: Modulation { source: ModSource::LfoSine, sync: true, rate: 1.0, ..Default::default() }, chain: Some(ModChain { from: a, mode: ChainMode::Multiply, amount: 1.0 }), value: 0.0 });
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
