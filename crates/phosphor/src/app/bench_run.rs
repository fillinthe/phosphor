//! Bench mode: stats summary, snapshot, export test.

use super::*;

impl PhosphorApp {
    pub(crate) fn finish_bench(&mut self, ctx: &egui::Context, now: f64) {
        let Some(run) = &mut self.bench else { return };
        let started = *run.started.get_or_insert(now);
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
        };
        println!("SUMMARY {}", summary.to_json());
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
        println!("BEAT bpm {:.1} confidence {:.2} clock {:.1}", an.bpm, an.confidence, self.state.clock.bpm);
        println!("MEDIA {} files in bin, {} thumbnails pending", self.state.comp.media_bin.len(), self.state.comp.media_bin.iter().filter(|m| self.state.thumbs.is_pending(&m.path)).count());
        if self.state.stream.running() {
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
}
