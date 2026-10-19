//! Per-frame rendering: media, engine, outputs (Syphon, NDI, stream, network), effect fades.

use super::*;

impl State {
    /// Is an engine frame due now, at the chosen render rate?
    pub(crate) fn render_due(&mut self, now: f64) -> bool {
        match self.render_rate.fps() {
            None => true,
            Some(fps) => self.render_pacer.due(now, fps),
        }
    }
}

impl PhosphorApp {
    /// Advances media, renders the frame, and keeps the preview + thumbnail textures registered.
    pub(crate) fn render(&mut self, now: f64) {
        #[cfg(not(target_arch = "wasm32"))]
        if self.bench.is_none() {
            self.heartbeat.beat(now);
        }
        let State {
            comp,
            engine,
            render_state,
            texture,
            texture_generation,
            layer_textures,
            media,
            stats,
            output,
            audio,
            mod_inputs,
            mod_phases,
            stream,
            clock,
            net,
            net_audio,
            preview_wants,
            preview_keys,
            preview_textures,
            preview_refreshed,
            selected,
            notice,
            monitor_wants,
            monitor_keys,
            monitor_textures,
            output_textures,
            #[cfg(target_os = "macos")]
            syphon,
            #[cfg(any(target_os = "macos", windows))]
            ndi,
            #[cfg(not(target_arch = "wasm32"))]
            dmx,
            ..
        } = &mut self.state;
        let Some(engine) = engine.as_mut() else { return };
        media.sync(comp, engine);
        media.update_video(comp, engine, now);

        net_audio.clear();
        net.take_audio(net_audio);
        if !net_audio.is_empty() {
            audio.push_external(net_audio);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            use crate::media::video::sound;
            // Nothing chosen to listen to, but a clip with sound is playing: listen to the clips, so the beat
            // and React work without anyone touching the Audio panel.
            if audio.current().is_none() && !audio.is_open() && !audio.is_clips() && media.any_sound() {
                audio.use_clips();
                *notice = Some("Listening to the clips' sound for the beat (Audio panel to change)".into());
            }
            sound::set_tap(audio.is_clips());
            if audio.is_clips() {
                let mix = sound::take_tap();
                audio.push_clips(&mix, sound::stats().0);
            }
        }
        let an = audio.update();
        if phosphor_engine::shaders::any_audio_slots() && audio.is_open_or_clips() {
            engine.set_audio(&audio.waveform(), audio.bins());
        }
        if an.beat {
            clock.on_audio_beat(now, an.bpm, an.confidence);
            if an.confidence >= 0.5 {
                clock.on_audio_downbeat(now, an.low);
            }
        }
        let beat_phase = clock.phase(now);
        *mod_inputs = ModInputs {
            beats: clock.beats(now) as f32,
            audio_beat: an.beat_env,
            beat_phase,
            time: now as f32,
            audio_level: an.level,
            audio_low: an.low,
            audio_mid: an.mid,
            audio_high: an.high,
            shared: Default::default(),
        };
        // Shared modulators (and their chains), once per frame, before anything reads them.
        mod_inputs.shared = phosphor_engine::evaluate(&comp.modulators, mod_inputs, mod_phases);
        let wants = std::mem::take(preview_wants);
        let source = selected.filter(|id| *id == phosphor_engine::MASTER_ID || comp.layers.iter().any(|l| l.id == *id)).unwrap_or(phosphor_engine::MASTER_ID);
        let keys: Vec<String> = wants.iter().map(|(k, _, _)| k.clone()).collect();
        if !wants.is_empty() {
            // Each tile renders once (a still); only hovered/live tiles keep updating.
            let _ = preview_refreshed;
            let stale = false;
            let items = wants
                .into_iter()
                .enumerate()
                .map(|(i, (key, item, live))| {
                    let moved = preview_keys.get(i) != Some(&key);
                    (item, live || stale || moved)
                })
                .collect();
            engine.request_previews(source, items);
        }
        let mwants = std::mem::take(monitor_wants);
        let mkeys: Vec<u8> = mwants.iter().map(|m| m.0).collect();
        engine.request_monitors(mwants.iter().map(|m| (m.1, m.2, m.3)).collect());
        // LED / DMX: sample the fixtures' pixels from this frame when the lights want one.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let points = if comp.dmx.enabled { comp.dmx.sample_points() } else { Vec::new() };
            engine.set_sample_points(&points);
            if dmx.frame_due(now, &comp.dmx) {
                engine.request_samples();
            }
        }
        let t0 = web_time::Instant::now();
        let info = engine.render(comp, now as f32, mod_inputs);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(colors) = engine.take_samples()
            && comp.dmx.enabled
        {
            let universes = comp.dmx.universes(&colors, mod_inputs.beats as f64);
            dmx.send(&comp.dmx, &universes);
        }
        let render_secs = t0.elapsed().as_secs_f64();
        // Layer thumbnails for phones on the remote page: one small readback at a time, only while one is connected.
        if let Some(i) = net.thumb_due(now, comp.layers.len())
            && let Some(l) = comp.layers.get(i)
        {
            engine.capture_thumbnail(l.id);
        }
        for f in engine.poll_thumbnails() {
            net.push_thumb(f.tag, f.width, f.height, f.rgba);
        }
        #[cfg(target_os = "macos")]
        {
            let mut frames: Vec<(u64, String, &wgpu::Texture)> = Vec::new();
            if comp.syphon
                && let Some(out) = engine.output()
            {
                frames.push((crate::syphon::PROGRAM, String::new(), &out.texture));
            }
            for o in comp.outputs.iter().filter(|o| o.syphon && o.enabled) {
                if let Some(t) = engine.output_texture(o.id) {
                    frames.push((o.id, o.name.clone(), &t.texture));
                }
            }
            syphon.publish(engine.device(), &frames);
        }

        let stream_due = stream.frame_due(now);
        let net_due = net.frame_due(now);
        #[cfg(any(target_os = "macos", windows))]
        let ndi_due = {
            ndi.sync(comp.ndi.enabled, &comp.ndi.name);
            ndi.frame_due(now, comp.ndi.fps)
        };
        #[cfg(not(any(target_os = "macos", windows)))]
        let ndi_due = false;
        if (stream_due || net_due || ndi_due) && !engine.capture_frame() && stream_due {
            stream.repeat_last();
        }
        for f in engine.poll_captures() {
            #[cfg(any(target_os = "macos", windows))]
            if ndi.connections() > 0 {
                ndi.push(f.width, f.height, f.rgba.clone(), comp.ndi.fps);
            }
            if net.running() {
                net.push_frame(f.width, f.height, f.rgba.clone());
            }
            if stream.running() {
                stream.push(f.width, f.height, f.rgba);
            }
        }

        if let Some(rs) = render_state {
            let mut renderer = rs.renderer.write();
            let linear = wgpu::FilterMode::Linear;
            if let Some(out) = engine.output()
                && (texture.is_none() || *texture_generation != engine.output_generation())
            {
                match *texture {
                    Some(id) => renderer.update_egui_texture_from_wgpu_texture(&rs.device, &out.view, linear, id),
                    None => *texture = Some(renderer.register_native_texture(&rs.device, &out.view, linear)),
                }
                *texture_generation = engine.output_generation();
            }
            // Layers and groups both have thumbnails.
            for lid in comp.layers.iter().map(|l| l.id).chain(comp.groups.iter().map(|g| g.id)) {
                let Some(t) = engine.layer_thumbnail(lid) else { continue };
                let key = (t.width, t.height);
                match layer_textures.get_mut(&lid) {
                    Some((_, k)) if *k == key => {}
                    Some((id, k)) => {
                        renderer.update_egui_texture_from_wgpu_texture(&rs.device, &t.view, linear, *id);
                        *k = key;
                    }
                    None => {
                        let id = renderer.register_native_texture(&rs.device, &t.view, linear);
                        layer_textures.insert(lid, (id, key));
                    }
                }
            }
            for o in &comp.outputs {
                let Some(t) = engine.output_texture(o.id) else { continue };
                let key = (t.width, t.height);
                match output_textures.get_mut(&o.id) {
                    Some((_, k)) if *k == key => {}
                    Some((id, k)) => {
                        renderer.update_egui_texture_from_wgpu_texture(&rs.device, &t.view, linear, *id);
                        *k = key;
                    }
                    None => {
                        let id = renderer.register_native_texture(&rs.device, &t.view, linear);
                        output_textures.insert(o.id, (id, key));
                    }
                }
            }
            output_textures.retain(|id, (tex, _)| {
                let live = comp.outputs.iter().any(|o| o.id == *id && o.enabled);
                if !live {
                    renderer.free_texture(tex);
                }
                live
            });
            for i in 0..mkeys.len() {
                let Some(t) = engine.monitor(i) else { continue };
                let key = (t.width, t.height);
                match monitor_textures.get_mut(i) {
                    Some((_, k)) if *k == key => {}
                    Some((id, k)) => {
                        renderer.update_egui_texture_from_wgpu_texture(&rs.device, &t.view, linear, *id);
                        *k = key;
                    }
                    None => {
                        let id = renderer.register_native_texture(&rs.device, &t.view, linear);
                        monitor_textures.push((id, key));
                    }
                }
            }
            // Explorer / editor previews, one texture slot per index.
            for i in 0..keys.len() {
                let Some(t) = engine.preview(i) else { continue };
                let key = (t.width, t.height);
                match preview_textures.get_mut(i) {
                    Some((_, k)) if *k == key => {}
                    Some((id, k)) => {
                        renderer.update_egui_texture_from_wgpu_texture(&rs.device, &t.view, linear, *id);
                        *k = key;
                    }
                    None => {
                        let id = renderer.register_native_texture(&rs.device, &t.view, linear);
                        preview_textures.push((id, key));
                    }
                }
            }
            layer_textures.retain(|id, (tex, _)| {
                let live = comp.layers.iter().any(|l| l.id == *id) || comp.group(*id).is_some();
                if !live {
                    renderer.free_texture(tex);
                }
                live
            });
        }
        *preview_keys = keys;
        *monitor_keys = mkeys;
        {
            let _ = texture;
            let mut map = output.lock().unwrap();
            for (i, o) in comp.outputs.iter().enumerate() {
                let e = map.entry(o.id).or_default();
                e.texture = output_textures.get(&o.id).map(|(t, _)| *t);
                let (ow, oh) = o.size((comp.width, comp.height));
                e.aspect = ow as f32 / oh.max(1) as f32;
                e.name = o.name.clone();
                e.number = i + 1;
                if !o.enabled {
                    e.open = false;
                }
            }
            map.retain(|id, _| comp.outputs.iter().any(|o| o.id == *id));
        }

        if let Some(snapshot) = stats.record(now, render_secs, info.passes, mod_inputs.audio_level as f64, engine.gpu_ms().unwrap_or(0.0) as f64)
            && let Some(run) = &mut self.bench
        {
            println!("{}", snapshot.to_json());
            run.samples.push(snapshot);
        }
    }

    /// Ramps newly added effects' mix up over the launch fade time.
    pub(crate) fn fade_in_effects(&mut self) {
        let st = &mut self.state;
        if st.fx_fades.is_empty() {
            return;
        }
        let now = st.mod_inputs.time;
        st.fx_fades.retain(|(id, start, secs, target)| {
            let Some(fx) = crate::control::find_effect_mut(&mut st.comp, *id) else { return false };
            let t = ((now - start) / secs).clamp(0.0, 1.0);
            // Smoothstep, so it eases in and out.
            fx.mix = target * t * t * (3.0 - 2.0 * t);
            t < 1.0
        });
    }
}
