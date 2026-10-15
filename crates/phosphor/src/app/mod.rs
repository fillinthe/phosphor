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


mod bench_run;
mod controls;
mod files;
mod frame;
mod init;
mod layout;
mod menus;
mod outputs;
mod recovery;
mod render;
mod settings;
mod show;

#[allow(unused_imports)]
pub(crate) use layout::*;
#[allow(unused_imports)]
pub(crate) use menus::*;

const KEY_COMPOSITION: &str = "composition";
const KEY_LAYOUTS: &str = "layouts";
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
const KEY_CONTROL: &str = "control";
const KEY_ACCOUNT_HELLO: &str = "account_hello_never";
const KEY_ACCOUNT_TAB_MOVED: &str = "account_tab_moved";

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
    /// Saved output + mapping setups (Mapping panel › Venues) and the name being typed.
    pub venues: PresetStore,
    pub venue_name: String,
    pub preset_name: String,
    /// A message worth showing the user (save failed, bad preset, …).
    pub notice: Option<String>,
    pub audio: AudioInput,
    pub control: ControlHub,
    /// Mappings editor: what the next heard input is for, the new mapping being made, and search.
    pub listen_for: Option<ListenFor>,
    pub map_draft: Option<(phosphor_engine::ControlSource, Option<phosphor_engine::ControlTarget>)>,
    pub map_search: String,
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
    /// phosphorfx.com account and the project open from it (id, name).
    pub account: crate::account::Account,
    pub account_ui: crate::panels::AccountUi,
    pub cloud_project: Option<(String, String)>,
    /// Account window open (top bar), and the startup hello about accounts (until "Don't show again").
    pub account_window: bool,
    pub account_hello: bool,
    pub account_hello_never: bool,
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
    /// Closes output windows whose output no longer exists (removed, or replaced by a venue).
    pub fn close_stale_outputs(&mut self) {
        let live: std::collections::HashSet<u64> = self.comp.outputs.iter().map(|o| o.id).collect();
        self.output.lock().unwrap().retain(|id, _| live.contains(id));
    }

    pub fn set_project_path(&mut self, path: &str) {
        self.project_path = Some(path.to_string());
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(12);
    }

    /// Writes the project to its file, or asks where when it has none yet (browser: downloads it).
    pub fn save_project(&mut self) {
        #[cfg(target_arch = "wasm32")]
        {
            let name = format!("{}.phosphor", self.cloud_project.as_ref().map_or("My show", |(_, n)| n.as_str()));
            self.notice = Some(match crate::web_files::download(&name, &self.comp.to_json()) {
                Ok(()) => format!("Downloaded {name} (File › Open loads it again)"),
                Err(e) => format!("Download failed: {e}"),
            });
        }
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
        #[cfg(target_arch = "wasm32")]
        self.save_project();
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

    /// Sets any control (0..1) from the UI, a surface widget or the remote, and carries out what it
    /// triggers (clip launches, tempo…). The same path controllers take.
    pub fn set_control(&mut self, target: &phosphor_engine::ControlTarget, v: f32, now: f64) {
        let mut out = Vec::new();
        crate::control::set_target(&mut self.comp, target, v, &mut out);
        for d in out {
            self.run_deferred(d, now);
        }
    }

    pub fn run_deferred(&mut self, d: crate::control::Deferred, now: f64) {
        match d {
            crate::control::Deferred::Clip(layer, slot) => self.trigger(Trigger::Clip { layer, slot }, now),
            crate::control::Deferred::Column(col) => self.trigger(Trigger::Column(col), now),
            crate::control::Deferred::App(a, v) => self.app_control(a, v, now),
            crate::control::Deferred::Scrub(layer, v) => {
                if let Some((_, dur)) = self.media.position(layer) {
                    self.media.seek(layer, v as f64 * dur);
                }
            }
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
        self.comp.ensure_outputs();
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
    /// `--ui-shot`: the window picture was requested.
    shot_requested: bool,
}

/// What a Listen in the Mappings editor is for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ListenFor {
    NewMapping,
    /// Re-capture the input of mapping `i`.
    Replace(usize),
}

/// The Export Show window.
#[derive(Default)]
pub struct ExportUi {
    pub open: bool,
    pub name: String,
    pub zip: bool,
    pub autostart: bool,
    /// (media files, bytes, missing), worked out when the window opens.
    pub summary: Option<(usize, u64, usize)>,
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
    /// Built-in and saved layouts, and which opens at startup.
    layouts: Layouts,
    /// Layouts from the account were applied this session; next upload check (seconds).
    cloud_layouts_applied: bool,
    cloud_layouts_check: f64,
    /// Save Layout window: (name, open at startup).
    layout_draft: Option<(String, bool)>,
    /// When the last UI frame ran (wall clock) and its egui time: `logic` keeps the show going
    /// on our own clock while the window is hidden.
    last_ui: (web_time::Instant, f64),
    /// Paces rendering while the window is hidden (60 fps).
    hidden_pacer: crate::pacing::FramePacer,
    #[cfg(not(target_arch = "wasm32"))]
    heartbeat: crate::recovery::Heartbeat,
}

impl eframe::App for PhosphorApp {
    /// Runs every frame, and on its own while the control window is minimized or covered
    /// (eframe then skips `ui`). The outputs, Syphon, NDI and streams must keep going then, so
    /// render here when the UI hasn't drawn recently.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let (at, time) = self.last_ui;
        let idle = at.elapsed().as_secs_f64();
        if idle > 0.05 {
            let now = time + idle;
            if self.hidden_pacer.due(now, 60) {
                self.render(now);
            }
            ctx.request_repaint();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // When the window is covered, macOS stops pacing redraws to the display, and the loop
        // would spin at thousands of fps. Faster than any display (< 4 ms apart): hold ~120 fps.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let since = self.last_ui.0.elapsed().as_secs_f64();
            if since < 0.004 {
                std::thread::sleep(std::time::Duration::from_secs_f64(1.0 / 120.0 - since));
            }
        }
        self.last_ui = (web_time::Instant::now(), ui.ctx().input(|i| i.time));
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
    /// Projects opened from / saved to the phosphorfx.com account (see `account.rs`).
    pub(crate) fn account_events(&mut self, ctx: &egui::Context) {
        self.sync_layouts(ctx);
        let st = &mut self.state;
        #[cfg(target_arch = "wasm32")]
        if let Some((name, json)) = crate::web_files::take_opened() {
            match Composition::from_json(&json) {
                Ok(comp) => {
                    st.load_composition(comp);
                    st.cloud_project = None;
                    st.notice = Some(format!("Opened {name}"));
                }
                Err(e) => st.notice = Some(format!("Couldn't open {name}: {e}")),
            }
        }
        let mut cloud_layouts = None;
        for e in st.account.tick(ctx) {
            match e {
                crate::account::Event::Opened(id, name, json) => match Composition::from_json(&json) {
                    Ok(comp) => {
                        st.load_composition(comp);
                        st.project_path = None;
                        st.account_ui.name = name.clone();
                        st.notice = Some(format!("Opened {name} from your account. Undo (⌘Z) brings the previous project back."));
                        st.cloud_project = Some((id, name));
                    }
                    Err(err) => st.notice = Some(format!("Couldn't open {name}: {err}")),
                },
                crate::account::Event::Saved(id, name) => {
                    st.notice = Some(format!("Saved {name} to your account"));
                    st.cloud_project = Some((id, name));
                }
                crate::account::Event::Layouts(text) => cloud_layouts = Some(text),
            }
        }
        if let Some(text) = cloud_layouts {
            self.apply_account_layouts(&text);
        }
    }

    /// Layouts from your account: saved ones are added (same name: the account's wins), and the arrangement you
    /// last used anywhere opens, once per session (signing in on a new computer brings your workspace along).
    fn apply_account_layouts(&mut self, text: &str) {
        let Some(cloud) = layout::from_text::<layout::AccountLayouts>(text) else { return };
        for l in &cloud.layouts.saved {
            self.layouts.save(&l.name, &l.dock, &l.detached);
        }
        if self.layouts.startup.is_none() {
            self.layouts.startup = cloud.layouts.startup.clone();
        }
        if !self.cloud_layouts_applied
            && let Some(cur) = cloud.current
        {
            self.dock = cur.dock;
            self.detached = cur.detached;
            self.state.notice = Some("Your layout from your account is open".into());
        }
        self.cloud_layouts_applied = true;
        self.install_native_menu();
    }

    /// Uploads your layouts when signed in and they changed (checked every 30 s; the account skips unchanged text).
    fn sync_layouts(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if now < self.cloud_layouts_check || !self.state.account.signed_in() {
            return;
        }
        self.cloud_layouts_check = now + 30.0;
        let current = layout::SavedLayout { name: "Current".into(), dock: self.dock.clone(), detached: self.detached.clone() };
        if let Some(text) = layout::to_text(&layout::AccountLayouts { layouts: self.layouts.clone(), current: Some(current) }) {
            self.state.account.push_layouts(ctx, &text);
        }
    }
}
