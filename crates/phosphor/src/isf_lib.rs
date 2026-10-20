//! Finds ISF shaders on this machine (the folders VDMX and other ISF hosts use) and converts them
//! to WGSL on a background thread. Converted shaders are registered with the engine and compiled
//! lazily the first time they're drawn.

use std::sync::mpsc::Receiver;

use phosphor_engine::Engine;
use phosphor_engine::shaders::{self, Param, ShaderDesc};

pub struct Loaded {
    id: String,
    name: String,
    about: String,
    params: Vec<Param>,
    entry: String,
    wgsl: String,
    /// Multi-pass shaders: how their passes run (empty plan = single pass).
    plan: Option<shaders::PassPlan>,
    /// `IMPORTED` pictures, decoded on the scanner thread: (buffer slot, width, height, RGBA).
    images: Vec<(usize, u32, u32, Vec<u8>)>,
    /// `audio` / `audioFFT` inputs: (texture slot, spectrum?).
    audio_slots: Vec<(usize, bool)>,
    /// A real vertex stage (ISF vertex shaders that move the quad).
    vertex_entry: Option<String>,
}

/// Decodes the pictures an ISF shader imports (paths relative to the shader file; missing ones are skipped)

fn load_images(shader_path: &std::path::Path, images: &[(usize, String)]) -> Vec<(usize, u32, u32, Vec<u8>)> {
    let dir = shader_path.parent().unwrap_or(std::path::Path::new("."));
    images
        .iter()
        .filter_map(|(slot, rel)| {
            let img = image::open(dir.join(rel)).ok()?.to_rgba8();
            Some((*slot, img.width(), img.height(), img.into_raw()))
        })
        .collect()
}

pub struct IsfLibrary {
    rx: Option<Receiver<Result<Loaded, String>>>,
    pub loaded: usize,
    pub skipped: usize,
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// The ISF shaders shipped with Phosphor: Phosphor.app/Contents/Resources/isf on the Mac, `isf` next to the exe
/// elsewhere, or the repo's assets/isf when run from a build folder.
fn bundled_isf() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    [dir.join("../Resources/isf"), dir.join("isf"), std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/isf"))].into_iter().find(|d| d.is_dir())
}

/// Folders scanned for `.fs` files.
pub fn folders() -> Vec<std::path::PathBuf> {
    let mut dirs = vec![std::path::PathBuf::from("/Library/Graphics/ISF")];
    if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
        dirs.push(home.join("Library/Graphics/ISF"));
        dirs.push(home.join("Documents/VDMX Media/ISF"));
    }
    if let Some(d) = crate::show::storage_dir() {
        dirs.push(d.join("isf"));
    }
    dirs.extend(crate::show::shader_dir("isf"));
    // Last, so copies installed elsewhere win: the ISF collection Phosphor ships (assets/isf).
    dirs.extend(bundled_isf());
    // MadMapper's own materials and FX, when it's installed (they're ISF with MadMapper's extras).
    if let Ok(apps) = std::fs::read_dir("/Applications") {
        for app in apps.flatten().map(|e| e.path()).filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("MadMapper") && n.ends_with(".app"))) {
            for sub in ["Materials", "FactorySurface2DFX", "FactoryGenerators"] {
                let d = app.join("Contents/Resources").join(sub);
                if d.is_dir() {
                    dirs.push(d);
                }
            }
        }
    }
    dirs
}

fn params_for(shader: &phosphor_isf::IsfShader) -> Vec<Param> {
    use phosphor_isf::IsfInputKind as K;
    let mut slots: Vec<Option<Param>> = vec![None; shader.param_count().min(shaders::MAX_PARAMS)];
    let mut put = |i: usize, name: String, min: f32, max: f32, default: f32| {
        if let Some(s) = slots.get_mut(i) {
            // Some shaders declare MIN above MAX (Radial Spectrogram: 1 → 0.05); a range must run low to high.
            let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
            *s = Some(Param { name: leak(name), min: lo, max: hi, default: default.clamp(lo, hi) });
        }
    };
    for input in &shader.inputs {
        let Some(o) = input.offset else { continue };
        let name = if input.label.is_empty() { input.name.clone() } else { input.label.clone() };
        match &input.kind {
            K::Float { min, max, default } => put(o, name, *min, *max, *default),
            K::Bool { default } => put(o, name, 0.0, 1.0, *default as i32 as f32),
            K::Event => put(o, name, 0.0, 1.0, 0.0),
            K::Long { values, default, .. } => {
                let lo = values.iter().copied().min().unwrap_or(0) as f32;
                let hi = values.iter().copied().max().unwrap_or(1) as f32;
                put(o, name, lo, hi.max(lo + 1.0), *default as f32)
            }
            K::Int { min, max, default } => put(o, name, *min as f32, *max as f32, *default as f32),
            K::Color { default } => {
                for (k, ch) in ["R", "G", "B", "A"].iter().enumerate() {
                    put(o + k, format!("{name} {ch}"), 0.0, 1.0, default[k]);
                }
            }
            K::Point2D { default, min, max } => {
                put(o, format!("{name} X"), min[0], max[0], default[0]);
                put(o + 1, format!("{name} Y"), min[1], max[1], default[1]);
            }
            K::Curve { .. } => {
                for (k, v) in input.default_floats().into_iter().enumerate() {
                    put(o + k, format!("{name} {}%", k * 100 / (phosphor_isf::CURVE_POINTS - 1)), 0.0, 1.0, v);
                }
            }
            K::Image | K::Audio { .. } => {}
        }
    }
    slots.into_iter().enumerate().map(|(i, p)| p.unwrap_or(Param { name: leak(format!("p{i}")), min: 0.0, max: 1.0, default: 0.0 })).collect()
}

/// The engine's pass plan for a multi-pass ISF shader: which buffer each pass writes (by index)
/// and which input names its size expressions may use. `None` for single-pass shaders.
fn pass_plan(sh: &phosphor_isf::IsfShader) -> Option<shaders::PassPlan> {
    if sh.pass_plan.len() <= 1 && sh.buffers.is_empty() {
        return None;
    }
    // A single-pass shader with imported pictures still runs through the plan (that's where buffers bind).
    if sh.pass_plan.is_empty() {
        return Some(shaders::PassPlan {
            passes: vec![shaders::PlanPass::default()],
            buffers: sh.buffers.len(),
            vars: Vec::new(),
            images: sh.images.iter().map(|(slot, _)| *slot).collect(),
        });
    }
    let index = |t: &Option<String>| t.as_ref().and_then(|t| sh.buffers.iter().position(|b| b == t));
    Some(shaders::PassPlan {
        passes: sh
            .pass_plan
            .iter()
            .map(|p| shaders::PlanPass { target: index(&p.target), float: p.float, width: p.width.clone(), height: p.height.clone() })
            .collect(),
        buffers: sh.buffers.len(),
        vars: sh.inputs.iter().filter_map(|i| Some((i.name.clone(), i.offset?))).collect(),
        images: sh.images.iter().map(|(slot, _)| *slot).collect(),
    })
}

impl IsfLibrary {
    /// Starts scanning in the background.
    pub fn scan() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("phosphor-isf".into())
            .spawn(move || {
                let mut seen = std::collections::HashSet::new();
                for dir in folders() {
                    // Two levels deep: MadMapper keeps each material in its own folder.
                    let mut files: Vec<std::path::PathBuf> = Vec::new();
                    let mut stack = vec![(dir, 0u8)];
                    while let Some((d, depth)) = stack.pop() {
                        let Ok(entries) = std::fs::read_dir(&d) else { continue };
                        for p in entries.flatten().map(|e| e.path()) {
                            if p.is_dir() && depth < 2 {
                                stack.push((p, depth + 1));
                            } else if p.extension().is_some_and(|e| e == "fs") && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
                                files.push(p);
                            }
                        }
                    }
                    files.sort();
                    for path in files {
                        let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().to_string()) else { continue };
                        let slug = phosphor_engine::custom::slug(&stem);
                        if !seen.insert(slug.clone()) {
                            continue;
                        }
                        let Ok(fs) = std::fs::read_to_string(&path) else { continue };
                        let vs = std::fs::read_to_string(path.with_extension("vs")).ok();
                        let converted = std::panic::catch_unwind(|| phosphor_isf::import(&slug, &fs, vs.as_deref()));
                        let Ok(converted) = converted else {
                            if tx.send(Err("converter crashed".into())).is_err() {
                                return;
                            }
                            continue;
                        };
                        let result = match converted {
                            Ok(sh) if sh.unsupported.is_none() && !sh.wgsl.is_empty() => {
                                let prefix = if sh.is_transition {
                                    shaders::ISF_TRANSITION
                                } else if sh.is_filter {
                                    shaders::ISF_EFFECT
                                } else {
                                    shaders::ISF_GENERATOR
                                };
                                let mut about = sh.description.clone();
                                if let Some(c) = sh.categories.first() {
                                    about = if about.is_empty() { c.clone() } else { format!("{about} ({c})") };
                                }
                                if !sh.credit.is_empty() {
                                    about = format!("{about}\nby {}", sh.credit);
                                }
                                let images = load_images(&path, &sh.images);
                                Ok(Loaded { id: format!("{prefix}{slug}"), name: if sh.name.is_empty() { stem.clone() } else { sh.name.clone() }, about, params: params_for(&sh), entry: sh.entry.clone(), plan: pass_plan(&sh), images, audio_slots: sh.audio_slots.clone(), vertex_entry: sh.vertex_entry.clone(), wgsl: sh.wgsl })
                            }
                            Ok(sh) => Err(sh.unsupported.unwrap_or_default()),
                            Err(e) => Err(format!("{e:?}")),
                        };
                        if tx.send(result).is_err() {
                            return;
                        }
                    }
                }
            })
            .ok();
        Self { rx: Some(rx), loaded: 0, skipped: 0 }
    }

    /// Registers whatever the scanner has converted so far. Call every frame.
    pub fn poll(&mut self, engine: &mut Engine) {
        let Some(rx) = &self.rx else { return };
        loop {
            match rx.try_recv() {
                Ok(Ok(l)) => {
                    let desc: &'static ShaderDesc = Box::leak(Box::new(ShaderDesc {
                        id: leak(l.id),
                        name: leak(l.name),
                        entry: leak(l.entry),
                        params: Box::leak(l.params.into_boxed_slice()),
                    }));
                    shaders::set_custom_info(desc.id, "ISF", leak(l.about));
                    if let Some(plan) = l.plan {
                        shaders::set_pass_plan(desc.entry, plan);
                    }
                    for (slot, w, h, rgba) in l.images {
                        engine.set_plan_image(desc.entry, slot, w, h, &rgba);
                    }
                    shaders::set_audio_slots(desc.entry, l.audio_slots.into_iter().map(|(s, fft)| (s, if fft { shaders::AudioTexture::Fft } else { shaders::AudioTexture::Wave })).collect());
                    if let Some(v) = l.vertex_entry {
                        shaders::set_vertex_entry(desc.entry, leak(v));
                    }
                    engine.register_lazy(desc, l.wgsl);
                    self.loaded += 1;
                }
                Ok(Err(_)) => self.skipped += 1,
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.rx = None;
                    return;
                }
            }
        }
    }

    pub fn scanning(&self) -> bool {
        self.rx.is_some()
    }
}
