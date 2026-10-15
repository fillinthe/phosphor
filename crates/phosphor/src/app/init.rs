//! Startup: restores settings, recovers after a crash, opens an exported show.

use super::*;

impl PhosphorApp {
    pub fn new(cc: &eframe::CreationContext<'_>, bench: Option<BenchConfig>) -> Self {
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
        let known: Vec<Tab> = stored(KEY_KNOWN_TABS).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        ensure_new_tabs(&mut dock, &known);

        let comp_bpm = comp.bpm;
        let render_state = cc.wgpu_render_state.clone();
        let engine = render_state.as_ref().map(|rs| Engine::new(rs.device.clone(), rs.queue.clone()));
        let now = cc.egui_ctx.input(|i| i.time);

        let open_audio = bench.as_ref().is_some_and(|b| b.audio);
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
                presets: PresetStore::default(),
                preset_name: String::new(),
                notice: None,
                audio: AudioInput::new(),
                control: ControlHub::new(),
                listen_for: None,
                map_draft: None,
                map_search: String::new(),
                stream: Streamer::new(stream_settings),
                stream_audio_devices: Vec::new(),
                net: NetSession::new(),
                net_audio: Vec::new(),
                remote_password: stored(KEY_REMOTE_PASSWORD).unwrap_or_default(),
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
                recent: stored(KEY_RECENT_PROJECTS).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
                history: Default::default(),
                export: Default::default(),
                #[cfg(target_os = "macos")]
                syphon: Default::default(),
                #[cfg(target_os = "macos")]
                ndi: Default::default(),
                clock: Clock::new(comp_bpm),
                pending_triggers: Vec::new(),
                clip_page: 0,
                mod_inputs: ModInputs::default(),
            },
            dock,
            #[cfg(target_os = "macos")]
            native_menu: None,
            title: String::new(),
            show_start: None,
            layouts,
            layout_draft: None,
            last_ui: (web_time::Instant::now(), 0.0),
            hidden_pacer: Default::default(),
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
        if let Some(name) = app.bench.as_ref().and_then(|b| b.cfg.tab.clone())
            && let Some(tab) = Tab::ALL.iter().find(|t| t.title() == name)
        {
            app.show_tab(*tab);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if app.bench.is_none() && crate::show::bundled().is_some() {
            app.show_start = Some(0);
            app.state.project_path = None;
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
        if app.bench.as_ref().is_some_and(|b| b.cfg.ndi) {
            app.state.comp.ndi.enabled = true;
        }
        if app.bench.as_ref().is_some_and(|b| b.cfg.net) {
            if let Err(e) = app.state.net.start() {
                eprintln!("net: {e}");
            }
            app.state.audio.use_network();
        }
        if let Some(path) = app.bench.as_ref().and_then(|b| b.cfg.record.clone()) {
            app.state.stream.settings.destination = crate::stream::Destination::File;
            app.state.stream.settings.file_path = path;
            app.state.stream.settings.height = 0;
            let (w, h) = (app.state.comp.width, app.state.comp.height);
            if let Err(e) = app.state.stream.start(w, h, now) {
                eprintln!("record: {e}");
            }
        }
        app
    }
}
