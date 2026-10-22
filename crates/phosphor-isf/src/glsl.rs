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

/// Removes top-level `uniform <type> <name>;` lines that declare one of the shader's own ISF inputs again
/// (ISF 1 style); every input is already declared by the prelude.
pub(crate) fn drop_input_uniforms(src: &str, inputs: &[&str]) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.split_inclusive('\n') {
        let t = line.trim();
        let redeclared = t.strip_prefix("uniform").filter(|r| r.starts_with(char::is_whitespace)).and_then(|r| r.trim().strip_suffix(';')).is_some_and(|decl| {
            let mut words = decl.split_whitespace();
            matches!((words.next(), words.next(), words.next()), (Some(_), Some(name), None) if inputs.contains(&name))
        });
        if !redeclared {
            out.push_str(line);
        }
    }
    out
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

/// Values a real vertex stage hands the fragment stage (`out float a;` in the vertex shader, `in float a;` in the
/// fragment shader; `varying` in both for ISF 1): each gets a location from 1 up (0 is the picture coordinate), and
/// both declarations are rewritten as `layout(location=N) out/in TYPE NAME;`. Returns the two sources and the
/// (type, name) list; None when the vertex shader hands nothing on.
pub(crate) fn link_varyings(vs: &str, fs: &str) -> Option<(String, String, Vec<(String, String)>)> {
    // `[flat] (out|varying) TYPE NAME;` at the top level, as (type, name).
    fn decl(line: &str, words: &[&str]) -> Option<(String, String)> {
        let mut t = line.trim();
        for q in ["flat ", "smooth ", "noperspective ", "highp ", "mediump ", "lowp "] {
            t = t.strip_prefix(q).unwrap_or(t).trim_start();
        }
        let rest = words.iter().find_map(|w| t.strip_prefix(w).filter(|r| r.starts_with(char::is_whitespace)))?;
        let body = rest.trim().strip_suffix(';')?.trim();
        let mut parts = body.split_whitespace();
        let (ty, name) = (parts.next()?, parts.next()?);
        (parts.next().is_none() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')).then(|| (ty.to_string(), name.to_string()))
    }
    let mut list: Vec<(String, String)> = Vec::new();
    let mut vs_out = String::with_capacity(vs.len());
    for line in vs.lines() {
        match decl(line, &["out", "varying"]) {
            Some((ty, name)) if name != "phosphor_uv" => {
                list.push((ty.clone(), name.clone()));
                vs_out.push_str(&format!("layout(location={}) out {ty} {name};\n", list.len()));
            }
            _ => {
                vs_out.push_str(line);
                vs_out.push('\n');
            }
        }
    }
    if list.is_empty() {
        return None;
    }
    let mut fs_out = String::with_capacity(fs.len());
    for line in fs.lines() {
        match decl(line, &["in", "varying"]).and_then(|(ty, name)| list.iter().position(|(_, n)| *n == name).map(|k| (ty, name, k + 1))) {
            Some((ty, name, k)) => fs_out.push_str(&format!("layout(location={k}) in {ty} {name};\n")),
            None => {
                fs_out.push_str(line);
                fs_out.push('\n');
            }
        }
    }
    Some((vs_out, fs_out, list))
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

/// Words reserved in GLSL 4.50 (what shaders are converted as) that older ISF / GLSL shaders use as plain names
/// (`vec3 sample = …`, `float filter`, `vec4 input`). Also the names the converted WGSL gives Phosphor's own bindings
/// (`u`, `samp`, `tex0`, `buf0`…): a local `vec2 u` would hide the uniforms (`TIME` reads `u.time`).
const RESERVED_SINCE_OLD_GLSL: [&str; 31] = [
    "sample", "input", "output", "filter", "buffer", "shared", "common", "partition", "active", "patch", "resource", "subroutine", "superp",
    "u", "samp", "samp_mirror", "tex0", "tex1", "tex2", "buf0", "buf1", "buf2", "buf3", "buf4", "buf5", "buf6", "buf7", "buf8", "buf9", "buf10", "buf11",
];

/// Renames those words where a shader uses them as names (`sample` → `isf_sample`), except `keep` (the shader's own
/// input names: Phosphor declares those itself).
pub(crate) fn rename_reserved(src: &str, keep: &[&str]) -> String {
    let mut out = src.to_string();
    for word in RESERVED_SINCE_OLD_GLSL.iter().filter(|w| !keep.contains(w)) {
        out = rename_ident(&out, word, &format!("isf_{word}"));
    }
    out
}

/// Every whole-identifier `word` in `src` → `to`.
pub(crate) fn rename_ident(src: &str, word: &str, to: &str) -> String {
    if !uses_ident(src, word) {
        return src.to_string();
    }
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len() + 16);
    let mut i = 0;
    while let Some(pos) = src[i..].find(word) {
        let s = i + pos;
        let e = s + word.len();
        out.push_str(&src[i..s]);
        let whole = (s == 0 || !is_ident(b[s - 1])) && (e >= b.len() || !is_ident(b[e]));
        out.push_str(if whole { to } else { word });
        i = e;
    }
    out.push_str(&src[i..]);
    out
}

/// Function parameters that take a picture: `vec4 blur(in sampler2D img, …)` → `texture2D img`. Every picture here
/// is a `texture2D` read with the shared sampler, so that's what callers pass and what the read macros expect.
/// Uniform declarations and `sampler2D(tex, samp)` constructors are left alone.
pub(crate) fn picture_params(src: &str) -> String {
    const QUALIFIERS: [&str; 6] = ["in", "const", "highp", "mediump", "lowp", "uniform"];
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while let Some(pos) = src[i..].find("sampler2D") {
        let s = i + pos;
        let e = s + "sampler2D".len();
        let whole = (s == 0 || !is_ident(b[s - 1])) && (e >= b.len() || !is_ident(b[e]));
        // What comes before, skipping parameter qualifiers: a parameter starts after `(` or `,`.
        let mut start = s;
        let mut before = src[..s].trim_end();
        while let Some(q) = QUALIFIERS.iter().find(|q| before.ends_with(**q) && before.len() > q.len() && !is_ident(before.as_bytes()[before.len() - q.len() - 1])) {
            if *q == "uniform" {
                break;
            }
            before = before[..before.len() - q.len()].trim_end();
            start = before.len();
        }
        let in_params = before.ends_with('(') || before.ends_with(',');
        // After: a name (or nothing, in a prototype), then `,` or `)`; a constructor has `(` straight after.
        let after = src[e..].trim_start();
        let name_len = after.bytes().take_while(|c| is_ident(*c)).count();
        let next = after[name_len..].trim_start();
        let is_param = in_params && (next.starts_with(',') || next.starts_with(')'));
        if whole && is_param {
            // Qualifiers go too (`const in texture2D` isn't something naga takes).
            let keep = if start < s { &src[i..start] } else { &src[i..s] };
            out.push_str(keep);
            if start < s {
                out.push(' ');
            }
            out.push_str("texture2D");
        } else {
            out.push_str(&src[i..e]);
        }
        i = e;
    }
    out.push_str(&src[i..]);
    out
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
    for t in header.pass_specs.iter().filter_map(|p| p.target.as_deref()).chain(imported) {
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
            (IsfInputKind::Curve { .. }, Some(o), _) => {
                // A smooth (Catmull-Rom) curve through the editable heights, read like the curve picture was.
                let ys: Vec<String> = (0..crate::CURVE_POINTS).map(|k| param_expr(o + k, 1)).collect();
                s.push_str(&format!(
                    "vec4 phosphor_curve_{name}(vec2 uv) {{ float ys[{n}] = float[{n}]({ys}); float x = clamp(uv.x, 0.0, 1.0) * {last}.0; int i = int(min(floor(x), {last1}.0)); float t = x - float(i); \
float p1 = ys[i]; float p2 = ys[i + 1]; float p0 = (i > 0) ? ys[max(i - 1, 0)] : 2.0 * p1 - p2; float p3 = (i + 2 <= {last}) ? ys[min(i + 2, {last})] : 2.0 * p2 - p1; \
float y = 0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t * t * t); \
y = clamp(y, 0.0, 1.0); return vec4(y, y, y, 1.0); }}\n",
                    name = input.name,
                    n = crate::CURVE_POINTS,
                    last = crate::CURVE_POINTS - 1,
                    last1 = crate::CURVE_POINTS - 2,
                    ys = ys.join(", ")
                ));
                continue;
            }
            (IsfInputKind::Point2D { .. }, Some(o), _) if input.pixels => ("vec2", format!("({} * phosphor_u.resolution)", param_expr(o, 2))),
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
    let mut s = String::from("void main() {\n    phosphor_init();\n    phosphor_init_inputs();\n    phosphor_init_globals();\n");
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

/// GLSL lets a global be initialised from uniforms (`float d = min(progress, 1.0);` at file scope); naga wants
/// constants there. Turns such declarations into plain globals and returns the assignments, in order, for an
/// init function that runs at the start of `main`. `const` declarations and anything inside braces are left alone.
pub(crate) fn hoist_global_inits(src: &str) -> (String, Vec<String>) {
    let mut out = String::with_capacity(src.len());
    let mut inits = Vec::new();
    let mut depth = 0i32;
    // Statements at depth 0 end at ';' (function bodies are skipped by the brace depth).
    let mut stmt = String::new();
    for c in src.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        if depth == 0 && c == ';' {
            stmt.push(c);
            let t = stmt.trim_start_matches(|c: char| c.is_whitespace()).to_string();
            if let Some((decl, init)) = global_init(&t) {
                out.push_str(&stmt[..stmt.len() - t.len()]);
                out.push_str(&decl);
                inits.push(init);
            } else {
                out.push_str(&stmt);
            }
            stmt.clear();
            continue;
        }
        if depth == 0 {
            stmt.push(c);
        } else {
            // Inside a function or a parameter list: copy straight through (with whatever was pending).
            out.push_str(&stmt);
            stmt.clear();
            out.push(c);
        }
    }
    out.push_str(&stmt);
    (out, inits)
}

/// `type name = expr;` (no `const`, no `#`, no array) → (`type name;`, `name = expr;`).
fn global_init(stmt: &str) -> Option<(String, String)> {
    let body = stmt.strip_suffix(';')?.trim();
    if body.starts_with('#') || body.starts_with("const ") || body.starts_with("uniform ") || body.starts_with("layout") || body.contains('[') || body.contains('{') {
        return None;
    }
    let (head, expr) = body.split_once('=')?;
    if head.ends_with(|c: char| matches!(c, '<' | '>' | '!')) || expr.starts_with('=') {
        return None;
    }
    let mut words = head.split_whitespace();
    let ty = words.next()?;
    let name = words.next()?;
    if words.next().is_some() || !["float", "int", "uint", "bool", "vec2", "vec3", "vec4", "ivec2", "ivec3", "ivec4", "mat2", "mat3", "mat4"].contains(&ty) {
        return None;
    }
    // Plain numbers are fine where they are.
    let e = expr.trim();
    if e.parse::<f64>().is_ok() {
        return None;
    }
    Some((format!("{ty} {name};"), format!("{name} = {e};")))
}

#[cfg(test)]
mod hoist_tests {
    #[test]
    fn uniform_dependent_globals_move_into_init() {
        let src = "float d = min(progress, 1.0 - progress);\nconst float PI = 3.14;\nfloat k = 2.0;\nvec4 f(vec2 uv) { float q = d * 2.0; return vec4(q); }\nvoid main() { gl_FragColor = f(uv); }\n";
        let (out, inits) = super::hoist_global_inits(src);
        assert!(out.contains("float d;"), "{out}");
        assert!(out.contains("const float PI = 3.14;") && out.contains("float k = 2.0;"), "{out}");
        assert!(out.contains("float q = d * 2.0;"), "{out}");
        assert_eq!(inits, vec!["d = min(progress, 1.0 - progress);".to_string()]);
    }
}

/// Prelude for a real vertex stage (ISF vertex shaders that move the quad): the same uniforms and inputs as the
/// fragment prelude, `phosphor_uv` as the varying the fragment stage reads, and `isf_vertShaderInit()` placing
/// the six vertices of a full-screen quad by `gl_VertexIndex`.
pub(crate) fn vertex_prelude(header: &Header, inputs: &[IsfInput]) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "#version 450\nlayout(std140, binding=0) uniform phosphor_U {{ vec4 p[{}]; vec2 resolution; float time; float opacity; vec4 uv_xform; uint mode; uint yuv; uint _pad0; uint _pad1; uint _pad2; uint _pad3; uint _pad4; uint _pad5; }} phosphor_u;\n",
        crate::UNIFORM_VEC4S
    ));
    s.push_str(
        "layout(location=0) out vec2 phosphor_uv;\n\
vec2 isf_FragNormCoord;\n\
vec2 vv_FragNormCoord;\n\
#define TIME (phosphor_u.time)\n\
#define RENDERSIZE (phosphor_u.resolution)\n\
#define TIMEDELTA (phosphor_u.p[15].z)\n\
#define FRAMEINDEX (int(phosphor_u.p[15].y))\n\
#define PASSINDEX (int(phosphor_u.p[15].x))\n\
#define DATE vec4(2026.0, 1.0, 1.0, phosphor_u.time)\n\
#define gl_InstanceID gl_InstanceIndex\n\
void isf_vertShaderInit() {\n\
    vec2 corners[6] = vec2[6](vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0), vec2(1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0));\n\
    vec2 c = corners[gl_VertexIndex];\n\
    gl_Position = vec4(c, 0.0, 1.0);\n\
    isf_FragNormCoord = c * 0.5 + 0.5;\n\
    vv_FragNormCoord = isf_FragNormCoord;\n\
}\n",
    );
    let mut init = String::from("void phosphor_init_inputs() {\n");
    for input in inputs {
        let (ty, expr) = match (&input.kind, input.offset) {
            (IsfInputKind::Float { .. }, Some(o)) => ("float", param_expr(o, 1)),
            (IsfInputKind::Bool { .. } | IsfInputKind::Event, Some(o)) => ("bool", format!("({} != 0.0)", param_expr(o, 1))),
            (IsfInputKind::Long { .. } | IsfInputKind::Int { .. }, Some(o)) => ("int", format!("int({})", param_expr(o, 1))),
            (IsfInputKind::Color { .. }, Some(o)) => ("vec4", param_expr(o, 4)),
            (IsfInputKind::Point2D { .. }, Some(o)) if input.pixels => ("vec2", format!("({} * phosphor_u.resolution)", param_expr(o, 2))),
            (IsfInputKind::Point2D { .. }, Some(o)) => ("vec2", param_expr(o, 2)),
            _ => continue,
        };
        s.push_str(&format!("{ty} {};\n", input.name));
        init.push_str(&format!("    {} = {expr};\n", input.name));
    }
    for (name, speed) in &header.time_bases {
        s.push_str(&format!("float {name};\n"));
        init.push_str(&format!("    {name} = phosphor_u.time * ({speed});\n"));
    }
    init.push_str("}\n");
    s.push_str(&init);
    s
}

/// MadMapper's vertex-function names on Phosphor's quad (a Surface FX's `fxVsFunc` as a vertex stage): the corner
/// position is the vertex (clip space, -1..1), the matrices are identity, and the picture coordinate is
/// `isf_FragNormCoord`, which becomes the varying the fragment stage reads.
pub(crate) const MAD_VERTEX_NAMES: &str = "vec4 phosphor_mm_vertex;
#define mm_Vertex phosphor_mm_vertex
#define mm_ModelViewProjectionMatrix mat4(1.0)
#define mm_TextureMatrix mat4(1.0)
#define mm_TexCoord0 vec4(isf_FragNormCoord, 0.0, 1.0)
#define mm_FragNormCoord isf_FragNormCoord
#define mm_SurfaceCoord isf_FragNormCoord
";

/// `main` for the vertex stage: inputs, the shader's own main (which calls `isf_vertShaderInit`), then the
/// varying the fragment stage expects (flipped: Phosphor's uv has y down).
pub(crate) fn vertex_epilogue() -> &'static str {
    "void main() {\n    phosphor_init_inputs();\n    phosphor_vs_user();\n    phosphor_uv = vec2(isf_FragNormCoord.x, 1.0 - isf_FragNormCoord.y);\n}\n"
}

/// `IMG_NORM_PIXEL(name, uv)` (also `texture2D(name, uv)` / `texture(name, uv)`) → `phosphor_curve_name(uv)`.
pub(crate) fn call_curve(body: &str, name: &str) -> String {
    let b = body.as_bytes();
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let skip_ws = |mut k: usize| {
        while k < b.len() && (b[k] as char).is_whitespace() {
            k += 1;
        }
        k
    };
    let mut out = String::with_capacity(body.len());
    let mut i = 0;
    'scan: while i < b.len() {
        for mac in ["IMG_NORM_PIXEL", "texture2D", "texture"] {
            if !body[i..].starts_with(mac) || (i > 0 && ident(b[i - 1])) {
                continue;
            }
            // mac ( ws name ws ,
            let mut k = skip_ws(i + mac.len());
            if b.get(k) != Some(&b'(') {
                continue;
            }
            k = skip_ws(k + 1);
            if !body[k..].starts_with(name) || b.get(k + name.len()).is_some_and(|c| ident(*c)) {
                continue;
            }
            k = skip_ws(k + name.len());
            if b.get(k) != Some(&b',') {
                continue;
            }
            out.push_str(&format!("phosphor_curve_{name}("));
            i = k + 1;
            continue 'scan;
        }
        let ch = body[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

#[cfg(test)]
mod curve_call_tests {
    #[test]
    fn curve_sampling_becomes_a_call() {
        let b = "col.r = IMG_NORM_PIXEL(mat_curve1,vec2(col.r,0.5)).r; x = IMG_NORM_PIXEL( mat_curve12 , uv); y = texture2D(mat_curve1, p);";
        let out = super::call_curve(b, "mat_curve1");
        assert_eq!(out, "col.r = phosphor_curve_mat_curve1(vec2(col.r,0.5)).r; x = IMG_NORM_PIXEL( mat_curve12 , uv); y = phosphor_curve_mat_curve1( p);");
    }
}
