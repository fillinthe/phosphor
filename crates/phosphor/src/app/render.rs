//! Per-frame rendering: media, engine, outputs (Syphon, NDI, stream, network), effect fades.

use super::*;
use super::render_core::RenderCore;

/// Who a captured program frame is for (bits of its tag): only they get a copy when it comes back.
const TO_STREAM: u64 = 1;
const TO_NET: u64 = 2;
const TO_NDI: u64 = 4;
const TO_SPOUT: u64 = 8;
const TO_VCAM: u64 = 16;

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
    ///
    /// With the render thread on (`crate::render_thread`), that thread plays the movies, draws the frame and shows the
    /// output windows; this publishes what it draws from (the project now, the beat, the sound heard) and, once per
    /// frame the thread drew, does the interface's share: previews, thumbnails, readbacks, recordings, NDI, DMX.
    pub(crate) fn render(&mut self, now: f64) {
        #[cfg(target_os = "macos")]
        crate::menu_test::RENDERS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // Recording what comes in (another program, a desk): what DMX in hears now.
        #[cfg(not(target_arch = "wasm32"))]
        if self.state.dmx_rec.records_input()
            && let Some(f) = self.state.control.dmx_in_frame()
        {
            self.state.dmx_rec.heard(now, &f);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if !self.bench_session {
            self.heartbeat.beat(now);
        }
        // Following another Phosphor: its project and beat; the show runs at its time. Leading: tell followers.
        #[cfg(not(target_arch = "wasm32"))]
        let show = self.state.follow_frame(now);
        #[cfg(target_arch = "wasm32")]
        let show = now;
        #[cfg(not(target_arch = "wasm32"))]
        let beside = self.render_thread.as_ref().is_some_and(crate::render_thread::RenderThread::running);
        #[cfg(target_arch = "wasm32")]
        let beside = false;
        let heard = {
            let State { clip_page, comp, core, audio, mod_inputs, clock, net, net_audio, notice, models, tracker, #[cfg(not(target_arch = "wasm32"))] milkdrop, #[cfg(not(target_arch = "wasm32"))] gdrop, .. } = &mut self.state;
            #[cfg(target_arch = "wasm32")]
            let _ = (tracker, clip_page);
            let mut held = core.lock();
            let RenderCore { engine, media, mod_phases, .. } = &mut *held;
            let Some(engine) = engine.as_mut() else { return };
            // (The render thread plays the movies itself, with the project it's given.)
            if !beside {
                media.sync(comp, engine);
                #[cfg(not(target_arch = "wasm32"))]
                media.preload_clips(comp, *clip_page, crate::panels::CLIP_PAGE_SIZE);
                media.update_video(comp, engine, now);
            }
            #[cfg(not(target_arch = "wasm32"))]
            milkdrop.update(comp, engine, audio, now, &mut media.errors);
            #[cfg(not(target_arch = "wasm32"))]
            gdrop.update(comp, engine, audio);
            for (id, e) in models.update(comp, engine) {
                media.errors.insert(id, e);
            }

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
            // Sound band modulators: up at once, down smoothly.
            for m in comp.modulators.iter_mut() {
                if let Some([lo, hi]) = m.band {
                    let v = crate::audio::band_level(audio.bins(), lo, hi);
                    m.value = if v > m.value { v } else { m.value + (v - m.value) * 0.15 };
                }
            }
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
                time: show as f32,
                audio_level: an.level,
                audio_low: an.low,
                audio_mid: an.mid,
                audio_high: an.high,
                shared: if beside { mod_inputs.shared } else { Default::default() },
            };
            // Data loopers keep what their knob does (while recording), then shared modulators (and their chains),
            // once per frame, before anything reads them.
            for m in comp.modulators.iter_mut().filter(|m| m.signal.source == phosphor_engine::ModSource::Hand) {
                m.looper.record(mod_inputs.beats, m.value);
            }
            // What video tracking found lands in its Hand modulators before they're read.
            #[cfg(not(target_arch = "wasm32"))]
            tracker.apply(comp);
            // (Beside the render thread, that thread works them out for each frame it draws.)
            if !beside {
                mod_inputs.shared = phosphor_engine::evaluate(&comp.modulators, mod_inputs, mod_phases);
            }
            #[cfg(not(target_arch = "wasm32"))]
            let heard = crate::render_thread::Heard { level: an.level, low: an.low, mid: an.mid, high: an.high, beat: an.beat_env };
            #[cfg(target_arch = "wasm32")]
            let heard = ();
            heard
        };
        // Beside the render thread: what it draws from next, then the interface's share once per frame it drew.
        #[cfg(not(target_arch = "wasm32"))]
        let drawn = if beside {
            self.publish_frame(now, show, heard);
            match self.take_thread_frames() {
                Some(t) => t,
                None => {
                    // No new frame to read back or show: what the panels asked for is asked again (they ask every
                    // frame), not piled up.
                    self.state.preview_wants.clear();
                    self.state.monitor_wants.clear();
                    self.state.monitor_held.clear();
                    return;
                }
            }
        } else {
            Vec::new()
        };
        #[cfg(target_arch = "wasm32")]
        let _ = heard;
        let State {
            comp,
            core,
            render_state,
            texture,
            texture_generation,
            layer_textures,
            stats,
            output,
            mod_inputs,
            stream,
            offline,
            recorders,
            clock,
            net,
            preview_wants,
            explorer,
            preview_keys,
            preview_textures,
            preview_stills,
            selected,
            monitor_wants,
            monitor_held,
            snapshot_source,
            snapshot_ready,
            project_picture,
            monitor_textures,
            output_textures,
            #[cfg(target_os = "macos")]
            vcam,
            #[cfg(any(target_os = "macos", windows))]
            ndi,
            #[cfg(windows)]
            spout,
            #[cfg(not(target_arch = "wasm32"))]
            dmx,
            #[cfg(not(target_arch = "wasm32"))]
            lasers,
            #[cfg(not(target_arch = "wasm32"))]
            dmx_rec,
            #[cfg(not(target_arch = "wasm32"))]
            tracker,
            #[cfg(target_os = "macos")]
            ffgl,
            scopes,
            #[cfg(not(target_arch = "wasm32"))]
            scanner,
            #[cfg(not(target_arch = "wasm32"))]
            led_scan,
            #[cfg(not(target_arch = "wasm32"))]
            led_identify,
            ..
        } = &mut self.state;
        let mut held = core.lock();
        #[cfg(target_os = "macos")]
        let RenderCore { engine, media, syphon, .. } = &mut *held;
        #[cfg(not(target_os = "macos"))]
        let RenderCore { engine, media, .. } = &mut *held;
        let Some(engine) = engine.as_mut() else { return };

        let wants = std::mem::take(preview_wants);
        // (The Effects panel can pin the tiles to one layer or the mix: `ExplorerState::preview_on`.)
        let source = explorer.preview_on.or(*selected).filter(|id| *id == phosphor_engine::MASTER_ID || comp.layers.iter().any(|l| l.id == *id)).unwrap_or(phosphor_engine::MASTER_ID);
        let keys: Vec<String> = wants.iter().map(|(k, _, _)| k.clone()).collect();
        if !wants.is_empty() {
            // Each tile renders a still; hovered / live tiles keep updating, and stills are taken again only when the
            // picture they're made from changes (`stills`): the source layer, what it plays, whether it has a picture.
            let source_key = {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                source.hash(&mut h);
                let layers: Vec<&phosphor_engine::LayerState> = if source == phosphor_engine::MASTER_ID { comp.layers.iter().collect() } else { comp.layers.iter().filter(|l| l.id == source).collect() };
                for l in layers {
                    l.active_clip.hash(&mut h);
                    l.enabled.hash(&mut h);
                    super::stills::hash_debug(&l.source, &mut h);
                    layer_textures.contains_key(&l.id).hash(&mut h);
                }
                h.finish()
            };
            let again = preview_stills.plan(now, keys != *preview_keys, source_key, keys.len());
            let items = wants
                .into_iter()
                .enumerate()
                .map(|(i, (key, item, live))| {
                    let refresh = if preview_keys.get(i) != Some(&key) {
                        phosphor_engine::PreviewRefresh::Moved
                    } else if live || again == super::stills::Again::All {
                        phosphor_engine::PreviewRefresh::Live
                    } else {
                        phosphor_engine::PreviewRefresh::No
                    };
                    (item, refresh)
                })
                .collect();
            engine.request_previews(source, items);
        }
        let mut mwants = std::mem::take(monitor_wants);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((src, w, h)) = tracker.picture(&comp.tracking, (comp.width, comp.height)) {
            mwants.push((crate::tracking::MONITOR_KEY, src, w, h));
            if tracker.due(now) {
                engine.read_monitor(mwants.len() - 1, crate::tracking::MONITOR_KEY as u64);
            }
        }
        scopes.layers = comp.layers.iter().filter(|l| l.enabled).filter_map(|l| Some((l.id, crate::scopes::ScopeKind::of_source(&l.source)?))).collect();
        if let Some((src, w, h)) = scopes.picture(now, (comp.width, comp.height)) {
            mwants.push((crate::scopes::MONITOR_KEY, src, w, h));
            if scopes.due(now) {
                engine.read_monitor(mwants.len() - 1, crate::scopes::MONITOR_KEY as u64);
            }
        }
        // Remove background: fresh mattes in, and the effects' pictures out when the worker is free.
        #[cfg(not(target_arch = "wasm32"))]
        {
            for (res, (w, h, mask)) in tracker.take_mattes() {
                engine.set_matte(res, w, h, &mask);
            }
            if tracker.cutout_due(now) {
                engine.request_cutout_frames();
            }
        }
        // Stage scanner: its pattern on the output, the camera read once per pattern, the finished photo set.
        #[cfg(not(target_arch = "wasm32"))]
        {
            scanner.poll();
            if let Some((o, path)) = scanner.finished.take()
                && let Some(out) = comp.output_mut(o)
            {
                out.stage_photo = path;
                out.stage_photo_opacity = out.stage_photo_opacity.max(0.6);
            }
            engine.set_scan(scanner.pattern());
            if let Some((src, w, h)) = scanner.picture() {
                mwants.push((crate::scanner::MONITOR_KEY, src, w, h));
                if scanner.due(now) {
                    engine.read_monitor(mwants.len() - 1, crate::scanner::MONITOR_KEY as u64);
                }
            }
            // LED scanner: the lights show its pictures (below, instead of the picture), the camera read once each;
            // the fixtures it found are placed.
            led_scan.poll();
            if let Some(placed) = led_scan.finished.take() {
                for (id, positions) in placed {
                    if let Some(f) = comp.dmx.fixtures.iter_mut().find(|f| f.id == id) {
                        f.shape = phosphor_engine::FixtureShape::Scanned;
                        f.positions = positions;
                    }
                }
            }
            if let Some((src, w, h)) = led_scan.picture() {
                mwants.push((crate::led_scan::MONITOR_KEY, src, w, h));
                if led_scan.due(now) {
                    engine.read_monitor(mwants.len() - 1, crate::led_scan::MONITOR_KEY as u64);
                }
            }
        }
        // A saved project's picture: the mix, small, read back without waiting (Projects window).
        if project_picture.is_some() {
            let w = 480;
            mwants.push((crate::panels::PROJECT_PICTURE_KEY, phosphor_engine::MASTER_ID, w, (w * comp.height / comp.width.max(1)).max(1)));
            engine.read_monitor(mwants.len() - 1, crate::panels::PROJECT_PICTURE_KEY as u64);
        }
        // A picture of one layer / group / comp: read through a monitor slot this frame.
        if let Some(id) = snapshot_source.take() {
            mwants.push((crate::panels::SNAPSHOT_KEY, id, comp.width, comp.height));
            engine.read_monitor(mwants.len() - 1, crate::panels::SNAPSHOT_KEY as u64);
        }
        // Recording one layer / group / comp: its picture at the canvas size, read back when a frame is due.
        let rec_src = Some(stream.settings.source).filter(|&id| id != 0 && stream.running() && (comp.layers.iter().any(|l| l.id == id) || comp.group(id).is_some() || comp.comp(id).is_some()));
        if let Some(id) = rec_src {
            mwants.push((crate::stream::MONITOR_KEY, id, comp.width, comp.height));
            if stream.frame_due(now) {
                engine.read_monitor(mwants.len() - 1, crate::stream::MONITOR_KEY as u64);
            }
        }
        // More recorders: each its source's picture at the canvas size, read back when its frame is due.
        for (i, r) in recorders.iter_mut().enumerate().filter(|(_, r)| r.running()) {
            let src = match r.settings.source {
                0 => phosphor_engine::MASTER_ID,
                id if comp.layers.iter().any(|l| l.id == id) || comp.group(id).is_some() || comp.comp(id).is_some() => id,
                _ => continue,
            };
            let key = crate::app::recorders::FIRST_KEY + i as u8;
            mwants.push((key, src, comp.width, comp.height));
            if r.frame_due(now) {
                engine.read_monitor(mwants.len() - 1, key as u64);
            }
        }
        // Video trace shapes: their pictures, small, read back a few times a second.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (pics, due) = lasers.pictures_wanted(&comp.lasers, (comp.width, comp.height), now);
            for (i, (src, w, h)) in pics.into_iter().enumerate() {
                let key = crate::lasers::FIRST_KEY + i as u8;
                mwants.push((key, src, w, h));
                if due {
                    engine.read_monitor(mwants.len() - 1, key as u64);
                }
            }
        }
        let mkeys: Vec<u8> = mwants.iter().map(|m| m.0).collect();
        engine.request_monitors(mwants.iter().map(|m| (m.1, m.2, m.3)).collect());
        let held = std::mem::take(monitor_held);
        engine.hold_monitors(mwants.iter().enumerate().filter(|(_, m)| held.contains(&m.0)).map(|(i, _)| i).collect());
        // LED / DMX: sample the fixtures' pixels from this frame when the lights want one.
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Playing a DMX recording, or the LED scanner's pictures: they go out instead of the picture (nothing to
            // sample).
            let playback = dmx_rec.playback(now);
            let playback = led_scan.universes(&comp.dmx).or(playback);
            // Finding a string (Flash / Chase) wins over everything: it's what someone across the venue is looking for.
            let playback = led_identify.universes(&comp.dmx, now).or(playback);
            // The points to sample go to the engine only when a fixture changed (`DmxOut::sample_points`), and only
            // when a frame of lights is due; with nothing to sample the engine's sampler is emptied once.
            let sampling = comp.dmx.enabled && playback.is_none();
            if !sampling && dmx.has_points() {
                dmx.forget_points();
                engine.set_sample_points(&[]);
            }
            if dmx.frame_due(now, &comp.dmx) {
                match &playback {
                    Some(unis) => dmx.send(&comp.dmx, unis),
                    None => {
                        if let Some(points) = dmx.sample_points(&comp.dmx) {
                            engine.set_sample_points(points);
                        }
                        engine.request_samples();
                    }
                }
            }
        }
        // Lasers: each armed projector gets its look (dark with Blackout).
        #[cfg(not(target_arch = "wasm32"))]
        lasers.run(&comp.lasers, show, mod_inputs, comp.blackout, now);
        // FFGL plugins draw what the last frame asked for, so this frame shows it.
        #[cfg(target_os = "macos")]
        {
            ffgl.poll();
            let beats = clock.beats(now);
            ffgl.run(engine, show, clock.bpm, (beats.rem_euclid(4.0) / 4.0) as f32);
        }
        let t0 = web_time::Instant::now();
        // (Beside the render thread, it drew the frame: what's below reads that one.)
        let info = (!beside).then(|| engine.render(comp, show as f32, mod_inputs));
        #[cfg(not(target_arch = "wasm32"))]
        if info.is_some() {
            crate::present::note_engine();
        }
        #[cfg(target_os = "macos")]
        ffgl.take_jobs(engine);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(colors) = engine.take_samples()
            && comp.dmx.enabled
        {
            let universes = comp.dmx.universes(&colors, mod_inputs.beats as f64);
            dmx.send(&comp.dmx, &universes);
            dmx_rec.sent(now, &universes);
        }
        // A recorder's read that didn't fit in the readback ring: its last frame again (the movie keeps time).
        for tag in engine.take_monitor_skips() {
            let k = tag as u8;
            if k == crate::stream::MONITOR_KEY {
                stream.repeat_last();
            } else if let Some(r) = k.checked_sub(crate::app::recorders::FIRST_KEY).and_then(|i| recorders.get_mut(i as usize)) {
                r.repeat_last();
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let mut picture_to_save = None;
        for f in engine.take_monitor_reads() {
            match f.tag as u8 {
                #[cfg(not(target_arch = "wasm32"))]
                crate::tracking::MONITOR_KEY => tracker.give(f.rgba, f.width, f.height, &comp.tracking),
                crate::stream::MONITOR_KEY => stream.push(f.width, f.height, f.rgba),
                #[cfg(not(target_arch = "wasm32"))]
                k if (crate::lasers::FIRST_KEY..crate::lasers::FIRST_KEY + crate::lasers::MAX_PICTURES as u8).contains(&k) => lasers.give_picture(k, f.width, f.height, f.rgba),
                k if (crate::app::recorders::FIRST_KEY..crate::app::recorders::FIRST_KEY + crate::app::recorders::MAX as u8).contains(&k) => {
                    if let Some(r) = recorders.get_mut((k - crate::app::recorders::FIRST_KEY) as usize) {
                        r.push(f.width, f.height, f.rgba);
                    }
                }
                crate::panels::SNAPSHOT_KEY => *snapshot_ready = Some((f.width, f.height, f.rgba)),
                // (Written once the core is let go: a file write must never hold the render thread up.)
                #[cfg(not(target_arch = "wasm32"))]
                crate::panels::PROJECT_PICTURE_KEY => {
                    if let Some(project) = project_picture.take() {
                        picture_to_save = Some((project, f.width, f.height, f.rgba));
                    }
                }
                crate::scopes::MONITOR_KEY => {
                    scopes.give(&f.rgba, f.width, f.height, now);
                    for (id, img) in scopes.take_layer_pictures() {
                        let [w, h] = img.size;
                        let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                        engine.upload_rgba(id, w as u32, h as u32, &rgba);
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                crate::scanner::MONITOR_KEY => scanner.give(f.rgba, f.width, f.height),
                #[cfg(not(target_arch = "wasm32"))]
                crate::led_scan::MONITOR_KEY => led_scan.give(f.rgba, f.width, f.height),
                _ => {}
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        for f in engine.take_cutout_frames() {
            tracker.give_cutout(f.tag, f.rgba, f.width, f.height);
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
        // (Beside the render thread, it publishes each frame it draws.)
        #[cfg(target_os = "macos")]
        if !beside {
            publish_syphon(syphon, Some(engine), comp);
        }

        // Rendering offline: a frame goes in once every movie has its picture (else the same moment renders again).
        let stream_due = match offline {
            Some(o) => stream.running() && o.take(media.caught_up(comp)),
            None => rec_src.is_none() && stream.frame_due(now),
        };
        let net_due = net.frame_due(now);
        #[cfg(any(target_os = "macos", windows))]
        let ndi_due = {
            ndi.sync(comp.ndi.enabled, &comp.ndi.name);
            ndi.frame_due(now, comp.ndi.fps)
        };
        #[cfg(not(any(target_os = "macos", windows)))]
        let ndi_due = false;
        // Spout (Windows): the composition through the same capture, at the render rate while it's on.
        #[cfg(windows)]
        let spout_due = {
            spout.sync(comp.spout, "Phosphor");
            spout.frame_due(now, 60)
        };
        #[cfg(not(windows))]
        let spout_due = false;
        // The virtual camera (macOS): only while it's on and the camera is there.
        #[cfg(target_os = "macos")]
        let vcam_due = vcam.frame_due(now);
        #[cfg(not(target_os = "macos"))]
        let vcam_due = false;
        // Who asked for this frame: it goes only to them when it comes back (a frame copied for every running output,
        // due or not, was wasted work).
        let wants = [(stream_due, TO_STREAM), (net_due, TO_NET), (ndi_due, TO_NDI), (spout_due, TO_SPOUT), (vcam_due, TO_VCAM)]
            .iter()
            .filter(|(due, _)| *due)
            .fold(0, |m, (_, bit)| m | bit);
        let captured = wants != 0 && engine.capture_frame(wants);
        match offline {
            Some(o) if stream_due && captured => o.done += 1,
            None if stream_due && !captured => stream.repeat_last(),
            _ => {}
        }
        // NDI sound: the movie mix, while someone's receiving and it's on.
        #[cfg(any(target_os = "macos", windows))]
        {
            let want = ndi.connections() > 0 && comp.ndi.sound;
            crate::media::video::sound::set_ndi_tap(want);
            if want {
                let block = crate::media::video::sound::take_ndi();
                ndi.push_audio(&block, crate::media::video::sound::stats().0);
            }
        }
        for f in engine.poll_captures() {
            let wants = |bit: u64| f.tag & bit != 0;
            #[cfg(any(target_os = "macos", windows))]
            let to_ndi = wants(TO_NDI) && ndi.connections() > 0;
            #[cfg(not(any(target_os = "macos", windows)))]
            let to_ndi = false;
            #[cfg(windows)]
            let to_spout = wants(TO_SPOUT) && spout.running();
            #[cfg(not(windows))]
            let to_spout = false;
            let to_net = wants(TO_NET) && net.running();
            let to_stream = wants(TO_STREAM) && stream.running() && rec_src.is_none();
            #[cfg(target_os = "macos")]
            if wants(TO_VCAM) && vcam.connected() {
                vcam.push(f.width, f.height, &f.rgba);
            }
            // A picture is 8 MB at 1080p, 33 MB at 4K: copied only when a second output takes it too (each used to
            // get a copy and the last one the original).
            let (width, height) = (f.width, f.height);
            let mut picture = Handout::new(f.rgba, [to_ndi, to_spout, to_net, to_stream].into_iter().filter(|t| *t).count());
            #[cfg(any(target_os = "macos", windows))]
            if to_ndi {
                ndi.push(width, height, picture.take(), comp.ndi.fps);
            }
            #[cfg(windows)]
            if to_spout {
                spout.push(width, height, picture.take());
            }
            if to_net {
                net.push_frame(width, height, picture.take());
            }
            if to_stream {
                stream.push(width, height, picture.take());
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
            // Layers, groups and comps all have thumbnails.
            for lid in comp.layers.iter().map(|l| l.id).chain(comp.groups.iter().map(|g| g.id)).chain(comp.comps.iter().map(|c| c.id)) {
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
            // Preview windows: slot i of the engine's monitors belongs to window mkeys[i]. A window whose picture
            // isn't there yet keeps nothing (never another window's picture); a replaced picture is re-linked.
            for (i, n) in mkeys.iter().enumerate() {
                let Some(t) = engine.monitor(i) else { continue };
                match monitor_textures.get_mut(n) {
                    Some((_, tex)) if *tex == t.texture => {}
                    Some((id, tex)) => {
                        renderer.update_egui_texture_from_wgpu_texture(&rs.device, &t.view, linear, *id);
                        *tex = t.texture.clone();
                    }
                    None => {
                        let id = renderer.register_native_texture(&rs.device, &t.view, linear);
                        monitor_textures.insert(*n, (id, t.texture.clone()));
                    }
                }
            }
            // Windows that closed (or show the output window itself) let go of theirs.
            monitor_textures.retain(|n, (id, _)| {
                let keep = mkeys.contains(n);
                if !keep {
                    renderer.free_texture(id);
                }
                keep
            });
            // Explorer / editor previews: texture slot i is preview i (a slot not ready yet keeps its place, so the tiles
            // after it don't show their neighbor's picture), and only shows once it holds what was asked for.
            if preview_textures.len() < keys.len() {
                preview_textures.resize(keys.len(), None);
            }
            for i in 0..keys.len() {
                let slot = &mut preview_textures[i];
                let Some(t) = engine.preview(i) else {
                    if let Some((_, _, ready)) = slot {
                        *ready = false;
                    }
                    continue;
                };
                let key = (t.width, t.height);
                match slot {
                    Some((_, k, ready)) if *k == key => *ready = true,
                    Some((id, k, ready)) => {
                        renderer.update_egui_texture_from_wgpu_texture(&rs.device, &t.view, linear, *id);
                        (*k, *ready) = (key, true);
                    }
                    None => {
                        let id = renderer.register_native_texture(&rs.device, &t.view, linear);
                        *slot = Some((id, key, true));
                    }
                }
            }
            layer_textures.retain(|id, (tex, _)| {
                let live = comp.layers.iter().any(|l| l.id == *id) || comp.group(*id).is_some() || comp.comp(*id).is_some();
                if !live {
                    renderer.free_texture(tex);
                }
                live
            });
        }
        *preview_keys = keys;

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
                e.thread_presents = beside;
                if !o.enabled {
                    e.open = false;
                }
            }
            map.retain(|id, _| comp.outputs.iter().any(|o| o.id == *id));
        }

        if let Some(rs) = render_state {
            stats.watch_gpu_memory(now, || rs.device.generate_allocator_report().map(|r| (r.total_reserved_bytes, r.total_allocated_bytes)));
        }
        // The frames drawn: this one, or the ones the render thread drew since the last time here.
        let mut snapshots = Vec::new();
        match info {
            Some(info) => snapshots.extend(stats.record(now, render_secs, info.passes, mod_inputs.audio_level as f64, engine.gpu_ms().unwrap_or(0.0) as f64)),
            #[cfg(not(target_arch = "wasm32"))]
            None => {
                for t in drawn {
                    snapshots.extend(stats.record_at(t.at, t.now, t.render_secs, t.passes, mod_inputs.audio_level as f64, t.gpu_ms));
                }
            }
            #[cfg(target_arch = "wasm32")]
            None => {}
        }
        drop(held);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((project, w, h, rgba)) = picture_to_save {
            crate::project_files::save_picture(&project, w, h, rgba);
        }
        if let Some(run) = &mut self.bench {
            for snapshot in snapshots {
                println!("{}", snapshot.to_json());
                run.samples.push(snapshot);
            }
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

/// One captured picture for several outputs: each but the last taker gets a copy, the last one the picture itself.
struct Handout {
    picture: Option<Vec<u8>>,
    takers: usize,
}

impl Handout {
    fn new(picture: Vec<u8>, takers: usize) -> Self {
        Self { picture: Some(picture), takers }
    }

    fn take(&mut self) -> Vec<u8> {
        self.takers = self.takers.saturating_sub(1);
        if self.takers == 0 { self.picture.take().unwrap_or_default() } else { self.picture.clone().unwrap_or_default() }
    }
}

#[cfg(test)]
mod handout_tests {
    use super::Handout;

    #[test]
    fn the_last_taker_gets_the_picture_itself() {
        let picture = vec![7u8; 64];
        let at = picture.as_ptr();
        let mut one = Handout::new(picture, 1);
        let only = one.take();
        assert_eq!(only.as_ptr(), at, "one output: no copy");
        let picture = vec![7u8; 64];
        let at = picture.as_ptr();
        let mut two = Handout::new(picture, 2);
        let first = two.take();
        assert_ne!(first.as_ptr(), at, "the first of two gets a copy");
        assert_eq!(first, vec![7u8; 64]);
        let last = two.take();
        assert_eq!(last.as_ptr(), at, "the last gets the original");
    }
}

/// Syphon out: the program and each output that has it on, from the frame just drawn (the render thread's, or this
/// thread's without it).
#[cfg(target_os = "macos")]
pub(crate) fn publish_syphon(syphon: &mut crate::syphon::Syphon, engine: Option<&phosphor_engine::Engine>, comp: &Composition) {
    let Some(engine) = engine else { return };
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
