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
                recovered.or(show).or_else(|| stored(KEY_COMPOSITION).and_then(|json| Composition::from_json(&json).ok())).unwrap_or_else(Composition::starter)
            }
        };
        let stream_settings = stored(KEY_STREAM).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let mut detached: Vec<Tab> = stored(KEY_DETACHED).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let mut dock = stored(KEY_DOCK).and_then(|s| from_text(&s)).unwrap_or_else(default_layout);
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
        crate::media::video::sound::set_enabled(bench.as_ref().is_none_or(|b| b.sound));
        #[cfg(not(target_arch = "wasm32"))]
        crate::media::video::set_gpu_hap(engine.as_ref().is_some_and(|e| e.compressed_supported()) && std::env::var_os("PHOSPHOR_CPU_HAP").is_none());
        let now = cc.egui_ctx.input(|i| i.time);

        let open_audio = bench.as_ref().is_some_and(|b| b.audio);
        let fresh_install = bench.is_none() && stored(KEY_COMPOSITION).is_none();
        let mut app = Self {
            state: State {
                selected: comp.layers.last().map(|l| l.id),
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
                bug: crate::support::BugReport::new(),
                #[cfg(not(target_arch = "wasm32"))]
                rhythm: Default::default(),
                updater: crate::update::Updater::with_skipped(stored(KEY_UPDATE_SKIP).unwrap_or_default()),
                remote_password: stored(KEY_REMOTE_PASSWORD).unwrap_or_default(),
                render_rate: stored(KEY_RENDER_RATE).and_then(|t| layout::from_text(&t)).unwrap_or_default(),
                render_pacer: Default::default(),
                require_password: stored(KEY_REMOTE_PASSWORD).is_some_and(|p| !p.is_empty()),
                #[cfg(not(target_arch = "wasm32"))]
                dialogs: Default::default(),
                #[cfg(not(target_arch = "wasm32"))]
                folders: crate::folders::FolderWatcher::new(stored(KEY_WATCHED).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()),
                explorer: crate::explorer::ExplorerState {
                    favorites: stored(KEY_FAVORITES).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
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
                fx_browser: None,
                monitors: stored(KEY_MONITORS).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
                monitor_wants: Vec::new(),
                monitor_keys: Vec::new(),
                monitor_textures: Vec::new(),
                displays: crate::screens::displays(),
                new_monitor: None,
                snapshot_requested: false,
                #[cfg(not(target_arch = "wasm32"))]
                isf: crate::isf_lib::IsfLibrary::scan(),
                fx_clipboard: Vec::new(),
                thumbs: Thumbs::new(),
                media_view: Default::default(),
                map_mode: false,
                fx_fades: Vec::new(),
                project_path: stored(KEY_PROJECT_PATH).filter(|p| !p.is_empty()),
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
                #[cfg(windows)]
                spout: Default::default(),
                clock: Clock::new(comp_bpm),
                pending_triggers: Vec::new(),
                autopilot: HashMap::new(),
                cue_run: Default::default(),
                #[cfg(not(target_arch = "wasm32"))]
                link: None,
                #[cfg(not(target_arch = "wasm32"))]
                avc_import: None,
                #[cfg(not(target_arch = "wasm32"))]
                relink: None,
                #[cfg(not(target_arch = "wasm32"))]
                dmx: Default::default(),
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
            cloud_layouts_applied: false,
            cloud_layouts_check: 5.0,
            layout_draft: None,
            last_ui: (web_time::Instant::now(), 0.0),
            hidden_pacer: Default::default(),
            ui_frames: 0,
            welcome_open: false,
            #[cfg(not(target_arch = "wasm32"))]
            heartbeat: Default::default(),
            bench: bench.map(|cfg| BenchRun { cfg, started: None, samples: Vec::new(), rebuilt: false, shot_requested: false }),
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
        if app.ui_brightness != theme::DEFAULT_BRIGHTNESS {
            theme::set_brightness(&cc.egui_ctx, app.ui_brightness);
        }
        if let Some(i) = app.bench.as_ref().and_then(|b| b.cfg.layout.as_deref()).and_then(|n| super::layout::LAYOUTS.iter().position(|(l, _)| *l == n)) {
            app.switch_workspace(i);
        }
        if let Some(rate) = app.bench.as_ref().and_then(|b| b.cfg.rate.clone()) {
            app.state.render_rate = match rate.as_str() {
                "display" => phosphor_engine::RenderRate::Display,
                "30" => phosphor_engine::RenderRate::Fps30,
                _ => phosphor_engine::RenderRate::Fps60,
            };
        }
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
            app.state.folders.add(dir);
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
        if let Some(path) = app.bench.as_ref().and_then(|b| b.cfg.record.clone()) {
            app.state.stream.settings.destination = crate::stream::Destination::File;
            app.state.stream.settings.file_path = path;
            app.state.stream.settings.height = 0;
            if app.bench.as_ref().is_some_and(|b| b.cfg.hap) {
                app.state.stream.settings.file_format = crate::stream::FileFormat::Hap;
            }
            if app.bench.as_ref().is_some_and(|b| b.cfg.hevc) {
                app.state.stream.settings.file_format = crate::stream::FileFormat::Hevc;
            }
            let (w, h) = (app.state.comp.width, app.state.comp.height);
            if let Err(e) = app.state.stream.start(w, h, now) {
                eprintln!("record: {e}");
            }
        }
        app
    }
}
