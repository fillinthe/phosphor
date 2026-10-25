//! Startup: restores settings, recovers after a crash, opens an exported show.

use super::*;

impl PhosphorApp {
    pub fn new(cc: &eframe::CreationContext<'_>, bench: Option<BenchConfig>) -> Self {
        // Find installed fonts in the background so the text layer's font list opens instantly.
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(|| {
            crate::text::fonts();
        });
        theme::apply(&cc.egui_ctx);

        let stored = |key: &str| if bench.is_none() { cc.storage.and_then(|s| s.get_string(key)) } else { None };
        let accent = stored(KEY_ACCENT).and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
        #[cfg(not(target_arch = "wasm32"))]
        let snapshot = if bench.is_none() { crate::recovery::begin_session() } else { None };
        #[cfg(target_arch = "wasm32")]
        let snapshot: Option<()> = None;
        let comp = match &bench {
            Some(cfg) => cfg.composition(),
            None => {
                #[cfg(not(target_arch = "wasm32"))]
                let recovered = snapshot.as_ref().and_then(|s| s.comp.clone());
                #[cfg(target_arch = "wasm32")]
                let recovered: Option<Composition> = None;
                #[cfg(not(target_arch = "wasm32"))]
                let show = crate::show::bundled().and_then(|b| crate::show::load(b).map_err(|e| eprintln!("show: {e}")).ok());
                #[cfg(target_arch = "wasm32")]
                let show: Option<Composition> = None;
                // An exported show always starts from its own project (unless recovering from a crash).
                recovered.or(show).or_else(|| stored(KEY_COMPOSITION).and_then(|json| Composition::from_json(&json).ok())).unwrap_or_else(Composition::opening)
            }
        };
        let stream_settings = stored(KEY_STREAM).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let recorder_settings: Vec<crate::stream::StreamSettings> = stored(KEY_RECORDERS).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let mut detached: Vec<Tab> = stored(KEY_DETACHED).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let mut dock = stored(KEY_DOCK).and_then(|s| from_text(&s)).unwrap_or_else(default_layout);
        // The Stack trial (2026-10-24) is over: arrangements saved during it get Clips back, once.
        let retire_stack = stored(KEY_STACK_RETIRED).is_none();
        if retire_stack {
            super::layout::retire_stack(&mut dock);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(s) = &snapshot {
            if let Some(d) = s.dock.as_ref().and_then(|v| match v {
                serde_json::Value::String(text) => from_text(text),
                other => serde_json::from_value(other.clone()).ok(),
            }) {
                dock = d;
            }
            detached = s.detached.clone();
        }
        let layouts: Layouts = stored(KEY_LAYOUTS).and_then(|s| from_text(&s)).unwrap_or_default();
        #[cfg(not(target_arch = "wasm32"))]
        let recovering = snapshot.is_some();
        #[cfg(target_arch = "wasm32")]
        let recovering = false;
        if !recovering {
            // An exported show opens with the layout it was exported with; otherwise your startup layout.
            #[cfg(not(target_arch = "wasm32"))]
            let show_layout = crate::show::bundled().and_then(|b| b.manifest.layout.as_deref()).and_then(from_text::<SavedLayout>);
            #[cfg(target_arch = "wasm32")]
            let show_layout: Option<SavedLayout> = None;
            let startup = layouts.startup.as_deref().and_then(|n| layouts.find(n)).and_then(|i| layouts.get(i));
            if let Some((d, det)) = show_layout.map(|l| (l.dock, l.detached)).or(startup) {
                dock = d;
                detached = det;
            }
        }
        // The areas, each its own arrangement (2026-10-19): the first launch with them starts from the defaults in
        // Show; after that every area comes back the way you left it.
        let first_areas = stored(KEY_AREAS).is_none() && !recovering;
        let mut area_docks: HashMap<usize, (DockState<Tab>, Vec<Tab>)> = if first_areas {
            dock = super::layout::layout_show();
            detached.clear();
            HashMap::new()
        } else {
            stored(KEY_AREA_DOCKS).and_then(|s| from_text::<Vec<(usize, DockState<Tab>, Vec<Tab>)>>(&s)).unwrap_or_default().into_iter().map(|(i, d, t)| (i, (d, t))).collect()
        };
        if retire_stack {
            for (d, _) in area_docks.values_mut() {
                super::layout::retire_stack(d);
            }
        }
        let area = if first_areas { 0 } else { stored(KEY_AREA).and_then(|s| s.parse().ok()).unwrap_or(0) };
        // The current area's arrangement is `dock` itself, not a saved copy.
        area_docks.remove(&area);
        let known: Vec<Tab> = stored(KEY_KNOWN_TABS).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        ensure_new_tabs(&mut dock, &known, area);
        // Account moved from the bottom panels to the top bar (2026-10-15): take it out of saved layouts once.
        if stored(KEY_ACCOUNT_TAB_MOVED).is_none()
            && let Some(path) = dock.find_tab(&Tab::Account)
        {
            let _ = dock.remove_tab(path);
        }

        let comp_bpm = comp.bpm;
        let render_state = cc.wgpu_render_state.clone();
        let engine = render_state.as_ref().map(|rs| Engine::new(rs.device.clone(), rs.queue.clone()));
        #[cfg(not(target_arch = "wasm32"))]
        #[cfg(target_os = "macos")]
        crate::media::video::set_zero_copy(engine.is_some() && std::env::var_os("PHOSPHOR_COPY_VIDEO").is_none());
        #[cfg(target_os = "macos")]
        crate::media::video::surface::set_sixteen_bit(engine.as_ref().is_some_and(|e| e.device().features().contains(wgpu::Features::TEXTURE_FORMAT_16BIT_NORM)));
        #[cfg(not(target_arch = "wasm32"))]
        crate::media::video::sound::set_enabled(bench.as_ref().is_none_or(|b| b.sound || b.movie_sound));
        #[cfg(not(target_arch = "wasm32"))]
        if bench.is_none() {
            crate::media::video::sound::set_output_device(stored(KEY_SOUND_OUT));
        } else if let Ok(name) = std::env::var("PHOSPHOR_SOUND_OUT") {
            crate::media::video::sound::set_output_device(Some(name));
        }
        #[cfg(not(target_arch = "wasm32"))]
        if bench.as_ref().is_some_and(|b| b.movie_sound && !b.sound) {
            crate::media::video::sound::set_local(false);
        }
        #[cfg(not(target_arch = "wasm32"))]
        crate::media::video::set_gpu_hap(engine.as_ref().is_some_and(|e| e.compressed_supported()) && std::env::var_os("PHOSPHOR_CPU_HAP").is_none());
        let now = cc.egui_ctx.input(|i| i.time);

        let open_audio = bench.as_ref().is_some_and(|b| b.audio);
        let fresh_install = bench.is_none() && stored(KEY_COMPOSITION).is_none();
        let library_default_done = stored(KEY_LIBRARY_DEFAULT).is_some();
        let mut app = Self {
            state: State {
                selected: comp.layers.last().map(|l| l.id),
                selected_clip: None,
                also_clips: Vec::new(),
                clip_before: None,
                also_selected: Vec::new(),
                multi_before: None,
                selected_surface: None,
                comp,
                engine,
                render_state,
                texture: None,
                texture_generation: 0,
                layer_textures: HashMap::new(),
                media: MediaManager::default(),
                stats: Stats::new(now),
                output: Arc::new(Mutex::new(std::collections::HashMap::new())),
                output_textures: HashMap::new(),
                mapping_output: 0,
                presets: PresetStore::looks(),
                venues: PresetStore::venues(),
                layer_presets: PresetStore::layers(),
                chain_presets: PresetStore::chains(),
                surface_presets: PresetStore::surfaces(),
                mapping_presets: PresetStore::mappings(),
                surface_preset_name: String::new(),
                mapping_preset_name: String::new(),
                layer_preset_name: String::new(),
                chain_preset_name: String::new(),
                venue_name: String::new(),
                preset_name: String::new(),
                notice: None,
                notice_since: 0.0,
                audio_declined: false,
                clip_cells: Vec::new(),
                audio: AudioInput::new(),
                control: ControlHub::with_settings(stored(KEY_CONTROL).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()),
                listen_for: None,
                map_draft: None,
                map_search: String::new(),
                stream: Streamer::new(stream_settings),
                stream_audio_devices: Vec::new(),
                net: NetSession::new(),
                net_audio: Vec::new(),
                jam: Default::default(),
                auto_run: Default::default(),
                bug: crate::support::BugReport::new(stored(KEY_SEND_CRASHES).is_some_and(|v| v == "1")),
                #[cfg(not(target_arch = "wasm32"))]
                rhythm: Default::default(),
                updater: crate::update::Updater::with_settings(stored(KEY_UPDATE_SKIP).unwrap_or_default(), stored(KEY_UPDATE_AUTO).is_none_or(|v| v != "0")),
                remote_password: stored(KEY_REMOTE_PASSWORD).unwrap_or_default(),
                render_rate: stored(KEY_RENDER_RATE).and_then(|t| layout::from_text(&t)).unwrap_or_default(),
                render_pacer: Default::default(),
                require_password: stored(KEY_REMOTE_PASSWORD).is_some_and(|p| !p.is_empty()),
                web_activity: None,
                web_log: Vec::new(),
                #[cfg(not(target_arch = "wasm32"))]
                dialogs: Default::default(),
                #[cfg(not(target_arch = "wasm32"))]
                folders: crate::folders::FolderWatcher::new(stored(KEY_WATCHED).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()),
                explorer: crate::explorer::ExplorerState {
                    favorites: stored(KEY_FAVORITES).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
                    hidden: stored(KEY_HIDDEN_FX).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
                    recent: stored(KEY_RECENT).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
                    ..Default::default()
                },
                editor: Default::default(),
                palette: Default::default(),
                preview_wants: Vec::new(),
                preview_keys: Vec::new(),
                preview_refreshed: 0.0,
                preview_textures: Vec::new(),
                open_tab: None,
                maps_clipboard: Vec::new(),
                snapshot_output: None,
                clip_thumb_request: None,
                timeline_pos: 0.0,
                timeline_play: None,
                timeline_record: false,
                timeline_fresh: false,
                #[cfg(not(target_arch = "wasm32"))]
                timeline_sound: None,
                timeline_set: Vec::new(),
                offline: None,
                recorders: recorder_settings.into_iter().take(recorders::MAX).map(Streamer::new).collect(),
                offline_setup: Default::default(),
                // Opening a project changes nothing: the timeline sets values once it plays or the playhead moves.
                timeline_applied: Some(0.0),
                #[cfg(not(target_arch = "wasm32"))]
                ltc_out: None,
                #[cfg(not(target_arch = "wasm32"))]
                color_ref: None,
                stream_bar: None,
                mouse_to_shaders: false,
                templates_open: false,
                #[cfg(not(target_arch = "wasm32"))]
                backgrounds: Default::default(),
                fx_browser: None,
                monitors: stored(KEY_MONITORS).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
                monitor_wants: Vec::new(),
                monitor_held: Vec::new(),
                monitor_drawn: HashMap::new(),
                monitor_textures: HashMap::new(),
                displays: crate::screens::displays(),
                new_monitor: None,
                snapshot_requested: false,
                snapshot_source: None,
                snapshot_ready: None,
                #[cfg(not(target_arch = "wasm32"))]
                isf: crate::isf_lib::IsfLibrary::scan(),
                fx_clipboard: Vec::new(),
                thumbs: Thumbs::new(),
                media_view: Default::default(),
                map_mode: false,
                fx_fades: Vec::new(),
                project_path: stored(KEY_PROJECT_PATH).filter(|p| !p.is_empty()),
                autosave: stored(KEY_AUTOSAVE).is_some_and(|v| v == "1"),
                autosave_at: 0.0,
                saved_hash: 0,
                account: Default::default(),
                account_ui: Default::default(),
                cloud_project: None,
                account_window: false,
                account_hello: bench.is_none() && !cfg!(target_arch = "wasm32") && stored(KEY_ACCOUNT_HELLO).is_none(),
                account_hello_never: false,
                recent: stored(KEY_RECENT_PROJECTS).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
                history: Default::default(),
                export: Default::default(),
                #[cfg(target_os = "macos")]
                syphon: Default::default(),
                #[cfg(any(target_os = "macos", windows))]
                ndi: Default::default(),
                #[cfg(target_os = "macos")]
                vcam: Default::default(),
                #[cfg(windows)]
                spout: Default::default(),
                clock: Clock::new(comp_bpm),
                pending_triggers: Vec::new(),
                autopilot: HashMap::new(),
                step_fired: HashMap::new(),
                step_painted: HashMap::new(),
                cue_run: Default::default(),
                #[cfg(not(target_arch = "wasm32"))]
                link: None,
                #[cfg(not(target_arch = "wasm32"))]
                avc_import: None,
                #[cfg(not(target_arch = "wasm32"))]
                relink: None,
                #[cfg(not(target_arch = "wasm32"))]
                dmx: Default::default(),
                #[cfg(not(target_arch = "wasm32"))]
                dmx_rec: Default::default(),
                #[cfg(not(target_arch = "wasm32"))]
                tracker: Default::default(),
                #[cfg(target_os = "macos")]
                ffgl: Default::default(),
                #[cfg(not(target_arch = "wasm32"))]
                following: None,
                follow_at_launch: if bench.is_some() { String::new() } else { stored(KEY_FOLLOW_AT_LAUNCH).unwrap_or_default() },
                #[cfg(not(target_arch = "wasm32"))]
                leader: Default::default(),
                #[cfg(not(target_arch = "wasm32"))]
                projectors: Default::default(),
                scopes: Default::default(),
                #[cfg(not(target_arch = "wasm32"))]
                scanner: Default::default(),
                #[cfg(not(target_arch = "wasm32"))]
                led_scan: Default::default(),
                #[cfg(not(target_arch = "wasm32"))]
                milkdrop: Default::default(),
                #[cfg(not(target_arch = "wasm32"))]
                link_import: Default::default(),
                #[cfg(not(target_arch = "wasm32"))]
                online: Default::default(),
                #[cfg(not(target_arch = "wasm32"))]
                feeds: Default::default(),
                #[cfg(not(target_arch = "wasm32"))]
                ofl: Default::default(),
                #[cfg(not(target_arch = "wasm32"))]
                ai_shader: Default::default(),
                clip_page: 0,
                media_memory_at: 0.0,
                #[cfg(not(target_arch = "wasm32"))]
                convert: Default::default(),
                mod_inputs: ModInputs::default(),
                mod_phases: Default::default(),
            },
            dock,
            workspace: Some(area),
            ui_brightness: bench.as_ref().and_then(|b| b.brightness).or_else(|| stored(KEY_BRIGHTNESS).and_then(|s| s.parse().ok())).unwrap_or(theme::DEFAULT_BRIGHTNESS),
            logged_notice: None,
            workspace_docks: area_docks,
            #[cfg(target_os = "macos")]
            native_menu: None,
            title: String::new(),
            show_start: None,
            layouts,
            lock: lock::Lock::from_setting(&stored(KEY_LOCK).unwrap_or_default()),
            panels_locked: stored(KEY_PANELS_LOCKED).is_some_and(|v| v == "1"),
            cloud_layouts_applied: false,
            cloud_layouts_check: 5.0,
            layout_draft: None,
            last_ui: (web_time::Instant::now(), 0.0),
            hidden_pacer: Default::default(),
            ui_frames: 0,
            welcome_open: false,
            #[cfg(not(target_arch = "wasm32"))]
            heartbeat: Default::default(),
            bench_session: bench.is_some(),
            bench: bench.map(|cfg| BenchRun { cfg, started: None, samples: Vec::new(), rebuilt: false, shot_requested: false, tabs_shown: 0 }),
            gather_windows: false,
            remote_published: 0.0,
            #[cfg(not(target_arch = "wasm32"))]
            pending_snapshot: None,
            #[cfg(not(target_arch = "wasm32"))]
            saver: Default::default(),
            recovered: None,
            panics: Vec::new(),
            detached,
        };
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(s) = snapshot {
            app.restore(s);
        }
        app.install_native_menu();
        // A fresh install (nothing saved yet): say hello once.
        app.welcome_open = fresh_install;
        theme::set_accent(accent);
        if app.ui_brightness != theme::DEFAULT_BRIGHTNESS || accent != 0 {
            theme::set_brightness(&cc.egui_ctx, app.ui_brightness);
        }
        if let Some(name) = app.bench.as_ref().and_then(|b| b.cfg.layout.clone()) {
            match super::layout::LAYOUTS.iter().position(|(l, _)| l.eq_ignore_ascii_case(&name)) {
                Some(i) => app.switch_workspace(i),
                None => println!("LAYOUT_ERROR no workspace \"{name}\" (there are: {})", super::layout::LAYOUTS.iter().map(|(l, _)| *l).collect::<Vec<_>>().join(", ")),
            }
        }
        if let Some(rate) = app.bench.as_ref().and_then(|b| b.cfg.rate.clone()) {
            app.state.render_rate = match rate.as_str() {
                "display" => phosphor_engine::RenderRate::Display,
                "30" => phosphor_engine::RenderRate::Fps30,
                _ => phosphor_engine::RenderRate::Fps60,
            };
        }
        // FFGL plugins: found and checked in the background; they appear in Effects / Sources when ready.
        #[cfg(target_os = "macos")]
        app.state.ffgl.start_scan();
        if let Some(name) = app.bench.as_ref().and_then(|b| b.cfg.area_tab.clone())
            && let Some(tab) = Tab::ALL.iter().find(|t| t.title() == name)
        {
            app.show_tab_anywhere(*tab);
        }
        if let Some(name) = app.bench.as_ref().and_then(|b| b.cfg.tab.clone())
            && let Some(tab) = Tab::ALL.iter().find(|t| t.title() == name)
        {
            app.show_tab(*tab);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if app.bench.is_none() && crate::show::bundled().is_some() {
            app.show_start = Some(0);
            app.state.project_path = None;
            // An exported show may open locked (player mode) with its own unlock password.
            if let Some(p) = crate::show::bundled().and_then(|b| b.manifest.lock.clone()) {
                app.lock = lock::Lock { active: true, password: p, ..Default::default() };
            }
        }
        if app.bench.is_none() && !library_default_done && !super::areas::library_open(&app.dock) {
            app.toggle_library();
        }
        // A project double-clicked in Explorer (Windows installs associate .phosphor) or given on the command line.
        if app.bench.is_none()
            && let Some(path) = std::env::args().nth(1).filter(|a| !a.starts_with("--") && a.to_lowercase().ends_with(".phosphor"))
        {
            app.state.open_project_file(&path);
        }
        if app.bench.is_none() {
            app.state.outputs_on_open();
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(p) = app.bench.as_ref().and_then(|b| b.cfg.import.clone()) {
            app.state.begin_resolume_import(&p);
        }
        if app.bench.as_ref().is_some_and(|b| b.cfg.show_clock) {
            app.state.cue_run.clock.toggle(now);
        }
        if open_audio && let Err(e) = app.state.audio.open(None) {
            eprintln!("audio input: {e}");
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(dir) = app.bench.as_ref().and_then(|b| b.cfg.watch.clone()) {
            app.state.folders.add(dir.clone());
            // (PHOSPHOR_MEDIA_DIR = a folder inside it to open in Media, for pictures of the panel.)
            let at = std::env::var("PHOSPHOR_MEDIA_DIR").ok().filter(|d| !d.is_empty()).unwrap_or_else(|| dir.clone());
            app.state.media_view.show(crate::panels::MediaSource::Folder(dir), Some(at));
        }
        // (PHOSPHOR_NOTICE = a message to show in the bar, for pictures of it.)
        if app.bench.is_some() && let Ok(n) = std::env::var("PHOSPHOR_NOTICE") {
            app.state.notice = Some(n);
        }
        if let Some(pw) = app.bench.as_ref().and_then(|b| b.cfg.remote_password.clone()) {
            app.state.remote_password = pw;
            app.state.require_password = true;
        }
        // Listen for other Phosphors from the start, so the Devices panel has them when opened.
        #[cfg(not(target_arch = "wasm32"))]
        crate::control::discovery::players();
        if app.bench.as_ref().is_some_and(|b| b.cfg.ndi) {
            #[cfg(any(target_os = "macos", windows))]
            crate::ndi::sources();
            app.state.comp.ndi.enabled = true;
        }
        // A player follows its leader from the start (its outputs open on their screens as usual).
        #[cfg(not(target_arch = "wasm32"))]
        if !app.state.follow_at_launch.trim().is_empty() {
            app.state.following = Some(crate::follow::Link::connect(&app.state.follow_at_launch.clone()));
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(addr) = app.bench.as_ref().and_then(|b| b.cfg.follow.clone()) {
            app.state.following = Some(crate::follow::Link::connect(&addr));
        }
        if app.bench.as_ref().is_some_and(|b| b.cfg.net) {
            if let Err(e) = app.state.net.start() {
                eprintln!("net: {e}");
            }
            app.state.audio.use_network();
        }
        if let Some(url) = app.bench.as_ref().and_then(|b| b.cfg.stream.clone()) {
            let s = &mut app.state.stream.settings;
            // http(s) addresses are WebRTC (WHIP) endpoints; anything else RTMP (or a file path).
            if url.starts_with("http") {
                s.destination = crate::stream::Destination::Whip;
                s.whip_url = url.clone();
            } else {
                s.destination = crate::stream::Destination::CustomRtmp;
            }
            s.rtmp_url = url;
            s.stream_key = String::new();
            s.height = 0;
            s.fps = 30;
            s.program_sound = app.bench.as_ref().is_some_and(|b| b.cfg.movie_sound);
            if let Some(u2) = app.bench.as_ref().and_then(|b| b.cfg.stream2.clone()) {
                s.second_enabled = true;
                s.second_destination = crate::stream::Destination::CustomRtmp;
                s.second_url = u2;
                s.second_key = String::new();
            }
            if let Some(c) = app.bench.as_ref().and_then(|b| b.cfg.stream_copy.clone()) {
                s.copy_enabled = true;
                s.copy_path = c;
            }
            let (w, h) = (app.state.comp.width, app.state.comp.height);
            if let Err(e) = app.state.stream.start(w, h, now) {
                eprintln!("stream: {e}");
            }
        }
        // Bench: more recorders, PHOSPHOR_BENCH_RECORDERS="file[@layer index];…" (no index = the program).
        if app.bench.is_some()
            && let Ok(list) = std::env::var("PHOSPHOR_BENCH_RECORDERS")
        {
            app.state.recorders.clear();
            for item in list.split(';').filter(|s| !s.is_empty()) {
                let (path, layer) = item.split_once('@').map_or((item, None), |(p, l)| (p, l.parse::<usize>().ok()));
                app.state.recorder_add();
                let i = app.state.recorders.len() - 1;
                let source = layer.and_then(|k| app.state.comp.layers.get(k)).map_or(0, |l| l.id);
                let s = &mut app.state.recorders[i].settings;
                (s.file_path, s.source) = (path.to_string(), source);
                app.state.recorder_toggle(i, now);
            }
        }
        if let Some(path) = app.bench.as_ref().and_then(|b| b.cfg.render.clone()) {
            let secs = std::env::var("PHOSPHOR_RENDER_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(2.0);
            let setup = super::offline::OfflineSetup { path, secs, ..Default::default() };
            app.state.offline_start(&setup, now);
        }
        if let Some(path) = app.bench.as_ref().and_then(|b| b.cfg.record.clone()) {
            app.state.stream.settings.destination = crate::stream::Destination::File;
            app.state.stream.settings.file_path = path;
            app.state.stream.settings.height = 0;
            app.state.stream.settings.program_sound = app.bench.as_ref().is_some_and(|b| b.cfg.movie_sound);
            if app.bench.as_ref().is_some_and(|b| b.cfg.hap) {
                use crate::stream::FileFormat as F;
                app.state.stream.settings.file_format = match app.bench.as_ref().and_then(|b| b.cfg.hap_kind.as_deref()) {
                    Some("alpha") => F::HapAlpha,
                    Some("q") => F::HapQ,
                    Some("qalpha") => F::HapQAlpha,
                    Some("r") => F::HapR,
                    _ => F::Hap,
                };
            }
            if app.bench.as_ref().is_some_and(|b| b.cfg.hevc) {
                app.state.stream.settings.file_format = crate::stream::FileFormat::Hevc;
            }
            // Record one layer on its own (bench tests): PHOSPHOR_RECORD_LAYER=index, 0 = the bottom layer.
            if let Some(l) = std::env::var("PHOSPHOR_RECORD_LAYER").ok().and_then(|v| v.parse::<usize>().ok()).and_then(|i| app.state.comp.layers.get(i)) {
                app.state.stream.settings.source = l.id;
            }
            let (w, h) = (app.state.comp.width, app.state.comp.height);
            if let Err(e) = app.state.stream.start(w, h, now) {
                eprintln!("record: {e}");
            }
        }
        app
    }
}
