//! User shaders written in WGSL inside Phosphor. A source looks like:
//!
//! ```wgsl
//! // @name Hue Pulse
//! // @kind effect            (or: generator)
//! // @param Speed 0 4 1      (name, min, max, default — up to 16, read with P(0u), P(1u), …)
//! @fragment
//! fn main(in: VOut) -> @location(0) vec4f {
//!     let c = src(in.uv);    // effects read their input with src(uv)
//!     return vec4f(c.rgb, c.a);
//! }
//! ```
//!
//! Every helper from the built-in shaders (src, luma, palette, hash21, rgb2hsv, …) is available.

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
    pub params: Vec<Param>,
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// File/shader name → id-safe slug ("My Shader.fs" stem → "my_shader"). Used for ISF imports,
/// Shader Editor files and show exports, so the same file always gets the same id.
pub fn slug(name: &str) -> String {
    name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

pub fn parse_header(source: &str) -> Result<Header, CompileError> {
    let mut header = Header { name: "Untitled".into(), generator: false, params: Vec::new() };
    for (i, line) in source.lines().enumerate() {
        let Some(rest) = line.trim().strip_prefix("// @") else { continue };
        let err = |m: &str| CompileError { line: Some(i as u32 + 1), message: m.to_string() };
        let (key, value) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let value = value.trim();
        match key {
            "name" => header.name = value.to_string(),
            "kind" => header.generator = value.eq_ignore_ascii_case("generator"),
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
                    return Err(err("at most 16 parameters"));
                }
                let name = parts[..parts.len() - 3].join(" ");
                let (lo, hi) = (nums[0].min(nums[1]), nums[0].max(nums[1]));
                header.params.push(Param { name: leak(name), min: lo, max: hi, default: nums[2].clamp(lo, hi) });
            }
            _ => {}
        }
    }
    Ok(header)
}

/// Builds the module source for a user shader, renaming `fn main(` to `entry`.
/// Returns the full module text and the number of prelude lines before the user's code.
pub(crate) fn module_source(source: &str, entry: &str) -> Result<(String, u32), CompileError> {
    let count = source.matches("fn main(").count();
    if count != 1 {
        return Err(CompileError { line: None, message: "define exactly one `@fragment fn main(in: VOut) -> @location(0) vec4f`".into() });
    }
    let user = source.replace("fn main(", &format!("fn {entry}("));
    let prelude = shaders::common_source();
    let prelude_lines = prelude.lines().count() as u32 + 1;
    Ok((format!("{prelude}\n{user}"), prelude_lines))
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

/// The source of a built-in single-pass effect or generator, as an editable user shader.
pub fn builtin_as_custom(id: &str) -> Option<String> {
    let (desc, generator) = match shaders::effect(id) {
        Some(d) => (d, false),
        None => (shaders::generator(id)?, true),
    };
    if !shaders::multipass(desc.entry).is_empty() || desc.entry.starts_with("fs_") {
        return None;
    }
    let all = shaders::wgsl_source();
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
    fn template_and_builtin_copies_compile() {
        for generator in [false, true] {
            let src = template(generator);
            let header = parse_header(&src).unwrap();
            assert_eq!(header.generator, generator);
            let (full, pre) = module_source(&src, "custom_test").unwrap();
            validate(&full, pre).unwrap();
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
