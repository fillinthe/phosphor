//! FFGL plugins (FreeFrame GL 2: the effect and source plugins Resolume, VDMX and others load), Mac only.
//!
//! Finding them: `.bundle`s in the usual FreeFrame folders, Resolume's Extra Effects folders and Phosphor's own
//! `ffgl` folder (`folders`). Each new or changed bundle is opened once in a separate Phosphor process
//! (`--ffgl-probe`) so a broken plugin can't take the app down; what it says (name, kind, parameters) is cached.
//! Each usable one becomes an effect (`ffgl:…`) or a source (`ffglgen:…`) like any shader, its parameters ordinary
//! settings (MIDI, modulation, cues).
//!
//! Running them (`Ffgl::run`, once per frame, UI thread): the engine lists what it drew with a plugin
//! (`take_external_jobs`); before the next frame each job's plugin instance draws in OpenGL from the effect's
//! input picture into one of its two result pictures (`gl::Shared`, no CPU copies) while the engine shows the
//! other, finished one. A new draw starts only once the last has finished (a GL fence, never waited on), so a
//! slow plugin updates less often but never slows the show down.

pub mod gl;
pub mod plugin;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use phosphor_engine::shaders::{self, Param, ShaderDesc};
use phosphor_engine::{Engine, ExternalJob};
use serde::{Deserialize, Serialize};

use plugin::{ParamInfo, Plugin};

/// Instances not asked for in this many frames are closed (the effect went away).
const KEEP_FRAMES: u64 = 120;

/// One plugin file and what it said about itself (or why it can't be used).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub path: PathBuf,
    /// Effect / generator id (`ffgl:slug` / `ffglgen:slug`).
    pub id: String,
    pub name: String,
    /// `plugin::EFFECT`, `SOURCE` or `MIXER`.
    pub kind: u32,
    pub params: Vec<ParamInfo>,
    pub error: Option<String>,
    /// The bundle's modification time (cache key).
    pub modified: u64,
}

/// Folders searched for plugins (the ones that exist).
pub fn folders() -> Vec<PathBuf> {
    let mut list = Vec::new();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        list.push(home.join("Library/Graphics/FreeFrame Plug-Ins"));
        for app in ["Resolume Arena", "Resolume Avenue"] {
            list.push(home.join("Documents").join(app).join("Extra Effects"));
        }
    }
    list.push(PathBuf::from("/Library/Graphics/FreeFrame Plug-Ins"));
    list.extend(crate::show::storage_dir().map(|d| d.join("ffgl")));
    // For tests: one more folder.
    list.extend(std::env::var_os("PHOSPHOR_FFGL_DIR").map(PathBuf::from));
    list.retain(|d| d.is_dir());
    list
}

/// Every `.bundle` in `dirs` (and one folder level down).
fn bundles(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "bundle") {
                out.push(p);
            } else if p.is_dir()
                && let Ok(inner) = std::fs::read_dir(&p)
            {
                out.extend(inner.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "bundle")));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn modified(path: &Path) -> u64 {
    std::fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())
}

fn slug(path: &Path) -> String {
    phosphor_engine::custom::slug(&path.file_stem().unwrap_or_default().to_string_lossy())
}

/// `--ffgl-probe <bundle>`: opens the plugin in this (throwaway) process and prints what it says as JSON.
pub fn probe_main(path: &Path) -> String {
    let entry = probe(path);
    serde_json::to_string(&entry).unwrap_or_default()
}

/// Opens a plugin and asks it about itself (in-process: only the probe process does this).
fn probe(path: &Path) -> Entry {
    let mut entry = Entry { path: path.to_path_buf(), id: String::new(), name: path.file_stem().unwrap_or_default().to_string_lossy().to_string(), kind: plugin::EFFECT, params: Vec::new(), error: None, modified: modified(path) };
    let result = Plugin::load(path).and_then(|p| p.init().map(|info| (p, info)));
    match result {
        Ok((p, info)) => {
            if !info.name.is_empty() {
                entry.name = info.name.clone();
            }
            entry.kind = info.kind;
            entry.params = p.params();
            entry.error = match info.kind {
                plugin::MIXER => Some("mixes two pictures (not supported yet)".into()),
                plugin::EFFECT if info.inputs > 1 => Some("needs two pictures (not supported yet)".into()),
                plugin::EFFECT | plugin::SOURCE => None,
                k => Some(format!("unknown kind {k}")),
            };
        }
        Err(e) => entry.error = Some(e),
    }
    let prefix = if entry.kind == plugin::SOURCE { shaders::FFGL_GENERATOR } else { shaders::FFGL_EFFECT };
    entry.id = format!("{prefix}{}", slug(path));
    entry
}

/// Probes a plugin in a child process (killed after 10 s): a crash there is just an unusable plugin.
fn probe_safely(path: &Path) -> Entry {
    let failed = |e: &str| Entry { path: path.to_path_buf(), id: format!("{}{}", shaders::FFGL_EFFECT, slug(path)), name: path.file_stem().unwrap_or_default().to_string_lossy().to_string(), kind: plugin::EFFECT, params: Vec::new(), error: Some(e.to_string()), modified: modified(path) };
    let Ok(exe) = std::env::current_exe() else { return failed("can't start the check") };
    let Ok(mut child) = std::process::Command::new(exe).arg("--ffgl-probe").arg(path).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).spawn() else {
        return failed("can't start the check");
    };
    // Read what it prints as it goes (a chatty plugin would otherwise fill the pipe and look hung).
    let reader = child.stdout.take().map(|mut out| {
        std::thread::spawn(move || {
            use std::io::Read;
            let mut s = String::new();
            let _ = out.read_to_string(&mut s);
            s
        })
    });
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed().as_secs() < 10 => std::thread::sleep(std::time::Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return failed("it hung when opened");
            }
        }
    }
    let out = reader.and_then(|r| r.join().ok()).unwrap_or_default();
    out.lines().rev().find_map(|l| serde_json::from_str::<Entry>(l).ok()).unwrap_or_else(|| failed("it crashed when opened"))
}

fn cache_path() -> Option<PathBuf> {
    crate::show::storage_dir().map(|d| d.join("ffgl-cache.json"))
}

/// Finds every plugin, probing the new or changed ones (background thread).
fn scan() -> Vec<Entry> {
    let cached: Vec<Entry> = cache_path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let mut list: Vec<Entry> = bundles(&folders())
        .into_iter()
        .map(|path| {
            let m = modified(&path);
            cached.iter().find(|e| e.path == path && e.modified == m).cloned().unwrap_or_else(|| probe_safely(&path))
        })
        .collect();
    // The same bundle name in two folders: the second gets its own id.
    for i in 0..list.len() {
        let mut n = 2;
        while list[..i].iter().any(|e| e.id == list[i].id) {
            let base = list[i].id.trim_end_matches(|c: char| c.is_ascii_digit() || c == '_').to_string();
            list[i].id = format!("{base}_{n}");
            n += 1;
        }
    }
    if let Some(p) = cache_path() {
        let _ = std::fs::write(p, serde_json::to_string(&list).unwrap_or_default());
    }
    list
}

/// The settings Phosphor shows for a plugin: its parameters that are numbers (text and file ones are left out).
fn shown_params(params: &[ParamInfo]) -> Vec<&ParamInfo> {
    params.iter().filter(|p| !matches!(p.kind, plugin::TYPE_TEXT | plugin::TYPE_FILE | plugin::TYPE_BUFFER)).collect()
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// Registers a usable plugin as an effect or source (`shaders::register_custom`).
fn register(e: &Entry) {
    let params: Vec<Param> = shown_params(&e.params)
        .into_iter()
        .map(|p| {
            let name = leak(p.name.clone());
            match p.kind {
                plugin::TYPE_OPTION if !p.options.is_empty() => {
                    let labels: Vec<&'static str> = p.options.iter().map(|(n, _)| leak(n.clone()) as &'static str).collect();
                    let default = p.options.iter().position(|(_, v)| (*v - p.default).abs() < 1e-4).unwrap_or(0) as f32;
                    Param { name, min: 0.0, max: (labels.len() - 1) as f32, default, labels: Box::leak(labels.into_boxed_slice()) }
                }
                plugin::TYPE_BOOLEAN | plugin::TYPE_EVENT => Param { name, min: 0.0, max: 1.0, default: p.default.round().clamp(0.0, 1.0), labels: &["Off", "On"] },
                _ => Param::new(name, 0.0, 1.0, p.default.clamp(0.0, 1.0)),
            }
        })
        .collect();
    let id = leak(e.id.clone());
    let desc: &'static ShaderDesc = Box::leak(Box::new(ShaderDesc { id, name: leak(e.name.clone()), entry: leak(format!("ext:{id}")), params: Box::leak(params.into_boxed_slice()) }));
    shaders::register_custom(desc);
    let kind = if e.kind == plugin::SOURCE { "source" } else { "effect" };
    shaders::set_custom_info(id, "FFGL", leak(format!("FFGL {kind}: {}", e.path.file_name().unwrap_or_default().to_string_lossy())));
}

/// A plugin opened in this process.
struct Loaded {
    plugin: Plugin,
    /// Phosphor setting i → (plugin parameter, option values).
    params: Vec<(u32, Vec<f32>)>,
}

/// One running plugin instance: its pictures and when it was last asked for.
struct Instance {
    id: &'static str,
    ptr: *mut std::ffi::c_void,
    size: (u32, u32),
    send: Option<gl::Shared>,
    /// Two results: the engine shows `recv[front]` while the plugin draws the other.
    recv: [gl::Shared; 2],
    front: usize,
    /// The draw into the back result, until the GPU has finished it.
    drawing: Option<gl::Fence>,
    input: gl::Canvas,
    output: gl::Canvas,
    used: u64,
}

/// FFGL in the app: the plugin list, the GL context and the running instances.
#[derive(Default)]
pub struct Ffgl {
    /// Every plugin found (usable or not), for the Effects list and Devices.
    pub entries: Vec<Entry>,
    scanning: Option<mpsc::Receiver<Vec<Entry>>>,
    loaded: HashMap<&'static str, Loaded>,
    /// Plugins that failed to open here (not tried again this session).
    broken: Vec<&'static str>,
    gl: Option<(gl::Gl, gl::Copier)>,
    gl_failed: bool,
    instances: HashMap<u64, Instance>,
    /// What the engine drew with a plugin in its last frame.
    jobs: Vec<ExternalJob>,
    frame: u64,
}

impl Drop for Ffgl {
    /// Instances go first, with their GL context current (their textures and framebuffers are GL objects), then
    /// the context.
    fn drop(&mut self) {
        if let Some((g, _)) = &self.gl {
            g.make_current();
            for (_, i) in self.instances.drain() {
                if let Some(l) = self.loaded.get(i.id) {
                    l.plugin.deinstantiate(i.ptr);
                }
            }
        }
    }
}

impl Ffgl {
    /// Looks for plugins in the background (call once at start; `poll` picks up the result).
    pub fn start_scan(&mut self) {
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("ffgl scan".into()).spawn(move || {
            let _ = tx.send(scan());
        });
        if spawned.is_ok() {
            self.scanning = Some(rx);
        }
    }

    /// Registers the plugins once the scan is done. Returns true when it just finished.
    pub fn poll(&mut self) -> bool {
        let Some(list) = self.scanning.as_ref().and_then(|rx| rx.try_recv().ok()) else { return false };
        self.scanning = None;
        for e in list.iter().filter(|e| e.error.is_none()) {
            register(e);
        }
        self.entries = list;
        true
    }

    /// Takes what the engine drew with a plugin this frame (call right after `Engine::render`).
    pub fn take_jobs(&mut self, engine: &mut Engine) {
        self.jobs = engine.take_external_jobs();
    }

    fn load(&mut self, id: &'static str) -> Option<&Loaded> {
        if self.broken.contains(&id) {
            return None;
        }
        if !self.loaded.contains_key(id) {
            let Some(e) = self.entries.iter().find(|e| e.id == id) else {
                self.broken.push(id);
                return None;
            };
            let opened = Plugin::load(&e.path).and_then(|p| p.init().map(|_| p));
            let Ok(plugin) = opened else {
                self.broken.push(id);
                return None;
            };
            let params = shown_params(&e.params).into_iter().map(|p| (p.index, p.options.iter().map(|(_, v)| *v).collect())).collect();
            self.loaded.insert(id, Loaded { plugin, params });
        }
        self.loaded.get(id)
    }

    /// Draws last frame's plugin jobs (before the next `Engine::render`): each instance reads its effect's input and
    /// draws its result into the picture the engine shows. `time` in seconds; the beat for plugins that follow it.
    pub fn run(&mut self, engine: &mut Engine, time: f64, bpm: f32, bar_phase: f32) {
        self.frame += 1;
        let jobs = std::mem::take(&mut self.jobs);
        if jobs.is_empty() && self.instances.is_empty() {
            return;
        }
        if self.gl.is_none() && !self.gl_failed {
            self.gl = gl::Gl::new().and_then(|g| {
                g.make_current();
                Some((g, gl::Copier::new()?))
            });
            self.gl_failed = self.gl.is_none();
        }
        let Some((glc, copier)) = self.gl.take() else { return };
        glc.make_current();
        // GL may only read the engine's input copies (and draw over results the engine last showed) once the GPU has
        // finished the frame that made them; if it hasn't yet, the plugins skip this frame (never a wait here).
        if !engine.gpu_caught_up() {
            let frame = self.frame;
            for j in &jobs {
                if let Some(i) = self.instances.get_mut(&j.key) {
                    i.used = frame;
                }
            }
            self.gl = Some((glc, copier));
            return;
        }
        for job in &jobs {
            self.run_job(&glc, &copier, engine, job, time, bpm, bar_phase);
        }
        glc.flush();
        // Close instances nothing asked for lately.
        let now = self.frame;
        let stale: Vec<u64> = self.instances.iter().filter(|(_, i)| now - i.used > KEEP_FRAMES).map(|(k, _)| *k).collect();
        for k in stale {
            if let Some(i) = self.instances.remove(&k)
                && let Some(l) = self.loaded.get(i.id)
            {
                l.plugin.deinstantiate(i.ptr);
            }
        }
        self.gl = Some((glc, copier));
    }

    #[allow(clippy::too_many_arguments)]
    fn run_job(&mut self, glc: &gl::Gl, copier: &gl::Copier, engine: &mut Engine, job: &ExternalJob, time: f64, bpm: f32, bar_phase: f32) {
        let (w, h) = (job.size.0.max(1), job.size.1.max(1));
        // A new instance (or a new size: start again).
        let fresh = self.instances.get(&job.key).is_none_or(|i| i.size != (w, h) || i.id != job.id);
        if fresh {
            if let Some(old) = self.instances.remove(&job.key)
                && let Some(l) = self.loaded.get(old.id)
            {
                l.plugin.deinstantiate(old.ptr);
            }
            let Some(ptr) = self.load(job.id).and_then(|l| l.plugin.instantiate(w, h)) else {
                if !self.broken.contains(&job.id) {
                    self.broken.push(job.id);
                }
                return;
            };
            let device = engine.device().clone();
            let made = (|| {
                let send = if job.has_input { Some(gl::Shared::new(glc, &device, w, h)?) } else { None };
                let recv = [gl::Shared::new(glc, &device, w, h)?, gl::Shared::new(glc, &device, w, h)?];
                Some((send, recv, gl::Canvas::new(w, h)?, gl::Canvas::new(w, h)?))
            })();
            let Some((send, recv, input, output)) = made else {
                if let Some(l) = self.loaded.get(job.id) {
                    l.plugin.deinstantiate(ptr);
                }
                self.broken.push(job.id);
                return;
            };
            // Input only until the first result is ready (the picture passes through, no blank frame).
            engine.set_external_textures(job.key, send.as_ref().map(|s| &s.texture), None);
            self.instances.insert(job.key, Instance { id: job.id, ptr, size: (w, h), send, recv, front: 1, drawing: None, input, output, used: self.frame });
        }
        let (Some(inst), Some(l)) = (self.instances.get_mut(&job.key), self.loaded.get(job.id)) else { return };
        inst.used = self.frame;
        // The last draw: still going (leave it), or done (show it, then start the next).
        if let Some(f) = &inst.drawing {
            if !f.done() {
                return;
            }
            inst.drawing = None;
            inst.front = 1 - inst.front;
            engine.set_external_textures(job.key, inst.send.as_ref().map(|s| &s.texture), Some(&inst.recv[inst.front].texture));
        }
        for ((index, options), v) in l.params.iter().zip(&job.params) {
            let value = if options.is_empty() { *v } else { options.get(v.round().max(0.0) as usize).copied().unwrap_or(0.0) };
            l.plugin.set_param(inst.ptr, *index, value);
        }
        l.plugin.set_time(inst.ptr, time);
        l.plugin.set_beat(inst.ptr, bpm, bar_phase);
        // Input: the shared picture, turned the right way up for GL into a plain texture.
        if let Some(send) = &inst.send {
            copier.read(send, &inst.input);
            // The engine writes this input again next frame: let GL's copy of it finish first (a tiny copy).
            gl::Fence::new().wait(std::time::Duration::from_millis(2));
        }
        inst.output.clear(w, h);
        let tex = plugin::GlTexture { width: w, height: h, hardware_width: w, hardware_height: h, handle: inst.input.tex };
        let inputs: Vec<&plugin::GlTexture> = if inst.send.is_some() { vec![&tex] } else { Vec::new() };
        l.plugin.process(inst.ptr, &inputs, inst.output.fbo);
        copier.write(&inst.output, &inst.recv[1 - inst.front]);
        inst.drawing = Some(gl::Fence::new());
    }
}

/// Bench `--quick ffgl`: the first usable plugin (an effect on the top layer, or a source for it). Waits for the
/// scan. Returns what it did.
pub fn bench_apply(st: &mut crate::app::State) -> Option<String> {
    for _ in 0..500 {
        if st.ffgl.poll() || st.ffgl.scanning.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let report: Vec<String> = st.ffgl.entries.iter().map(|e| format!("{} [{}]{}", e.name, e.id, e.error.as_ref().map(|x| format!(" ({x})")).unwrap_or_default())).collect();
    println!("FFGL found {}: {}", st.ffgl.entries.len(), report.join(", "));
    let want = std::env::var("PHOSPHOR_FFGL_PICK").ok();
    let e = st.ffgl.entries.iter().find(|e| e.error.is_none() && want.as_ref().is_none_or(|w| e.id.contains(w.as_str())))?.clone();
    let layer = st.comp.layers.last().map(|l| l.id)?;
    let id: &'static str = shaders::effect(&e.id).or_else(|| shaders::generator(&e.id)).map(|d| d.id)?;
    crate::explorer::apply_palette(st, crate::explorer::PaletteDrag { effect: shaders::effect(id).map(|d| d.id), generator: shaders::generator(id).map(|d| d.id) }, Some(layer));
    // PHOSPHOR_FFGL_VALUES="1,0.5": the effect's settings (checks they reach the plugin).
    if let Some(values) = std::env::var("PHOSPHOR_FFGL_VALUES").ok().map(|v| v.split(',').filter_map(|x| x.parse::<f32>().ok()).collect::<Vec<_>>())
        && let Some(fx) = st.comp.layers.iter_mut().find(|l| l.id == layer).and_then(|l| l.effects.iter_mut().rev().find(|f| f.effect == id))
    {
        for (slot, v) in fx.values.iter_mut().zip(values) {
            *slot = v;
        }
    }
    Some(format!("{} on layer {layer}", e.name))
}
