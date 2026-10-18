//! ISF (Interactive Shader Format) importer for Phosphor.
//!
//! Converts an ISF fragment shader (GLSL with a JSON header) into a WGSL
//! fragment entry point that targets Phosphor's binding contract (`u`, `samp`,
//! `tex0..tex2`, `VOut`) and describes its inputs so the UI can build controls.
//!
//! Pipeline: header parse → GLSL rewrite (prelude with the ISF built-ins as
//! macros into `u.p`) → naga `glsl-in` → naga `wgsl-out` → text post-pass that
//! renames bindings and prefixes helpers with the entry name so many shaders can
//! share one module.

mod glsl;
mod mad_noise;
mod mad_sdf;
mod mad_shim;
mod header;
mod wgsl;

use glsl::Dialect;
use std::fmt;

/// Maximum number of floats packed into `u.p`.
/// vec4s in the shared uniform block (`Uniforms::p` in the engine). Must match the engine.
pub const UNIFORM_VEC4S: usize = 16;
/// Must equal `phosphor_engine::shaders::MAX_PARAMS`: all but the last (reserved) vec4.
pub const MAX_PARAMS: usize = (UNIFORM_VEC4S - 1) * 4;
/// Multi-pass buffers an ISF shader may use (texture bindings `BUFFER_BINDING..`).
pub const MAX_BUFFERS: usize = 12;
/// Binding of the first pass buffer (after the engine's tex0–tex2 at 3–5).
pub const BUFFER_BINDING: usize = 6;

pub use header::PassSpec;

/// Distinct pass targets, in order of first use.
fn buffer_names(passes: &[PassSpec]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in passes.iter().filter_map(|p| p.target.clone()) {
        if !out.contains(&t) {
            out.push(t);
        }
    }
    out
}
/// Number of image bindings (`tex0..tex2`).
pub const MAX_TEXTURES: usize = 3;

/// Kind of an ISF input, with its header metadata.
#[derive(Debug, Clone, PartialEq)]
pub enum IsfInputKind {
    Float { min: f32, max: f32, default: f32 },
    Bool { default: bool },
    /// Menu: `values[i]` is the integer the shader sees, `labels[i]` its UI label.
    Long { values: Vec<i32>, labels: Vec<String>, default: i32 },
    /// MadMapper `int` (ranged integer, no menu).
    Int { min: i32, max: i32, default: i32 },
    Color { default: [f32; 4] },
    Point2D { default: [f32; 2], min: [f32; 2], max: [f32; 2] },
    Image,
    /// `audio` (waveform, one row of samples centred on 0.5) / `audioFFT` (spectrum, one row of magnitudes):
    /// bound as a picture the app refreshes every frame from its audio analyser.
    Audio { fft: bool },
    /// Momentary trigger: 1.0 on the frame it fires, else 0.0.
    Event,
    /// MadMapper `curve`: control points as x, y pairs (0..1). Becomes `CURVE_POINTS` editable heights (at x = 0,
    /// ¼, ½, ¾, 1) and a smooth function the shader calls where it sampled the curve picture.
    Curve { points: Vec<f32> },
}

/// One entry of the header's `INPUTS`.
#[derive(Debug, Clone, PartialEq)]
pub struct IsfInput {
    pub name: String,
    pub label: String,
    pub kind: IsfInputKind,
    /// Index of the first float of this input in `u.p` (flattened), for value inputs.
    pub offset: Option<usize>,
    /// Texture slot (`tex<n>`) for image/audio inputs.
    pub texture: Option<usize>,
    /// A point2D with no MIN/MAX that the shader uses as pixels (divides by RENDERSIZE) while its defaults are
    /// 0..1: the host scales it by the render size, as VDMX does (Vertex Manipulator).
    pub pixels: bool,
}

impl IsfInput {
    /// Floats this input occupies in `u.p`.
    pub fn float_count(&self) -> usize {
        match self.kind {
            IsfInputKind::Color { .. } => 4,
            IsfInputKind::Point2D { .. } => 2,
            IsfInputKind::Curve { .. } => CURVE_POINTS,
            IsfInputKind::Image | IsfInputKind::Audio { .. } => 0,
            _ => 1,
        }
    }

    /// Default value as packed floats (empty for images).
    pub fn default_floats(&self) -> Vec<f32> {
        match &self.kind {
            IsfInputKind::Float { default, .. } => vec![*default],
            IsfInputKind::Bool { default } => vec![if *default { 1.0 } else { 0.0 }],
            IsfInputKind::Long { default, .. } => vec![*default as f32],
            IsfInputKind::Int { default, .. } => vec![*default as f32],
            IsfInputKind::Color { default } => default.to_vec(),
            IsfInputKind::Point2D { default, .. } => default.to_vec(),
            IsfInputKind::Event => vec![0.0],
            // The default curve's height at each editable point.
            IsfInputKind::Curve { points } => (0..CURVE_POINTS).map(|k| curve_value(points, k as f32 / (CURVE_POINTS - 1) as f32)).collect(),
            IsfInputKind::Image | IsfInputKind::Audio { .. } => Vec::new(),
        }
    }
}

/// An imported shader.
#[derive(Debug, Clone)]
pub struct IsfShader {
    pub name: String,
    pub description: String,
    pub credit: String,
    pub categories: Vec<String>,
    pub inputs: Vec<IsfInput>,
    /// Has an `inputImage` input (or is a MadMapper FX): the engine binds the source to `tex0`.
    pub is_filter: bool,
    /// Render passes declared by the header (1 for plain single-pass shaders).
    pub passes: usize,
    /// The passes, in order (empty for single-pass shaders).
    pub pass_plan: Vec<PassSpec>,
    /// Buffers the passes render into, in order of first use: buffer `i` is bound at
    /// `BUFFER_BINDING + i` and sampled by name in the shader.
    pub buffers: Vec<String>,
    /// `IMPORTED` pictures: (buffer index among `buffers`, file path relative to the shader). The app loads
    /// each file and binds it at that buffer slot; the shader samples it by name like any buffer.
    pub images: Vec<(usize, String)>,
    /// `audio` / `audioFFT` inputs: (texture slot 1 or 2, spectrum?). The app binds its live audio pictures there.
    pub audio_slots: Vec<(usize, bool)>,
    /// WGSL fragment entry + helpers (and the vertex entry, when the shader has one); empty when `unsupported` is set.
    pub wgsl: String,
    /// A real vertex stage (ISF vertex shaders that move the quad): the engine draws six vertices through it.
    pub vertex_entry: Option<String>,
    /// Name of the fragment entry function.
    pub entry: String,
    /// Reason the shader cannot run in Phosphor yet (inputs are still reported).
    pub unsupported: Option<String>,
}

impl IsfShader {
    /// Number of floats packed into `u.p` (max [`MAX_PARAMS`]).
    pub fn param_count(&self) -> usize {
        self.inputs.iter().map(IsfInput::float_count).sum()
    }

    /// Default `u.p` contents, padded to [`MAX_PARAMS`].
    pub fn default_params(&self) -> [f32; MAX_PARAMS] {
        let mut p = [0.0; MAX_PARAMS];
        for input in &self.inputs {
            if let Some(off) = input.offset {
                for (k, v) in input.default_floats().into_iter().enumerate() {
                    if let Some(slot) = p.get_mut(off + k) {
                        *slot = v;
                    }
                }
            }
        }
        p
    }
}

#[derive(Debug, Clone)]
pub enum IsfError {
    /// Header missing or malformed.
    Header(String),
    /// naga could not parse the (rewritten) GLSL.
    Glsl(String),
    /// naga rejected the module (validation) or failed to emit WGSL.
    Wgsl(String),
}

impl fmt::Display for IsfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IsfError::Header(s) => write!(f, "header: {s}"),
            IsfError::Glsl(s) => write!(f, "glsl: {s}"),
            IsfError::Wgsl(s) => write!(f, "wgsl: {s}"),
        }
    }
}

impl std::error::Error for IsfError {}

/// Makes `name_slug` a valid WGSL identifier fragment.
fn sanitize_slug(name_slug: &str) -> String {
    let mut s: String = name_slug
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    if s.is_empty() || s.as_bytes()[0].is_ascii_digit() {
        s.insert(0, 's');
    }
    s
}

/// Assigns `u.p` offsets and texture slots. Returns a reason if the layout does not fit.
fn layout_inputs(inputs: &mut [IsfInput], is_fx: bool) -> Option<String> {
    let mut offset = 0;
    let mut next_tex = if is_fx { 1 } else { 0 };
    let has_input_image = inputs.iter().any(|i| i.name == "inputImage");
    if has_input_image {
        next_tex = 1;
    }
    for input in inputs.iter_mut() {
        match input.kind {
            IsfInputKind::Image | IsfInputKind::Audio { .. } => {
                let slot = if input.name == "inputImage" {
                    0
                } else {
                    let s = next_tex;
                    next_tex += 1;
                    s
                };
                if slot >= MAX_TEXTURES {
                    return Some(format!("more than {MAX_TEXTURES} image inputs"));
                }
                input.texture = Some(slot);
            }
            _ => {
                input.offset = Some(offset);
                offset += input.float_count();
            }
        }
    }
    if offset > MAX_PARAMS {
        return Some(format!("{offset} parameter floats (max {MAX_PARAMS})"));
    }
    None
}

/// Imports an ISF shader. `name_slug` names the WGSL entry (`isf_<slug>`) and
/// prefixes every helper. `vs_source` is the optional companion `.vs`; its
/// `main` is inlined into the fragment stage (varyings become globals), which
/// is exact for the usual "precompute texture offsets" vertex shaders.
pub fn import(name_slug: &str, fs_source: &str, vs_source: Option<&str>) -> Result<IsfShader, IsfError> {
    let (json, body) = header::split_header(fs_source)?;
    let header = header::parse_header(json)?;
    let entry = format!("isf_{}", sanitize_slug(name_slug));

    let stripped = glsl::strip_comments(body);
    // Includes named by the fragment shader and (MadMapper materials) its vertex shader.
    let includes: Vec<String> = stripped
        .lines()
        .chain(vs_source.unwrap_or("").lines())
        .filter(|l| l.trim_start().starts_with("#include"))
        .map(|l| l.split('"').nth(1).unwrap_or("").to_string())
        .collect();
    let body = glsl::fix_returns(&glsl::resolve_directives(&stripped));
    let dialect = glsl::detect_dialect(&body);
    let mut inputs = header.inputs.clone();
    // Unranged points count as pixels only where the shader divides them by the render size.
    let all_src = format!("{body}\n{}", vs_source.unwrap_or(""));
    for i in inputs.iter_mut().filter(|i| i.pixels) {
        let divided = all_src.match_indices(i.name.as_str()).any(|(k, _)| {
            let before_ok = k == 0 || !all_src.as_bytes()[k - 1].is_ascii_alphanumeric() && all_src.as_bytes()[k - 1] != b'_';
            before_ok && all_src[k + i.name.len()..].trim_start().strip_prefix('/').is_some_and(|r| r.trim_start().starts_with("RENDERSIZE"))
        });
        let defaults_unit = matches!(&i.kind, IsfInputKind::Point2D { default, .. } if default.iter().all(|v| (0.0..=1.0).contains(v)));
        i.pixels = divided && defaults_unit;
    }
    let is_filter = dialect == Dialect::MadFx || inputs.iter().any(|i| i.name == "inputImage");

    let mut shader = IsfShader {
        name: name_slug.to_string(),
        description: header.description.clone(),
        credit: header.credit.clone(),
        categories: header.categories.clone(),
        inputs: Vec::new(),
        is_filter,
        passes: header.passes.max(1),
        pass_plan: header.pass_specs.clone(),
        buffers: buffer_names(&header.pass_specs),
        images: Vec::new(),
        audio_slots: Vec::new(),
        vertex_entry: None,
        wgsl: String::new(),
        entry: entry.clone(),
        unsupported: None,
    };

    let mut unsupported = header.unsupported.clone();
    if let Some(reason) = layout_inputs(&mut inputs, dialect == Dialect::MadFx) {
        unsupported.get_or_insert(reason);
    }
    shader.inputs = inputs;
    shader.audio_slots = shader
        .inputs
        .iter()
        .filter_map(|i| match (&i.kind, i.texture) {
            (IsfInputKind::Audio { fft }, Some(t)) => Some((t, *fft)),
            _ => None,
        })
        .collect();
    // Imported pictures take buffer slots after the pass buffers (same bindings, same IMG_* macros).
    for (name, path) in &header.imported {
        if !shader.buffers.contains(name) {
            shader.images.push((shader.buffers.len(), path.clone()));
            shader.buffers.push(name.clone());
        }
    }
    if shader.buffers.len() > MAX_BUFFERS {
        unsupported.get_or_insert(format!("{} buffers (max {MAX_BUFFERS})", shader.buffers.len()));
    }
    for file in &includes {
        if !matches!(file.as_str(), "MadCommon.glsl" | "MadNoise.glsl" | "MadSDF.glsl") {
            unsupported.get_or_insert(format!("MadMapper include {file}"));
        }
    }
    if glsl::uses_ident(&body, "mm_LastFrame") {
        unsupported.get_or_insert("mm_LastFrame (feedback)".into());
    }

    // Vertex shader: inline unless it transforms geometry (MadMapper .vs files are boilerplate).
    let mut vs_part = String::new();
    let mut vs_names = Vec::new();
    // MadMapper material vertex function: runs per pixel instead (its `flat out` values become globals).
    let mut mad_vs = false;
    if let Some(vs) = vs_source.filter(|_| dialect == Dialect::MadMaterial) {
        let vs = glsl::resolve_directives(&glsl::strip_comments(vs));
        if glsl::uses_ident(&vs, "materialVsFunc") {
            let (rewritten, names) = glsl::rewrite_io_decls(&vs, &[]);
            vs_names = names;
            vs_part = rewritten;
            mad_vs = true;
        }
    }
    // A real vertex stage, when the shader moves the quad (3d Rotate, Vertex Manipulator): the vertex source is
    // converted on its own and the fragment stage reads the varying it writes.
    let mut vertex_glsl: Option<String> = None;
    if let Some(vs) = vs_source.filter(|_| dialect == Dialect::Isf) {
        let vs = glsl::resolve_directives(&glsl::strip_comments(vs));
        let (rewritten, names) = glsl::rewrite_io_decls(&vs, &[]);
        if glsl::uses_ident(&vs, "gl_Position") {
            if !names.is_empty() {
                unsupported.get_or_insert("vertex shader with its own varyings or attributes".into());
            } else {
                match glsl::rename_main(&vs, "phosphor_vs_user") {
                    Some(user) => vertex_glsl = Some(user),
                    None => {
                        unsupported.get_or_insert("vertex shader without main".into());
                    }
                }
            }
        } else {
            vs_names = names;
            match glsl::rename_main(&rewritten, "phosphor_vs_main") {
                Some(s) => vs_part = s,
                None => {
                    unsupported.get_or_insert("vertex shader without main".into());
                }
            }
        }
    }
    if let Some(reason) = unsupported {
        shader.unsupported = Some(reason);
        return Ok(shader);
    }

    let (body, global_inits) = glsl::hoist_global_inits(&body);
    // Curves are functions now: `IMG_NORM_PIXEL(curve, uv)` → `phosphor_curve_<curve>(uv)`.
    let mut body = body;
    for input in shader.inputs.iter().filter(|i| matches!(i.kind, IsfInputKind::Curve { .. })) {
        body = glsl::call_curve(&body, &input.name);
    }
    let (fs_part, _) = glsl::rewrite_io_decls(&body, &vs_names);
    let fs_part = match dialect {
        Dialect::Isf => glsl::rename_main(&fs_part, "phosphor_fs_main")
            .ok_or_else(|| IsfError::Glsl("no `void main()` in fragment shader".into()))?,
        _ => fs_part,
    };

    let mut full = glsl::prelude(&header, &shader.inputs, dialect);
    let whole = format!("{vs_part}\n{fs_part}");
    // Plain `texture(img, uv)` on an input: every picture here is a `texture2D` + the shared sampler
    // (shaders that define their own `texture` function keep it).
    if mad_shim::called_not_defined(&whole, "texture") {
        full.push_str("#define texture(img, nuv) textureLod(sampler2D(img, phosphor_samp), phosphor_flip(nuv), 0.0)\n");
    }
    if includes.iter().any(|f| f == "MadCommon.glsl") {
        full.push_str(&glsl::mad_common_shim(&whole));
    }
    if includes.iter().any(|f| f == "MadNoise.glsl") {
        full.push_str(&mad_noise::shim(&whole));
    }
    if includes.iter().any(|f| f == "MadSDF.glsl") {
        full.push_str(&mad_sdf::shim(&whole));
    }
    full.push_str(&vs_part);
    full.push('\n');
    full.push_str(&fs_part);
    full.push('\n');
    // Globals initialised from inputs: assigned once main starts (after the inputs are read).
    full.push_str("void phosphor_init_globals() {\n");
    for init in &global_inits {
        full.push_str("    ");
        full.push_str(init);
        full.push('\n');
    }
    full.push_str("}\n");
    full.push_str(&glsl::epilogue(dialect, !vs_part.is_empty(), mad_vs));

    let naga_out = glsl_to_wgsl(&full, naga::ShaderStage::Fragment)?;
    shader.wgsl = wgsl::finalize(&naga_out, &entry)?;
    if let Some(user) = vertex_glsl {
        let vs_entry = format!("phosphor_vs_{entry}");
        let mut vfull = glsl::vertex_prelude(&header, &shader.inputs);
        vfull.push_str(&user);
        vfull.push('\n');
        vfull.push_str(glsl::vertex_epilogue());
        let vnaga = glsl_to_wgsl(&vfull, naga::ShaderStage::Vertex)?;
        shader.wgsl.push('\n');
        shader.wgsl.push_str(&wgsl::finalize_vertex(&vnaga, &vs_entry)?);
        shader.vertex_entry = Some(vs_entry);
    }
    Ok(shader)
}

/// Runs naga: GLSL 450 fragment → validated module → WGSL text.
fn glsl_to_wgsl(src: &str, stage: naga::ShaderStage) -> Result<String, IsfError> {
    let mut frontend = naga::front::glsl::Frontend::default();
    let mut options = naga::front::glsl::Options::from(stage);
    options.defines.insert("__VERSION__".into(), "450".into());
    let mut module = frontend
        .parse(&options, src)
        .map_err(|e| IsfError::Glsl(e.emit_to_string(src)))?;
    // Drop prelude helpers and globals the shader never touched.
    naga::compact::compact(&mut module, naga::compact::KeepUnused::No);
    let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
        .validate(&module)
        .map_err(|e| IsfError::Wgsl(format!("{}", e.emit_to_string(src))))?;
    naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
        .map_err(|e| IsfError::Wgsl(e.to_string()))
}

/// Phosphor's common prelude declarations, for validating imported shaders in isolation.
pub fn test_prelude() -> String {
    let bufs: String = (0..MAX_BUFFERS).map(|i| format!("@group(0) @binding({}) var buf{i}: texture_2d<f32>;\n", BUFFER_BINDING + i)).collect();
    format!(
        r#"
struct U {{
    p: array<vec4f, {UNIFORM_VEC4S}>,
    resolution: vec2f,
    time: f32,
    opacity: f32,
    uv_xform: vec4f,
    mode: u32,
    yuv: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    _pad3: u32,
    _pad4: u32,
    _pad5: u32,
}};
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var samp_mirror: sampler;
@group(0) @binding(3) var tex0: texture_2d<f32>;
@group(0) @binding(4) var tex1: texture_2d<f32>;
@group(0) @binding(5) var tex2: texture_2d<f32>;
{bufs}
struct VOut {{ @builtin(position) pos: vec4f, @location(0) uv: vec2f }};
"#
    )
}

/// Parses and validates `wgsl` appended to [`test_prelude`] with naga's WGSL frontend.
pub fn validate_wgsl(wgsl: &str) -> Result<(), String> {
    let src = format!("{}\n{wgsl}", test_prelude());
    let module = naga::front::wgsl::parse_str(&src).map_err(|e| e.emit_to_string(&src))?;
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
        .validate(&module)
        .map(|_| ())
        .map_err(|e| e.emit_to_string(&src))
}

/// Editable heights per `curve` input (at x = 0, ¼, ½, ¾, 1).
pub const CURVE_POINTS: usize = 5;

/// The height of a `curve` input's default at `x` (0..1): a Catmull-Rom spline through the control points
/// (x, y pairs, sorted by x), ends extended in a straight line (so a straight curve stays straight), clamped to 0..1.
pub fn curve_value(points: &[f32], x: f32) -> f32 {
    let mut pts: Vec<(f32, f32)> = points.chunks_exact(2).map(|c| (c[0], c[1])).collect();
    if pts.is_empty() {
        pts = vec![(0.0, 0.0), (1.0, 1.0)];
    }
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let m = pts.len();
    if m == 1 {
        return pts[0].1.clamp(0.0, 1.0);
    }
    let i = pts.iter().rposition(|p| p.0 <= x).unwrap_or(0).min(m - 2);
    let (p1, p2) = (pts[i], pts[i + 1]);
    let p0 = if i == 0 { (2.0 * p1.0 - p2.0, 2.0 * p1.1 - p2.1) } else { pts[i - 1] };
    let p3 = if i + 2 >= m { (2.0 * p2.0 - p1.0, 2.0 * p2.1 - p1.1) } else { pts[i + 2] };
    let t = ((x - p1.0) / (p2.0 - p1.0).max(1e-6)).clamp(0.0, 1.0);
    let (t2, t3) = (t * t, t * t * t);
    let y = 0.5 * ((2.0 * p1.1) + (-p0.1 + p2.1) * t + (2.0 * p0.1 - 5.0 * p1.1 + 4.0 * p2.1 - p3.1) * t2 + (-p0.1 + 3.0 * p1.1 - 3.0 * p2.1 + p3.1) * t3);
    y.clamp(0.0, 1.0)
}

/// `curve_value` sampled into `n` RGBA8 pixels (one row), value in every channel.
pub fn curve_lut(points: &[f32], n: usize) -> Vec<u8> {
    (0..n)
        .flat_map(|k| {
            let v = (curve_value(points, k as f32 / (n.max(2) - 1) as f32) * 255.0).round() as u8;
            [v, v, v, v]
        })
        .collect()
}

#[cfg(test)]
mod tests;
