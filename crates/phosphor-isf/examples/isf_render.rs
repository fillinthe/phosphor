//! Draws ISF shaders through Phosphor's engine, the way the app uses them, without building the app: generators
//! as a layer's source, effects over a moving test picture, transitions from one picture to another. Every frame of
//! 3.6 s at 30 fps is drawn, so feedback builds up; a made-up beat feeds `audio` / `audioFFT` inputs. Writes one
//! PNG per shader (four moments side by side; a transition at 20, 40, 60 and 90 %) and prints a line each: OK with
//! its kind, brightness (0–255) and motion (0 ≈ still), or why it failed. A folder is read two levels deep, like
//! the app's ISF folders. Mac only (Metal); elsewhere `isf_one` still checks that shaders convert.
//!
//! Usage: `cargo run -p phosphor-isf --release --example isf_render -- <shader.fs or folder> <out folder>`

use phosphor_engine::shaders::{self, AudioTexture, Param, PassPlan, PlanPass, ShaderDesc};
use phosphor_engine::{Composition, EffectState, Engine, LayerState, ModInputs, SourceSpec, Transition, TransitionKind, custom, wgpu};

const W: u32 = 480;
const H: u32 = 270;
/// Frames drawn (30 fps) and the ones kept.
const FRAMES: usize = 108;
const KEEP: [usize; 4] = [24, 48, 72, 108];

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// The test picture: a gradient, a row of bars and a white ball going round (`warm` = the transition's second one).
fn picture(t: f32, warm: bool) -> Vec<u8> {
    let (bx, by) = (W as f32 * (0.5 + 0.3 * (t * 1.7).cos()), H as f32 * (0.5 + 0.3 * (t * 1.7).sin()));
    let mut px = Vec::with_capacity((W * H * 4) as usize);
    for y in 0..H {
        for x in 0..W {
            let (fx, fy) = (x as f32 / W as f32, y as f32 / H as f32);
            let mut c = if warm { [40.0 + 120.0 * fy, 30.0, 90.0 + 120.0 * fx] } else { [30.0 + 60.0 * fx, 50.0 + 80.0 * fy, 60.0] };
            if (fx * 8.0) as i32 % 2 == 0 && fy > 0.82 {
                c = [200.0, 200.0, 60.0];
            }
            if (x as f32 - bx).hypot(y as f32 - by) < 22.0 {
                c = [255.0; 3];
            }
            px.extend_from_slice(&[c[0] as u8, c[1] as u8, c[2] as u8, 255]);
        }
    }
    px
}

/// The `.fs` files in a folder, two levels deep (as the app's `isf_lib::shader_files`).
fn shader_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), 0u8)];
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
    files
}

/// Same as the app's `isf_lib::params_for` (without the menu labels): names (a transition's `progress` is found by
/// name), ranges low to high, defaults kept inside them.
fn params_for(shader: &phosphor_isf::IsfShader) -> Vec<Param> {
    use phosphor_isf::IsfInputKind as K;
    let mut slots: Vec<Option<Param>> = vec![None; shader.param_count().min(shaders::MAX_PARAMS)];
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
            K::Long { values, default, .. } => {
                let lo = values.iter().copied().min().unwrap_or(0);
                let hi = values.iter().copied().max().unwrap_or(1).max(lo + 1);
                put(&mut slots, o, name, lo as f32, hi as f32, *default as f32, &[]);
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

/// Same as the app's `isf_lib::pass_plan`.
fn pass_plan(sh: &phosphor_isf::IsfShader) -> Option<PassPlan> {
    if sh.pass_plan.len() <= 1 && sh.buffers.is_empty() {
        return None;
    }
    let images = sh.images.iter().map(|(slot, _)| *slot).collect();
    if sh.pass_plan.is_empty() {
        return Some(PassPlan { passes: vec![PlanPass::default()], buffers: sh.buffers.len(), vars: Vec::new(), images, layer_images: Vec::new() });
    }
    let index = |t: &Option<String>| t.as_ref().and_then(|t| sh.buffers.iter().position(|b| b == t));
    Some(PassPlan {
        passes: sh.pass_plan.iter().map(|p| PlanPass { target: index(&p.target), float: p.float, width: p.width.clone(), height: p.height.clone() }).collect(),
        buffers: sh.buffers.len(),
        vars: sh.inputs.iter().filter_map(|i| Some((i.name.clone(), i.offset?))).collect(),
        images,
        layer_images: Vec::new(),
    })
}

/// Converts and registers one shader with the engine, as the app does. Returns its description, settings and kind.
fn register(engine: &mut Engine, path: &std::path::Path, slug: &str) -> Result<(&'static ShaderDesc, Vec<f32>, &'static str), String> {
    let fs = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let vs = std::fs::read_to_string(path.with_extension("vs")).ok();
    // A shader that crashes the converter fails alone (the app skips it the same way).
    let converted = std::panic::catch_unwind(|| phosphor_isf::import(slug, &fs, vs.as_deref())).map_err(|_| "converter crashed".to_string())?;
    let sh = converted.map_err(|e| match e {
        phosphor_isf::IsfError::Header(m) => format!("header: {m}"),
        phosphor_isf::IsfError::Glsl(m) => format!("GLSL:\n{m}"),
        phosphor_isf::IsfError::Wgsl(m) => format!("WGSL:\n{m}"),
    })?;
    if let Some(why) = sh.unsupported {
        return Err(format!("unsupported: {why}"));
    }
    // More buffers than there are bindings: the app skips it too.
    if sh.buffers.len() > shaders::MAX_BUFFERS {
        return Err(format!("needs {} buffers (at most {})", sh.buffers.len(), shaders::MAX_BUFFERS));
    }
    let (prefix, kind) = if sh.is_transition {
        (shaders::ISF_TRANSITION, "transition")
    } else if sh.is_filter {
        (shaders::ISF_EFFECT, "effect")
    } else {
        (shaders::ISF_GENERATOR, "generator")
    };
    let params = params_for(&sh);
    let values: Vec<f32> = params.iter().map(|p| p.default).collect();
    let desc: &'static ShaderDesc = Box::leak(Box::new(ShaderDesc {
        id: leak(format!("{prefix}{slug}")),
        name: leak(slug.to_string()),
        entry: leak(sh.entry.clone()),
        params: Box::leak(params.into_boxed_slice()),
    }));
    if let Some(plan) = pass_plan(&sh) {
        shaders::set_pass_plan(desc.entry, plan);
    }
    // IMPORTED pictures, relative to the shader file.
    let dir = path.parent().unwrap_or(std::path::Path::new("."));
    for (slot, rel) in &sh.images {
        match image::open(dir.join(rel)) {
            Ok(img) => {
                let img = img.to_rgba8();
                engine.set_plan_image(desc.entry, *slot, img.width(), img.height(), img.as_raw());
            }
            Err(e) => eprintln!("{slug}: imported picture {rel}: {e}"),
        }
    }
    shaders::set_audio_slots(desc.entry, sh.audio_slots.iter().map(|(s, fft)| (*s, if *fft { AudioTexture::Fft } else { AudioTexture::Wave })).collect());
    if let Some(v) = sh.vertex_entry.clone() {
        shaders::set_vertex_entry(desc.entry, leak(v));
    }
    if let Some(n) = sh.instances {
        shaders::set_instances(desc.entry, match n {
            phosphor_isf::Instances::Fixed(n) => shaders::Instances::Fixed(n),
            phosphor_isf::Instances::Param(i) => shaders::Instances::Param(i),
        });
    }
    engine.register_lazy(desc, sh.wgsl);
    Ok((desc, values, kind))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, out] = args.as_slice() else {
        eprintln!("usage: isf_render <shader.fs or folder> <out folder>");
        std::process::exit(2);
    };
    if !cfg!(target_os = "macos") {
        // phosphor-isf's tests build on Linux and Windows too; a GPU backend there would switch on the engine's GPU
        // tests in CI, so this draws on the Mac only.
        eprintln!("isf_render draws on the Mac only (Metal); use isf_one to check a shader converts");
        std::process::exit(1);
    }
    let (input, out) = (std::path::Path::new(input), std::path::Path::new(out));
    let files = if input.is_dir() { shader_files(input) } else { vec![input.to_path_buf()] };
    std::fs::create_dir_all(out).expect("make the out folder");

    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        eprintln!("no GPU found");
        std::process::exit(1);
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).expect("GPU device");
    let mut engine = Engine::new(device, queue);
    let mut failed = 0;
    let mut seen: Vec<(String, String, String)> = Vec::new();

    for (n, path) in files.iter().enumerate() {
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let slug = custom::slug(&stem);
        // Two files with the same name (give or take spaces, dashes and case): the app loads the first only.
        let loose: String = stem.chars().filter(char::is_ascii_alphanumeric).collect::<String>().to_lowercase();
        if let Some((_, _, first)) = seen.iter().find(|(s, l, _)| *s == slug || *l == loose) {
            println!("{stem}: SKIPPED, same name as {first} (the app loads only the first)");
            continue;
        }
        seen.push((slug.clone(), loose, path.display().to_string()));
        let (desc, values, kind) = match register(&mut engine, path, &slug) {
            Ok(r) => r,
            Err(e) => {
                println!("{stem}: FAILED {e}");
                failed += 1;
                continue;
            }
        };
        let layer_id = 1000 + n as u64;
        let mut layer = LayerState { id: layer_id, ..Default::default() };
        let movie = |name: &str| SourceSpec::Movie { name: name.into(), path: Some(format!("{name}.png")) };
        match kind {
            "transition" => {
                layer.source = movie("to");
                // The last kept frame lands at 90 %.
                let duration = KEEP[3] as f32 / 30.0 / 0.9;
                layer.transition = Some(Transition { from: movie("from"), from_values: Vec::new(), start: 0.0, duration, kind: TransitionKind::Shader, effect: desc.id.into(), values });
                engine.upload_rgba_outgoing(layer_id, W, H, &picture(0.0, false));
            }
            "effect" => {
                layer.source = movie("picture");
                layer.effects.push(EffectState { id: 1, effect: desc.id.into(), enabled: true, values, mix: 1.0, mods: Vec::new(), blend: Default::default(), other: 0, light: 0 });
            }
            _ => {
                layer.source = SourceSpec::Generator(desc.id.into());
                layer.generator_values = values;
            }
        }
        let mut comp = Composition { width: W, height: H, ..Default::default() };
        comp.layers.push(layer);

        let mut shots: Vec<Vec<u8>> = Vec::new();
        let mut error = None;
        for f in 0..=FRAMES {
            let t = f as f32 / 30.0;
            engine.upload_rgba(layer_id, W, H, &if kind == "transition" { picture(0.0, true) } else { picture(t, false) });
            let beat = (-(t * 2.0).fract() * 6.0).exp();
            let wave: Vec<[f32; 2]> = (0..512).map(|i| [(i as f32 * 0.07 + t * 9.0).sin() * (0.2 + 0.6 * beat); 2]).collect();
            let fft: Vec<f32> = (0..512)
                .map(|i| {
                    let low = 1.0 - i as f32 / 512.0;
                    (beat * low * low + 0.25 * low * ((i as f32 * 0.05 + t * 3.0).sin() * 0.5 + 0.5)).min(1.0)
                })
                .collect();
            engine.set_audio(&wave, &fft);
            let mods = ModInputs { time: t, ..Default::default() };
            if f == 0 {
                // Compiled on a worker thread: draw until it's ready.
                let started = std::time::Instant::now();
                loop {
                    engine.render(&comp, t, &mods);
                    if !engine.shader_pending(desc.entry) || started.elapsed().as_secs() >= 30 {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                if let Some(e) = engine.failed_modules.get(desc.entry) {
                    error = Some(format!("GPU compile: {}", e.lines().next().unwrap_or("")));
                    break;
                }
                if engine.shader_pending(desc.entry) {
                    error = Some("still compiling after 30 s".to_string());
                    break;
                }
            }
            engine.render(&comp, t, &mods);
            if KEEP.contains(&f) {
                shots.push(engine.read_output().expect("read the output").2);
            }
        }
        if let Some(e) = error {
            println!("{stem}: FAILED {e}");
            failed += 1;
            continue;
        }

        let mut strip = image::RgbaImage::new(W * 4 + 30, H);
        for (i, px) in shots.iter().enumerate() {
            let shot = image::RgbaImage::from_raw(W, H, px.clone()).expect("frame size");
            image::imageops::overlay(&mut strip, &shot, (i as u32 * (W + 10)) as i64, 0);
        }
        let file = out.join(format!("{slug}.png"));
        strip.save(&file).expect("write PNG");
        let rgb = |px: &[u8]| px.chunks_exact(4).flat_map(|p| p[..3].to_vec()).collect::<Vec<u8>>();
        let (first, last) = (rgb(&shots[0]), rgb(&shots[3]));
        let brightness = last.iter().map(|v| *v as f32).sum::<f32>() / last.len() as f32;
        let motion = first.iter().zip(&last).map(|(a, b)| (*a as f32 - *b as f32).abs()).sum::<f32>() / last.len() as f32;
        println!("{stem}: OK {kind}, brightness {brightness:.0}, motion {motion:.1} → {}", file.display());
    }
    if failed > 0 {
        std::process::exit(1);
    }
}
