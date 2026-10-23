//! User shaders written in WGSL inside Phosphor. A source looks like:
//!
//! ```wgsl
//! // @name Hue Pulse
//! // @kind effect            (or: generator)
//! // @param Speed 0 4 1      (name, min, max, default — up to 60, read with P(0u), P(1u), …)
//! @fragment
//! fn main(in: VOut) -> @location(0) vec4f {
//!     let c = src(in.uv);    // effects read their input with src(uv)
//!     return vec4f(c.rgb, c.a);
//! }
//! ```
//!
//! Every helper from the built-in shaders (src, luma, palette, hash21, rgb2hsv, …) is available.
//!
//! More passes: `// @pass blur_x 0.5` declares a pass drawn first (here at half size) by `fn blur_x(in: VOut) ->
//! vec4f`; later passes and `main` read its picture with `read_blur_x(uv)` (blurs, glows, bloom). `// @feedback`
//! gives `last(uv)`, the shader's own previous frame (with or without passes).

use crate::shaders::{self, MAX_PARAMS, Param, ShaderDesc};

#[derive(Clone, Debug, PartialEq)]
pub struct CompileError {
    /// 1-based line in the user's source, when known.
    pub line: Option<u32>,
    pub message: String,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(l) => write!(f, "line {l}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Header {
    pub name: String,
    pub generator: bool,
    /// `// @kind blend`: a blend mode (`src(uv)` = the layer, `below(uv)` = what's under it).
    pub blend: bool,
    pub params: Vec<Param>,
    /// `// @feedback`: `last(uv)` reads this shader's previous frame (trails, echoes, smears).
    pub feedback: bool,
    /// `// @pass name [scale]`: passes drawn before `main`, in order, each into its own picture (`read_<name>(uv)`).
    pub passes: Vec<(String, f32)>,
}

/// At most this many `@pass` lines (each one is a buffer).
pub const MAX_PASSES: usize = 8;

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// File/shader name → id-safe slug ("My Shader.fs" stem → "my_shader"). Used for ISF imports,
/// Shader Editor files and show exports, so the same file always gets the same id.
pub fn slug(name: &str) -> String {
    name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

pub fn parse_header(source: &str) -> Result<Header, CompileError> {
    let mut header = Header { name: "Untitled".into(), generator: false, blend: false, params: Vec::new(), feedback: false, passes: Vec::new() };
    for (i, line) in source.lines().enumerate() {
        let Some(rest) = line.trim().strip_prefix("// @") else { continue };
        let err = |m: &str| CompileError { line: Some(i as u32 + 1), message: m.to_string() };
        let (key, value) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let value = value.trim();
        match key {
            "name" => header.name = value.to_string(),
            "kind" => {
                header.generator = value.eq_ignore_ascii_case("generator");
                header.blend = value.eq_ignore_ascii_case("blend");
            }
            "feedback" => header.feedback = true,
            "pass" => {
                let mut parts = value.split_whitespace();
                let name = parts.next().unwrap_or_default();
                if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || name.starts_with(|c: char| c.is_ascii_digit()) || name == "main" {
                    return Err(err("@pass needs a name made of letters, digits and _ (then an optional size, 0.05 to 1)"));
                }
                let scale = match parts.next() {
                    Some(s) => s.parse::<f32>().map_err(|_| err("@pass size must be a number (1 = full size, 0.5 = half)"))?.clamp(0.05, 1.0),
                    None => 1.0,
                };
                if header.passes.iter().any(|(n, _)| n == name) {
                    return Err(err("two passes with the same name"));
                }
                if header.passes.len() >= MAX_PASSES {
                    return Err(err("at most 8 passes"));
                }
                header.passes.push((name.to_string(), scale));
            }
            "param" => {
                let parts: Vec<&str> = value.split_whitespace().collect();
                if parts.len() < 4 {
                    return Err(err("@param needs: name min max default"));
                }
                let nums: Vec<f32> = parts[parts.len() - 3..].iter().filter_map(|p| p.parse().ok()).collect();
                if nums.len() != 3 {
                    return Err(err("@param min/max/default must be numbers"));
                }
                if header.params.len() >= MAX_PARAMS {
                    return Err(err(&format!("at most {MAX_PARAMS} parameters")));
                }
                let name = parts[..parts.len() - 3].join(" ");
                let (lo, hi) = (nums[0].min(nums[1]), nums[0].max(nums[1]));
                header.params.push(Param::new(leak(name), lo, hi, nums[2].clamp(lo, hi)));
            }
            _ => {}
        }
    }
    // With @feedback the shader's own picture takes one more buffer (after the passes).
    if header.feedback && header.passes.len() >= MAX_PASSES {
        return Err(CompileError { line: None, message: "with @feedback, at most 7 passes".into() });
    }
    Ok(header)
}

/// How a user shader runs when it isn't one plain pass: `@feedback` or its `@pass` list.
pub fn plan_for(header: &Header) -> Option<shaders::PassPlan> {
    if header.feedback && header.passes.is_empty() {
        return Some(feedback_plan());
    }
    if header.passes.is_empty() {
        return None;
    }
    let mut passes: Vec<shaders::PlanPass> = header
        .passes
        .iter()
        .enumerate()
        .map(|(i, (_, scale))| shaders::PlanPass { target: Some(i), float: false, width: Some(format!("$WIDTH * {scale}")), height: Some(format!("$HEIGHT * {scale}")) })
        .collect();
    let mut buffers = header.passes.len();
    if header.feedback {
        // main draws into one more (float) buffer that keeps last frame's picture for last(); a final pass shows it.
        passes.push(shaders::PlanPass { target: Some(buffers), float: true, width: None, height: None });
        buffers += 1;
    }
    passes.push(shaders::PlanPass::default());
    Some(shaders::PassPlan { passes, buffers, vars: Vec::new(), images: Vec::new(), layer_images: Vec::new() })
}

/// Checks a user shader the way the engine compiles it, without a GPU (any thread): its header, then the WGSL.
pub fn check(source: &str) -> Result<Header, CompileError> {
    let header = parse_header(source)?;
    let (full, prelude_lines) = module_source(source, "custom_check")?;
    validate(&full, prelude_lines)?;
    Ok(header)
}

/// Builds the module source for a user shader, renaming `fn main(` to `entry`.
/// Returns the full module text and the number of prelude lines before the user's code.
pub(crate) fn module_source(source: &str, entry: &str) -> Result<(String, u32), CompileError> {
    let count = source.matches("fn main(").count();
    if count != 1 {
        return Err(CompileError { line: None, message: "define exactly one `@fragment fn main(in: VOut) -> @location(0) vec4f`".into() });
    }
    let prelude = shaders::common_source();
    let prelude_lines = prelude.lines().count() as u32 + 1;
    let header = parse_header(source).ok();
    // Blend modes read the picture below (tex1). Added after the user's code so error lines stay theirs.
    let below = if header.as_ref().is_some_and(|h| h.blend) { "\nfn below(uv: vec2f) -> vec4f {\n    return textureSampleLevel(tex1, samp, uv, 0.0);\n}\n" } else { "" };
    let feedback = header.as_ref().is_some_and(|h| h.feedback);
    let passes = header.map(|h| h.passes).unwrap_or_default();
    if feedback || !passes.is_empty() {
        // Several draws (`plan_for`): one entry point that runs pass k's function, then `main`. Each pass's picture is
        // buf k, read with read_<name>(uv). With @feedback, main draws into the next buffer, which still holds last
        // frame's picture for last(), and one more draw shows it. Added after the user's code so error lines stay theirs.
        for (name, _) in &passes {
            if !source.contains(&format!("fn {name}(")) {
                return Err(CompileError { line: None, message: format!("@pass {name}: define fn {name}(in: VOut) -> vec4f") });
            }
        }
        // A plain function now: no entry-point attributes.
        let user = source.replace("@fragment", "").replace("-> @location(0)", "->").replace("fn main(", &format!("fn {entry}_main("));
        let mut wrapper = String::from("\n// Passes: their pictures, and which one this draw is.\n");
        for (i, (name, _)) in passes.iter().enumerate() {
            wrapper.push_str(&format!("fn read_{name}(uv: vec2f) -> vec4f {{\n    return textureSampleLevel(buf{i}, samp, uv, 0.0);\n}}\n"));
        }
        let n = passes.len();
        if feedback {
            wrapper.push_str(&format!("fn last(uv: vec2f) -> vec4f {{\n    return textureSampleLevel(buf{n}, samp, uv, 0.0);\n}}\n"));
        }
        wrapper.push_str(&format!("@fragment\nfn {entry}(in: VOut) -> @location(0) vec4f {{\n"));
        for (i, (name, _)) in passes.iter().enumerate() {
            wrapper.push_str(&format!("    if u.p[15].x < {i}.5 {{\n        return {name}(in);\n    }}\n"));
        }
        if feedback {
            wrapper.push_str(&format!("    if u.p[15].x < {n}.5 {{\n        return {entry}_main(in);\n    }}\n    return textureSampleLevel(buf{n}, samp, in.uv, 0.0);\n}}\n"));
        } else {
            wrapper.push_str(&format!("    return {entry}_main(in);\n}}\n"));
        }
        return Ok((format!("{prelude}\n{user}{wrapper}{below}"), prelude_lines));
    }
    let user = source.replace("fn main(", &format!("fn {entry}("));
    Ok((format!("{prelude}\n{user}{below}"), prelude_lines))
}

/// Parses and validates with naga, reporting errors against the user's own line numbers.
pub(crate) fn validate(full: &str, prelude_lines: u32) -> Result<(), CompileError> {
    // naga can panic on unusual input; treat that as a compile error rather than a crash.
    std::panic::catch_unwind(|| validate_inner(full, prelude_lines))
        .unwrap_or_else(|_| Err(CompileError { line: None, message: "the shader compiler crashed on this source".into() }))
}

fn validate_inner(full: &str, prelude_lines: u32) -> Result<(), CompileError> {
    let to_user = |line: Option<u32>| line.and_then(|l| l.checked_sub(prelude_lines));
    let module = naga::front::wgsl::parse_str(full).map_err(|e| CompileError {
        line: to_user(e.location(full).map(|l| l.line_number)),
        message: e.message().to_string(),
    })?;
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::default())
        .validate(&module)
        .map_err(|e| CompileError {
            line: to_user(e.location(full).map(|l| l.line_number)),
            message: e.as_inner().to_string(),
        })?;
    Ok(())
}

pub(crate) fn make_desc(id: &str, header: &Header, entry: &'static str) -> ShaderDesc {
    ShaderDesc {
        id: leak(id.to_string()),
        name: leak(header.name.clone()),
        entry,
        params: Box::leak(header.params.clone().into_boxed_slice()),
    }
}

/// A starting point for "New effect" / "New generator".
pub fn template(generator: bool) -> String {
    if generator {
        "// @name My Generator\n// @kind generator\n// @param Speed 0 4 1\n// @param Scale 1 20 6\n\n@fragment\nfn main(in: VOut) -> @location(0) vec4f {\n    let p = centered(in.uv) * P(1u);\n    let v = sin(p.x + u.time * P(0u)) + sin(p.y * 1.3 - u.time * P(0u));\n    return vec4f(palette(v * 0.25), 1.0);\n}\n".into()
    } else {
        "// @name My Effect\n// @kind effect\n// @param Amount 0 1 1\n// @param Speed 0 4 1\n\n@fragment\nfn main(in: VOut) -> @location(0) vec4f {\n    let c = src(in.uv);\n    var hsv = rgb2hsv(c.rgb);\n    hsv.x = fract(hsv.x + u.time * 0.1 * P(1u));\n    return vec4f(mix(c.rgb, hsv2rgb(hsv), P(0u)), c.a);\n}\n".into()
    }
}

/// A starting point for a new effect or generator (Shader Editor › New from template): each does one clear
/// thing, with every line explained, so changing it is easier than starting from a blank page.
pub struct Template {
    pub name: &'static str,
    pub about: &'static str,
    pub source: &'static str,
}

/// Starters for the Shader Editor. Every one compiles (tested).
pub const TEMPLATES: &[Template] = &[
    Template {
        name: "Colour: tint and hue spin",
        about: "Recolours the picture: a tint you mix in, and a hue that turns over time",
        source: "// @name My Colour
// @kind effect
// @param Amount 0 1 1
// @param Hue spin 0 2 0.3
// @param Tint red 0 1 1
// @param Tint green 0 1 0.4
// @param Tint blue 0 1 0.8

// Each @param line above becomes a slider: name, lowest, highest, starting value. P(0u) is the first.
// Tip: right-click any slider in Phosphor > Modulate to make it follow the kick or an LFO.
@fragment
fn main(in: VOut) -> @location(0) vec4f {
    let c = src(in.uv);                                  // the layer's picture at this point
    var hsv = rgb2hsv(c.rgb);                            // hue, saturation, brightness
    hsv.x = fract(hsv.x + u.time * 0.1 * P(1u));         // turn the hue with time
    let spun = hsv2rgb(hsv);
    let tint = vec3f(P(2u), P(3u), P(4u));
    let out = spun * tint;                               // multiply = tint
    return vec4f(mix(c.rgb, out, P(0u)), c.a);           // Amount mixes it in
}
",
    },
    Template {
        name: "Distort: wave warp",
        about: "Bends the picture with moving waves (where each pixel reads from is moved)",
        source: "// @name My Wave Warp
// @kind effect
// @param Strength 0 0.1 0.02
// @param Waves 1 40 8
// @param Speed 0 6 1.5

// Distortion = reading the picture from somewhere else. Move `uv` and the picture moves.
@fragment
fn main(in: VOut) -> @location(0) vec4f {
    var uv = in.uv;                                      // 0..1 across and down
    let t = u.time * P(2u);
    uv.x += sin(uv.y * P(1u) + t) * P(0u);               // sideways ripples, travelling down
    uv.y += cos(uv.x * P(1u) * 0.7 + t) * P(0u) * 0.5;
    return src(uv);
}
",
    },
    Template {
        name: "Pixelate: blocks",
        about: "Chunky pixels: snaps each point to the centre of its block",
        source: "// @name My Pixelate
// @kind effect
// @param Blocks across 4 200 40

@fragment
fn main(in: VOut) -> @location(0) vec4f {
    let across = P(0u);
    let cells = vec2f(across, across / aspect());        // square blocks whatever the picture's shape
    let uv = (floor(in.uv * cells) + 0.5) / cells;       // the middle of this point's block
    return src(uv);
}
",
    },
    Template {
        name: "Mirror: kaleidoscope",
        about: "Folds the picture around the centre into slices",
        source: "// @name My Kaleidoscope
// @kind effect
// @param Slices 2 16 6
// @param Spin 0 2 0.2
// @param Zoom 0.3 3 1

@fragment
fn main(in: VOut) -> @location(0) vec4f {
    let p = centered(in.uv) * 2.0;                       // from the centre: -1..1 top to bottom, wider across
    var a = atan2(p.y, p.x) + u.time * P(1u);            // angle around the centre
    let r = length(p) / P(2u);                           // distance from the centre (Zoom shrinks it)
    let slice = 6.2831853 / max(P(0u), 1.0);
    a = abs((a % slice + slice) % slice - slice * 0.5);  // fold every slice onto the first, mirrored
    let q = vec2f(cos(a), sin(a)) * r;
    let uv = vec2f(q.x / aspect(), q.y) * 0.5 + 0.5;
    return src(1.0 - abs(fract(uv * 0.5) * 2.0 - 1.0));  // past an edge, mirror back in (no smeared edges)
}
",
    },
    Template {
        name: "Mask: soft circle",
        about: "Shows the picture inside a circle with a soft edge (outside is see-through)",
        source: "// @name My Circle Mask
// @kind effect
// @param Size 0 2 0.8
// @param Softness 0 0.5 0.1
// @param Invert 0 1 0

@fragment
fn main(in: VOut) -> @location(0) vec4f {
    let c = src(in.uv);
    let d = length(centered(in.uv)) * 2.0;               // 0 at the centre, 1 at the top and bottom edges
    var m = 1.0 - smoothstep(P(0u) - P(1u), P(0u) + P(1u), d); // 1 inside, 0 outside, soft between
    m = mix(m, 1.0 - m, P(2u));                          // Invert flips it
    return vec4f(c.rgb * m, c.a * m);                    // alpha 0 = layers below show through
}
",
    },
    Template {
        name: "Glow: bright parts bloom",
        about: "Blurs only the bright parts and adds them back, so lights glow",
        source: "// @name My Glow
// @kind effect
// @param Threshold 0 1 0.6
// @param Size 0 0.05 0.012
// @param Strength 0 3 1.2

// Samples a ring of points around each pixel: a cheap blur.
@fragment
fn main(in: VOut) -> @location(0) vec4f {
    let c = src(in.uv);
    var glow = vec3f(0.0);
    for (var i = 0; i < 16; i++) {
        let a = f32(i) * 0.3927;                         // 16 directions around the circle
        for (var k = 1; k < 4; k++) {
            let off = vec2f(cos(a) / aspect(), sin(a)) * P(1u) * f32(k);
            let s = src(in.uv + off).rgb;
            glow += max(s - vec3f(P(0u)), vec3f(0.0));   // only what's brighter than Threshold
        }
    }
    glow /= 48.0;
    return vec4f(c.rgb + glow * P(2u), c.a);
}
",
    },
    Template {
        name: "Feedback: trails",
        about: "Each frame keeps a fading copy of the last one, zoomed and turned a little, so movement leaves trails",
        source: "// @name My Trails
// @kind effect
// @feedback
// @param Trail 0 0.99 0.9
// @param Zoom 0.95 1.05 1.01
// @param Spin -0.05 0.05 0.005
// @param Hue drift 0 0.1 0.01

// @feedback gives you last(uv): this effect's own picture from the frame before.
// Mixing it back in, a bit faded, zoomed and turned, makes trails, echoes and tunnels.
@fragment
fn main(in: VOut) -> @location(0) vec4f {
    let c = src(in.uv);
    var p = centered(in.uv);                             // around the centre, square
    let a = P(2u);
    p = vec2f(p.x * cos(a) - p.y * sin(a), p.x * sin(a) + p.y * cos(a)) / P(1u); // turn and zoom
    let back = vec2f(p.x / aspect(), p.y) + 0.5;
    var prev = last(back);                               // last frame, from slightly further in
    var hsv = rgb2hsv(prev.rgb);
    hsv.x = fract(hsv.x + P(3u));                        // colours drift as they fade
    prev = vec4f(hsv2rgb(hsv), prev.a) * P(0u);          // Trail: how much of it stays
    return max(c, prev);                                 // new picture over the fading trail
}
",
    },
    Template {
        name: "Passes: soft glow",
        about: "A glow built from three passes: bright parts picked out and blurred across at half size, then down, then added back",
        source: "// @name My Glow
// @kind effect
// @pass bright 0.5
// @pass blur_x 0.5
// @pass blur_y 0.5
// @param Threshold 0 1 0.6
// @param Size 0 8 3
// @param Amount 0 3 1.2

// Each @pass line is a step drawn before main, into its own picture (0.5 = half size: faster, softer).
// Write a function with the pass's name; later steps read its picture with read_<name>(uv).

fn bright(in: VOut) -> vec4f {
    let c = src(in.uv);
    return c * smoothstep(P(0u), P(0u) + 0.2, luma(c.rgb));   // keep only the bright parts
}

fn blur_x(in: VOut) -> vec4f {
    let step = vec2f(P(1u) / u.resolution.x, 0.0);
    var sum = vec4f(0.0);
    for (var i = -4; i <= 4; i++) {
        sum += read_bright(in.uv + step * f32(i));         // sideways
    }
    return sum / 9.0;
}

fn blur_y(in: VOut) -> vec4f {
    let step = vec2f(0.0, P(1u) / u.resolution.y);
    var sum = vec4f(0.0);
    for (var i = -4; i <= 4; i++) {
        sum += read_blur_x(in.uv + step * f32(i));         // then up and down
    }
    return sum / 9.0;
}

@fragment
fn main(in: VOut) -> @location(0) vec4f {
    let c = src(in.uv);
    let glow = read_blur_y(in.uv);
    return vec4f(c.rgb + glow.rgb * P(2u), c.a);           // the glow added on top
}
",
    },
    Template {
        name: "Generator: pulsing rings",
        about: "Rings moving out from the centre; map Pulse to the kick to make them hit with the music",
        source: "// @name My Rings
// @kind generator
// @param Rings 1 30 8
// @param Speed -4 4 1
// @param Pulse 0 1 0
// @param Hue 0 1 0.6

// A generator makes its own picture: no src() here.
// Right-click Pulse in Phosphor > Modulate > Kick to make the rings jump with the music.
@fragment
fn main(in: VOut) -> @location(0) vec4f {
    let d = length(centered(in.uv));
    let wave = sin(d * P(0u) * 6.2831853 - u.time * P(1u) * 3.0);
    let ring = smoothstep(0.0, 0.15, wave) * (1.0 + P(2u) * 2.0);
    let col = hsv2rgb(vec3f(fract(P(3u) + d * 0.2), 0.8, 1.0));
    return vec4f(col * ring, 1.0);
}
",
    },
    Template {
        name: "Generator: noise clouds",
        about: "Soft drifting clouds from layered noise, coloured by a palette",
        source: "// @name My Clouds
// @kind generator
// @param Scale 0.5 12 3
// @param Speed 0 2 0.3
// @param Detail 1 6 4
// @param Colour shift 0 1 0

@fragment
fn main(in: VOut) -> @location(0) vec4f {
    var p = centered(in.uv) * P(0u);
    let t = u.time * P(1u);
    var v = 0.0;
    var amp = 0.5;
    for (var i = 0; i < i32(P(2u)); i++) {               // add finer and finer noise (fractal)
        v += value_noise(p + vec2f(t, t * 0.7)) * amp;
        p *= 2.0;
        amp *= 0.5;
    }
    return vec4f(palette(v + P(3u)), 1.0);
}
",
    },
    Template {
        name: "Blend mode: soft glow",
        about: "Your own way of putting a layer on the ones below (pick it in the layer's Blend shader menu)",
        source: "// @name My Blend
// @kind blend
// @param Glow 0 2 1

// src(uv) = this layer, below(uv) = the layers under it.
@fragment
fn main(in: VOut) -> @location(0) vec4f {
    let top = src(in.uv);
    let base = below(in.uv);
    // Screen, pushed brighter where the layer is bright.
    let screen = 1.0 - (1.0 - base.rgb) * (1.0 - top.rgb);
    let c = mix(base.rgb, screen, top.a) + top.rgb * top.a * 0.25 * P(0u);
    return vec4f(clamp(c, vec3f(0.0), vec3f(1.0)), 1.0);
}
",
    },
];

/// How a `// @feedback` shader runs: pass 0 into a float buffer (keeps last frame), pass 1 to the output.
pub fn feedback_plan() -> shaders::PassPlan {
    shaders::PassPlan {
        passes: vec![shaders::PlanPass { target: Some(0), float: true, width: None, height: None }, shaders::PlanPass::default()],
        buffers: 1,
        vars: Vec::new(),
        images: Vec::new(),
        layer_images: Vec::new(),
    }
}

/// The source of a built-in single-pass effect or generator, as an editable user shader.
pub fn builtin_as_custom(id: &str) -> Option<String> {
    let (desc, generator) = match shaders::effect(id) {
        Some(d) => (d, false),
        None => (shaders::generator(id)?, true),
    };
    if !shaders::multipass(desc.entry).is_empty() || desc.entry.starts_with("fs_") {
        return None;
    }
    // CRLF-safe (a Windows checkout without .gitattributes would otherwise never find "\n}\n").
    let all = shaders::wgsl_source().replace("\r\n", "\n");
    let start = all.find(&format!("fn {}(", desc.entry))?;
    let end = start + all[start..].find("\n}\n")? + 3;
    let body = all[start..end].replacen(&format!("fn {}(", desc.entry), "fn main(", 1);
    let mut out = format!("// @name {} (copy)\n// @kind {}\n", desc.name, if generator { "generator" } else { "effect" });
    for p in desc.params {
        out.push_str(&format!("// @param {} {} {} {}\n", p.name.replace(['(', ')'], ""), p.min, p.max, p.default));
    }
    out.push_str("\n@fragment\n");
    out.push_str(&body);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_are_declared_and_checked() {
        let h = parse_header("// @pass blur_x 0.5\n// @pass blur_y\n").unwrap();
        assert_eq!(h.passes, vec![("blur_x".to_string(), 0.5), ("blur_y".to_string(), 1.0)]);
        let plan = plan_for(&h).unwrap();
        assert_eq!((plan.passes.len(), plan.buffers), (3, 2), "two buffers, then the output");
        assert_eq!(plan.passes[0].width.as_deref(), Some("$WIDTH * 0.5"));
        assert!(parse_header("// @pass 2fast\n").is_err());
        assert!(parse_header("// @pass a\n// @pass a\n").is_err());
        // Both: the passes, main into a float buffer kept for last(), then the show pass.
        let both = parse_header("// @pass a\n// @feedback\n").unwrap();
        let plan = plan_for(&both).unwrap();
        assert_eq!((plan.passes.len(), plan.buffers), (3, 2));
        assert!(plan.passes[1].float && plan.passes[1].target == Some(1) && plan.passes[2].target.is_none());
        let src = "// @pass a\n// @feedback\nfn a(in: VOut) -> vec4f { return src(in.uv); }\n@fragment\nfn main(in: VOut) -> @location(0) vec4f { return read_a(in.uv) + last(in.uv) * 0.5; }\n";
        let (full, lines) = module_source(src, "e").unwrap();
        validate(&full, lines).unwrap();
        assert!(full.contains("textureSampleLevel(buf1"), "last() reads the buffer after the passes");
        let missing = module_source("// @pass glow\n@fragment\nfn main(in: VOut) -> @location(0) vec4f { return src(in.uv); }\n", "e");
        assert!(missing.unwrap_err().message.contains("fn glow("));
    }

    #[test]
    fn template_and_builtin_copies_compile() {
        for generator in [false, true] {
            let src = template(generator);
            let header = parse_header(&src).unwrap();
            assert_eq!(header.generator, generator);
            let (full, pre) = module_source(&src, "custom_test").unwrap();
            validate(&full, pre).unwrap();
        }
        for t in TEMPLATES {
            let header = parse_header(t.source).unwrap_or_else(|e| panic!("{}: {e}", t.name));
            assert!(!header.name.is_empty());
            let lines = t.source.lines().filter(|l| l.starts_with("// @param")).count();
            assert_eq!(header.params.len(), lines, "{}: every @param line is a slider", t.name);
            let (full, pre) = module_source(t.source, "custom_template").unwrap();
            validate(&full, pre).unwrap_or_else(|e| panic!("{}: {e}", t.name));
        }
        for id in ["kaleidoscope", "plasma", "crt"] {
            let src = builtin_as_custom(id).unwrap_or_else(|| panic!("no copy for {id}"));
            let (full, pre) = module_source(&src, "custom_copy").unwrap();
            validate(&full, pre).unwrap_or_else(|e| panic!("{id}: {e}"));
        }
    }

    #[test]
    fn errors_point_at_user_lines() {
        let src = "// @name Broken\n@fragment\nfn main(in: VOut) -> @location(0) vec4f {\n    return vec4f(nope);\n}\n";
        let (full, pre) = module_source(src, "custom_broken").unwrap();
        let err = validate(&full, pre).unwrap_err();
        assert_eq!(err.line, Some(4), "{err}");
    }
}

