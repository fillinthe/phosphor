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
use crate::control::ControlHub;
use crate::media::{self, MediaManager};
use crate::panels::{self, Tab, Viewer};
use crate::presets::PresetStore;
use crate::stats::{Snapshot, Stats};
use crate::stream::Streamer;
use crate::theme;

const KEY_COMPOSITION: &str = "composition";
const KEY_DOCK: &str = "dock";
const KEY_STREAM: &str = "stream";

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

    pub fn load_composition(&mut self, comp: Composition) {
        self.selected = comp.layers.last().map(|l| l.id);
        self.comp = comp;
        self.notice = None;
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
}

pub struct PhosphorApp {
    state: State,
    dock: DockState<Tab>,
    bench: Option<BenchRun>,
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
        let mut dock = stored(KEY_DOCK).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(default_layout);
        ensure_all_tabs(&mut dock);

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
                mod_inputs: ModInputs::default(),
            },
            dock,
            bench: bench.map(|cfg| BenchRun { cfg, started: None, samples: Vec::new() }),
        };
        if open_audio && let Err(e) = app.state.audio.open(None) {
            eprintln!("audio input: {e}");
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
        let State { comp, engine, render_state, texture, texture_generation, layer_textures, media, stats, output, audio, mod_inputs, stream, .. } =
            &mut self.state;
        let Some(engine) = engine.as_mut() else { return };
        media.sync(comp, engine);
        media.update_video(comp, engine, now);

        let an = audio.update();
        *mod_inputs = ModInputs {
            time: now as f32,
            audio_level: an.level,
            audio_low: an.low,
            audio_mid: an.mid,
            audio_high: an.high,
        };
        let t0 = web_time::Instant::now();
        let info = engine.render(comp, now as f32, mod_inputs);
        let render_secs = t0.elapsed().as_secs_f64();

        if stream.frame_due(now) && !engine.capture_frame() {
            stream.repeat_last();
        }
        for f in engine.poll_captures() {
            stream.push(f.width, f.height, f.rgba);
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
            layer_textures.retain(|id, (tex, _)| {
                let live = comp.layers.iter().any(|l| l.id == *id);
                if !live {
                    renderer.free_texture(tex);
                }
                live
            });
        }
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
                ui.label(egui::RichText::new("PHOSPHOR").strong().size(15.0).color(theme::ACCENT));
                ui.separator();
                if ui.button(format!("{} Layer", icon::PLUS)).clicked() {
                    self.state.add_layer();
                }
                let output_label = if self.state.output_open() { format!("{} Output", icon::STOP) } else { format!("{} Output", icon::PLAY) };
                if ui.button(output_label).on_hover_text("Output window. F = fullscreen, Esc = exit").clicked() {
                    self.state.toggle_output();
                }
                ui.separator();
                panels::project_buttons(ui, &mut self.state);
                ui.separator();
                ui.menu_button("View", |ui| {
                    ui.weak("Drag any tab to move it; drag it outside to float it.");
                    if ui.button("Reset layout").clicked() {
                        self.dock = default_layout();
                        ui.close();
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let s = self.state.stats.latest;
                    ui.monospace(format!("{:5.1} fps  engine {:4.2} ms  CPU {:4.1}%  {:4.0} MB", s.fps, s.render_ms, s.cpu_percent, s.memory_mb));
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
            egui::ViewportBuilder::default().with_title("Phosphor Output").with_inner_size([960.0, 540.0]),
            move |ui, _class| {
                let ctx = ui.ctx().clone();
                let mut out = shared.lock().unwrap();
                let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                if ctx.input(|i| i.key_pressed(egui::Key::E)) {
                    out.edit_mapping = !out.edit_mapping;
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
        self.sync_mapping_from_output();
        {
            let State { control, comp, .. } = &mut self.state;
            control.poll(comp);
            control.apply(comp);
        }
        self.render(now);
        self.top_bar(ui);

        egui::CentralPanel::no_frame().show(ui, |ui| {
            let mut style = egui_dock::Style::from_egui(ui.style());
            style.tab_bar.fill_tab_bar = true;
            DockArea::new(&mut self.dock)
                .style(style)
                .show_close_buttons(false)
                .show_leaf_close_all_buttons(false)
                .show_leaf_collapse_buttons(false)
                .show_inside(ui, &mut Viewer { st: &mut self.state });
        });

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
    }
}

/// Preview + library on the left, layer stack + inspector on the right.
fn default_layout() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Preview]);
    let surface = dock.main_surface_mut();
    let [left, right] = surface.split_right(NodeIndex::root(), 0.64, vec![Tab::Layers]);
    surface.split_below(right, 0.5, vec![Tab::Inspector]);
    surface.split_below(left, 0.68, vec![Tab::Library, Tab::Presets, Tab::Audio, Tab::Control, Tab::Mapping, Tab::Stream, Tab::Output]);
    dock
}

/// Panels added in newer builds show up in layouts saved by older ones.
fn ensure_all_tabs(dock: &mut DockState<Tab>) {
    for tab in Tab::ALL {
        if dock.find_tab(&tab).is_none() {
            dock.push_to_focused_leaf(tab);
        }
    }
}
