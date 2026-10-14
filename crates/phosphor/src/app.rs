//! Top-level app: owns the composition, engine, dockable layout, and output window.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32};
use eframe::egui_wgpu;
use egui_dock::{DockArea, DockState, NodeIndex};
use egui_phosphor::regular as icon;
use phosphor_engine::{Composition, Engine, Mapping, ModInputs, wgpu};

use crate::audio::AudioInput;
use crate::bench::BenchConfig;
use crate::clock::Clock;
use crate::control::ControlHub;
use crate::media::{self, MediaManager};
use crate::net::NetSession;
use crate::panels::{self, Tab, Viewer};
use crate::presets::PresetStore;
use crate::stats::{Snapshot, Stats};
use crate::stream::Streamer;
use crate::thumbs::Thumbs;
use crate::theme;

const KEY_COMPOSITION: &str = "composition";
const KEY_DOCK: &str = "dock";
const KEY_STREAM: &str = "stream";
const KEY_KNOWN_TABS: &str = "known_tabs";
const KEY_DETACHED: &str = "detached";
const KEY_FAVORITES: &str = "favorites";
const KEY_MONITORS: &str = "monitors";
const KEY_RECENT: &str = "recent_effects";
const KEY_REMOTE_PASSWORD: &str = "remote_password";
const KEY_RECENT_PROJECTS: &str = "recent_projects";
/// In-window menu clicks, queued for `menu_actions`.
pub const MENU_REQUEST: &str = "phosphor_menu_request";
const KEY_PROJECT_PATH: &str = "project_path";
const KEY_WATCHED: &str = "watched_folders";

/// State one output window (a separate OS window) reads each frame.
#[derive(Default)]
pub struct OutputShared {
    pub open: bool,
    pub name: String,
    /// 1-based position in the outputs list, shown by Identify.
    pub number: usize,
    /// Show the big Identify overlay until this time.
    pub identify_until: f64,
    pub texture: Option<egui::TextureId>,
    pub aspect: f32,
    /// Copy of the composition's mapping; the output window edits it when `edit_mapping` is on.
    pub mapping: Mapping,
    pub edit_mapping: bool,
    pub mapping_changed: bool,
    pub selected_surface: Option<u64>,
    /// Display the window was sent to, and whether it's fullscreen (for crash recovery).
    pub display: Option<u32>,
    pub fullscreen: bool,
    /// Move the output window to this display and go fullscreen: (display, step, frames waited).
    pub place: Option<(crate::screens::Display, u8, u32)>,
}

/// A launch: a clip, a whole column, or any new source on a layer. Fired now, or on the next
/// beat/bar, per the launch settings.
#[derive(Clone, Debug, PartialEq)]
pub enum Trigger {
    Clip { layer: u64, slot: usize },
    Column(usize),
    Source { layer: u64, source: phosphor_engine::SourceSpec, values: Vec<f32>, name: String },
}

/// Everything panels can read and edit. Kept separate from the dock layout so both can be
/// borrowed mutably at once.
pub struct State {
    pub comp: Composition,
    pub engine: Option<Engine>,
    render_state: Option<egui_wgpu::RenderState>,
    pub texture: Option<egui::TextureId>,
    texture_generation: u64,
    /// Per-layer thumbnail textures, keyed by layer id, with the size they were registered at.
    pub layer_textures: HashMap<u64, (egui::TextureId, (u32, u32))>,
    pub media: MediaManager,
    pub stats: Stats,
    pub selected: Option<u64>,
    pub selected_surface: Option<u64>,
    /// Output windows by output id.
    pub output: Arc<Mutex<std::collections::HashMap<u64, OutputShared>>>,
    output_textures: HashMap<u64, (egui::TextureId, (u32, u32))>,
    /// Output being edited in the Mapping panel.
    pub mapping_output: u64,
    pub presets: PresetStore,
    pub preset_name: String,
    /// A message worth showing the user (save failed, bad preset, …).
    pub notice: Option<String>,
    pub audio: AudioInput,
    pub control: ControlHub,
    pub learn_pick: Option<phosphor_engine::ControlTarget>,
    pub stream: Streamer,
    pub stream_audio_devices: Vec<(i32, String)>,
    pub net: NetSession,
    net_audio: Vec<f32>,
    /// Password phones need to control (viewing never needs it). Empty = no password.
    pub remote_password: String,
    pub require_password: bool,
    #[cfg(not(target_arch = "wasm32"))]
    pub dialogs: crate::dialogs::Dialogs,
    #[cfg(not(target_arch = "wasm32"))]
    pub folders: crate::folders::FolderWatcher,
    pub explorer: crate::explorer::ExplorerState,
    pub editor: crate::explorer::EditorState,
    pub palette: crate::explorer::PaletteState,
    /// Previews requested by panels this frame (key, item); rendered on the next frame.
    pub preview_wants: Vec<(String, phosphor_engine::PreviewItem, bool)>,
    preview_keys: Vec<String>,
    /// When stills were last refreshed.
    preview_refreshed: f64,
    preview_textures: Vec<(egui::TextureId, (u32, u32))>,
    /// A panel to bring to the front (set by explorer actions and the palette).
    pub open_tab: Option<Tab>,
    pub fx_browser: Option<crate::explorer::FxBrowser>,
    pub monitors: std::collections::HashMap<u8, panels::MonitorCfg>,
    /// Monitor pictures requested this frame: (monitor number, source, width, height).
    pub monitor_wants: Vec<(u8, u64, u32, u32)>,
    monitor_keys: Vec<u8>,
    monitor_textures: Vec<(egui::TextureId, (u32, u32))>,
    pub displays: Vec<crate::screens::Display>,
    /// Set by panels: open a new monitor with this source.
    pub new_monitor: Option<panels::MonitorSource>,
    pub snapshot_requested: bool,
    #[cfg(not(target_arch = "wasm32"))]
    pub isf: crate::isf_lib::IsfLibrary,
    /// Effect chain copied with the Copy button, for pasting onto another layer or MASTER.
    pub fx_clipboard: Vec<phosphor_engine::EffectState>,
    pub thumbs: Thumbs,
    pub media_view: panels::MediaView,
    /// Map mode: click any control, then move a knob / send OSC / press a key to link them.
    pub map_mode: bool,
    /// Effects fading in after being added: (effect id, start time, seconds, final mix).
    pub fx_fades: Vec<(u64, f32, f32, f32)>,
    /// The project file being worked on (Save writes here).
    pub project_path: Option<String>,
    pub recent: Vec<String>,
    pub history: crate::history::History,
    pub export: ExportUi,
    #[cfg(target_os = "macos")]
    pub syphon: crate::syphon::Syphon,
    #[cfg(target_os = "macos")]
    pub ndi: crate::ndi::Ndi,
    pub clock: Clock,
    pub pending_triggers: Vec<Trigger>,
    /// Which bank of clip columns the grid and the number keys address.
    pub clip_page: usize,
    /// This frame's audio/LFO signals, also shown in the Audio panel.
    pub mod_inputs: ModInputs,
}

impl State {
    pub fn aspect(&self) -> f32 {
        self.comp.width as f32 / self.comp.height.max(1) as f32
    }

    pub fn add_layer(&mut self) -> u64 {
        let id = self.comp.add_layer().id;
        self.selected = Some(id);
        id
    }

    /// A monitor's picture (from the previous frame).
    pub fn monitor_tex(&self, n: u8) -> Option<egui::TextureId> {
        let i = self.monitor_keys.iter().position(|k| *k == n)?;
        self.monitor_textures.get(i).map(|(t, _)| *t)
    }

    /// A rendered explorer/editor preview by key (from the previous frame).
    pub fn preview_tex(&self, key: &str) -> Option<egui::TextureId> {
        let i = self.preview_keys.iter().position(|k| k == key)?;
        self.preview_textures.get(i).map(|(t, _)| *t)
    }

    /// Remembers the project file (title, Save, Open Recent).
    pub fn set_project_path(&mut self, path: &str) {
        self.project_path = Some(path.to_string());
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(12);
    }

    /// Writes the project to its file, or asks where when it has none yet.
    pub fn save_project(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        match &self.project_path {
            Some(p) => match std::fs::write(p, self.comp.to_json()) {
                Ok(()) => self.notice = Some(format!("Saved {}", std::path::Path::new(p).file_name().map_or(p.clone(), |n| n.to_string_lossy().into_owned()))),
                Err(e) => self.notice = Some(format!("Save failed: {e}")),
            },
            None => self.save_project_as(),
        }
    }

    pub fn save_project_as(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let name = self
                .project_path
                .as_deref()
                .and_then(|p| std::path::Path::new(p).file_name())
                .map_or("Untitled.phosphor".to_string(), |n| n.to_string_lossy().into_owned());
            self.dialogs.save_file(crate::dialogs::Purpose::SaveProject, ("Phosphor project", &["phosphor"]), &name);
        }
    }

    pub fn open_project_file(&mut self, path: &str) {
        match std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|j| Composition::from_json(&j).map_err(|e| e.to_string())) {
            Ok(comp) => {
                self.load_composition(comp);
                self.set_project_path(path);
            }
            Err(e) => {
                self.notice = Some(format!("Open failed: {e}"));
                self.recent.retain(|p| p != path);
            }
        }
    }

    pub fn load_composition(&mut self, comp: Composition) {
        self.selected = comp.layers.last().map(|l| l.id);
        self.comp = comp;
        self.comp.ensure_outputs();
        self.notice = None;
    }

    /// Fires a trigger now, or queues it for the next beat when quantise is on.
    pub fn trigger(&mut self, t: Trigger, now: f64) {
        if self.comp.launch.timing == phosphor_engine::LaunchTiming::Now {
            self.fire(t, now as f32);
            return;
        }
        // A newer source for the same layer replaces one still waiting for the beat.
        if let Trigger::Source { layer, .. } = &t {
            let l = *layer;
            self.pending_triggers.retain(|p| !matches!(p, Trigger::Source { layer, .. } if *layer == l));
        }
        if !self.pending_triggers.contains(&t) {
            self.pending_triggers.push(t);
        }
    }

    /// Carries out an app-wide control from a controller (value 0..1, or `control::FLIP`).
    pub fn app_control(&mut self, a: phosphor_engine::AppControl, v: f32, now: f64) {
        use crate::control::{FLIP, unit_to};
        use phosphor_engine::AppControl as A;
        let press = v > 0.5;
        match a {
            A::Tempo => self.clock.bpm = unit_to(v, 40.0, 300.0, false),
            A::TapTempo if press => self.clock.tap(now),
            A::Resync if press => self.clock.resync(now),
            A::FollowAudio => self.clock.follow_audio = if v == FLIP { !self.clock.follow_audio } else { press },
            A::AudioGain => self.audio.gain = unit_to(v, 0.1, 10.0, true),
            A::KickThreshold => self.audio.beats.sensitivity = unit_to(v, 1.1, 3.0, false),
            _ => {}
        }
    }

    /// Puts a source on a layer using the launch settings (cut/fade, now/beat/bar).
    pub fn launch_source(&mut self, layer: u64, source: phosphor_engine::SourceSpec, values: Vec<f32>, name: String, now: f64) {
        self.trigger(Trigger::Source { layer, source, values, name }, now);
    }

    fn fire(&mut self, t: Trigger, now: f32) {
        match t {
            Trigger::Clip { layer, slot } => self.comp.trigger_clip(layer, slot, now),
            Trigger::Column(col) => self.comp.trigger_column(col, now),
            Trigger::Source { layer, source, values, name } => self.comp.set_layer_source(layer, source, values, name, now),
        }
    }

    pub fn primary_output(&self) -> u64 {
        self.comp.outputs.first().map_or(0, |o| o.id)
    }

    /// Opens the first output on `display`, fullscreen.
    pub fn send_output_to(&mut self, display: crate::screens::Display) {
        let id = self.primary_output();
        self.send_output_id_to(id, display);
    }

    pub fn send_output_id_to(&mut self, id: u64, display: crate::screens::Display) {
        let mut map = self.output.lock().unwrap();
        let out = map.entry(id).or_default();
        out.open = true;
        out.display = Some(display.id);
        out.place = Some((display, 0, 0));
    }

    /// Opens every enabled output's window, or closes them all if any is open.
    pub fn toggle_output(&mut self) {
        let mut map = self.output.lock().unwrap();
        let any = map.values().any(|o| o.open);
        for o in self.comp.outputs.iter().filter(|o| o.enabled) {
            map.entry(o.id).or_default().open = !any;
        }
        if any {
            map.values_mut().for_each(|o| o.open = false);
        }
    }

    pub fn output_open(&self) -> bool {
        self.output.lock().unwrap().values().any(|o| o.open)
    }

    pub fn output_window_open(&self, id: u64) -> bool {
        self.output.lock().unwrap().get(&id).is_some_and(|o| o.open)
    }

    pub fn toggle_output_window(&mut self, id: u64) {
        let mut map = self.output.lock().unwrap();
        let o = map.entry(id).or_default();
        o.open = !o.open;
    }

    /// Flashes each output's number on its window for a few seconds.
    pub fn identify_outputs(&mut self, now: f64) {
        for o in self.output.lock().unwrap().values_mut() {
            o.identify_until = now + 3.0;
        }
    }

    pub fn output_tex(&self, id: u64) -> Option<egui::TextureId> {
        self.output_textures.get(&id).map(|(t, _)| *t)
    }
}

struct BenchRun {
    cfg: BenchConfig,
    started: Option<f64>,
    samples: Vec<Snapshot>,
    /// Rebuilt the composition once imported (ISF) shaders were registered.
    rebuilt: bool,
}

/// The Export Show window.
#[derive(Default)]
pub struct ExportUi {
    pub open: bool,
    pub name: String,
    pub zip: bool,
    pub autostart: bool,
    #[cfg(not(target_arch = "wasm32"))]
    pub progress: Option<crate::export::Shared>,
}

pub struct PhosphorApp {
    state: State,
    #[cfg(target_os = "macos")]
    native_menu: Option<(muda::Menu, Vec<crate::menu::Action>)>,
    title: String,
    /// Exported show: open its outputs on the first frames.
    show_start: Option<u32>,
    dock: DockState<Tab>,
    bench: Option<BenchRun>,
    gather_windows: bool,
    remote_published: f64,
    #[cfg(not(target_arch = "wasm32"))]
    pending_snapshot: Option<(u32, u32, Vec<u8>)>,
    #[cfg(not(target_arch = "wasm32"))]
    saver: crate::recovery::Saver,
    /// Shown after restoring from an unexpected quit; `true` if a stream was live.
    recovered: Option<bool>,
    /// Panics caught in the last few seconds (UI keeps running instead of closing).
    panics: Vec<f64>,
    /// Panels living in their own OS windows.
    detached: Vec<Tab>,
}

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
        let mut dock = stored(KEY_DOCK).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(default_layout);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(s) = &snapshot {
            if let Some(d) = s.dock.clone().and_then(|v| serde_json::from_value(v).ok()) {
                dock = d;
            }
            detached = s.detached.clone();
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
                learn_pick: None,
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
            bench: bench.map(|cfg| BenchRun { cfg, started: None, samples: Vec::new(), rebuilt: false }),
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
        #[cfg(target_os = "macos")]
        if app.bench.is_none() {
            let panels: Vec<String> = Tab::ALL.iter().map(|t| t.title().to_string()).collect();
            let layouts: Vec<&str> = LAYOUTS.iter().map(|(n, _)| *n).collect();
            app.native_menu = crate::menu::native::install(&panels, &layouts);
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

    /// Advances media, renders the frame, and keeps the preview + thumbnail textures registered.
    fn render(&mut self, now: f64) {
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
            stream,
            clock,
            net,
            net_audio,
            preview_wants,
            preview_keys,
            preview_textures,
            preview_refreshed,
            selected,
            monitor_wants,
            monitor_keys,
            monitor_textures,
            output_textures,
            #[cfg(target_os = "macos")]
            syphon,
            #[cfg(target_os = "macos")]
            ndi,
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
        let an = audio.update();
        if an.beat {
            clock.on_audio_beat(now, an.bpm, an.confidence);
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
        };
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
        let t0 = web_time::Instant::now();
        let info = engine.render(comp, now as f32, mod_inputs);
        let render_secs = t0.elapsed().as_secs_f64();
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
        #[cfg(target_os = "macos")]
        let ndi_due = {
            ndi.sync(comp.ndi.enabled, &comp.ndi.name);
            ndi.frame_due(now, comp.ndi.fps)
        };
        #[cfg(not(target_os = "macos"))]
        let ndi_due = false;
        if (stream_due || net_due || ndi_due) && !engine.capture_frame() && stream_due {
            stream.repeat_last();
        }
        for f in engine.poll_captures() {
            #[cfg(target_os = "macos")]
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
            for layer in &comp.layers {
                let Some(t) = engine.layer_thumbnail(layer.id) else { continue };
                let key = (t.width, t.height);
                match layer_textures.get_mut(&layer.id) {
                    Some((_, k)) if *k == key => {}
                    Some((id, k)) => {
                        renderer.update_egui_texture_from_wgpu_texture(&rs.device, &t.view, linear, *id);
                        *k = key;
                    }
                    None => {
                        let id = renderer.register_native_texture(&rs.device, &t.view, linear);
                        layer_textures.insert(layer.id, (id, key));
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
                let live = comp.layers.iter().any(|l| l.id == *id);
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

        if let Some(snapshot) = stats.record(now, render_secs, info.passes, mod_inputs.audio_level as f64)
            && let Some(run) = &mut self.bench
        {
            println!("{}", snapshot.to_json());
            run.samples.push(snapshot);
        }
    }

    /// Mapped targets for the UI badges, Learn/Forget requests from right-click menus, and
    /// keyboard keys as controller input.
    fn control_requests(&mut self, ctx: &egui::Context) {
        let st = &mut self.state;
        let mapped: panels::MappedList = std::sync::Arc::new(st.comp.control_maps.iter().map(|m| (m.target.clone(), m.source.label())).collect());
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(panels::MAPPED_TARGETS), mapped));
        if let Some((target, source)) = ctx.data_mut(|d| {
            let id = egui::Id::new(panels::MAP_REQUEST);
            let t = d.get_temp::<(phosphor_engine::ControlTarget, phosphor_engine::ControlSource)>(id);
            d.remove::<(phosphor_engine::ControlTarget, phosphor_engine::ControlSource)>(id);
            t
        }) {
            st.comp.control_maps.retain(|m| m.target != target);
            st.comp.control_maps.push(phosphor_engine::ControlMap { source, target });
        }
        if let Some(t) = ctx.data_mut(|d| {
            let id = egui::Id::new(panels::LEARN_REQUEST);
            let t = d.get_temp::<phosphor_engine::ControlTarget>(id);
            d.remove::<phosphor_engine::ControlTarget>(id);
            t
        }) {
            st.control.learning = Some(t);
        }
        if let Some(t) = ctx.data_mut(|d| {
            let id = egui::Id::new(panels::UNLEARN_REQUEST);
            let t = d.get_temp::<phosphor_engine::ControlTarget>(id);
            d.remove::<phosphor_engine::ControlTarget>(id);
            t
        }) {
            st.comp.control_maps.retain(|m| m.target != t);
        }
        // Keys act as controller buttons (ignored while typing in a text field).
        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let keys: Vec<(String, bool)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key { key, pressed, repeat: false, modifiers, .. } if !modifiers.command && *key != egui::Key::Escape => {
                        Some((key.name().to_string(), *pressed))
                    }
                    _ => None,
                })
                .collect()
        });
        for (key, pressed) in keys {
            let source = phosphor_engine::ControlSource::Key { key };
            let mapped = st.comp.control_maps.iter().any(|m| m.source == source);
            // Unmapped key releases don't need to reach the hub; presses do (for Learn).
            if pressed || mapped {
                st.control.inject(&mut st.comp, crate::control::Incoming { source, value: if pressed { 1.0 } else { 0.0 } });
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            // Esc cancels the control being learned, then leaves Map mode.
            if st.control.learning.take().is_none() {
                st.map_mode = false;
            }
        }
    }

    fn learning_banner(&mut self, ctx: &egui::Context) {
        if self.state.map_mode && self.state.control.learning.is_none() {
            egui::Area::new(egui::Id::new("map_banner")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 44.0)).show(ctx, |ui| {
                egui::Frame::popup(ui.style()).fill(egui::Color32::from_rgb(14, 34, 40)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("{} MAP MODE", icon::PLUG)).strong().color(theme::ACCENT));
                        ui.label("Click any highlighted control, then move a knob, send OSC, or press a key.");
                        if ui.small_button("Done (Esc)").clicked() {
                            self.state.map_mode = false;
                        }
                    });
                });
            });
            return;
        }
        let Some(t) = self.state.control.learning.clone() else { return };
        let name = crate::control::target_label(&self.state.comp, &t);
        egui::Area::new(egui::Id::new("learn_banner")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 44.0)).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).fill(egui::Color32::from_rgb(40, 34, 14)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("{} Learning: {name}", icon::BROADCAST)).color(egui::Color32::from_rgb(255, 205, 90)));
                    ui.weak("move a knob, send OSC, or press a key");
                    if ui.small_button("Cancel (Esc)").clicked() {
                        self.state.control.learning = None;
                    }
                });
            });
        });
    }

    /// Grabs the output now; the save dialog opens as a sheet (the show keeps running).
    #[cfg(not(target_arch = "wasm32"))]
    fn save_snapshot(&mut self) {
        let Some(engine) = &self.state.engine else { return };
        self.pending_snapshot = engine.read_output();
        self.state.dialogs.save_file(crate::dialogs::Purpose::SaveSnapshot, ("PNG", &["png"]), "Phosphor snapshot.png");
    }

    /// Answers from file dialogs, and files found in watched folders.
    #[cfg(not(target_arch = "wasm32"))]
    fn handle_dialogs_and_folders(&mut self, now: f64) {
        use crate::dialogs::Purpose;
        let name_of = |p: &std::path::Path| p.file_name().map_or_else(|| p.to_string_lossy().to_string(), |n| n.to_string_lossy().to_string());
        for (purpose, paths) in self.state.dialogs.poll() {
            let st = &mut self.state;
            match purpose {
                Purpose::LayerSource(id) => {
                    if let Some(p) = paths.first() {
                        st.launch_source(id, crate::media::spec_for_path(&p.to_string_lossy()), Vec::new(), name_of(p), now);
                    }
                }
                Purpose::ImportFiles => {
                    for p in &paths {
                        st.comp.add_media(&name_of(p), &p.to_string_lossy());
                    }
                }
                Purpose::WatchFolder => {
                    if let Some(p) = paths.first() {
                        st.folders.add(p.to_string_lossy().to_string());
                    }
                }
                Purpose::OpenProject => {
                    if let Some(p) = paths.first() {
                        match std::fs::read_to_string(p).map_err(|e| e.to_string()).and_then(|j| Composition::from_json(&j).map_err(|e| e.to_string())) {
                            Ok(comp) => {
                                st.load_composition(comp);
                                st.set_project_path(&p.to_string_lossy());
                            }
                            Err(e) => st.notice = Some(format!("Open failed: {e}")),
                        }
                    }
                }
                Purpose::SaveProject => {
                    if let Some(p) = paths.first() {
                        st.set_project_path(&p.to_string_lossy());
                        st.save_project();
                    }
                }
                Purpose::ExportShow => {
                    if let Some(dest) = paths.first() {
                        let screens = {
                            let map = st.output.lock().unwrap();
                            let displays = crate::screens::displays();
                            map.iter()
                                .filter_map(|(id, o)| {
                                    let d = displays.iter().find(|d| Some(d.id) == o.display)?;
                                    Some((*id, d.name.clone()))
                                })
                                .collect()
                        };
                        let req = crate::export::Request {
                            comp: st.comp.clone(),
                            name: st.export.name.clone(),
                            dest: dest.clone(),
                            zip: st.export.zip,
                            autostart: st.export.autostart,
                            screens,
                        };
                        st.export.progress = Some(crate::export::start(req));
                    }
                }
                Purpose::SaveSnapshot => {
                    if let (Some(p), Some((w, h, px))) = (paths.first().cloned(), self.pending_snapshot.take()) {
                        std::thread::spawn(move || {
                            let _ = image::save_buffer(&p, &px, w, h, image::ColorType::Rgba8);
                        });
                    }
                }
                Purpose::StreamFile => {
                    if let Some(p) = paths.first() {
                        st.stream.settings.file_path = p.to_string_lossy().to_string();
                    }
                }
            }
        }
        if let Some(found) = self.state.folders.poll(now) {
            // Files that vanished from a watched folder leave the bin; new ones join it.
            let folders = self.state.folders.folders.clone();
            let paths: std::collections::HashSet<&str> = found.iter().map(|(_, p, _)| p.as_str()).collect();
            self.state.comp.media_bin.retain(|m| m.folder.is_empty() || !folders.contains(&m.folder) || paths.contains(m.path.as_str()));
            for (name, path, folder) in &found {
                self.state.comp.add_folder_media(name, path, folder);
            }
        }
    }

    fn add_monitor_with(&mut self, source: panels::MonitorSource) {
        let n = self.add_monitor();
        if let Some(cfg) = self.state.monitors.get_mut(&n) {
            cfg.source = source;
        }
    }

    /// Opens a new Monitor panel as a floating window and returns its number.
    fn add_monitor(&mut self) -> u8 {
        let used: Vec<u8> = self.dock.iter_all_tabs().filter_map(|(_, t)| if let Tab::Monitor(n) = t { Some(*n) } else { None }).collect();
        let n = (1..=99).find(|n| !used.contains(n) && !self.detached.contains(&Tab::Monitor(*n))).unwrap_or(99);
        self.state.monitors.entry(n).or_default();
        self.dock.add_window(vec![Tab::Monitor(n)]);
        n
    }

    /// Brings a panel to the front, reopening it if it was closed.
    fn show_tab(&mut self, tab: Tab) {
        if self.detached.contains(&tab) {
            return;
        }
        match self.dock.find_tab(&tab) {
            Some(path) => {
                let _ = self.dock.set_active_tab(path);
            }
            None => {
                self.dock.push_to_focused_leaf(tab);
            }
        }
    }

    fn run_command(&mut self, cmd: crate::explorer::Command, now: f64) {
        use crate::explorer::{Command, PaletteDrag, apply_palette};
        let st = &mut self.state;
        match cmd {
            Command::AddEffect(id) => apply_palette(st, PaletteDrag { effect: Some(id), generator: None }, None),
            Command::SetGenerator(id) => apply_palette(st, PaletteDrag { effect: None, generator: Some(id) }, None),
            Command::OpenTab(tab) => self.show_tab(tab),
            Command::TriggerColumn(c) => st.trigger(Trigger::Column(c), now),
            Command::ToggleOutput => st.toggle_output(),
            Command::ResetLayout => self.dock = default_layout(),
            Command::NewLayer => {
                st.add_layer();
            }
            Command::TapTempo => st.clock.tap(now),
            Command::NewShader(generator) => {
                crate::explorer::new_shader(st, phosphor_engine::custom::template(generator));
                self.show_tab(Tab::ShaderEditor);
            }
            Command::AddMonitor => {
                let _ = self.add_monitor();
            }
            Command::ToggleMapping => {
                let id = st.primary_output();
                if let Some(o) = st.comp.output_mut(id) {
                    o.mapping.enabled = !o.mapping.enabled;
                }
            }
            Command::ToggleSession => {
                if st.net.running() {
                    st.net.stop();
                } else if let Err(e) = st.net.start() {
                    st.notice = Some(e);
                }
            }
        }
    }

    /// Moves a panel out of the dock into its own OS window.
    fn detach(&mut self, tab: Tab) {
        if let Some(path) = self.dock.find_tab(&tab) {
            self.dock.remove_tab(path);
        }
        if !self.detached.contains(&tab) {
            self.detached.push(tab);
        }
    }

    /// Shows every detached panel in its own window; closing the window docks it again.
    fn detached_windows(&mut self, ctx: &egui::Context) {
        let mut redock = Vec::new();
        for tab in self.detached.clone() {
            let id = egui::ViewportId::from_hash_of(("phosphor_panel", tab));
            let builder = egui::ViewportBuilder::default().with_title(format!("Phosphor · {}", tab.title())).with_inner_size([460.0, 640.0]);
            ctx.show_viewport_immediate(id, builder, |ctx, _class| {
                if ctx.input(|i| i.viewport().close_requested()) {
                    redock.push(tab);
                }
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(tab.title()).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Dock back").clicked() {
                                redock.push(tab);
                            }
                        });
                    });
                    ui.separator();
                    let mut tab = tab;
                    egui_dock::TabViewer::ui(&mut Viewer { st: &mut self.state, detach: None }, ui, &mut tab);
                });
            });
        }
        for tab in redock {
            self.detached.retain(|t| *t != tab);
            if self.dock.find_tab(&tab).is_none() {
                self.dock.push_to_focused_leaf(tab);
            }
        }
    }

    /// Keyboard shortcuts that act on the selected layer (ignored while typing in a field).
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let Some(sel) = self.state.selected else { return };
        if sel == phosphor_engine::MASTER_ID {
            return;
        }
        let digit = ctx.input(|i| {
            [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4, egui::Key::Num5, egui::Key::Num6, egui::Key::Num7, egui::Key::Num8, egui::Key::Num9]
                .iter()
                .position(|k| i.key_pressed(*k))
        });
        if let Some(col) = digit {
            let now = ctx.input(|i| i.time);
            let col = col + self.state.clip_page * panels::CLIP_PAGE_SIZE;
            self.state.trigger(Trigger::Column(col), now);
        }
        // ⌘D (duplicate) lives in the Edit menu.
        let (delete, toggle) = ctx.input(|i| (i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace), i.key_pressed(egui::Key::Space)));
        if delete {
            panels::remove_layer(&mut self.state, sel);
        } else if toggle && let Some(l) = self.state.comp.layer_mut(sel) {
            l.enabled = !l.enabled;
        }
    }

    /// Menu bar clicks (system and in-window) and their shortcuts.
    fn menu_actions(&mut self, ctx: &egui::Context, now: f64) {
        let mut actions = crate::menu::pressed(ctx);
        #[cfg(target_os = "macos")]
        if let Some((_, ids)) = &self.native_menu {
            actions.extend(crate::menu::native::poll(ids));
        }
        actions.extend(ctx.data_mut(|d| d.remove_temp::<Vec<crate::menu::Action>>(egui::Id::new(MENU_REQUEST))).unwrap_or_default());
        for a in actions {
            self.run_action(ctx, a, now);
        }
    }

    fn run_action(&mut self, ctx: &egui::Context, a: crate::menu::Action, now: f64) {
        use crate::menu::Action as A;
        let st = &mut self.state;
        let selected_layer = st.selected.filter(|id| *id != phosphor_engine::MASTER_ID && st.comp.layers.iter().any(|l| l.id == *id));
        match a {
            A::NewProject => {
                st.load_composition(Composition::starter());
                st.project_path = None;
                st.notice = Some("New project. Undo (⌘Z) brings the previous one back.".into());
            }
            A::Open => {
                #[cfg(not(target_arch = "wasm32"))]
                st.dialogs.pick_file(crate::dialogs::Purpose::OpenProject, Some(("Phosphor project", &["phosphor"])));
            }
            A::Save => st.save_project(),
            A::SaveAs => st.save_project_as(),
            A::Import => {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let exts = crate::media::dialog_extensions();
                    st.dialogs.pick_files(crate::dialogs::Purpose::ImportFiles, Some(("Media", &exts)));
                }
            }
            A::WatchFolder => {
                #[cfg(not(target_arch = "wasm32"))]
                st.dialogs.pick_folder(crate::dialogs::Purpose::WatchFolder);
            }
            A::Snapshot => {
                #[cfg(not(target_arch = "wasm32"))]
                self.save_snapshot();
            }
            A::ExportShow => {
                let e = &mut st.export;
                e.open = true;
                if e.name.is_empty() {
                    e.name = st
                        .project_path
                        .as_deref()
                        .and_then(|p| std::path::Path::new(p).file_stem())
                        .map_or("My Show".to_string(), |s| s.to_string_lossy().into_owned());
                    e.autostart = true;
                }
            }
            A::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            A::Undo => {
                if let Some(c) = st.history.undo(&st.comp) {
                    st.comp = c;
                    st.comp.ensure_outputs();
                }
            }
            A::Redo => {
                if let Some(c) = st.history.redo(&st.comp) {
                    st.comp = c;
                    st.comp.ensure_outputs();
                }
            }
            A::AddLayer => {
                st.add_layer();
            }
            A::DuplicateLayer => {
                if let Some(id) = selected_layer {
                    panels::duplicate_layer(st, id);
                }
            }
            A::DeleteLayer => {
                if let Some(id) = selected_layer {
                    panels::remove_layer(st, id);
                }
            }
            A::Palette => st.palette.toggle(),
            A::MapMode => {
                st.map_mode = !st.map_mode;
                st.control.learning = None;
            }
            A::ResetLayout => self.dock = default_layout(),
            A::AddMonitor => {
                let _ = self.add_monitor();
            }
            A::Gather => self.gather_windows = true,
            A::ZoomIn => ctx.set_zoom_factor((ctx.zoom_factor() * 1.1).min(3.0)),
            A::ZoomOut => ctx.set_zoom_factor((ctx.zoom_factor() / 1.1).max(0.5)),
            A::ZoomReset => ctx.set_zoom_factor(1.0),
            A::ToggleOutput => st.toggle_output(),
            A::Identify => st.identify_outputs(now),
            A::Syphon => st.comp.syphon = !st.comp.syphon,
            A::Ndi => st.comp.ndi.enabled = !st.comp.ndi.enabled,
            A::Notes => open_doc("NOTES.md"),
            A::Roadmap => open_doc("../ROADMAP.md"),
            A::Website => ctx.open_url(egui::OpenUrl::new_tab("https://phosphorfx.com")),
            A::Panel(i) => {
                if let Some(tab) = Tab::ALL.get(i).copied() {
                    self.toggle_panel(tab);
                }
            }
            A::Layout(i) => {
                if let Some((_, make)) = LAYOUTS.get(i) {
                    self.dock = make();
                }
            }
        }
    }

    /// Ramps newly added effects' mix up over the launch fade time.
    fn fade_in_effects(&mut self) {
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

    fn toggle_panel(&mut self, tab: Tab) {
        if self.detached.contains(&tab) {
            self.detached.retain(|t| *t != tab);
            self.dock.push_to_focused_leaf(tab);
            return;
        }
        match self.dock.find_tab(&tab) {
            Some(path) => {
                self.dock.remove_tab(path);
            }
            None => {
                self.dock.add_window(vec![tab]);
            }
        }
    }

    /// Window title: project name (and the show name for exported shows).
    fn update_title(&mut self, ctx: &egui::Context) {
        let project = self.state.project_path.as_deref().and_then(|p| std::path::Path::new(p).file_stem()).map(|s| s.to_string_lossy().into_owned());
        #[cfg(not(target_arch = "wasm32"))]
        let show = crate::show::bundled().map(|b| b.manifest.name.clone());
        #[cfg(target_arch = "wasm32")]
        let show: Option<String> = None;
        let title = match (show, project) {
            (Some(s), _) => format!("{s} — Phosphor show"),
            (None, Some(p)) => format!("{p} — Phosphor"),
            (None, None) => "Phosphor".into(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    /// Exported shows open their outputs by themselves, on the screens they were exported with.
    #[cfg(not(target_arch = "wasm32"))]
    fn start_show(&mut self, _now: f64) {
        let Some(frame) = self.show_start.as_mut() else { return };
        *frame += 1;
        // Give the main window a moment to settle first.
        if *frame < 30 {
            return;
        }
        self.show_start = None;
        let Some(b) = crate::show::bundled() else { return };
        if !b.manifest.autostart {
            return;
        }
        let displays = crate::screens::displays();
        self.state.displays = displays.clone();
        let others: Vec<_> = displays.iter().filter(|d| !d.main).cloned().collect();
        let outputs: Vec<u64> = self.state.comp.outputs.iter().filter(|o| o.enabled).map(|o| o.id).collect();
        for (i, id) in outputs.iter().enumerate() {
            let saved = b.manifest.screens.iter().find(|(o, _)| o == id).and_then(|(_, name)| displays.iter().find(|d| &d.name == name));
            // Same screen as when exported; otherwise the extra screens in order; otherwise this one.
            let target = saved.cloned().or_else(|| others.get(i).cloned()).or_else(|| (i == 0).then(|| displays.first().cloned()).flatten());
            match target {
                Some(d) => self.state.send_output_id_to(*id, d),
                None => self.state.toggle_output_window(*id),
            }
        }
    }

    /// Export Show: name, options, then a folder; shows progress until done.
    fn export_window(&mut self, ctx: &egui::Context) {
        let st = &mut self.state;
        if !st.export.open {
            return;
        }
        let mut open = true;
        egui::Window::new(format!("{} Export Show", icon::PACKAGE)).open(&mut open).collapsible(false).resizable(false).default_width(420.0).show(ctx, |ui| {
            #[cfg(target_arch = "wasm32")]
            ui.label("Export Show is available in the Mac app.");
            #[cfg(not(target_arch = "wasm32"))]
            {
                let progress = st.export.progress.as_ref().map(|p| p.lock().unwrap().clone());
                match progress {
                    Some(p) if !p.done => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(&p.step);
                        });
                        ui.weak("The show keeps running while this works.");
                    }
                    Some(p) => {
                        if let Some(app) = &p.output {
                            ui.label(egui::RichText::new(format!("{} Exported {}", icon::CHECK_CIRCLE, app.file_name().unwrap_or_default().to_string_lossy())).strong().color(theme::ACCENT));
                            ui.label(format!("{} media file(s), {:.1} MB of media", p.media_files, p.bytes as f64 / 1e6));
                            if let Some(z) = &p.zip {
                                ui.label(format!("Zip: {}", z.display()));
                            }
                            ui.weak("Copy it to the other Mac and double-click. First time: right-click → Open (it isn't notarized yet).");
                            ui.horizontal(|ui| {
                                if ui.button(label_text(icon::FOLDER_OPEN, "Show in Finder")).clicked() {
                                    let _ = std::process::Command::new("open").arg("-R").arg(app).spawn();
                                }
                                if ui.button("Export again").clicked() {
                                    st.export.progress = None;
                                }
                            });
                        }
                        if let Some(e) = &p.error {
                            ui.colored_label(egui::Color32::from_rgb(255, 120, 90), e);
                            if p.output.is_none() && ui.button("Try again").clicked() {
                                st.export.progress = None;
                            }
                        }
                    }
                    None => {
                        ui.label("Packs this project, every media file, the shaders it uses, and all MIDI/OSC/key mappings and outputs into one app that runs on another Mac — no Phosphor install needed.");
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label("Show name");
                            ui.text_edit_singleline(&mut st.export.name);
                        });
                        ui.checkbox(&mut st.export.autostart, "Open the outputs full screen when the show starts");
                        ui.checkbox(&mut st.export.zip, "Also make a .zip (easier to send)");
                        let mut files: Vec<String> = Vec::new();
                        st.comp.clone().map_paths(|p| {
                            files.push(p.to_string());
                            p.to_string()
                        });
                        files.sort();
                        files.dedup();
                        let missing = files.iter().filter(|f| !std::path::Path::new(f).is_file()).count();
                        let bytes: u64 = files.iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len()).sum();
                        ui.weak(format!("{} media file(s), {:.1} MB · {} output(s) · {} mapping(s)", files.len(), bytes as f64 / 1e6, st.comp.outputs.len(), st.comp.control_maps.len()));
                        if missing > 0 {
                            ui.colored_label(egui::Color32::from_rgb(255, 196, 70), format!("{missing} file(s) can't be found and will be left out"));
                        }
                        ui.add_space(4.0);
                        if ui.add_enabled(!st.export.name.trim().is_empty(), egui::Button::new(label_text(icon::PACKAGE, "Choose folder and export…"))).clicked() {
                            st.dialogs.pick_folder(crate::dialogs::Purpose::ExportShow);
                        }
                    }
                }
            }
        });
        if !open {
            st.export.open = false;
            #[cfg(not(target_arch = "wasm32"))]
            if st.export.progress.as_ref().is_some_and(|p| p.lock().unwrap().done) {
                st.export.progress = None;
            }
        }
    }

    /// Puts the show back the way it was before an unexpected quit.
    #[cfg(not(target_arch = "wasm32"))]
    fn restore(&mut self, s: crate::recovery::Snapshot) {
        let st = &mut self.state;
        st.comp.ensure_outputs();
        st.selected = s.selected.or(st.selected);
        {
            let mut map = st.output.lock().unwrap();
            for w in s.outputs.iter().filter(|w| w.open) {
                let e = map.entry(w.id).or_default();
                e.open = true;
                if let Some(d) = w.display.and_then(|id| st.displays.iter().find(|d| d.id == id)) {
                    e.display = Some(d.id);
                    if w.fullscreen {
                        e.place = Some((d.clone(), 0, 0));
                    }
                }
            }
        }
        if s.session && let Err(e) = st.net.start() {
            st.notice = Some(e);
        }
        if s.audio_open {
            let _ = st.audio.open(s.audio_device.as_deref());
        }
        self.recovered = Some(s.streaming);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn snapshot(&self) -> crate::recovery::Snapshot {
        let st = &self.state;
        let outputs = st
            .output
            .lock()
            .unwrap()
            .iter()
            .map(|(id, o)| crate::recovery::OutputWindowState { id: *id, open: o.open, display: o.display, fullscreen: o.fullscreen })
            .collect();
        crate::recovery::Snapshot {
            comp: Some(st.comp.clone()),
            dock: serde_json::to_value(&self.dock).ok(),
            detached: self.detached.clone(),
            outputs,
            session: st.net.running(),
            audio_open: st.audio.is_open() && !st.audio.is_network(),
            audio_device: st.audio.current().map(str::to_string),
            streaming: st.stream.running(),
            selected: st.selected,
        }
    }

    fn recovered_banner(&mut self, ctx: &egui::Context) {
        let Some(was_streaming) = self.recovered else { return };
        egui::Area::new(egui::Id::new("recovered_banner")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 44.0)).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).fill(egui::Color32::from_rgb(20, 44, 32)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("{} Recovered after an unexpected quit", icon::LIFEBUOY)).color(theme::ACCENT).strong());
                    ui.weak("project, layout, outputs, session and audio were restored");
                    if was_streaming && ui.button(format!("{} Resume stream", icon::BROADCAST)).clicked() {
                        let (w, h) = (self.state.comp.width, self.state.comp.height);
                        let now = ui.ctx().input(|i| i.time);
                        if let Err(e) = self.state.stream.start(w, h, now) {
                            self.state.notice = Some(e);
                        }
                        self.recovered = None;
                    }
                    if ui.small_button("OK").clicked() {
                        self.recovered = None;
                    }
                });
            });
        });
    }

    /// Phone/tablet remote: apply what remotes sent, then publish the state (≈20 times a second).
    fn remote_sync(&mut self, now: f64) {
        if !self.state.net.running() {
            return;
        }
        let pw = self.state.require_password.then(|| self.state.remote_password.clone());
        self.state.net.set_password(pw);
        for cmd in self.state.net.take_remote_commands() {
            crate::remote::apply(&mut self.state, &cmd, now);
        }
        if now - self.remote_published > 0.05 {
            self.remote_published = now;
            let json = crate::remote::state_json(&self.state);
            self.state.net.set_remote_state(json);
        }
    }

    /// Pulls corner drags made in the output window back into the composition.
    fn sync_mapping_from_output(&mut self) {
        let mut map = self.state.output.lock().unwrap();
        for (id, out) in map.iter_mut() {
            if out.mapping_changed {
                if let Some(o) = self.state.comp.outputs.iter_mut().find(|o| o.id == *id) {
                    o.mapping = out.mapping.clone();
                }
                self.state.selected_surface = out.selected_surface;
                self.state.mapping_output = *id;
                out.mapping_changed = false;
            }
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for (n, file) in dropped.into_iter().enumerate() {
            // Native: absolute path. Browser: just the file name (bytes arrive asynchronously).
            let path = file.path().to_string_lossy().to_string();
            let name = file.path().file_name().map_or(path.clone(), |n| n.to_string_lossy().to_string());
            if !media::is_supported(&name) {
                continue;
            }
            // First file replaces the selected layer's source; any others get new layers.
            let target = match self.state.selected {
                Some(id) if n == 0 && self.state.comp.layers.iter().any(|l| l.id == id) => id,
                _ => self.state.add_layer(),
            };
            let path = (!cfg!(target_arch = "wasm32")).then_some(path);
            let now = ctx.input(|i| i.time);
            self.state.launch_source(target, media::spec_for(&name, path), Vec::new(), name.clone(), now);
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top_bar").show(ui, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("PHOSPHOR").strong().size(13.5).color(theme::ACCENT));
                let now = ui.ctx().input(|i| i.time);
                let clicked = self.menu_bar(ui);
                let ctx = ui.ctx().clone();
                for a in clicked {
                    self.run_action(&ctx, a, now);
                }
                ui.separator();
                if ui.button(format!("{} Layer", icon::PLUS)).clicked() {
                    self.state.add_layer();
                }
                let output_label = if self.state.output_open() { format!("{} Output", icon::STOP) } else { format!("{} Output", icon::PLAY) };
                if ui.button(output_label).on_hover_text("Output window. F = fullscreen, Esc = exit").clicked() {
                    self.state.toggle_output();
                }
                ui.menu_button(format!("{} {}", icon::LIGHTNING, launch_summary(&self.state.comp)), |ui| {
                    ui.horizontal(|ui| panels::launch_controls(ui, &mut self.state.comp));
                })
                .response
                .on_hover_text("How clips and media land: Cut/Fade, Now/On beat/On bar");
                ui.menu_button(icon::CARET_DOWN, |ui| {
                    ui.label(egui::RichText::new("Send output to").strong());
                    if self.state.displays.is_empty() {
                        self.state.displays = crate::screens::displays();
                    }
                    for d in self.state.displays.clone() {
                        if ui.button(d.label()).clicked() {
                            self.state.send_output_to(d);
                            ui.close();
                        }
                    }
                    if ui.button("Rescan displays").clicked() {
                        self.state.displays = crate::screens::displays();
                    }
                })
                .response
                .on_hover_text("Choose the screen / projector");
                ui.separator();
                if ui.button(format!("{} Search  ⌘K", icon::MAGNIFYING_GLASS)).on_hover_text("Command palette: effects, sources, panels, actions").clicked() {
                    self.state.palette.toggle();
                }
                let map_text = egui::RichText::new(format!("{} MAP", icon::PLUG)).strong();
                let map_text = if self.state.map_mode { map_text.color(egui::Color32::BLACK) } else { map_text };
                if ui
                    .add(egui::Button::new(map_text).selected(self.state.map_mode).fill(if self.state.map_mode { theme::ACCENT } else { egui::Color32::TRANSPARENT }))
                    .on_hover_text("Map mode (⇧⌘M): click any control, then move a MIDI knob, send OSC or press a key to link them")
                    .clicked()
                {
                    self.state.map_mode = !self.state.map_mode;
                    self.state.control.learning = None;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let s = self.state.stats.latest;
                    let wide = ui.available_width() > 460.0;
                    if wide {
                        ui.monospace(format!("{:5.1} fps  engine {:4.2} ms  CPU {:4.1}%  {:4.0} MB", s.fps, s.render_ms, s.cpu_percent, s.memory_mb));
                    } else if ui.available_width() > 120.0 {
                        ui.monospace(format!("{:3.0} fps", s.fps));
                    }
                });
            });
            ui.add_space(2.0);
        });
    }

    /// File / Edit / View / Output / Help inside the window (same actions as the macOS menu bar).
    fn menu_bar(&mut self, ui: &mut egui::Ui) -> Vec<crate::menu::Action> {
        use crate::menu::{Action, Entry, MENUS, shortcut_text};
        let mut clicked: Vec<Action> = Vec::new();
        let can_undo = self.state.history.can_undo(&self.state.comp);
        let can_redo = self.state.history.can_redo();
        let has_layer = self.state.selected.is_some_and(|id| id != phosphor_engine::MASTER_ID && self.state.comp.layers.iter().any(|l| l.id == id));
        for (title, entries) in MENUS {
            ui.menu_button(*title, |ui| {
                ui.set_min_width(230.0);
                for e in entries.iter() {
                    match e {
                        Entry::Sep => {
                            ui.separator();
                        }
                        Entry::Item(a, label, sc) => {
                            let enabled = match a {
                                Action::Undo => can_undo,
                                Action::Redo => can_redo,
                                Action::DuplicateLayer | Action::DeleteLayer => has_layer,
                                Action::Syphon | Action::Ndi => cfg!(target_os = "macos"),
                                _ => true,
                            };
                            let text = match a {
                                Action::Syphon => format!("{label}  ({})", if self.state.comp.syphon { "on" } else { "off" }),
                                Action::Ndi => format!("{label}  ({})", if self.state.comp.ndi.enabled { "on" } else { "off" }),
                                _ => label.to_string(),
                            };
                            if ui.add_enabled(enabled, egui::Button::new(text).shortcut_text(shortcut_text(*sc))).clicked() {
                                clicked.push(*a);
                                ui.close();
                            }
                        }
                        Entry::Dynamic("Open Recent") => {
                            ui.add_enabled_ui(!self.state.recent.is_empty(), |ui| {
                                ui.menu_button("Open Recent", |ui| {
                                    for p in self.state.recent.clone() {
                                        let name = std::path::Path::new(&p).file_name().map_or(p.clone(), |n| n.to_string_lossy().into_owned());
                                        if ui.button(name).on_hover_text(&p).clicked() {
                                            self.state.open_project_file(&p);
                                            ui.close();
                                        }
                                    }
                                    ui.separator();
                                    if ui.button("Clear Menu").clicked() {
                                        self.state.recent.clear();
                                        ui.close();
                                    }
                                });
                            });
                        }
                        Entry::Dynamic("Panels") => {
                            ui.menu_button("Panels", |ui| {
                                ui.weak("Drag any tab to move it; drag it outside to float it.");
                                for (i, tab) in Tab::ALL.iter().enumerate() {
                                    let open = self.detached.contains(tab) || self.dock.find_tab(tab).is_some();
                                    if ui.selectable_label(open, tab.title()).clicked() {
                                        clicked.push(Action::Panel(i));
                                    }
                                }
                            });
                        }
                        Entry::Dynamic("Layouts") => {
                            ui.menu_button("Layouts", |ui| {
                                for (i, (name, _)) in LAYOUTS.iter().enumerate() {
                                    if ui.button(*name).clicked() {
                                        clicked.push(Action::Layout(i));
                                        ui.close();
                                    }
                                }
                            });
                        }
                        Entry::Dynamic("Send Output To") => {
                            ui.menu_button("Send Output To", |ui| {
                                if self.state.displays.is_empty() {
                                    self.state.displays = crate::screens::displays();
                                }
                                for d in self.state.displays.clone() {
                                    if ui.button(d.label()).clicked() {
                                        self.state.send_output_to(d);
                                        ui.close();
                                    }
                                }
                                if ui.button("Rescan displays").clicked() {
                                    self.state.displays = crate::screens::displays();
                                }
                            });
                        }
                        Entry::Dynamic(_) => {}
                    }
                }
            });
        }
        clicked
    }

    fn output_windows(&self, ctx: &egui::Context) {
        let open: Vec<(u64, String)> =
            self.state.output.lock().unwrap().iter().filter(|(_, o)| o.open).map(|(id, o)| (*id, o.name.clone())).collect();
        for (oid, name) in open {
            self.output_window(ctx, oid, &name);
        }
    }

    fn output_window(&self, ctx: &egui::Context, oid: u64, name: &str) {
        let id = egui::ViewportId::from_hash_of(("phosphor_output", oid));
        let shared = self.state.output.clone();
        ctx.show_viewport_deferred(
            id,
            // Borderless: nothing but picture, so a projector sees no title bar or menu bar.
            egui::ViewportBuilder::default()
                .with_title(format!("Phosphor · {name}"))
                .with_decorations(false)
                .with_inner_size([960.0, 540.0]),
            move |ui, _class| {
                let ctx = ui.ctx().clone();
                let mut map = shared.lock().unwrap();
                let Some(out) = map.get_mut(&oid) else { return };
                let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                out.fullscreen = fullscreen;
                if ctx.input(|i| i.key_pressed(egui::Key::E)) {
                    out.edit_mapping = !out.edit_mapping;
                }
                // Screen placement: leave fullscreen, move to the display, then fullscreen there.
                if let Some((d, step, wait)) = out.place.as_mut() {
                    *wait += 1;
                    match *step {
                        0 => {
                            if fullscreen {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                                *step = 1;
                            } else {
                                *step = 2;
                            }
                            *wait = 0;
                        }
                        1 if *wait > 45 => {
                            *step = 2;
                            *wait = 0;
                        }
                        2 => {
                            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(d.x + 40.0, d.y + 40.0)));
                            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(d.w * 0.6, d.h * 0.6)));
                            *step = 3;
                            *wait = 0;
                        }
                        3 if *wait > 10 => {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
                            *step = 4;
                        }
                        _ => {}
                    }
                    if *step == 4 {
                        out.place = None;
                    }
                }
                if ctx.input(|i| i.viewport().close_requested()) {
                    out.open = false;
                }
                if ctx.input(|i| i.key_pressed(egui::Key::F)) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
                }
                if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    if fullscreen {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                    } else {
                        out.open = false;
                    }
                }
                let rect = ui.max_rect();
                ui.painter().rect_filled(rect, 0.0, Color32::BLACK);
                let frame = panels::fit_rect(rect, out.aspect);
                if let Some(tex) = out.texture {
                    panels::paint_texture(ui.painter(), tex, frame);
                }
                if fullscreen {
                    ctx.set_cursor_icon(egui::CursorIcon::None);
                }
                if !out.edit_mapping {
                    // No title bar, so dragging the picture moves the window (to the projector).
                    let drag = ui.interact(rect, egui::Id::new("output_drag"), egui::Sense::drag());
                    if drag.drag_started() && !fullscreen {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if drag.double_clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
                    }
                    if !fullscreen {
                        ui.painter().text(
                            rect.left_bottom() + egui::vec2(12.0, -12.0),
                            egui::Align2::LEFT_BOTTOM,
                            "Drag to move · double-click or F = fullscreen · E = mapping handles · Esc = close",
                            egui::FontId::proportional(12.0),
                            Color32::from_white_alpha(110),
                        );
                    }
                }
                if ctx.input(|i| i.time) < out.identify_until {
                    let c = rect.center();
                    ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha(140));
                    ui.painter().text(c, egui::Align2::CENTER_CENTER, out.number.to_string(), egui::FontId::proportional(rect.height() * 0.45), theme::ACCENT);
                    ui.painter().text(c + egui::vec2(0.0, rect.height() * 0.28), egui::Align2::CENTER_CENTER, &out.name, egui::FontId::proportional(28.0), Color32::WHITE);
                }
                if out.edit_mapping {
                    let OutputShared { mapping, selected_surface, mapping_changed, .. } = &mut *out;
                    if panels::mapping_handles(ui, frame, mapping, selected_surface, true) {
                        *mapping_changed = true;
                    }
                    ui.painter().text(
                        rect.left_bottom() + egui::vec2(12.0, -12.0),
                        egui::Align2::LEFT_BOTTOM,
                        "Mapping edit: drag corners · E hides handles · F fullscreen",
                        egui::FontId::proportional(13.0),
                        Color32::from_white_alpha(160),
                    );
                }
            },
        );
        ctx.request_repaint_of(id);
    }

    fn finish_bench(&mut self, ctx: &egui::Context, now: f64) {
        let Some(run) = &mut self.bench else { return };
        let started = *run.started.get_or_insert(now);
        if let Some(col) = run.cfg.trigger_column.take_if(|_| now - started >= run.cfg.trigger_at) {
            self.state.trigger(Trigger::Column(col), now);
            return;
        }
        if now - started < run.cfg.seconds {
            return;
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
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(dir) = run.cfg.export.clone() {
            let req = crate::export::Request {
                comp: self.state.comp.clone(),
                name: "Bench Show".into(),
                dest: dir.into(),
                zip: true,
                autostart: false,
                screens: Vec::new(),
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
            // Mapped outputs too, as <name>-out<N>.png.
            for (i, o) in self.state.comp.outputs.iter().enumerate().filter(|(_, o)| o.mapping.active()) {
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

impl eframe::App for PhosphorApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // A bug in one frame should not take the show down: skip that frame and carry on.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.ui_frame(ui, frame)));
        if result.is_err() {
            let now = ui.ctx().input(|i| i.time);
            self.panics.retain(|t| now - t < 5.0);
            self.panics.push(now);
            self.state.notice = Some("Phosphor hit an internal error and skipped a frame (details in crash.log).".into());
            ui.ctx().request_repaint();
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let now = ui.ctx().input(|i| i.time);
            if self.bench.is_none() && self.saver.due(now) {
                let snap = self.snapshot();
                self.saver.save(now, &snap);
            }
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.save_settings(storage);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn on_exit(&mut self) {
        if self.bench.is_none() {
            let snap = self.snapshot();
            self.saver = Default::default();
            if let (Ok(json), Some(dir)) = (serde_json::to_string(&snap), crate::show::storage_dir()) {
                let _ = std::fs::write(dir.join("recovery.json"), json);
            }
            crate::recovery::end_session();
        }
    }
}

impl PhosphorApp {
    fn ui_frame(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);

        self.handle_dropped_files(&ctx);
        self.state.comp.ensure_outputs();
        self.state.comp.sync_media_bin();
        self.state.thumbs.poll(&ctx);
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
            control.apply(comp, now as f32);
        }
        for d in std::mem::take(&mut self.state.control.triggers) {
            match d {
                crate::control::Deferred::Clip(layer, slot) => self.state.trigger(Trigger::Clip { layer, slot }, now),
                crate::control::Deferred::Column(col) => self.state.trigger(Trigger::Column(col), now),
                crate::control::Deferred::App(a, v) => self.state.app_control(a, v, now),
            }
        }
        {
            let st = &mut self.state;
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
        }
        if let Some(n) = self.bench.as_ref().map(|b| b.cfg.previews).filter(|n| *n > 0) {
            for (i, d) in phosphor_engine::shaders::all_effects().iter().cycle().take(n).enumerate() {
                let fx = phosphor_engine::EffectState { id: 8_000_000 + i as u64, effect: d.id.to_string(), enabled: true, values: d.defaults(), mix: 1.0, mods: Vec::new() };
                self.state.preview_wants.push((format!("bench:{i}"), phosphor_engine::PreviewItem::Effect(fx), true));
            }
        }
        self.render(now);
        self.top_bar(ui);
        rescue_windows(&mut self.dock, ctx.content_rect(), std::mem::take(&mut self.gather_windows));

        egui::CentralPanel::no_frame().show(ui, |ui| {
            let mut style = egui_dock::Style::from_egui(ui.style());
            style.tab_bar.fill_tab_bar = true;
            let mut viewer = Viewer { st: &mut self.state, detach: None };
            DockArea::new(&mut self.dock)
                .style(style)
                .show_close_buttons(true)
                .show_leaf_close_all_buttons(false)
                .show_leaf_collapse_buttons(false)
                .show_inside(ui, &mut viewer);
            if let Some(tab) = viewer.detach {
                self.detach(tab);
            }
        });
        self.detached_windows(&ctx);
        crate::explorer::fx_browser_window(&ctx, &mut self.state);
        if let Some(cmd) = crate::explorer::palette(&ctx, &mut self.state) {
            self.run_command(cmd, now);
        }
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

impl PhosphorApp {
    fn save_settings(&mut self, storage: &mut dyn eframe::Storage) {
        if self.bench.is_some() {
            return;
        }
        storage.set_string(KEY_COMPOSITION, self.state.comp.to_json());
        if let Ok(json) = serde_json::to_string(&self.dock) {
            storage.set_string(KEY_DOCK, json);
        }
        if let Ok(json) = serde_json::to_string(&self.state.stream.settings) {
            storage.set_string(KEY_STREAM, json);
        }
        if let Ok(json) = serde_json::to_string(&Tab::ALL) {
            storage.set_string(KEY_KNOWN_TABS, json);
        }
        if let Ok(json) = serde_json::to_string(&self.detached) {
            storage.set_string(KEY_DETACHED, json);
        }
        if let Ok(json) = serde_json::to_string(&self.state.explorer.favorites) {
            storage.set_string(KEY_FAVORITES, json);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(json) = serde_json::to_string(&self.state.folders.folders) {
            storage.set_string(KEY_WATCHED, json);
        }
        if let Ok(json) = serde_json::to_string(&self.state.recent) {
            storage.set_string(KEY_RECENT_PROJECTS, json);
        }
        storage.set_string(KEY_PROJECT_PATH, self.state.project_path.clone().unwrap_or_default());
        storage.set_string(KEY_REMOTE_PASSWORD, if self.state.require_password { self.state.remote_password.clone() } else { String::new() });
        if let Ok(json) = serde_json::to_string(&self.state.explorer.recent) {
            storage.set_string(KEY_RECENT, json);
        }
        if let Ok(json) = serde_json::to_string(&self.state.monitors) {
            storage.set_string(KEY_MONITORS, json);
        }
    }
}

/// Short label for the launch settings, e.g. "Fade 0.5 s · On beat".
fn launch_summary(comp: &Composition) -> String {
    let how = if comp.launch.fade { format!("Fade {:.1}s", comp.launch.fade_secs) } else { "Cut".to_string() };
    format!("{how} · {}", comp.launch.timing.name())
}

/// Everything at once: preview + monitor, clips, mixer/effects along the bottom, layers and
/// inspector down the right.
fn default_layout() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Preview]);
    let surface = dock.main_surface_mut();
    let [left, right] = surface.split_right(NodeIndex::root(), 0.72, vec![Tab::Layers]);
    surface.split_below(right, 0.46, vec![Tab::Inspector, Tab::Presets]);
    let [top, bottom] = surface.split_below(left, 0.5, vec![Tab::Clips, Tab::Media]);
    surface.split_right(top, 0.62, vec![Tab::Monitor(1), Tab::Mapping]);
    surface.split_below(
        bottom,
        0.42,
        vec![Tab::Mixer, Tab::Library, Tab::Audio, Tab::Control, Tab::Stream, Tab::Network, Tab::Devices, Tab::ShaderEditor, Tab::Output],
    );
    dock
}

/// Panels added in newer builds show up in layouts saved by older ones; panels the user closed
/// on purpose stay closed.
fn ensure_new_tabs(dock: &mut DockState<Tab>, known: &[Tab]) {
    for tab in Tab::ALL {
        if !known.contains(&tab) && dock.find_tab(&tab).is_none() {
            dock.push_to_focused_leaf(tab);
        }
    }
}

/// Keeps floating panels reachable: a window saved on a bigger display, or dragged off the
/// edge, is moved back so its title bar is inside `screen`. `force` pulls every window fully inside.
fn rescue_windows(dock: &mut DockState<Tab>, screen: egui::Rect, force: bool) {
    let margin = 48.0;
    for surface in dock.iter_surfaces_mut() {
        let egui_dock::Surface::Window(_, ws) = surface else { continue };
        let r = ws.rect();
        if !r.is_finite() || r.width() <= 0.0 {
            continue;
        }
        let max_size = (screen.size() - egui::vec2(16.0, 16.0)).max(egui::vec2(200.0, 120.0));
        if r.width() > max_size.x || r.height() > max_size.y {
            ws.set_size(r.size().min(max_size));
        }
        let visible = egui::Rect::from_min_max(
            egui::pos2(screen.left() - r.width() + margin, screen.top()),
            egui::pos2(screen.right() - margin, screen.bottom() - margin),
        );
        let lost = !visible.contains(r.min) || r.min.y < screen.top();
        if lost || force {
            let max_pos = (screen.max - r.size()).max(screen.min);
            let target = egui::pos2(r.min.x.clamp(screen.left(), max_pos.x.max(screen.left())), r.min.y.clamp(screen.top(), max_pos.y.max(screen.top())));
            ws.set_position(target);
        }
    }
}

const LAYOUTS: &[(&str, fn() -> DockState<Tab>)] =
    &[("Default", default_layout), ("Performance", layout_performance), ("Mapping", layout_mapping)];

/// Big preview, layers on the right, audio + control underneath: for playing a show.
fn layout_performance() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Preview]);
    let surface = dock.main_surface_mut();
    let [left, right] = surface.split_right(NodeIndex::root(), 0.7, vec![Tab::Layers]);
    surface.split_below(right, 0.6, vec![Tab::Inspector, Tab::Presets]);
    surface.split_below(left, 0.78, vec![Tab::Audio, Tab::Control, Tab::Stream]);
    dock
}

/// Mapping editor front and centre, with the output controls beside it.
fn layout_mapping() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Mapping]);
    let surface = dock.main_surface_mut();
    let [left, right] = surface.split_right(NodeIndex::root(), 0.7, vec![Tab::Output, Tab::Layers]);
    surface.split_below(right, 0.5, vec![Tab::Inspector]);
    surface.split_below(left, 0.72, vec![Tab::Preview, Tab::Library, Tab::Presets]);
    dock
}

fn label_text(icon: &str, text: &str) -> String {
    format!("{icon}  {text}")
}

/// Opens one of the docs (notes, roadmap) from the source folder, or the website if not there.
fn open_doc(name: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs").join(name);
        let target = if path.is_file() { path.to_string_lossy().into_owned() } else { "https://phosphorfx.com".into() };
        let _ = std::process::Command::new("open").arg(target).spawn();
    }
    #[cfg(target_arch = "wasm32")]
    let _ = name;
}
