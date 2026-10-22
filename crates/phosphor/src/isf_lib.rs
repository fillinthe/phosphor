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
    /// Instanced: how many copies of its quad it draws.
    instances: Option<phosphor_isf::Instances>,
    /// Explorer category.
    category: &'static str,
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
    bundled("isf")
}

/// A folder shipped with Phosphor (see `bundled_isf`).
fn bundled(name: &str) -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    [dir.join("../Resources").join(name), dir.join(name), std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets")).join(name)].into_iter().find(|d| d.is_dir())
}

/// A shader's name for the lists, from its file name: "BookFlip" / "book_flip" / "book-flip" → "Book Flip".
fn display_name(stem: &str) -> String {
    let mut out = String::with_capacity(stem.len() + 4);
    let mut prev: Option<char> = None;
    for c in stem.chars() {
        let c = if c == '_' || c == '-' { ' ' } else { c };
        if c.is_uppercase() && prev.is_some_and(|p| p.is_lowercase()) {
            out.push(' ');
        }
        if !(c == ' ' && out.ends_with(' ')) {
            out.push(c);
        }
        prev = Some(c);
    }
    let out = out.trim();
    let mut chars = out.chars();
    chars.next().map_or_else(String::new, |f| f.to_uppercase().chain(chars).collect())
}

#[cfg(test)]
mod name_tests {
    #[test]
    fn file_names_read_as_names() {
        assert_eq!(super::display_name("BookFlip"), "Book Flip");
        assert_eq!(super::display_name("powerKaleido"), "Power Kaleido");
        assert_eq!(super::display_name("directional-easing"), "Directional easing");
        assert_eq!(super::display_name("Multi Pass Gaussian Blur"), "Multi Pass Gaussian Blur");
        assert_eq!(super::display_name("old_tv_lost_signal"), "Old tv lost signal");
        assert_eq!(super::display_name("CV Motion RGB Tear"), "CV Motion RGB Tear");
    }
}

/// A `.cube` LUT as an effect: id `isf:lut_<slug>`, one Amount, the table as the shader's picture (buffer 0).
fn lut_effect(slug: &str, stem: &str, lut: &crate::lut::Lut) -> Loaded {
    let entry = format!("isf_lut_{slug}");
    let (w, h, rgba) = lut.pack();
    Loaded {
        id: format!("{}lut_{slug}", shaders::ISF_EFFECT),
        name: display_name(stem),
        about: format!("Colour grade from {stem}.cube ({n}³ LUT)", n = lut.size),
        params: vec![Param::new("Amount", 0.0, 1.0, 1.0)],
        wgsl: crate::lut::wgsl(&entry, lut.size),
        entry,
        plan: Some(shaders::PassPlan { passes: vec![shaders::PlanPass::default()], buffers: 1, vars: Vec::new(), images: vec![0] }),
        images: vec![(0, w, h, rgba)],
        audio_slots: Vec::new(),
        vertex_entry: None,
        instances: None,
        category: "LUT",
    }
}

/// VDMX's LUT Mixer: every LUT found, LUT A and LUT B picked by name, Mix between them, Amount over the picture.
fn lut_mix_effect(luts: &[(String, crate::lut::Lut)]) -> Loaded {
    let names: Vec<&'static str> = luts.iter().map(|(n, _)| leak(n.clone())).collect();
    let names: &'static [&'static str] = Box::leak(names.into_boxed_slice());
    let last = (luts.len() - 1) as f32;
    let tables: Vec<crate::lut::Lut> = luts.iter().map(|(_, l)| l.resample(crate::lut::MIX_SIZE)).collect();
    let (w, h, rgba) = crate::lut::pack_all(&tables);
    let entry = "isf_lut_mixer".to_string();
    Loaded {
        id: format!("{}lut_mixer", shaders::ISF_EFFECT),
        name: "LUT Mix".into(),
        about: format!("Blend between two of your {} colour grades (LUT A to LUT B)", luts.len()),
        params: vec![
            Param { name: "LUT A", min: 0.0, max: last, default: 0.0, labels: names },
            Param { name: "LUT B", min: 0.0, max: last, default: 1.0, labels: names },
            Param::new("Mix", 0.0, 1.0, 0.5),
            Param::new("Amount", 0.0, 1.0, 1.0),
        ],
        wgsl: crate::lut::wgsl_mix(&entry, luts.len()),
        entry,
        plan: Some(shaders::PassPlan { passes: vec![shaders::PlanPass::default()], buffers: 1, vars: Vec::new(), images: vec![0] }),
        images: vec![(0, w, h, rgba)],
        audio_slots: Vec::new(),
        vertex_entry: None,
        instances: None,
        category: "LUT",
    }
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
        // Colour-grading LUTs (.cube) you add.
        dirs.push(d.join("luts"));
    }
    dirs.extend(crate::show::shader_dir("isf"));
    // Last, so copies installed elsewhere win: the ISF collection Phosphor ships (assets/isf).
    dirs.extend(bundled_isf());
    // gl-transitions (converted to ISF transitions as they load); after the ISF ones, which win on the same name.
    dirs.extend(bundled("gl-transitions"));
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
    // Some shaders declare MIN above MAX (Radial Spectrogram: 1 → 0.05); a range must run low to high.
    fn put(slots: &mut [Option<Param>], i: usize, name: String, min: f32, max: f32, default: f32, labels: &'static [&'static str]) {
        if let Some(s) = slots.get_mut(i) {
            let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
            *s = Some(Param { name: leak(name), min: lo, max: hi, default: default.clamp(lo, hi), labels });
        }
    }
    for input in &shader.inputs {
        let Some(o) = input.offset else { continue };
        let name = if input.label.is_empty() { input.name.clone() } else { input.label.clone() };
        match &input.kind {
            K::Float { min, max, default } => put(&mut slots, o, name, *min, *max, *default, &[]),
            K::Bool { default } => put(&mut slots, o, name, 0.0, 1.0, *default as i32 as f32, &[]),
            K::Event => put(&mut slots, o, name, 0.0, 1.0, 0.0, &[]),
            K::Long { values, labels, default } => {
                let lo = values.iter().copied().min().unwrap_or(0);
                let hi = values.iter().copied().max().unwrap_or(1).max(lo + 1);
                // A menu: one name per whole number from the lowest value (gaps keep their number).
                let names: Vec<&'static str> = (lo..=hi)
                    .map(|v| values.iter().position(|x| *x == v).and_then(|k| labels.get(k)).map_or_else(|| leak(v.to_string()), |l| leak(l.clone())))
                    .collect();
                let names: &'static [&'static str] = if labels.is_empty() { &[] } else { Box::leak(names.into_boxed_slice()) };
                put(&mut slots, o, name, lo as f32, hi as f32, *default as f32, names)
            }
            K::Int { min, max, default } => put(&mut slots, o, name, *min as f32, *max as f32, *default as f32, &[]),
            K::Color { default } => {
                for (k, ch) in ["R", "G", "B", "A"].iter().enumerate() {
                    put(&mut slots, o + k, format!("{name} {ch}"), 0.0, 1.0, default[k], &[]);
                }
            }
            K::Point2D { default, min, max } => {
                put(&mut slots, o, format!("{name} X"), min[0], max[0], default[0], &[]);
                put(&mut slots, o + 1, format!("{name} Y"), min[1], max[1], default[1], &[]);
            }
            K::Curve { .. } => {
                for (k, v) in input.default_floats().into_iter().enumerate() {
                    put(&mut slots, o + k, format!("{name} {}%", k * 100 / (phosphor_isf::CURVE_POINTS - 1)), 0.0, 1.0, v, &[]);
                }
            }
            K::Image | K::Audio { .. } => {}
        }
    }
    slots.into_iter().enumerate().map(|(i, p)| p.unwrap_or(Param::new(leak(format!("p{i}")), 0.0, 1.0, 0.0))).collect()
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

/// Converts one ISF shader file (`.fs`, or a gl-transition `.glsl`) into an engine shader; None = not a shader.
fn load(path: &std::path::Path, stem: &str, slug: &str, fs: String) -> Option<Result<Loaded, String>> {
    // A gl-transition (.glsl): becomes an ISF transition first; other .glsl files are skipped.
    let fs = if path.extension().is_some_and(|e| e == "glsl") {
        if !fs.contains("getToColor") {
            return None;
        }
        match phosphor_isf::gltransition::to_isf(&fs) {
            Ok(isf) => isf,
            Err(e) => return Some(Err(e)),
        }
    } else {
        fs
    };
    let vs = std::fs::read_to_string(path.with_extension("vs")).ok();
    let converted = std::panic::catch_unwind(|| phosphor_isf::import(slug, &fs, vs.as_deref()));
    let Ok(converted) = converted else { return Some(Err("converter crashed".into())) };
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
            let images = load_images(path, &sh.images);
            Ok(Loaded { id: format!("{prefix}{slug}"), name: display_name(stem), about, params: params_for(&sh), entry: sh.entry.clone(), plan: pass_plan(&sh), images, audio_slots: sh.audio_slots.clone(), vertex_entry: sh.vertex_entry.clone(), instances: sh.instances, wgsl: sh.wgsl, category: "ISF" })
        }
        Ok(sh) => Err(sh.unsupported.unwrap_or_default()),
        Err(e) => Err(format!("{e:?}")),
    };
    Some(result)
}

/// Hands a converted shader to the engine (compiled the first time it's drawn).
fn register(engine: &mut Engine, l: Loaded) {
    let desc: &'static ShaderDesc = Box::leak(Box::new(ShaderDesc {
        id: leak(l.id),
        name: leak(l.name),
        entry: leak(l.entry),
        params: Box::leak(l.params.into_boxed_slice()),
    }));
    shaders::set_custom_info(desc.id, l.category, leak(l.about));
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
    if let Some(n) = l.instances {
        shaders::set_instances(desc.entry, match n {
            phosphor_isf::Instances::Fixed(n) => shaders::Instances::Fixed(n),
            phosphor_isf::Instances::Param(i) => shaders::Instances::Param(i),
        });
    }
    engine.register_lazy(desc, l.wgsl);
}

impl IsfLibrary {
    /// Starts scanning in the background.
    pub fn scan() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("phosphor-isf".into())
            .spawn(move || {
                let mut seen = std::collections::HashSet::new();
                let mut seen_loose = std::collections::HashSet::new();
                // Every LUT found, for the LUT mixer (at most 32: the picture stays small).
                let mut luts: Vec<(String, crate::lut::Lut)> = Vec::new();
                for dir in folders() {
                    // Two levels deep: MadMapper keeps each material in its own folder.
                    let mut files: Vec<std::path::PathBuf> = Vec::new();
                    let mut stack = vec![(dir, 0u8)];
                    while let Some((d, depth)) = stack.pop() {
                        let Ok(entries) = std::fs::read_dir(&d) else { continue };
                        for p in entries.flatten().map(|e| e.path()) {
                            if p.is_dir() && depth < 2 {
                                stack.push((p, depth + 1));
                            } else if p.extension().is_some_and(|e| e == "fs" || e == "glsl" || e == "cube") && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
                                files.push(p);
                            }
                        }
                    }
                    files.sort();
                    for path in files {
                        let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().to_string()) else { continue };
                        let slug = phosphor_engine::custom::slug(&stem);
                        // The same shader under a differently spelled name ("Bow Tie Horizontal" / "BowTieHorizontal").
                        let loose: String = stem.chars().filter(char::is_ascii_alphanumeric).collect::<String>().to_lowercase();
                        if !seen.insert(slug.clone()) || !seen_loose.insert(loose) {
                            continue;
                        }
                        let Ok(fs) = std::fs::read_to_string(&path) else { continue };
                        // A colour-grading LUT (.cube): an effect of its own.
                        if path.extension().is_some_and(|e| e == "cube") {
                            let sent = match crate::lut::parse_cube(&fs) {
                                Ok(lut) => {
                                    let effect = lut_effect(&slug, &stem, &lut);
                                    if luts.len() < 32 {
                                        luts.push((display_name(&stem), lut));
                                    }
                                    tx.send(Ok(effect))
                                }
                                Err(e) => tx.send(Err(format!("{stem}.cube: {e}"))),
                            };
                            if sent.is_err() {
                                return;
                            }
                            continue;
                        }
                        let Some(result) = load(&path, &stem, &slug, fs) else { continue };
                        if tx.send(result).is_err() {
                            return;
                        }
                    }
                }
                // Two or more LUTs: the LUT mixer too.
                if luts.len() >= 2 {
                    let _ = tx.send(Ok(lut_mix_effect(&luts)));
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
                    register(engine, l);
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

/// Contact sheets for choosing shaders from a big pack: every effect in a folder drawn over a picture at three
/// moments (no feedback: single frames), one PNG each (`<slug>_<n>.png`), plus `sheet.tsv`: slug, name, error, then
/// numbers for sorting (0–255 scale): spread (blank ≈ 0), change from the picture (untouched ≈ 0), motion between
/// the first and last moment, flicker (brightness jump from one frame to the next), colourfulness, and a 12×6
/// thumbnail as hex (for finding near-duplicates). With `PHOSPHOR_ISF_SHEET_COST=1`, a last column: milliseconds per
/// 1920×1080 frame (render and read back, so an upper bound). Run with
/// `PHOSPHOR_ISF_SHEET="<shader folder>:<out folder>[:<picture>]" cargo test -p phosphor --release isf_sheet`.
/// Sound: a made-up beat, so audio-reactive shaders move. Mac and Linux (the Windows build has no `wgpu` of its own).
#[cfg(all(test, not(windows)))]
#[test]
fn isf_sheet() {
    use phosphor_engine::{Composition, EffectState, LayerState, ModInputs, SourceSpec};
    let Some(arg) = std::env::var_os("PHOSPHOR_ISF_SHEET") else { return };
    let arg = arg.to_string_lossy().to_string();
    let mut parts = arg.split(':');
    let (Some(src), Some(out)) = (parts.next(), parts.next()) else { panic!("PHOSPHOR_ISF_SHEET=in:out[:picture]") };
    let out = std::path::Path::new(out);
    std::fs::create_dir_all(out).unwrap();
    let (w, h) = (320u32, 180u32);
    let picture = match parts.next() {
        Some(p) => image::open(p).unwrap().resize_exact(w, h, image::imageops::FilterType::Triangle).to_rgba8().into_raw(),
        None => (0..w * h).flat_map(|i| [(i % w * 255 / w) as u8, (i / w * 255 / h) as u8, 160, 255]).collect(),
    };
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).expect("a GPU");
    let required_features = adapter.features() & (wgpu::Features::TEXTURE_COMPRESSION_BC | wgpu::Features::TEXTURE_FORMAT_16BIT_NORM);
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_features, ..Default::default() })).unwrap();
    let mut engine = Engine::new(device, queue);
    let mut files: Vec<_> = std::fs::read_dir(src).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "fs")).collect();
    files.sort();
    let mut tsv = String::new();
    for (n, path) in files.iter().enumerate() {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let slug = phosphor_engine::custom::slug(&stem);
        let fs = std::fs::read_to_string(path).unwrap();
        let loaded = match load(path, &stem, &slug, fs) {
            Some(Ok(l)) => l,
            Some(Err(e)) => {
                tsv.push_str(&format!("{slug}\t{stem}\t{}\n", e.lines().next().unwrap_or("")));
                continue;
            }
            None => continue,
        };
        let id = loaded.id.clone();
        let entry = loaded.entry.clone();
        let values: Vec<f32> = loaded.params.iter().map(|p| p.default).collect();
        register(&mut engine, loaded);
        let layer_id = 1000 + n as u64;
        let mut comp = Composition { width: w, height: h, ..Default::default() };
        let mut layer = LayerState { id: layer_id, source: SourceSpec::Movie { name: "p".into(), path: Some("p.png".into()) }, ..Default::default() };
        layer.effects.push(EffectState { id: 1, effect: id, enabled: true, values, mix: 1.0, mods: Vec::new() });
        comp.layers.push(layer);
        engine.upload_rgba(layer_id, w, h, &picture);
        // Built on a worker thread: draw until it's ready.
        let started = std::time::Instant::now();
        loop {
            engine.render(&comp, 0.0, &ModInputs::default());
            if !engine.shader_pending(&entry) || started.elapsed().as_secs() > 20 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if let Some(e) = engine.failed_modules.get(entry.as_str()) {
            tsv.push_str(&format!("{slug}\t{stem}\t{}\n", e.lines().next().unwrap_or("")));
            continue;
        }
        // Three moments (no feedback shaders here, so no need to draw the frames between), and the frame after the
        // second (flicker).
        let mut shots: Vec<Vec<u8>> = Vec::new();
        for (shot, t) in [0.5f32, 1.5, 3.0, 1.5 + 1.0 / 30.0].into_iter().enumerate() {
            let beat = (-(t * 2.0).fract() * 6.0).exp();
            let wave: Vec<[f32; 2]> = (0..512).map(|i| {
                let v = (i as f32 * 0.07 + t * 9.0).sin() * (0.2 + 0.6 * beat);
                [v, v * 0.8]
            }).collect();
            let fft: Vec<f32> = (0..512).map(|i| (beat * (1.0 - i as f32 / 512.0) + 0.1 * ((i as f32 * 0.3 + t * 5.0).sin() * 0.5 + 0.5)).min(1.0)).collect();
            engine.set_audio(&wave, &fft);
            engine.render(&comp, t, &ModInputs { time: t, ..Default::default() });
            let (ow, oh, px) = engine.read_output().expect("output");
            if shot < 3 {
                image::save_buffer(out.join(format!("{slug}_{shot}.png")), &px, ow, oh, image::ExtendedColorType::Rgba8).unwrap();
            }
            shots.push(px);
        }
        let luma = |px: &[u8]| -> Vec<f32> { px.chunks_exact(4).map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32).collect() };
        let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len().max(1) as f32;
        let diff = |a: &[u8], b: &[u8]| a.iter().zip(b).enumerate().filter(|(i, _)| i % 4 != 3).map(|(_, (x, y))| (*x as f32 - *y as f32).abs()).sum::<f32>() / (a.len() as f32 * 0.75);
        let l1 = luma(&shots[1]);
        let m1 = mean(&l1);
        let spread = (l1.iter().map(|v| (v - m1).powi(2)).sum::<f32>() / l1.len() as f32).sqrt();
        let change = diff(&shots[1], &picture);
        let motion = diff(&shots[0], &shots[2]);
        let flicker = (m1 - mean(&luma(&shots[3]))).abs();
        let colour = shots[1].chunks_exact(4).map(|p| (*p[..3].iter().max().unwrap() - *p[..3].iter().min().unwrap()) as f32).sum::<f32>() / (w * h) as f32;
        let mut thumb = String::new();
        for ty in 0..6 {
            for tx in 0..12 {
                for c in 0..3 {
                    let (mut sum, mut count) = (0u32, 0u32);
                    for y in (ty * h / 6..(ty + 1) * h / 6).step_by(3) {
                        for x in (tx * w / 12..(tx + 1) * w / 12).step_by(3) {
                            sum += shots[1][((y * w + x) * 4 + c) as usize] as u32;
                            count += 1;
                        }
                    }
                    thumb.push_str(&format!("{:02x}", sum / count.max(1)));
                }
            }
        }
        let mut cost = String::new();
        if std::env::var_os("PHOSPHOR_ISF_SHEET_COST").is_some() {
            let mut big = comp.clone();
            (big.width, big.height) = (1920, 1080);
            engine.render(&big, 0.0, &ModInputs::default());
            let _ = engine.read_output();
            let started = std::time::Instant::now();
            for i in 0..10 {
                engine.render(&big, i as f32 / 30.0, &ModInputs::default());
                let _ = engine.read_output();
            }
            cost = format!("\t{:.2}", started.elapsed().as_secs_f32() * 100.0);
        }
        tsv.push_str(&format!("{slug}\t{stem}\t\t{spread:.1}\t{change:.1}\t{motion:.1}\t{flicker:.1}\t{colour:.1}\t{thumb}{cost}\n"));
    }
    std::fs::write(out.join("sheet.tsv"), tsv).unwrap();
}
