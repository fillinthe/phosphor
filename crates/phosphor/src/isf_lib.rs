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
    rx: Option<Receiver<Found>>,
    pub loaded: usize,
    pub skipped: usize,
    /// The first scan is still running.
    scanning: bool,
    /// Shaders reloaded since start because their file changed (the Effects panel says so).
    pub reloaded: Vec<String>,
}

/// From the scanner thread: a shader converted (or not), one converted again after its file changed, or the end of
/// the first scan (the thread then keeps watching the folders).
enum Found {
    New(Result<Loaded, String>),
    Changed(String, Result<Loaded, String>),
    ScanDone,
}

/// The shader, LUT and GLSL files in an ISF folder, two levels deep (MadMapper keeps each material in a folder).
fn shader_files(dir: std::path::PathBuf) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
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
    files
}

fn modified(p: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
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
        plan: Some(shaders::PassPlan { passes: vec![shaders::PlanPass::default()], buffers: 1, vars: Vec::new(), images: vec![0], layer_images: Vec::new() }),
        images: vec![(0, w, h, rgba)],
        audio_slots: Vec::new(),
        vertex_entry: None,
        instances: None,
        category: "LUT",
    }
}

/// VDMX's LUT Mixer: every LUT found, LUT A and LUT B picked by name, Mix between them, Amount over the picture.
/// What decides where LUT Split changes from LUT A to LUT B.
const SPLIT_BY: &[&str] = &["Brightness", "Left to right", "Top to bottom", "From the centre", "Mask layer"];

fn lut_mix_effect(luts: &[(String, crate::lut::Lut)]) -> Loaded {
    lut_mixer(luts, false)
}

/// LUT A in one part of the picture, LUT B in another: shadows and highlights, left and right, centre and edges.
fn lut_split_effect(luts: &[(String, crate::lut::Lut)]) -> Loaded {
    lut_mixer(luts, true)
}

fn lut_mixer(luts: &[(String, crate::lut::Lut)], split: bool) -> Loaded {
    let names: Vec<&'static str> = luts.iter().map(|(n, _)| leak(n.clone())).collect();
    let names: &'static [&'static str] = Box::leak(names.into_boxed_slice());
    let last = (luts.len() - 1) as f32;
    let tables: Vec<crate::lut::Lut> = luts.iter().map(|(_, l)| l.resample(crate::lut::MIX_SIZE)).collect();
    let (w, h, rgba) = crate::lut::pack_all(&tables);
    let key = if split { "lut_split" } else { "lut_mixer" };
    let entry = format!("isf_{key}");
    let pair = [Param { name: "LUT A", min: 0.0, max: last, default: 0.0, labels: names }, Param { name: "LUT B", min: 0.0, max: last, default: 1.0, labels: names }];
    let params = if split {
        let by = Param { name: "Split by", min: 0.0, max: (SPLIT_BY.len() - 1) as f32, default: 0.0, labels: SPLIT_BY };
        // (The mask layer comes last: saved settings keep their places.)
        [pair.to_vec(), vec![by, Param::new("Point", 0.0, 1.0, 0.5), Param::new("Softness", 0.0, 0.5, 0.15), Param::new("Amount", 0.0, 1.0, 1.0), Param::new("Mask layer #", 0.0, 64.0, 0.0)]].concat()
    } else {
        [pair.to_vec(), vec![Param::new("Mix", 0.0, 1.0, 0.5), Param::new("Amount", 0.0, 1.0, 1.0)]].concat()
    };
    Loaded {
        id: format!("{}{key}", shaders::ISF_EFFECT),
        name: if split { "LUT Split".into() } else { "LUT Mix".into() },
        about: if split {
            format!("Two of your {} colour grades in different parts of the picture: shadows and highlights, left and right, centre and edges, or where another layer is bright (Mask layer)", luts.len())
        } else {
            format!("Blend between two of your {} colour grades (LUT A to LUT B)", luts.len())
        },
        params,
        wgsl: crate::lut::wgsl_mix(&entry, luts.len(), split),
        entry,
        plan: Some(shaders::PassPlan { passes: vec![shaders::PlanPass::default()], buffers: if split { 2 } else { 1 }, vars: Vec::new(), images: vec![0], layer_images: if split { vec![(1, 6)] } else { Vec::new() } }),
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
    // The looks Phosphor ships (scripts/make-luts.py); yours of the same name win.
    dirs.extend(bundled("luts"));
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
            layer_images: Vec::new(),
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
        layer_images: Vec::new(),
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
        // More buffers than there are bindings: it couldn't work.
        Ok(sh) if sh.buffers.len() > shaders::MAX_BUFFERS => Err(format!("needs {} buffers (at most {})", sh.buffers.len(), shaders::MAX_BUFFERS)),
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
                // (Credits often say "by …" already.)
                let credit = sh.credit.trim();
                let credit = credit.strip_prefix("by ").or_else(|| credit.strip_prefix("By ")).unwrap_or(credit);
                about = format!("{about}\nby {credit}");
            }
            let images = load_images(path, &sh.images);
            Ok(Loaded { id: format!("{prefix}{slug}"), name: display_name(stem), about, params: params_for(&sh), entry: sh.entry.clone(), plan: pass_plan(&sh), images, audio_slots: sh.audio_slots.clone(), vertex_entry: sh.vertex_entry.clone(), instances: sh.instances, wgsl: sh.wgsl, category: phosphor_category(&sh.categories, stem) })
        }
        Ok(sh) => Err(sh.unsupported.unwrap_or_default()),
        Err(e) => Err(format!("{e:?}")),
    };
    Some(result)
}

/// Hands a converted shader to the engine (compiled the first time it's drawn).
fn register(engine: &mut Engine, l: Loaded) {
    install(engine, l, false);
}

/// A shader whose file changed: the new version replaces the one built before.
fn reload(engine: &mut Engine, l: Loaded) {
    install(engine, l, true);
}

fn install(engine: &mut Engine, l: Loaded, replace: bool) {
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
    if replace {
        engine.replace_lazy(desc, l.wgsl);
    } else {
        engine.register_lazy(desc, l.wgsl);
    }
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
                // Every file loaded, with when it was last changed (watched after the first scan).
                let mut watched: Vec<(std::path::PathBuf, String, String, Option<std::time::SystemTime>)> = Vec::new();
                for dir in folders() {
                    for path in shader_files(dir) {
                        let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().to_string()) else { continue };
                        let slug = phosphor_engine::custom::slug(&stem);
                        // The same shader under a differently spelled name ("Bow Tie Horizontal" / "BowTieHorizontal").
                        let loose: String = stem.chars().filter(char::is_ascii_alphanumeric).collect::<String>().to_lowercase();
                        if !seen.insert(slug.clone()) || !seen_loose.insert(loose) {
                            continue;
                        }
                        let Ok(fs) = std::fs::read_to_string(&path) else { continue };
                        watched.push((path.clone(), stem.clone(), slug.clone(), modified(&path)));
                        // A colour-grading LUT (.cube): an effect of its own.
                        if path.extension().is_some_and(|e| e == "cube") {
                            let sent = match crate::lut::parse_cube(&fs) {
                                Ok(lut) => {
                                    let effect = lut_effect(&slug, &stem, &lut);
                                    if luts.len() < 32 {
                                        luts.push((display_name(&stem), lut));
                                    }
                                    tx.send(Found::New(Ok(effect)))
                                }
                                Err(e) => tx.send(Found::New(Err(format!("{stem}.cube: {e}")))),
                            };
                            if sent.is_err() {
                                return;
                            }
                            continue;
                        }
                        let Some(result) = load(&path, &stem, &slug, fs) else { continue };
                        if tx.send(Found::New(result)).is_err() {
                            return;
                        }
                    }
                }
                // Two or more LUTs: the LUT mixer too.
                if luts.len() >= 2 {
                    let _ = tx.send(Found::New(Ok(lut_mix_effect(&luts))));
                    let _ = tx.send(Found::New(Ok(lut_split_effect(&luts))));
                }
                if tx.send(Found::ScanDone).is_err() {
                    return;
                }
                // Then watch: a file saved again (in an editor) is converted again and replaces the old one.
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    for (path, stem, slug, when) in &mut watched {
                        let now = modified(path);
                        if now == *when {
                            continue;
                        }
                        *when = now;
                        let Ok(fs) = std::fs::read_to_string(&*path) else { continue };
                        let result = if path.extension().is_some_and(|e| e == "cube") {
                            Some(crate::lut::parse_cube(&fs).map(|lut| lut_effect(slug, stem, &lut)).map_err(|e| format!("{stem}.cube: {e}")))
                        } else {
                            load(path, stem, slug, fs)
                        };
                        if let Some(result) = result
                            && tx.send(Found::Changed(display_name(stem), result)).is_err()
                        {
                            return;
                        }
                    }
                }
            })
            .ok();
        Self { rx: Some(rx), loaded: 0, skipped: 0, scanning: true, reloaded: Vec::new() }
    }

    /// Registers whatever the scanner has converted so far. Call every frame.
    pub fn poll(&mut self, engine: &mut Engine) {
        let Some(rx) = &self.rx else { return };
        loop {
            match rx.try_recv() {
                Ok(Found::New(Ok(l))) => {
                    register(engine, l);
                    self.loaded += 1;
                }
                Ok(Found::New(Err(_))) => self.skipped += 1,
                Ok(Found::ScanDone) => self.scanning = false,
                Ok(Found::Changed(name, Ok(l))) => {
                    log::info!("ISF: {name} changed on disk, reloaded");
                    reload(engine, l);
                    self.reloaded.retain(|n| *n != name);
                    self.reloaded.push(name);
                }
                Ok(Found::Changed(name, Err(e))) => log::warn!("ISF: {name} changed but doesn't convert now: {e}"),
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.rx = None;
                    return;
                }
            }
        }
    }

    pub fn scanning(&self) -> bool {
        self.rx.is_some() && self.scanning
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
        layer.effects.push(EffectState { id: 1, effect: id, enabled: true, values, mix: 1.0, mods: Vec::new(), blend: Default::default(), other: 0, light: 0 });
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

/// A shader file saved again replaces the one on screen (ISF hot reload): black, then red after the reload.
#[cfg(all(test, not(windows)))]
#[test]
fn an_edited_shader_replaces_the_old_one() {
    use phosphor_engine::{Composition, EffectState, LayerState, ModInputs, SourceSpec};
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else { return };
    let Ok((device, queue)) = pollster::block_on(adapter.request_device(&Default::default())) else { return };
    let mut engine = Engine::new(device, queue);
    let fs = |colour: &str| format!("/*{{ \"INPUTS\": [ {{ \"NAME\": \"inputImage\", \"TYPE\": \"image\" }} ] }}*/\nvoid main() {{ gl_FragColor = vec4({colour}, 1.0) + IMG_THIS_PIXEL(inputImage) * 0.0; }}\n");
    let path = std::path::Path::new("Reload Test.fs");
    let mut comp = Composition { width: 16, height: 16, ..Default::default() };
    let mut red_at_middle = |engine: &mut Engine, loaded: Loaded, replace: bool| {
        let (id, entry) = (loaded.id.clone(), loaded.entry.clone());
        install(engine, loaded, replace);
        comp.layers = vec![LayerState { id: 1, source: SourceSpec::Generator("solid".into()), ..Default::default() }];
        comp.layers[0].effects.push(EffectState { id: 2, effect: id, enabled: true, values: Vec::new(), mix: 1.0, mods: Vec::new(), blend: Default::default(), other: 0, light: 0 });
        let started = std::time::Instant::now();
        loop {
            engine.render(&comp, 0.0, &ModInputs::default());
            if !engine.shader_pending(&entry) || started.elapsed().as_secs() > 20 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        engine.render(&comp, 0.0, &ModInputs::default());
        let (w, h, px) = engine.read_output().expect("output");
        px[((h / 2 * w + w / 2) * 4) as usize]
    };
    let black = load(path, "Reload Test", "reload_test", fs("0.0, 0.0, 0.0")).unwrap().unwrap();
    assert!(red_at_middle(&mut engine, black, false) < 10);
    let red = load(path, "Reload Test", "reload_test", fs("1.0, 0.0, 0.0")).unwrap().unwrap();
    assert!(red_at_middle(&mut engine, red, true) > 245, "the edited version draws");
}

/// Shadertoy's iMouse follows the preview's mouse: Mouse X / Y (set like any point) and Mouse down reach the shader.
#[cfg(all(test, not(windows)))]
#[test]
fn shadertoy_mouse_reaches_the_shader() {
    use phosphor_engine::{shaders, Composition, LayerState, ModInputs, SourceSpec};
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else { return };
    let Ok((device, queue)) = pollster::block_on(adapter.request_device(&Default::default())) else { return };
    let mut engine = Engine::new(device, queue);
    let code = "void mainImage(out vec4 o, in vec2 p) {\n    o = vec4(iMouse.xy / iResolution.xy, iMouse.z > 0.0 ? 1.0 : 0.0, 1.0);\n}\n";
    let isf = phosphor_isf::shadertoy::to_isf(code, "test").unwrap();
    let loaded = load(std::path::Path::new("Mouse Test.fs"), "Mouse Test", "mouse_test", isf).unwrap().unwrap();
    let (id, entry) = (loaded.id.clone(), loaded.entry.clone());
    install(&mut engine, loaded, false);
    let params = shaders::generator(&id).expect("registered").params;
    let mut comp = Composition { width: 16, height: 16, ..Default::default() };
    comp.layers = vec![LayerState { id: 1, source: SourceSpec::Generator(id), ..Default::default() }];
    let mut middle = |engine: &mut Engine, values: &[f32]| {
        comp.layers[0].generator_values = values.to_vec();
        let started = std::time::Instant::now();
        while engine.shader_pending(&entry) && started.elapsed().as_secs() < 20 {
            engine.render(&comp, 0.0, &ModInputs::default());
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        engine.render(&comp, 0.0, &ModInputs::default());
        let (w, h, px) = engine.read_output().expect("output");
        let i = ((h / 2 * w + w / 2) * 4) as usize;
        [px[i], px[i + 1], px[i + 2]]
    };
    // The mouse a quarter of the way in from the left and the top, button held.
    let mut v = Vec::new();
    shaders::set_points(params, &mut v, [0.25, 0.25]);
    shaders::set_mouse_down(params, &mut v, true);
    let [r, g, b] = middle(&mut engine, &v);
    assert!(r.abs_diff(64) < 4 && g.abs_diff(191) < 4 && b > 250, "{r} {g} {b}");
    shaders::set_mouse_down(params, &mut v, false);
    assert!(middle(&mut engine, &v)[2] < 5, "let go: z is negative");
}

/// LUT Split by a mask layer: where the layer picked as the mask is bright, LUT B; where it's dark, LUT A.
#[cfg(all(test, not(windows)))]
#[test]
fn lut_split_follows_a_mask_layer() {
    use phosphor_engine::{Composition, EffectState, LayerState, ModInputs, SourceSpec};
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else { return };
    let Ok((device, queue)) = pollster::block_on(adapter.request_device(&Default::default())) else { return };
    let mut engine = Engine::new(device, queue);
    let corners = |f: fn(f32) -> f32| crate::lut::Lut { size: 2, data: (0..8).map(|i| [f((i & 1) as f32), f(((i >> 1) & 1) as f32), f(((i >> 2) & 1) as f32)]).collect() };
    let luts = vec![("Same".to_string(), corners(|v| v)), ("Inverted".to_string(), corners(|v| 1.0 - v))];
    let loaded = lut_split_effect(&luts);
    let (id, entry) = (loaded.id.clone(), loaded.entry.clone());
    install(&mut engine, loaded, false);
    let solid = |id: u64, grey: f32| {
        let mut l = LayerState { id, source: SourceSpec::Generator("solid".into()), ..Default::default() };
        l.generator_values = vec![grey; 3];
        l
    };
    let mut comp = Composition { width: 16, height: 16, ..Default::default() };
    let mut middle = |engine: &mut Engine, mask: f32| {
        // The mask (layer 1, underneath) and a 20% grey on top graded by LUT Split › Mask layer = layer 1.
        let mut top = solid(2, 0.2);
        // LUT A, LUT B, Split by (4 = Mask layer), Point, Softness, Amount, Mask layer #.
        top.effects.push(EffectState { id: 3, effect: id.clone(), enabled: true, values: vec![0.0, 1.0, 4.0, 0.5, 0.05, 1.0, 1.0], mix: 1.0, mods: Vec::new(), blend: Default::default(), other: 0, light: 0 });
        comp.layers = vec![solid(1, mask), top];
        assert_eq!(comp.effect_layer_picks(), vec![1], "the mask layer's picture is wanted");
        let started = std::time::Instant::now();
        while engine.shader_pending(&entry) && started.elapsed().as_secs() < 20 {
            engine.render(&comp, 0.0, &ModInputs::default());
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        engine.render(&comp, 0.0, &ModInputs::default());
        engine.render(&comp, 0.0, &ModInputs::default());
        let (w, h, px) = engine.read_output().expect("output");
        px[((h / 2 * w + w / 2) * 4) as usize]
    };
    let dark = middle(&mut engine, 0.0);
    let bright = middle(&mut engine, 1.0);
    assert!(dark < 80 && bright > 170, "black mask = LUT A (the same grey), white mask = LUT B (inverted): {dark} {bright}");
}

/// Where an ISF shader goes in the Effects panel: the first of its own CATEGORIES that means one of Phosphor's,
/// else what its name says, else "ISF".
pub(crate) fn phosphor_category(categories: &[String], name: &str) -> &'static str {
    // (words → category), checked in this order.
    const WORDS: &[(&[&str], &str)] = &[
        (&["3d"], "3D"),
        (&["blur", "sharpen", "bokeh", "defocus"], "Blur"),
        (&["retro", "v002", "glitch", "vhs", "crt", "8bit", "8-bit", "pixel", "dither", "scanline"], "Retro"),
        (&["film", "grain", "vignette", "sepia", "old movie"], "Film"),
        (&["feedback", "trail", "echo", "delay", "time", "slit"], "Time"),
        (&["mask", "key", "chroma", "alpha"], "Key"),
        (&["glow", "bloom", "light", "flare", "shine", "god ray"], "Light"),
        (&["halftone", "stylize", "drawing", "sketch", "edge", "toon", "cartoon", "posterize", "emboss", "ascii", "dots"], "Stylize"),
        (&["distort", "warp", "wave", "kaleido", "tile", "mirror", "geometry", "twirl", "swirl", "bulge", "pinch", "ripple", "fisheye", "zoom", "rotate"], "Distort"),
        (&["color", "colour", "hue", "saturat", "contrast", "levels", "histogram", "lut", "tint", "invert", "gamma", "exposure", "curves"], "Color"),
        (&["pattern", "noise", "generator", "test pattern", "visualizer", "visualiser", "gradient", "stripes", "checker"], "Patterns"),
        (&["motion", "move", "scroll", "strobe"], "Motion"),
        (&["utility", "overlay", "transform", "crop", "flip", "scale", "position"], "Utility"),
    ];
    let pick = |text: &str| {
        let text = text.to_lowercase();
        WORDS.iter().find(|(words, _)| words.iter().any(|w| text.contains(w))).map(|(_, cat)| *cat)
    };
    categories.iter().find_map(|c| pick(c)).or_else(|| pick(name)).unwrap_or("ISF")
}

#[cfg(test)]
mod category_tests {
    use super::phosphor_category;

    #[test]
    fn isf_shaders_land_in_phosphors_categories() {
        let cats = |c: &[&str]| c.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(phosphor_category(&cats(&["Color Effect"]), "x"), "Color");
        assert_eq!(phosphor_category(&cats(&["Distortion Effect", "Stylize"]), "x"), "Distort");
        assert_eq!(phosphor_category(&cats(&["v002"]), "x"), "Retro");
        assert_eq!(phosphor_category(&cats(&["Halftone Effect"]), "x"), "Stylize");
        assert_eq!(phosphor_category(&cats(&["Tile Effect"]), "x"), "Distort");
        assert_eq!(phosphor_category(&[], "Gaussian Blur"), "Blur", "from its name");
        assert_eq!(phosphor_category(&cats(&["Something"]), "Thing"), "ISF", "nothing to go by");
    }
}

/// A preview tile that showed one effect and then shows another starts clean: an effect with memory (Interlace keeps
/// the last frame) never shows the picture of the effect that was in that tile before.
#[cfg(all(test, not(windows)))]
#[test]
fn a_preview_slot_forgets_the_effect_before() {
    use phosphor_engine::{Composition, EffectState, LayerState, ModInputs, PreviewItem, PreviewRefresh, SourceSpec};
    let isf = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/isf");
    let Some(adapter) = pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default())).ok() else { return };
    let (w, h) = (160u32, 90u32);
    // Busy: coloured noise (JPEG corruption shows on detail).
    let picture: Vec<u8> = (0..w * h).flat_map(|i| {
        let n = (i.wrapping_mul(2_654_435_761) >> 7) as u8;
        [n, n.wrapping_mul(3), n.wrapping_add(90), 255]
    }).collect();
    // An engine with both effects built and a picture in the mix (the previews' source).
    let engine = || {
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let mut engine = Engine::new(device, queue);
        let mut ids = Vec::new();
        for name in ["JPEG Block Corruption", "Interlace"] {
            let path = isf.join(format!("{name}.fs"));
            let slug = phosphor_engine::custom::slug(name);
            let loaded = load(&path, name, &slug, std::fs::read_to_string(&path).unwrap()).unwrap().unwrap();
            ids.push((loaded.id.clone(), loaded.entry.clone(), loaded.params.iter().map(|p| p.default).collect::<Vec<f32>>()));
            register(&mut engine, loaded);
        }
        let mut comp = Composition { width: w, height: h, ..Default::default() };
        comp.layers.push(LayerState { id: 7, source: SourceSpec::Movie { name: "p".into(), path: Some("p.png".into()) }, ..Default::default() });
        engine.upload_rgba(7, w, h, &picture);
        for (n, (id, ..)) in ids.iter().enumerate() {
            comp.layers[0].effects.push(EffectState { id: 50 + n as u64, effect: id.clone(), enabled: false, values: Vec::new(), mix: 1.0, mods: Vec::new(), blend: Default::default(), other: 0, light: 0 });
        }
        let started = std::time::Instant::now();
        while ids.iter().any(|(_, e, _)| engine.shader_pending(e)) && started.elapsed().as_secs() < 30 {
            comp.layers[0].effects.iter_mut().for_each(|e| e.enabled = true);
            engine.render(&comp, 0.0, &ModInputs::default());
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        comp.layers[0].effects.iter_mut().for_each(|e| e.enabled = false);
        (engine, comp, ids)
    };
    // The explorer's preview: one tile id per list position, whatever effect is in it.
    let tile = |(id, _, values): &(String, String, Vec<f32>)| EffectState { id: 9_000_000, effect: id.clone(), enabled: true, values: values.clone(), mix: 1.0, mods: Vec::new(), blend: Default::default(), other: 0, light: 0 };
    let show = |engine: &mut Engine, comp: &Composition, fx: EffectState, frames: usize| {
        for f in 0..frames {
            let refresh = if f == 0 { PreviewRefresh::Moved } else { PreviewRefresh::Live };
            engine.request_previews(phosphor_engine::MASTER_ID, vec![(PreviewItem::Effect(fx.clone()), refresh)]);
            engine.render(comp, 0.0, &ModInputs::default());
        }
        engine.read_preview(0).expect("a preview").2
    };
    let (mut fresh, comp, ids) = engine();
    let clean = show(&mut fresh, &comp, tile(&ids[1]), 6);
    let (mut reused, comp, ids) = engine();
    let jpeg = show(&mut reused, &comp, tile(&ids[0]), 6);
    let after = show(&mut reused, &comp, tile(&ids[1]), 6);
    let diff = |a: &[u8], b: &[u8]| a.iter().zip(b).map(|(x, y)| (*x as f32 - *y as f32).abs()).sum::<f32>() / a.len() as f32;
    assert!(diff(&clean, &jpeg) > 5.0, "the two effects look different ({})", diff(&clean, &jpeg));
    assert!(diff(&after, &clean) < 2.0, "Interlace after JPEG in the same tile looks like a fresh Interlace ({} away; {} from JPEG)", diff(&after, &clean), diff(&after, &jpeg));
}


/// Effects that drew black tiles in the Effects panel (2026-10-26) draw a picture: Key Frame Artifacts (frame count
/// per effect), Color Relookup (an unconnected picture input is the effect's own picture), Micro Buffer RGB (a frame
/// delay: a picture once its buffers fill), Bad TV.
#[cfg(all(test, not(windows)))]
#[test]
fn effects_that_were_black_draw_a_picture() {
    use phosphor_engine::{Composition, EffectState, LayerState, ModInputs, SourceSpec};
    let isf = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/isf");
    let Some(adapter) = pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default())).ok() else { return };
    let (w, h) = (160u32, 90u32);
    let picture: Vec<u8> = (0..w * h).flat_map(|i| { let n = (i.wrapping_mul(2_654_435_761) >> 7) as u8; [n, n.wrapping_mul(3), n.wrapping_add(90), 255] }).collect();
    for name in ["Bad TV", "Color Relookup", "Key Frame Artifacts", "Micro Buffer RGB"] {
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let mut engine = Engine::new(device, queue);
        let path = isf.join(format!("{name}.fs"));
        let slug = phosphor_engine::custom::slug(name);
        let loaded = load(&path, name, &slug, std::fs::read_to_string(&path).unwrap()).unwrap().unwrap();
        let (id, entry) = (loaded.id.clone(), loaded.entry.clone());
        let values: Vec<f32> = loaded.params.iter().map(|p| p.default).collect();
        register(&mut engine, loaded);
        let mut comp = Composition { width: w, height: h, ..Default::default() };
        let mut layer = LayerState { id: 7, source: SourceSpec::Movie { name: "p".into(), path: Some("p.png".into()) }, ..Default::default() };
        // Added after the app has drawn a while (the frame count that broke Key Frame Artifacts).
        for _ in 0..20 {
            engine.render(&comp, 0.0, &ModInputs::default());
        }
        layer.effects.push(EffectState { id: 1, effect: id, enabled: true, values, mix: 1.0, mods: Vec::new(), blend: Default::default(), other: 0, light: 0 });
        comp.layers.push(layer);
        engine.upload_rgba(7, w, h, &picture);
        let started = std::time::Instant::now();
        while engine.shader_pending(&entry) && started.elapsed().as_secs() < 30 {
            engine.render(&comp, 0.0, &ModInputs::default());
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        for f in 0..40 {
            engine.render(&comp, 1.0 + f as f32 / 30.0, &ModInputs { time: 1.0 + f as f32 / 30.0, ..Default::default() });
        }
        let (_, _, px) = engine.read_output().unwrap();
        let brightness = px.chunks_exact(4).map(|p| p[0] as f32 + p[1] as f32 + p[2] as f32).sum::<f32>() / (px.len() as f32 / 4.0 * 3.0);
        assert!(brightness > 20.0, "{name} draws a picture (brightness {brightness})");
    }
}
