//! Top-level app: owns the composition, engine, dockable layout, and output window.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32};
use eframe::egui_wgpu;
use egui_dock::{DockArea, DockState, NodeIndex};
use egui_phosphor::regular as icon;
use phosphor_engine::{Composition, Engine, Mapping, ModInputs, wgpu};

pub(crate) use files::file_drag_point;

/// `State::preview_tex` for code that has split `State` into fields.
pub(crate) fn preview_lookup(keys: &[String], textures: &[(egui::TextureId, (u32, u32))], key: &str) -> Option<egui::TextureId> {
    let i = keys.iter().position(|k| k == key)?;
    textures.get(i).map(|(t, _)| *t)
}
pub use quick::{Quick, quick_menu, quick_row};
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


mod auto_show;
#[cfg(not(target_arch = "wasm32"))]
mod backgrounds;
mod welcome;
mod autopilot;
mod step_fires;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod beat_match;
mod bench_run;
mod cues;
pub(crate) use cues::format_tc;
mod controls;
mod files;
mod frame;
#[cfg(not(target_arch = "wasm32"))]
mod import;
mod init;
mod layout;
pub(crate) mod lock;
mod menus;
mod outputs;
pub(crate) mod play;
mod quick;
pub(crate) use quick::dice;
mod recovery;
mod render;
mod settings;
mod areas;
mod web_control;
pub(crate) use web_control::web_badge;
#[cfg(not(target_arch = "wasm32"))]
mod convert_window;
mod status;
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
const KEY_HIDDEN_FX: &str = "hidden_effects";
const KEY_MONITORS: &str = "monitors";
const KEY_RECENT: &str = "recent_effects";
const KEY_REMOTE_PASSWORD: &str = "remote_password";
const KEY_UPDATE_SKIP: &str = "update_skip";
/// "Install updates by themselves" ("0" = off).
const KEY_UPDATE_AUTO: &str = "update_auto";
/// Send crash reports by themselves ("1").
const KEY_SEND_CRASHES: &str = "send_crashes";
/// The output movie sound plays on (empty = the default).
const KEY_SOUND_OUT: &str = "sound_out_device";
/// A player: the leader to follow every time Phosphor starts ("" = none).
const KEY_FOLLOW_AT_LAUNCH: &str = "follow_at_launch";
const KEY_LOCK: &str = "lock";
const KEY_PANELS_LOCKED: &str = "panels_locked";
const KEY_RENDER_RATE: &str = "render_rate";
const KEY_RECENT_PROJECTS: &str = "recent_projects";
/// In-window menu clicks, queued for `menu_actions`.
pub const MENU_REQUEST: &str = "phosphor_menu_request";
const KEY_PROJECT_PATH: &str = "project_path";
const KEY_AUTOSAVE: &str = "autosave";
/// How often autosave saves a changed project.
pub(crate) const AUTOSAVE_MINUTES: u32 = 5;
const KEY_WATCHED: &str = "watched_folders";
/// Set once the Library has been opened by default for an existing arrangement (0.2.67).
const KEY_LIBRARY_DEFAULT: &str = "library_default_v1";
const KEY_CONTROL: &str = "control";
const KEY_ACCOUNT_HELLO: &str = "account_hello_never";
const KEY_ACCOUNT_TAB_MOVED: &str = "account_tab_moved";
/// Set once the app has opened with the three areas (2026-10-19): the first time, it starts in Show.
const KEY_AREAS: &str = "areas_v2";
/// Every area's own arrangement (RON: [(area, dock, floating panels)]), and the area you were in.
const KEY_AREA_DOCKS: &str = "area_docks";
const KEY_AREA: &str = "area";
const KEY_BRIGHTNESS: &str = "ui_brightness";
const KEY_ACCENT: &str = "ui_accent";

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
    /// LED / DMX sender (Art-Net, sACN).
    #[cfg(not(target_arch = "wasm32"))]
    pub dmx: crate::dmx::DmxOut,
    /// Projector control (PJLink) per output.
    #[cfg(not(target_arch = "wasm32"))]
    pub projectors: crate::pjlink::Projectors,
    /// The Scopes panel's measuring (only while it's on screen).
    pub scopes: crate::scopes::Scopes,
    /// "From a link…" (posts on X, Instagram…): the window and its download.
    #[cfg(not(target_arch = "wasm32"))]
    pub link_import: crate::panels::LinkImport,
    /// Shader Editor › Describe it… (the AI writes a shader).
    #[cfg(not(target_arch = "wasm32"))]
    pub ai_shader: crate::panels::AiShader,
    /// Stage scanner (patterns on an output, a camera watching): makes stage photos.
    #[cfg(not(target_arch = "wasm32"))]
    pub scanner: crate::scanner::Scanner,
    /// Video tracking (faces, hands, body → `Track …` modulators).
    #[cfg(not(target_arch = "wasm32"))]
    pub tracker: crate::tracking::Tracker,
    /// Following another Phosphor (`crate::follow`): the link to the leader, and what this one sends when it leads.
    #[cfg(not(target_arch = "wasm32"))]
    pub following: Option<crate::follow::Link>,
    /// A player: follow this leader every time Phosphor starts (empty = don't).
    pub follow_at_launch: String,
    #[cfg(not(target_arch = "wasm32"))]
    pub leader: crate::follow::Leader,
    /// FFGL plugins: the list found, and the instances drawing for effects and sources.
    #[cfg(target_os = "macos")]
    pub ffgl: crate::ffgl::Ffgl,
    /// A Resolume composition waiting for the deck choice.
    #[cfg(not(target_arch = "wasm32"))]
    pub avc_import: Option<import::AvcImport>,
    /// A missing-media search running in the background.
    #[cfg(not(target_arch = "wasm32"))]
    pub relink: Option<std::sync::mpsc::Receiver<import::Relinked>>,
    /// Ableton Link session (None = Link off).
    #[cfg(not(target_arch = "wasm32"))]
    pub link: Option<crate::link::LinkSync>,
    /// The cue list while the show runs.
    pub cue_run: cues::CueRun,
    /// Autopilot's per-layer state (which clip, since when).
    pub(crate) autopilot: HashMap<u64, autopilot::AutoRun>,
    /// The step each firing step track was on last frame (`run_step_fires`).
    pub(crate) step_fired: HashMap<u64, usize>,
    pub(crate) render_state: Option<egui_wgpu::RenderState>,
    pub texture: Option<egui::TextureId>,
    texture_generation: u64,
    /// Per-layer thumbnail textures, keyed by layer id, with the size they were registered at.
    pub layer_textures: HashMap<u64, (egui::TextureId, (u32, u32))>,
    pub media: MediaManager,
    pub stats: Stats,
    pub selected: Option<u64>,
    /// More layers selected with Shift / Cmd-click: edits to the selected layer go to these too (`run_multi_edit`).
    pub also_selected: Vec<u64>,
    /// The selected layer as of the last frame (what changed since is copied to `also_selected`).
    pub multi_before: Option<phosphor_engine::LayerState>,
    /// The clip last clicked in the grid (layer, slot): the Inspector shows its settings while that layer is selected.
    pub selected_clip: Option<(u64, usize)>,
    pub selected_surface: Option<u64>,
    /// Output windows by output id.
    pub output: Arc<Mutex<std::collections::HashMap<u64, OutputShared>>>,
    output_textures: HashMap<u64, (egui::TextureId, (u32, u32))>,
    /// Output being edited in the Mapping panel.
    pub mapping_output: u64,
    pub presets: PresetStore,
    /// Saved output + mapping setups (Mapping panel › Venues) and the name being typed.
    pub venues: PresetStore,
    /// Saved layers and effect chains (Presets panel), with the names being typed.
    pub layer_presets: PresetStore,
    pub chain_presets: PresetStore,
    pub surface_presets: PresetStore,
    pub mapping_presets: PresetStore,
    /// Name typed for the next Control Surface / mappings save.
    pub surface_preset_name: String,
    pub mapping_preset_name: String,
    pub layer_preset_name: String,
    pub chain_preset_name: String,
    pub venue_name: String,
    pub preset_name: String,
    /// A message worth showing the user (save failed, bad preset, …).
    pub notice: Option<String>,
    /// Next time movies' settings are saved into the media bin (`Composition::remember_media_settings`).
    pub media_memory_at: f64,
    /// The Convert window (right-click a media file > Convert…).
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) convert: convert_window::ConvertUi,
    /// Opening the default audio input failed once (don't retry on every Quick action).
    pub audio_declined: bool,
    /// Clip cells drawn last frame (screen rect, layer, slot): where files dropped from the Finder land.
    pub clip_cells: Vec<(egui::Rect, u64, usize)>,
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
    /// Audience play: whose turn it is.
    pub jam: play::Jam,
    /// Auto show runtime (bar counter, level average).
    pub auto_run: auto_show::AutoShowRun,
    pub bug: crate::support::BugReport,
    /// Visual beat scans (Match the beat).
    #[cfg(not(target_arch = "wasm32"))]
    pub rhythm: crate::media::rhythm::Analyzer,
    pub updater: crate::update::Updater,
    /// Password phones need to control (viewing never needs it). Empty = no password.
    pub remote_password: String,
    /// How often the engine draws (Output panel); the UI repaints at the same pace.
    pub render_rate: phosphor_engine::RenderRate,
    pub render_pacer: crate::pacing::FramePacer,
    pub require_password: bool,
    /// When someone last changed something from the web, and who (the web control badge).
    pub web_activity: Option<(f64, String)>,
    /// Changes made from phones / browsers, newest last (Devices panel).
    pub(crate) web_log: Vec<web_control::WebChange>,
    #[cfg(not(target_arch = "wasm32"))]
    pub dialogs: crate::dialogs::Dialogs,
    #[cfg(not(target_arch = "wasm32"))]
    pub folders: crate::folders::FolderWatcher,
    pub explorer: crate::explorer::ExplorerState,
    pub editor: crate::explorer::EditorState,
    pub palette: crate::explorer::PaletteState,
    /// Previews requested by panels this frame (key, item); rendered on the next frame.
    pub preview_wants: Vec<(String, phosphor_engine::PreviewItem, bool)>,
    pub(crate) preview_keys: Vec<String>,
    /// When stills were last refreshed.
    preview_refreshed: f64,
    pub(crate) preview_textures: Vec<(egui::TextureId, (u32, u32))>,
    /// A panel to bring to the front (set by explorer actions and the palette).
    pub open_tab: Option<Tab>,
    /// Dragging in the Preview moves the selected layer's shader points (ISF point inputs).
    pub mouse_to_shaders: bool,
    /// File › New from Template is open.
    pub templates_open: bool,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) backgrounds: backgrounds::Backgrounds,
    /// A snapshot of this output was asked for (Output panel); the app saves it.
    pub snapshot_output: Option<u64>,
    /// Make this clip's grid picture from its layer's picture now (layer, slot).
    pub clip_thumb_request: Option<(u64, usize)>,
    /// Mappings copied from a control's menu (their target is replaced on paste).
    pub maps_clipboard: Vec<phosphor_engine::ControlMap>,
    pub fx_browser: Option<crate::explorer::FxBrowser>,
    pub monitors: std::collections::HashMap<u8, panels::MonitorCfg>,
    /// Monitor pictures requested this frame: (monitor number, source, width, height).
    pub monitor_wants: Vec<(u8, u64, u32, u32)>,
    /// Preview windows keeping last frame's picture this frame (frame-rate cap), and when each last drew.
    pub monitor_held: Vec<u8>,
    pub monitor_drawn: HashMap<u8, f64>,
    /// Preview windows' pictures by window number, with the engine texture each was made from (re-linked when that
    /// texture is replaced).
    monitor_textures: HashMap<u8, (egui::TextureId, egui_wgpu::wgpu::Texture)>,
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
    /// Save the project by itself every `AUTOSAVE_MINUTES` when it changed (File › Autosave).
    pub autosave: bool,
    /// When autosave last looked, and a hash of what was last written (only a changed project is saved).
    autosave_at: f64,
    saved_hash: u64,
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
    #[cfg(any(target_os = "macos", windows))]
    pub ndi: crate::ndi::Ndi,
    /// Phosphor as a camera in other apps (macOS camera extension).
    #[cfg(target_os = "macos")]
    pub vcam: crate::virtual_camera::VirtualCamera,
    #[cfg(windows)]
    pub spout: crate::spout::Spout,
    pub clock: Clock,
    pub pending_triggers: Vec<Trigger>,
    /// Which bank of clip columns the grid and the number keys address.
    pub clip_page: usize,
    /// This frame's audio/LFO signals, also shown in the Audio panel.
    pub mod_inputs: ModInputs,
    /// Shared modulators' running phases (speed changes never jump).
    pub mod_phases: phosphor_engine::ModPhases,
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
        self.monitor_textures.get(&n).map(|(t, _)| *t)
    }

    /// A rendered explorer/editor/clip preview by key (from the previous frame).
    pub fn preview_tex(&self, key: &str) -> Option<egui::TextureId> {
        preview_lookup(&self.preview_keys, &self.preview_textures, key)
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
            Some(p) => match crate::backups::back_up(std::path::Path::new(p), crate::backups::now_secs()).and_then(|_| {
                let json = self.comp.to_json();
                self.saved_hash = hash_text(&json);
                std::fs::write(p, json)
            }) {
                Ok(()) => self.notice = Some(format!("Saved {}", std::path::Path::new(p).file_name().map_or(p.clone(), |n| n.to_string_lossy().into_owned()))),
                Err(e) => self.notice = Some(format!("Save failed: {e}")),
            },
            None => self.save_project_as(),
        }
    }

    /// Autosave: every few minutes, a project that has a file and changed since it was saved is saved (with a backup).
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn autosave_tick(&mut self, now: f64) {
        if !self.autosave || self.project_path.is_none() || now - self.autosave_at < AUTOSAVE_MINUTES as f64 * 60.0 {
            return;
        }
        self.autosave_at = now;
        if hash_text(&self.comp.to_json()) != self.saved_hash {
            self.save_project();
            if let Some(n) = &mut self.notice {
                *n = format!("Autosaved ({})", n.trim_start_matches("Saved "));
            }
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
                let newer = comp.made_by_newer();
                self.load_composition(comp);
                self.set_project_path(path);
                self.saved_hash = hash_text(&self.comp.to_json());
                if newer {
                    self.notice = Some("This project was saved by a newer Phosphor: some of its settings may be missing here. Update Phosphor (Help > Check for Updates) before saving over it.".into());
                }
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
        self.cue_run.opened = false;
        self.outputs_on_open();
    }

    /// Projects set to "open outputs on their screens" do so when loaded (and at launch).
    pub fn outputs_on_open(&mut self) {
        if self.comp.outputs_on_open {
            let missing = self.send_outputs_to_screens();
            if !missing.is_empty() {
                self.notice = Some(format!("Outputs not opened, screen not connected: {}", missing.join(", ")));
            }
        }
    }

    /// Fires a trigger now, or queues it for the next beat when quantise is on.
    pub fn trigger(&mut self, t: Trigger, now: f64) {
        let locked = match &t {
            Trigger::Clip { layer, slot } => self.comp.control_locked(&phosphor_engine::ControlTarget::TriggerClip { layer: *layer, slot: *slot }),
            Trigger::Column(c) => self.comp.control_locked(&phosphor_engine::ControlTarget::TriggerColumn(*c)),
            Trigger::Source { .. } => false,
        };
        if locked {
            return;
        }
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
            crate::control::Deferred::ClipRelease(layer, slot) => self.release_clip(layer, slot, now),
            crate::control::Deferred::Column(col) => self.trigger(Trigger::Column(col), now),
            crate::control::Deferred::App(a, v) => self.app_control(a, v, now),
            crate::control::Deferred::Scrub(layer, v) => {
                if let Some((_, dur)) = self.media.position(layer) {
                    self.media.seek(layer, v as f64 * dur);
                }
            }
            crate::control::Deferred::Jump(id, forward) => {
                if let (Some((pos, dur)), Some(l)) = (self.media.position(id), self.comp.layers.iter().find(|l| l.id == id)) {
                    let step = l.jump_secs(Some(dur), self.comp.bpm);
                    self.media.seek(id, if forward { pos + step } else { pos - step });
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
            A::Resync if press => {
                self.clock.resync(now);
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(l) = &mut self.link {
                    l.resync();
                }
            }
            A::FollowAudio => self.clock.follow_audio = if v == FLIP { !self.clock.follow_audio } else { press },
            A::AudioGain => self.audio.gain = unit_to(v, 0.1, 10.0, true),
            A::KickThreshold => self.audio.beats.sensitivity = unit_to(v, 1.1, 3.0, false),
            A::CueGo if press => self.cue_next(now),
            A::Surprise if press => self.quick(Quick::Surprise, now),
            A::Shuffle if press => self.quick(Quick::Shuffle, now),
            A::CueBack if press => self.cue_back(now),
            A::CueRandom if press => self.cue_random(now),
            A::AutoFade if press => self.auto_fade(now),
            #[cfg(not(target_arch = "wasm32"))]
            A::ProjectorsOn if press => self.projectors.ask_all(&self.comp.outputs, crate::pjlink::Ask::PowerOn),
            #[cfg(not(target_arch = "wasm32"))]
            A::ProjectorsOff if press => self.projectors.ask_all(&self.comp.outputs, crate::pjlink::Ask::PowerOff),
            #[cfg(not(target_arch = "wasm32"))]
            A::ProjectorsShutter => {
                let closed = if v == FLIP { !self.projectors.shutter_closed } else { press };
                self.projectors.ask_all(&self.comp.outputs, crate::pjlink::Ask::Shutter(closed));
            }
            #[cfg(not(target_arch = "wasm32"))]
            A::Link => {
                let on = if v == FLIP { self.link.is_none() } else { press };
                if on != self.link.is_some() {
                    self.link = on.then(|| crate::link::LinkSync::new(self.clock.bpm));
                }
            }
            _ => {}
        }
    }

    /// The crossfader glides to the side it's further from (A ↔ B) over the auto-fade time.
    pub fn auto_fade(&mut self, now: f64) {
        let target = phosphor_engine::ControlTarget::App(phosphor_engine::AppControl::Crossfader);
        let from = self.comp.crossfader;
        // Pressed on the way: turn back. Otherwise go to the far side.
        let to = match self.control.glide_target(&target) {
            Some(going) => 1.0 - going.round(),
            None if from < 0.5 => 1.0,
            None => 0.0,
        };
        self.control.glide_to(target, from, to, now as f32, self.comp.auto_fade_secs);
    }

    /// A clip button was let go: flash clips go back, piano clips stop (right away, never quantized).
    pub fn release_clip(&mut self, layer: u64, slot: usize, now: f64) {
        self.comp.release_clip(layer, slot, now as f32);
    }

    /// Puts a source on a layer using the launch settings (cut/fade, now/beat/bar).
    pub fn launch_source(&mut self, layer: u64, source: phosphor_engine::SourceSpec, values: Vec<f32>, name: String, now: f64) {
        self.trigger(Trigger::Source { layer, source, values, name }, now);
    }

    fn fire(&mut self, t: Trigger, now: f32) {
        match t {
            Trigger::Clip { layer, slot } => self.comp.press_clip(layer, slot, now),
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
        self.displays = crate::screens::displays();
        if let Some(o) = self.comp.output_mut(id) {
            o.screen = crate::screens::screen_key(&self.displays, &display);
        }
        let mut map = self.output.lock().unwrap();
        let out = map.entry(id).or_default();
        out.open = true;
        out.display = Some(display.id);
        out.place = Some((display, 0, 0));
    }

    /// Sends every enabled output to the screen saved for it. Returns the names of saved screens that aren't connected.
    pub fn send_outputs_to_screens(&mut self) -> Vec<String> {
        self.displays = crate::screens::displays();
        let mut missing = Vec::new();
        let wanted: Vec<(u64, String)> = self.comp.outputs.iter().filter(|o| o.enabled && !o.screen.is_empty()).map(|o| (o.id, o.screen.clone())).collect();
        for (id, key) in wanted {
            match crate::screens::find_screen(&self.displays, &key) {
                Some(d) => self.send_output_id_to(id, d),
                None => missing.push(key),
            }
        }
        missing
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
    /// `--tab all`: how many panels have been shown so far.
    tabs_shown: usize,
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
    /// Open the exported show locked; the password to unlock it (empty = no password).
    pub lock: bool,
    pub lock_password: String,
    /// (media files, bytes, missing), worked out when the window opens.
    pub summary: Option<(usize, u64, usize)>,
    #[cfg(not(target_arch = "wasm32"))]
    pub progress: Option<crate::export::Shared>,
}

pub struct PhosphorApp {
    /// The workspace (built-in layout) showing, and how each one was left this session.
    pub(crate) workspace: Option<usize>,

    /// Interface brightness, 0 = darkest … 1 = light (`theme::set_brightness`).
    pub(crate) ui_brightness: f32,
    /// The last notice written to the session log (each new one is logged once).
    pub(crate) logged_notice: Option<String>,
    pub(crate) workspace_docks: HashMap<usize, (DockState<Tab>, Vec<Tab>)>,
    state: State,
    #[cfg(target_os = "macos")]
    native_menu: Option<(muda::Menu, Vec<crate::menu::Action>)>,
    title: String,
    /// Exported show: open its outputs on the first frames.
    show_start: Option<u32>,
    dock: DockState<Tab>,
    bench: Option<BenchRun>,
    /// This run is a bench/test (stays true after the bench finishes and `bench` is cleared): nothing it does may be
    /// saved over the user's project, settings or crash-recovery snapshot.
    bench_session: bool,
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
    /// Locked player mode (see `lock.rs`).
    pub(crate) lock: lock::Lock,
    /// View › Lock Panels: tabs can't be dragged, closed or split off (everything else works as usual).
    pub(crate) panels_locked: bool,
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
    /// UI passes so far (the bench reports UI fps next to engine fps).
    ui_frames: u64,
    /// The first-launch card is showing.
    welcome_open: bool,
    #[cfg(not(target_arch = "wasm32"))]
    heartbeat: crate::recovery::Heartbeat,
}

/// Bench `--drag`: one step of a scripted mouse drag per UI frame, starting a second in (the panels are drawn).
fn bench_drag(run: &mut BenchRun, ctx: &egui::Context, raw: &mut egui::RawInput) {
    let Some((from, to)) = run.cfg.drag.clone() else { return };
    let point = |s: &str| -> Option<egui::Pos2> {
        if s == "studio" {
            return ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("studio_canvas_rect"))).map(|r| r.center());
        }
        let (x, y) = s.split_once(',')?;
        Some(egui::pos2(x.trim().parse().ok()?, y.trim().parse().ok()?))
    };
    let step = ctx.data(|d| d.get_temp::<u32>(egui::Id::new("bench_drag_step"))).unwrap_or(0);
    if ctx.input(|i| i.time) < 1.0 {
        return;
    }
    let (Some(a), Some(b)) = (point(&from), point(&to)) else { return };
    const STEPS: u32 = 12;
    let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
    match step {
        0 => raw.events.extend([egui::Event::PointerMoved(a), button(a, true)]),
        s if s <= STEPS => raw.events.push(egui::Event::PointerMoved(a + (b - a) * (s as f32 / STEPS as f32))),
        s if s == STEPS + 1 => raw.events.push(button(b, false)),
        _ => {
            run.cfg.drag = None;
            println!("DRAG done {from} -> {to}");
        }
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("bench_drag_step"), step + 1));
}

impl eframe::App for PhosphorApp {
    /// Bench `--right-click-studio`: a real right-click (press + release) in the middle of the Studio canvas, a
    /// moment after it has been drawn, so a UI shot can show the menu.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        let Some(run) = self.bench.as_mut() else { return };
        bench_drag(run, ctx, raw);
        if !run.cfg.right_click_studio {
            return;
        }
        let Some(rect) = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("studio_canvas_rect"))) else { return };
        let p = rect.center() + egui::vec2(rect.width() * 0.08, 0.0);
        raw.events.push(egui::Event::PointerMoved(p));
        for pressed in [true, false] {
            raw.events.push(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed, modifiers: Default::default() });
        }
        run.cfg.right_click_studio = false;
    }

    /// Runs every frame, and on its own while the control window is minimized or covered
    /// (eframe then skips `ui`). The outputs, Syphon, NDI and streams must keep going then, so
    /// render here when the UI hasn't drawn recently.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let (at, time) = self.last_ui;
        let idle = at.elapsed().as_secs_f64();
        if idle > 0.05 {
            let now = time + idle;
            if self.hidden_pacer.due(now, self.state.render_rate.fps().unwrap_or(60).min(60)) {
                self.render(now);
                // A bench whose window is covered gets no UI frames: it still has to finish.
                if self.bench.is_some() {
                    self.finish_bench(ctx, now);
                }
            }
            ctx.request_repaint();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // When the window is covered, macOS stops pacing redraws to the display, and the loop
        // would spin at thousands of fps: hold the render rate (120 at most) with a sleep.
        // With a render-rate cap, a pass that comes early (a 120 Hz display) waits for its slot instead:
        // UI and engine then both run at the cap, and the core idles in between.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let since = self.last_ui.0.elapsed().as_secs_f64();
            let slot = self.state.render_rate.fps().map_or(1.0 / 120.0, |f| 1.0 / f as f64);
            if since < slot - 0.0015 {
                std::thread::sleep(std::time::Duration::from_secs_f64(slot - since - 0.001));
            }
        }
        self.last_ui = (web_time::Instant::now(), ui.ctx().input(|i| i.time));
        self.ui_frames += 1;
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
            if !self.bench_session && self.saver.due(now) {
                let snap = self.snapshot();
                self.saver.save(now, &snap);
            }
            if !self.bench_session {
                self.state.autosave_tick(now);
            }
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.save_settings(storage);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn on_exit(&mut self) {
        if !self.bench_session {
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
                crate::account::Event::Saves(text) => {
                    let added = crate::presets::merge_from_account(&text);
                    if added > 0 {
                        for store in [&mut st.presets, &mut st.layer_presets, &mut st.chain_presets, &mut st.venues, &mut st.surface_presets, &mut st.mapping_presets] {
                            store.refresh();
                        }
                        st.notice = Some(format!("{added} saved look(s), venue(s), surface(s) or mapping set(s) came from your account"));
                    }
                }
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

    /// Uploads your layouts and named saves when signed in and they changed (checked every 30 s; the account skips
    /// unchanged text).
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
        // Named saves ride along (the account skips unchanged text).
        #[cfg(not(target_arch = "wasm32"))]
        self.state.account.push_doc(ctx, crate::account::DocKind::Saves, &crate::presets::export_all());
    }
}

/// A quick fingerprint of a project's text (autosave: has it changed since the last save?).
fn hash_text(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}
