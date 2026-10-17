//! Turns ISF-flavoured GLSL (1.20-era, host-provided uniforms) into a
//! self-contained GLSL 450 fragment shader that naga's frontend accepts.

use crate::header::Header;
use crate::{IsfInput, IsfInputKind};

/// Blanks `//` and `/* */` comments, preserving newlines so brace scanning stays simple.
pub(crate) fn strip_comments(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i < b.len() && !(b[i] == b'*' && b.get(i + 1) == Some(&b'/')) {
                if b[i] == b'\n' {
                    out.push('\n');
                }
                i += 1;
            }
            i = (i + 2).min(b.len());
            out.push(' ');
        } else {
            // Copy one UTF-8 scalar.
            let ch = src[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Resolves the `#if __VERSION__ ...` / `#ifdef GL_ES` conditionals ISF files
/// use to stay portable, assuming GLSL 450 core. Other directives pass through
/// (naga's preprocessor handles `#define`), except `#version`, `#extension`,
/// `#include` and `precision` which are dropped.
pub(crate) fn resolve_directives(src: &str) -> String {
    #[derive(Clone, Copy)]
    struct Frame {
        /// This branch is being emitted.
        active: bool,
        /// Some branch of this `#if` chain has already been taken.
        taken: bool,
        /// Condition could not be evaluated: leave the directives for naga.
        passthrough: bool,
    }
    let mut stack: Vec<Frame> = Vec::new();
    let mut out = String::with_capacity(src.len());
    let emitting = |stack: &[Frame]| stack.iter().all(|f| f.active || f.passthrough);
    for line in src.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix('#') {
            let rest = rest.trim_start();
            let (word, arg) = rest.split_once(|c: char| c.is_whitespace()).unwrap_or((rest, ""));
            let arg = arg.trim();
            match word {
                "if" | "ifdef" | "ifndef" => {
                    // MadMapper's `name_IS_choice` macros are defined by the prelude: leave those to naga.
                    let cond = match word {
                        _ if arg.contains("_IS_") => None,
                        "ifdef" => Some(arg == "__VERSION__"),
                        "ifndef" => Some(arg != "__VERSION__"),
                        _ => eval_version_cond(arg),
                    };
                    match cond {
                        Some(c) => stack.push(Frame { active: c, taken: c, passthrough: false }),
                        None => {
                            stack.push(Frame { active: true, taken: true, passthrough: true });
                            out.push_str(line);
                        }
                    }
                }
                "elif" => {
                    if let Some(f) = stack.last_mut() {
                        if f.passthrough {
                            out.push_str(line);
                        } else {
                            let c = !f.taken && eval_version_cond(arg).unwrap_or(false);
                            f.active = c;
                            f.taken |= c;
                        }
                    }
                }
                "else" => {
                    if let Some(f) = stack.last_mut() {
                        if f.passthrough {
                            out.push_str(line);
                        } else {
                            f.active = !f.taken;
                            f.taken = true;
                        }
                    }
                }
                "endif" => {
                    if let Some(f) = stack.pop()
                        && f.passthrough
                    {
                        out.push_str(line);
                    }
                }
                "version" | "extension" | "include" | "pragma" => {}
                _ => {
                    if emitting(&stack) {
                        out.push_str(line);
                    }
                }
            }
            out.push('\n');
            continue;
        }
        if emitting(&stack) && !t.starts_with("precision ") {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// Evaluates `__VERSION__ <op> N`, `defined(GL_ES)` style conditions; `None` when unknown.
fn eval_version_cond(expr: &str) -> Option<bool> {
    let e: String = expr.split_whitespace().collect();
    if e == "GL_ES" || e == "defined(GL_ES)" {
        return Some(false);
    }
    if e == "!defined(GL_ES)" {
        return Some(true);
    }
    const VERSION: i64 = 450;
    for (op, f) in [
        ("<=", (|a: i64, b: i64| a <= b) as fn(i64, i64) -> bool),
        (">=", |a, b| a >= b),
        ("==", |a, b| a == b),
        ("!=", |a, b| a != b),
        ("<", |a, b| a < b),
        (">", |a, b| a > b),
    ] {
        if let Some((l, r)) = e.split_once(op) {
            let (l, r) = (l.trim_matches(|c| c == '(' || c == ')'), r.trim_matches(|c| c == '(' || c == ')'));
            let val = |s: &str| if s == "__VERSION__" { Some(VERSION) } else { s.parse::<i64>().ok() };
            return Some(f(val(l)?, val(r)?));
        }
    }
    None
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Rewrites top-level `varying` / `in` / `out` / `attribute` declarations into
/// plain globals (or removes them when `drop` is set and they were already
/// declared by the vertex shader). Returns the rewritten source and the names declared.
pub(crate) fn rewrite_io_decls(src: &str, drop: &[String]) -> (String, Vec<String>) {
    let mut out = String::with_capacity(src.len());
    let mut names = Vec::new();
    let mut depth = 0i32;
    let mut paren = 0i32;
    for line in src.lines() {
        let t = line.trim_start();
        let mut handled = false;
        if depth == 0 && paren == 0 && !t.starts_with('#') {
            let mut rest = t;
            let mut matched = false;
            loop {
                let mut progressed = false;
                for q in ["varying", "attribute", "flat", "smooth", "noperspective", "in", "out", "highp", "mediump", "lowp"] {
                    if let Some(r) = rest.strip_prefix(q)
                        && r.starts_with(|c: char| c.is_whitespace())
                    {
                        rest = r.trim_start();
                        matched |= matches!(q, "varying" | "attribute" | "in" | "out");
                        progressed = true;
                    }
                }
                if !progressed {
                    break;
                }
            }
            if matched && rest.contains(';') {
                let decl = rest.split(';').next().unwrap_or("").trim();
                let name = decl
                    .split_whitespace()
                    .last()
                    .unwrap_or("")
                    .split(|c: char| c == '[' || c == '=')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !drop.contains(&name) {
                    out.push_str(rest);
                    names.push(name);
                }
                handled = true;
            }
        }
        if !handled {
            out.push_str(line);
        }
        out.push('\n');
        for &c in line.as_bytes() {
            match c {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                b'(' => paren += 1,
                b')' => paren -= 1,
                _ => {}
            }
        }
    }
    (out, names)
}

/// Replaces `void main(void)` with `void <name>()`. Returns `None` if there is no main.
pub(crate) fn rename_main(src: &str, name: &str) -> Option<String> {
    let b = src.as_bytes();
    let mut i = 0;
    while let Some(pos) = src[i..].find("main") {
        let s = i + pos;
        let e = s + 4;
        i = e;
        if s > 0 && is_ident(b[s - 1]) || e < b.len() && is_ident(b[e]) {
            continue;
        }
        // Must be preceded by `void` and followed by `(` [void] `)`.
        let before = src[..s].trim_end();
        if !before.ends_with("void") {
            continue;
        }
        let after = src[e..].trim_start();
        let Some(args) = after.strip_prefix('(') else { continue };
        let args = args.trim_start();
        let Some(tail) = args.strip_prefix("void").map(str::trim_start).or(Some(args)).and_then(|a| a.strip_prefix(')')) else {
            continue;
        };
        let tail_start = src.len() - tail.len();
        return Some(format!("{}{}(){}", &src[..before.len() - 4], format_args!("void {name}"), &src[tail_start..]));
    }
    None
}

/// True if `name` appears as an identifier in `src`.
pub(crate) fn uses_ident(src: &str, name: &str) -> bool {
    let b = src.as_bytes();
    let mut i = 0;
    while let Some(pos) = src[i..].find(name) {
        let s = i + pos;
        let e = s + name.len();
        i = e;
        if (s == 0 || !is_ident(b[s - 1])) && (e >= b.len() || !is_ident(b[e])) {
            return true;
        }
    }
    false
}

/// Dialect of the fragment source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dialect {
    /// Standard ISF: `void main()` writes `gl_FragColor`.
    Isf,
    /// MadMapper material: `vec4 materialColorForPixel(vec2)`.
    MadMaterial,
    /// MadMapper surface FX: `vec4 fxColorForPixel(vec2)`, input via `FX_NORM_PIXEL`.
    MadFx,
}

pub(crate) fn detect_dialect(body: &str) -> Dialect {
    if uses_ident(body, "materialColorForPixel") {
        Dialect::MadMaterial
    } else if uses_ident(body, "fxColorForPixel") {
        Dialect::MadFx
    } else {
        Dialect::Isf
    }
}

/// GLSL expression reading `n` consecutive floats from `u.p` starting at float `offset`.
fn param_expr(offset: usize, n: usize) -> String {
    const SW: [&str; 4] = ["x", "y", "z", "w"];
    let mut parts = Vec::new();
    let mut i = offset;
    while i < offset + n {
        let vec = i / 4;
        let start = i % 4;
        let take = (4 - start).min(offset + n - i);
        let sw: String = (start..start + take).map(|k| SW[k]).collect();
        parts.push(format!("phosphor_u.p[{vec}].{sw}"));
        i += take;
    }
    match n {
        1 => parts.join(""),
        _ if parts.len() == 1 && n == 4 => parts.remove(0),
        _ => format!("vec{n}({})", parts.join(", ")),
    }
}

/// Builds the prelude: uniform block, bindings, ISF built-ins and one macro per input.
/// Minimal stand-in for MadMapper's built-in `MadCommon.glsl` (not shipped as a file).
/// Each helper is emitted only if the shader does not define it itself.
pub(crate) fn mad_common_shim(body: &str) -> String {
    let mut s = String::new();
    if crate::mad_shim::called_not_defined(body, "luma") {
        s.push_str("float luma(vec3 c) { return dot(c, vec3(0.299, 0.587, 0.114)); }\nfloat luma(vec4 c) { return dot(c.rgb, vec3(0.299, 0.587, 0.114)); }\n");
    }
    if crate::mad_shim::called_not_defined(body, "saturate") {
        s.push_str("float saturate(float v) { return clamp(v, 0.0, 1.0); }\nvec2 saturate(vec2 v) { return clamp(v, 0.0, 1.0); }\nvec3 saturate(vec3 v) { return clamp(v, 0.0, 1.0); }\nvec4 saturate(vec4 v) { return clamp(v, 0.0, 1.0); }\n");
    }
    if !uses_ident(body, "PI") || !body.contains("define PI") && !body.contains("float PI") {
        s.push_str("#define PI 3.14159265358979\n");
    }
    if !body.contains("rgb2hsv(") || !body.contains("vec3 rgb2hsv") {
        s.push_str(
            "vec3 rgb2hsv(vec3 c) { vec4 K = vec4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0); vec4 p = mix(vec4(c.bg, K.wz), vec4(c.gb, K.xy), step(c.b, c.g)); \
vec4 q = mix(vec4(p.xyw, c.r), vec4(c.r, p.yzx), step(p.x, c.r)); float d = q.x - min(q.w, q.y); float e = 1.0e-10; \
return vec3(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x); }\n",
        );
    }
    if !body.contains("vec3 hsv2rgb") {
        s.push_str(
            "vec3 hsv2rgb(vec3 c) { vec4 K = vec4(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0); vec3 p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www); \
return c.z * mix(K.xxx, clamp(p - K.xxx, 0.0, 1.0), c.y); }\n",
        );
    }
    s
}

pub(crate) fn prelude(header: &Header, inputs: &[IsfInput], dialect: Dialect) -> String {
    let mut s = String::new();
    // The uniform block, sized from the one constant (see `UNIFORM_VEC4S`).
    s.push_str(&format!(
        "#version 450\nlayout(std140, binding=0) uniform phosphor_U {{ vec4 p[{}]; vec2 resolution; float time; float opacity; vec4 uv_xform; uint mode; uint yuv; uint _pad0; uint _pad1; uint _pad2; uint _pad3; uint _pad4; uint _pad5; }} phosphor_u;\n",
        crate::UNIFORM_VEC4S
    ));
    s.push_str(
        "layout(binding=1) uniform sampler phosphor_samp;\n\
layout(binding=3) uniform texture2D phosphor_tex0;\n\
layout(binding=4) uniform texture2D phosphor_tex1;\n\
layout(binding=5) uniform texture2D phosphor_tex2;\n\
layout(location=0) in vec2 phosphor_uv;\n\
layout(location=0) out vec4 phosphor_out;\n\
vec4 phosphor_fragcoord;\n\
vec2 isf_FragNormCoord;\n\
vec2 mm_FragNormCoord;\n\
vec2 mm_SurfaceCoord;\n\
void phosphor_init() {\n\
    phosphor_fragcoord = vec4(gl_FragCoord.x, phosphor_u.resolution.y - gl_FragCoord.y, gl_FragCoord.z, gl_FragCoord.w);\n\
    isf_FragNormCoord = vec2(phosphor_uv.x, 1.0 - phosphor_uv.y);\n\
    mm_FragNormCoord = isf_FragNormCoord;\n\
    mm_SurfaceCoord = isf_FragNormCoord;\n\
}\n\
vec2 phosphor_flip(vec2 v) { return vec2(v.x, 1.0 - v.y); }\n\
#define gl_FragCoord phosphor_fragcoord\n\
#define gl_FragColor phosphor_out\n\
#define vv_FragNormCoord isf_FragNormCoord\n\
#define TIME (phosphor_u.time)\n\
#define RENDERSIZE (phosphor_u.resolution)\n\
#define TIMEDELTA (phosphor_u.p[15].z)\n\
#define FRAMEINDEX (int(phosphor_u.p[15].y))\n\
#define PASSINDEX (int(phosphor_u.p[15].x))\n\
#define DATE vec4(2026.0, 1.0, 1.0, phosphor_u.time)\n\
#define IMG_NORM_PIXEL(img, nuv) textureLod(sampler2D(img, phosphor_samp), phosphor_flip(nuv), 0.0)\n\
#define IMG_PIXEL(img, px) textureLod(sampler2D(img, phosphor_samp), phosphor_flip((px) / vec2(textureSize(img, 0))), 0.0)\n\
#define IMG_THIS_PIXEL(img) textureLod(sampler2D(img, phosphor_samp), phosphor_uv, 0.0)\n\
#define IMG_THIS_NORM_PIXEL(img) textureLod(sampler2D(img, phosphor_samp), phosphor_uv, 0.0)\n\
#define IMG_SIZE(img) vec2(textureSize(img, 0))\n\
#define texture2D(img, nuv) textureLod(sampler2D(img, phosphor_samp), phosphor_flip(nuv), 0.0)\n\
#define FX_NORM_PIXEL(nuv) textureLod(sampler2D(phosphor_tex0, phosphor_samp), phosphor_flip(nuv), 0.0)\n\
#define FX_IMG_SIZE() vec2(textureSize(phosphor_tex0, 0))\n\
#define mm_Alpha 1.0\n\
#define mm_ModulationColor vec4(1.0)\n\
#define mm_BlendMode 2\n\
#define mm_IsRenderingPreview false\n\
#define mm_SurfaceIndex 0\n",
    );
    // Multi-pass buffers: textures named like their TARGET, so IMG_PIXEL(name, …) samples them.
    let mut seen: Vec<&str> = Vec::new();
    let imported = header.imported.iter().map(|(n, _)| n.as_str());
    let curves = inputs.iter().filter(|i| matches!(i.kind, IsfInputKind::Curve { .. })).map(|i| i.name.as_str());
    for t in header.pass_specs.iter().filter_map(|p| p.target.as_deref()).chain(imported).chain(curves) {
        if !seen.contains(&t) && seen.len() < crate::MAX_BUFFERS {
            s.push_str(&format!("layout(binding={}) uniform texture2D {t};\n", crate::BUFFER_BINDING + seen.len()));
            s.push_str(&format!("#define _{t}_imgRect vec4(0.0, 0.0, vec2(textureSize({t}, 0)))\n"));
            seen.push(t);
        }
    }
    // Value inputs are real globals (so shaders may shadow them, as with uniforms),
    // assigned from `u.p` in `phosphor_init`. Images stay macros for the texture bindings.
    let mut init = String::from("void phosphor_init_inputs() {\n");
    for input in inputs {
        let (ty, expr) = match (&input.kind, input.offset, input.texture) {
            (IsfInputKind::Float { .. }, Some(o), _) => ("float", param_expr(o, 1)),
            (IsfInputKind::Bool { .. } | IsfInputKind::Event, Some(o), _) => ("bool", format!("({} != 0.0)", param_expr(o, 1))),
            (IsfInputKind::Long { .. } | IsfInputKind::Int { .. }, Some(o), _) => ("int", format!("int({})", param_expr(o, 1))),
            (IsfInputKind::Color { .. }, Some(o), _) => ("vec4", param_expr(o, 4)),
            (IsfInputKind::Point2D { .. }, Some(o), _) => ("vec2", param_expr(o, 2)),
            (IsfInputKind::Image | IsfInputKind::Audio { .. }, _, Some(t)) => {
                s.push_str(&format!("#define {} phosphor_tex{t}\n", input.name));
                // ISF's `_<name>_imgRect` (x, y, width, height of the picture in its texture): ours fill the texture.
                s.push_str(&format!("#define _{}_imgRect vec4(0.0, 0.0, vec2(textureSize(phosphor_tex{t}, 0)))\n", input.name));
                continue;
            }
            _ => continue,
        };
        s.push_str(&format!("{ty} {};\n", input.name));
        init.push_str(&format!("    {} = {expr};\n", input.name));
    }
    // MadMapper's `#if defined(name_IS_choice)`: the chosen branch is fixed at import from the default.
    for input in inputs {
        match &input.kind {
            IsfInputKind::Long { values, labels, default } => {
                if let Some(i) = values.iter().position(|v| v == default)
                    && let Some(label) = labels.get(i)
                {
                    let label: String = label.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
                    s.push_str(&format!("#define {}_IS_{label} 1\n", input.name));
                }
            }
            IsfInputKind::Bool { default } => {
                s.push_str(&format!("#define {}_IS_{} 1\n", input.name, if *default { "true" } else { "false" }));
            }
            _ => {}
        }
    }
    for (name, speed) in &header.time_bases {
        s.push_str(&format!("float {name};\n"));
        init.push_str(&format!("    {name} = phosphor_u.time * ({speed});\n"));
    }
    init.push_str("}\n");
    s.push_str(&init);
    if dialect == Dialect::MadFx {
        s.push_str("#define inputImage phosphor_tex0\n#define fx_restrict false\n#define fx_fake false\n");
    }
    // Inlined vertex shaders call this; the fragment prelude already did the work.
    s.push_str("void isf_vertShaderInit() {}\n");
    s
}

/// Final `main` that runs the (inlined) vertex stage, then the fragment stage.
pub(crate) fn epilogue(dialect: Dialect, has_vs: bool, mad_vs: bool) -> String {
    let mut s = String::from("void main() {\n    phosphor_init();\n    phosphor_init_inputs();\n");
    if mad_vs {
        s.push_str("    materialVsFunc(isf_FragNormCoord);\n");
    } else if has_vs {
        s.push_str("    phosphor_vs_main();\n");
    }
    match dialect {
        Dialect::Isf => s.push_str("    phosphor_fs_main();\n"),
        Dialect::MadMaterial => s.push_str("    phosphor_out = materialColorForPixel(isf_FragNormCoord);\n"),
        Dialect::MadFx => s.push_str("    phosphor_out = fxColorForPixel(isf_FragNormCoord);\n"),
    }
    s.push_str("}\n");
    s
}

/// GLSL lets a `float f()` say `return 0;` or fall off its end; naga's front end doesn't. Rewrites integer
/// literals returned from float functions to floats and adds a zero return where a non-void function's body
/// ends without one. Works on top-level function definitions only.
pub(crate) fn fix_returns(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len() + 64);
    let mut i = 0;
    let mut last = 0;
    while i < bytes.len() {
        // A definition: `<type> <name>(` at brace depth 0 followed (after the parameter list) by `{`.
        let Some((ty, body_start)) = function_at(src, i) else {
            i += 1;
            continue;
        };
        let Some(body_end) = matching_brace(src, body_start) else { break };
        let body = &src[body_start + 1..body_end];
        let zero = match ty {
            "float" => "0.0",
            "int" => "0",
            "vec2" => "vec2(0.0)",
            "vec3" => "vec3(0.0)",
            "vec4" => "vec4(0.0)",
            _ => "",
        };
        out.push_str(&src[last..body_start + 1]);
        let mut fixed = body.to_string();
        if ty == "float" {
            fixed = fix_int_returns(&fixed);
        }
        let trimmed = fixed.trim_end();
        let ends_with_return = trimmed.ends_with(';') && trimmed.rsplit(';').nth(1).is_some_and(|s| s.trim_start().starts_with("return"));
        if !zero.is_empty() && !ends_with_return {
            fixed.push_str(&format!("\n    return {zero};\n"));
        }
        out.push_str(&fixed);
        last = body_end;
        i = body_end;
    }
    out.push_str(&src[last..]);
    out
}

/// `return 12;` → `return 12.0;` (integer literal only, not expressions).
fn fix_int_returns(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(pos) = rest.find("return ") {
        let (head, tail) = rest.split_at(pos + 7);
        out.push_str(head);
        let lit_end = tail.find(|c: char| !c.is_ascii_digit()).unwrap_or(tail.len());
        if lit_end > 0 && tail[lit_end..].trim_start().starts_with(';') {
            out.push_str(&tail[..lit_end]);
            out.push_str(".0");
            rest = &tail[lit_end..];
        } else {
            rest = tail;
        }
    }
    out.push_str(rest);
    out
}

/// If a top-level function definition starts at `i`: its return type and the index of its `{`.
fn function_at(src: &str, i: usize) -> Option<(&str, usize)> {
    let b = src.as_bytes();
    if i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_') {
        return None;
    }
    let rest = &src[i..];
    let ty = ["float", "int", "vec2", "vec3", "vec4", "void", "mat2", "mat3", "mat4", "bool"].into_iter().find(|t| rest.starts_with(t) && rest[t.len()..].starts_with(|c: char| c.is_whitespace()))?;
    let after = rest[ty.len()..].trim_start();
    let name_len = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))?;
    if name_len == 0 || !after[name_len..].trim_start().starts_with('(') {
        return None;
    }
    let paren = i + (rest.len() - after.len()) + name_len + after[name_len..].find('(')?;
    let close = matching_paren(src, paren)?;
    let after_params = src[close + 1..].trim_start();
    if !after_params.starts_with('{') {
        return None;
    }
    // Only definitions at the top level (not nested blocks, which GLSL doesn't allow anyway).
    if src[..i].bytes().filter(|&c| c == b'{').count() != src[..i].bytes().filter(|&c| c == b'}').count() {
        return None;
    }
    Some((ty, close + 1 + (src.len() - close - 1 - after_params.len())))
}

fn matching_paren(src: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (k, c) in src[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + k);
                }
            }
            _ => {}
        }
    }
    None
}

fn matching_brace(src: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (k, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + k);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod return_tests {
    #[test]
    fn float_functions_get_float_returns_and_an_ending() {
        let src = "float a(float x) {\n    if (x > 0.0) return 1;\n}\nvec2 b() { return vec2(1.0); }\nvoid main() { float y = a(2.0); }\n";
        let out = super::fix_returns(src);
        assert!(out.contains("return 1.0;"), "{out}");
        assert!(out.contains("return 0.0;\n}"), "{out}");
        assert_eq!(out.matches("return vec2(0.0)").count(), 0, "{out}");
        assert!(!out.contains("main() { float y = a(2.0); \n    return"), "{out}");
    }
}
