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

/// State the output window (a separate OS window) reads each frame.
#[derive(Default)]
pub struct OutputShared {
    pub open: bool,
    pub texture: Option<egui::TextureId>,
    pub aspect: f32,
    /// Copy of the composition's mapping; the output window edits it when `edit_mapping` is on.
    pub mapping: Mapping,
    pub edit_mapping: bool,
    pub mapping_changed: bool,
    pub selected_surface: Option<u64>,
    /// Move the output window to this display and go fullscreen: (display, step, frames waited).
    pub place: Option<(crate::screens::Display, u8, u32)>,
}

/// A clip trigger, fired now or on the next beat when quantised.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Trigger {
    Clip { layer: u64, slot: usize },
    Column(usize),
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
    pub output: Arc<Mutex<OutputShared>>,
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
    pub clock: Clock,
    pub quantize: panels::Quantize,
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

    pub fn load_composition(&mut self, comp: Composition) {
        self.selected = comp.layers.last().map(|l| l.id);
        self.comp = comp;
        self.notice = None;
    }

    /// Fires a trigger now, or queues it for the next beat when quantise is on.
    pub fn trigger(&mut self, t: Trigger, now: f64) {
        if self.quantize != panels::Quantize::Off {
            if !self.pending_triggers.contains(&t) {
                self.pending_triggers.push(t);
            }
        } else {
            self.fire(t, now as f32);
        }
    }

    fn fire(&mut self, t: Trigger, now: f32) {
        match t {
            Trigger::Clip { layer, slot } => {
                if let Some(l) = self.comp.layer_mut(layer) {
                    l.trigger_clip(slot, now);
                }
            }
            Trigger::Column(col) => self.comp.trigger_column(col, now),
        }
    }

    /// Opens the output on `display`, fullscreen.
    pub fn send_output_to(&mut self, display: crate::screens::Display) {
        let mut out = self.output.lock().unwrap();
        out.open = true;
        out.place = Some((display, 0, 0));
    }

    pub fn toggle_output(&mut self) {
        let mut out = self.output.lock().unwrap();
        out.open = !out.open;
    }

    pub fn output_open(&self) -> bool {
        self.output.lock().unwrap().open
    }
}

struct BenchRun {
    cfg: BenchConfig,
    started: Option<f64>,
    samples: Vec<Snapshot>,
    /// Rebuilt the composition once imported (ISF) shaders were registered.
    rebuilt: bool,
}

pub struct PhosphorApp {
    state: State,
    dock: DockState<Tab>,
    bench: Option<BenchRun>,
    gather_windows: bool,
    /// Panels living in their own OS windows.
    detached: Vec<Tab>,
}

impl PhosphorApp {
    pub fn new(cc: &eframe::CreationContext<'_>, bench: Option<BenchConfig>) -> Self {
        theme::apply(&cc.egui_ctx);

        let stored = |key: &str| if bench.is_none() { cc.storage.and_then(|s| s.get_string(key)) } else { None };
        let comp = match &bench {
            Some(cfg) => cfg.composition(),
            None => stored(KEY_COMPOSITION)
                .and_then(|json| Composition::from_json(&json).ok())
                .unwrap_or_else(Composition::starter),
        };
        let stream_settings = stored(KEY_STREAM).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let detached: Vec<Tab> = stored(KEY_DETACHED).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let mut dock = stored(KEY_DOCK).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(default_layout);
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
                output: Arc::new(Mutex::new(OutputShared::default())),
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
                clock: Clock::new(comp_bpm),
                quantize: panels::Quantize::Off,
                pending_triggers: Vec::new(),
                clip_page: 0,
                mod_inputs: ModInputs::default(),
            },
            dock,
            bench: bench.map(|cfg| BenchRun { cfg, started: None, samples: Vec::new(), rebuilt: false }),
            gather_windows: false,
            detached,
        };
        if open_audio && let Err(e) = app.state.audio.open(None) {
            eprintln!("audio input: {e}");
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
        let beat_phase = clock.phase(now);
        *mod_inputs = ModInputs {
            beats: clock.beats(now) as f32,
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
            // Stills refresh when the set of tiles changes and every 2 s; live tiles every frame.
            let stale = now - *preview_refreshed > 2.0;
            if stale {
                *preview_refreshed = now;
            }
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

        let stream_due = stream.frame_due(now);
        let net_due = net.frame_due(now);
        if (stream_due || net_due) && !engine.capture_frame() && stream_due {
            stream.repeat_last();
        }
        for f in engine.poll_captures() {
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
            let mut out = output.lock().unwrap();
            out.texture = *texture;
            out.aspect = comp.width as f32 / comp.height.max(1) as f32;
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
        let mapped: std::sync::Arc<Vec<phosphor_engine::ControlTarget>> = std::sync::Arc::new(st.comp.control_maps.iter().map(|m| m.target.clone()).collect());
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(panels::MAPPED_TARGETS), mapped));
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
            st.control.learning = None;
        }
    }

    fn learning_banner(&mut self, ctx: &egui::Context) {
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

    #[cfg(not(target_arch = "wasm32"))]
    fn save_snapshot(&mut self) {
        let Some(engine) = &self.state.engine else { return };
        let Some((w, h, pixels)) = engine.read_output() else { return };
        if let Some(path) = rfd::FileDialog::new().add_filter("PNG", &["png"]).set_file_name("Phosphor snapshot.png").save_file()
            && let Err(e) = image::save_buffer(&path, &pixels, w, h, image::ColorType::Rgba8)
        {
            self.state.notice = Some(format!("Snapshot failed: {e}"));
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
            Command::ToggleMapping => st.comp.mapping.enabled = !st.comp.mapping.enabled,
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
        let (delete, duplicate, toggle) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
                i.modifiers.command && i.key_pressed(egui::Key::D),
                i.key_pressed(egui::Key::Space),
            )
        });
        if delete {
            panels::remove_layer(&mut self.state, sel);
        } else if duplicate {
            panels::duplicate_layer(&mut self.state, sel);
        } else if toggle && let Some(l) = self.state.comp.layer_mut(sel) {
            l.enabled = !l.enabled;
        }
    }

    /// Pulls corner drags made in the output window back into the composition.
    fn sync_mapping_from_output(&mut self) {
        let mut out = self.state.output.lock().unwrap();
        if out.mapping_changed {
            self.state.comp.mapping = out.mapping.clone();
            self.state.selected_surface = out.selected_surface;
            out.mapping_changed = false;
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
            if let Some(layer) = self.state.comp.layer_mut(target) {
                layer.name = name.clone();
                layer.source = media::spec_for(&name, path);
            }
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top_bar").show(ui, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("PHOSPHOR").strong().size(13.5).color(theme::ACCENT));
                ui.separator();
                if ui.button(format!("{} Layer", icon::PLUS)).clicked() {
                    self.state.add_layer();
                }
                let output_label = if self.state.output_open() { format!("{} Output", icon::STOP) } else { format!("{} Output", icon::PLAY) };
                if ui.button(output_label).on_hover_text("Output window. F = fullscreen, Esc = exit").clicked() {
                    self.state.toggle_output();
                }
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
                panels::project_buttons(ui, &mut self.state);
                ui.separator();
                if ui.button(format!("{} Search  ⌘K", icon::MAGNIFYING_GLASS)).on_hover_text("Command palette: effects, sources, panels, actions").clicked() {
                    self.state.palette.toggle();
                }
                if ui.button(format!("{} Reset Layout", icon::LAYOUT)).on_hover_text("Bring back every panel in the default arrangement").clicked() {
                    self.dock = default_layout();
                }
                ui.menu_button("View", |ui| {
                    ui.weak("Drag any tab to move it; drag it outside to float it.");
                    if ui.button(format!("{} Add monitor", icon::MONITOR)).on_hover_text("A new preview panel showing the output, the mix, or any layer").clicked() {
                        let _ = self.add_monitor();
                        ui.close();
                    }
                    ui.menu_button("Panels", |ui| {
                        for tab in Tab::ALL {
                            let detached = self.detached.contains(&tab);
                            let open = detached || self.dock.find_tab(&tab).is_some();
                            let name = if detached { format!("{} (own window)", tab.title()) } else { tab.title().to_string() };
                            if ui.selectable_label(open, name).clicked() {
                                if detached {
                                    self.detached.retain(|t| *t != tab);
                                    self.dock.push_to_focused_leaf(tab);
                                } else {
                                    match self.dock.find_tab(&tab) {
                                        Some(path) => {
                                            self.dock.remove_tab(path);
                                        }
                                        None => {
                                            self.dock.add_window(vec![tab]);
                                        }
                                    }
                                }
                            }
                        }
                    });
                    ui.menu_button("Layouts", |ui| {
                        for (name, make) in LAYOUTS {
                            if ui.button(*name).clicked() {
                                self.dock = make();
                                ui.close();
                            }
                        }
                    });
                    if ui.button("Gather floating windows").on_hover_text("Bring every floating panel back onto this screen").clicked() {
                        self.gather_windows = true;
                        ui.close();
                    }
                });
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

    fn output_window(&self, ctx: &egui::Context) {
        if !self.state.output_open() {
            return;
        }
        let id = egui::ViewportId::from_hash_of("phosphor_output");
        let shared = self.state.output.clone();
        ctx.show_viewport_deferred(
            id,
            // Borderless: nothing but picture, so a projector sees no title bar or menu bar.
            egui::ViewportBuilder::default()
                .with_title("Phosphor Output")
                .with_decorations(false)
                .with_inner_size([960.0, 540.0]),
            move |ui, _class| {
                let ctx = ui.ctx().clone();
                let mut out = shared.lock().unwrap();
                let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
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
            self.state.comp.trigger_column(col, now as f32);
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
        }
        self.bench = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

impl eframe::App for PhosphorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);

        self.handle_dropped_files(&ctx);
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
                self.state.comp = run.cfg.composition();
            }
            run.started = None;
            run.samples.clear();
        }
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("phosphor_mod_inputs"), self.state.mod_inputs));
        self.control_requests(&ctx);
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::K)) {
            self.state.palette.toggle();
        }
        self.shortcuts(&ctx);
        self.sync_mapping_from_output();
        {
            let State { control, comp, .. } = &mut self.state;
            control.poll(comp);
            control.apply(comp, now as f32);
        }
        {
            let st = &mut self.state;
            st.comp.bpm = st.clock.bpm;
            let beat = st.clock.tick(now);
            let fire_now = match st.quantize {
                panels::Quantize::Off => true,
                panels::Quantize::Beat => beat,
                panels::Quantize::Bar => beat && st.clock.beat_in_bar(now) == 0,
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
            let mut out = self.state.output.lock().unwrap();
            if !out.mapping_changed {
                out.mapping = self.state.comp.mapping.clone();
                out.selected_surface = self.state.selected_surface;
            }
        }
        self.output_window(&ctx);
        self.finish_bench(&ctx, now);
        // A VJ app renders continuously; the GPU frame rate follows the display's vsync.
        ctx.request_repaint();
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
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
        if let Ok(json) = serde_json::to_string(&self.state.explorer.recent) {
            storage.set_string(KEY_RECENT, json);
        }
        if let Ok(json) = serde_json::to_string(&self.state.monitors) {
            storage.set_string(KEY_MONITORS, json);
        }
    }
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
        vec![Tab::Mixer, Tab::Library, Tab::Audio, Tab::Control, Tab::Stream, Tab::Network, Tab::ShaderEditor, Tab::Output],
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
