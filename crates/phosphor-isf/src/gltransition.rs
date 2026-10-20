//! gl-transitions (https://gl-transitions.com, MIT/BSD per file) → ISF transitions, so the ISF importer handles them.
//!
//! A gl-transition is `vec4 transition(vec2 uv)` using `getFromColor(uv)`, `getToColor(uv)`, `progress` and `ratio`,
//! with its settings as `uniform` lines whose defaults (and `// @range(min, max)` hints) live in comments. This turns
//! one into an ISF file: `startImage` / `endImage` / `progress` plus one input per adjustable setting (float, bool,
//! vec2, vec4); int / vec3 / ivec2 settings become constants at their defaults. Ones that need a picture of their own
//! (`uniform sampler2D`) aren't supported.

/// The ISF source for a gl-transition, or why it can't be converted.
pub fn to_isf(source: &str) -> Result<String, String> {
    let mut author = String::new();
    let mut license = String::new();
    let mut description = String::new();
    let mut inputs: Vec<String> = Vec::new();
    let mut body = String::new();
    let mut range: Option<(f64, f64)> = None;
    for line in source.lines() {
        let t = line.trim();
        if let Some(v) = t.strip_prefix("// Author:") {
            author = v.trim().to_string();
            continue;
        }
        if let Some(v) = t.strip_prefix("// License:") {
            license = v.trim().to_string();
            continue;
        }
        if let Some(v) = t.strip_prefix("// Description:") {
            description = v.trim().to_string();
            continue;
        }
        if let Some(r) = t.strip_prefix("// @range(") {
            let nums: Vec<f64> = r.split(')').next().unwrap_or("").split(',').filter_map(|n| n.trim().parse().ok()).collect();
            range = (nums.len() >= 2).then(|| (nums[0], nums[1]));
            continue;
        }
        let Some(decl) = t.strip_prefix("uniform ") else {
            body.push_str(line);
            body.push('\n');
            continue;
        };
        let (ty, name, default) = parse_uniform(decl).ok_or_else(|| format!("can't read `{t}`"))?;
        let hint = range.take();
        match ty.as_str() {
            "sampler2D" => return Err(format!("needs a picture of its own ({name})")),
            "float" => {
                let d = default.first().copied().unwrap_or(0.0);
                let (lo, hi) = hint.unwrap_or_else(|| if d < 0.0 { (d * 2.0, -d * 2.0) } else { (0.0, (d * 2.0).max(1.0)) });
                inputs.push(format!(r#"{{"NAME": "{name}", "TYPE": "float", "DEFAULT": {}, "MIN": {}, "MAX": {}}}"#, num(d), num(lo.min(d)), num(hi.max(d))));
            }
            "bool" => inputs.push(format!(r#"{{"NAME": "{name}", "TYPE": "bool", "DEFAULT": {}}}"#, default.first().copied().unwrap_or(0.0) != 0.0)),
            "vec2" => {
                let v = splat(&default, 2);
                inputs.push(format!(r#"{{"NAME": "{name}", "TYPE": "point2D", "DEFAULT": [{}, {}], "MIN": [-2.0, -2.0], "MAX": [2.0, 2.0]}}"#, num(v[0]), num(v[1])));
            }
            "vec4" => {
                let v = splat(&default, 4);
                inputs.push(format!(r#"{{"NAME": "{name}", "TYPE": "color", "DEFAULT": [{}, {}, {}, {}]}}"#, num(v[0]), num(v[1]), num(v[2]), num(v[3])));
            }
            // Not adjustable in ISF: fixed at the default.
            "int" => body.push_str(&format!("const int {name} = {};\n", default.first().copied().unwrap_or(0.0) as i64)),
            "vec3" => {
                let v = splat(&default, 3);
                body.push_str(&format!("const vec3 {name} = vec3({}, {}, {});\n", num(v[0]), num(v[1]), num(v[2])));
            }
            "ivec2" => {
                let v = splat(&default, 2);
                body.push_str(&format!("const ivec2 {name} = ivec2({}, {});\n", v[0] as i64, v[1] as i64));
            }
            other => return Err(format!("setting type {other} ({name})")),
        }
    }
    if !body.contains("transition") {
        return Err("no `vec4 transition(vec2 uv)`".into());
    }
    let mut all_inputs = vec![
        r#"{"NAME": "startImage", "TYPE": "image"}"#.to_string(),
        r#"{"NAME": "endImage", "TYPE": "image"}"#.to_string(),
        r#"{"NAME": "progress", "TYPE": "float", "DEFAULT": 0.0, "MIN": 0.0, "MAX": 1.0}"#.to_string(),
    ];
    all_inputs.extend(inputs);
    let credit = match (author.is_empty(), license.is_empty()) {
        (false, false) => format!("{author} (gl-transitions, {license})"),
        (false, true) => format!("{author} (gl-transitions)"),
        _ => "gl-transitions".to_string(),
    };
    let header = format!(
        "/*{{\n\"DESCRIPTION\": {},\n\"CREDIT\": {},\n\"ISFVSN\": \"2\",\n\"CATEGORIES\": [\"Transition\"],\n\"INPUTS\": [\n{}\n]\n}}*/\n",
        json_str(&description),
        json_str(&credit),
        all_inputs.join(",\n")
    );
    Ok(format!(
        "{header}vec4 getFromColor(vec2 uv) {{ return IMG_NORM_PIXEL(startImage, uv); }}\nvec4 getToColor(vec2 uv) {{ return IMG_NORM_PIXEL(endImage, uv); }}\nfloat ratio;\n{body}\nvoid main() {{ ratio = RENDERSIZE.x / RENDERSIZE.y; gl_FragColor = transition(isf_FragNormCoord.xy); }}\n"
    ))
}

/// `float amount; // = 0.5` (also `//=0.5`, `/* = 0.5 */`) → ("float", "amount", [0.5]).
fn parse_uniform(decl: &str) -> Option<(String, String, Vec<f64>)> {
    let (head, rest) = decl.split_once(';').or_else(|| decl.split_once("/*").map(|(h, r)| (h, r)))?;
    let mut words = head.split_whitespace();
    let ty = words.next()?.to_string();
    let name = words.next()?.trim_end_matches(';').to_string();
    // The default is in a comment after the declaration (or inside a /* */ before the semicolon).
    let comment = if decl.contains("/*") { decl.split("/*").nth(1).unwrap_or("").split("*/").next().unwrap_or("") } else { rest.split("//").nth(1).unwrap_or("") };
    let value = comment.trim().trim_start_matches('=').trim();
    Some((ty, name, numbers(value)))
}

/// Numbers in a GLSL literal: "vec4(0.,0.,0.,.6)" → [0, 0, 0, 0.6]; "true" → [1]; "3" → [3].
fn numbers(v: &str) -> Vec<f64> {
    match v.trim_end_matches(';') {
        "true" => return vec![1.0],
        "false" => return vec![0.0],
        _ => {}
    }
    let inner = v.split_once('(').map_or(v, |(_, r)| r.split(')').next().unwrap_or(""));
    inner.split(',').filter_map(|n| n.trim().trim_end_matches(';').parse::<f64>().ok()).collect()
}

/// `vec2(0.5)` means (0.5, 0.5): fill missing components with the last one (0 when empty).
fn splat(v: &[f64], n: usize) -> Vec<f64> {
    (0..n).map(|i| v.get(i).or(v.last()).copied().unwrap_or(0.0)).collect()
}

fn num(v: f64) -> String {
    if v.fract() == 0.0 { format!("{v:.1}") } else { format!("{v}") }
}

fn json_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    const BOUNCE: &str = "// Author: Adrian Purser\n// License: MIT\n// Description: Bounces out\n\n// @color Shadow\nuniform vec4 shadow_colour; // = vec4(0.,0.,0.,.6)\n// @range(0.01, 0.5) Height\nuniform float shadow_height; // = 0.075\nuniform int steps /* = 3 */;\nuniform bool flip; // = true\n\nvec4 transition (vec2 uv) {\n  float d = uv.y - progress * float(steps);\n  return mix(getFromColor(uv), getToColor(uv) * shadow_colour, step(d, shadow_height) * ratio);\n}\n";

    #[test]
    fn a_gl_transition_becomes_an_isf_transition() {
        let isf = super::to_isf(BOUNCE).unwrap();
        assert!(isf.contains(r#""CREDIT": "Adrian Purser (gl-transitions, MIT)""#), "{isf}");
        assert!(isf.contains(r#""NAME": "shadow_height", "TYPE": "float", "DEFAULT": 0.075, "MIN": 0.01, "MAX": 0.5"#), "{isf}");
        assert!(isf.contains("const int steps = 3;"), "{isf}");
        assert!(isf.contains(r#""NAME": "flip", "TYPE": "bool", "DEFAULT": true"#), "{isf}");
        let sh = crate::import("bounce", &isf, None).expect("imports");
        assert!(sh.is_transition && sh.unsupported.is_none(), "{:?}", sh.unsupported);
        crate::validate_wgsl(&sh.wgsl).expect("valid WGSL");
    }

    #[test]
    fn transitions_with_their_own_picture_are_refused() {
        let src = "uniform sampler2D luma;\nvec4 transition(vec2 uv) { return getToColor(uv); }\n";
        assert!(super::to_isf(src).unwrap_err().contains("picture"));
    }
}

#[cfg(test)]
mod corpus {
    /// Every gl-transition in `PHOSPHOR_GLTRANSITIONS` (a folder of .glsl): adapter, ISF import, WGSL validation.
    ///   PHOSPHOR_GLTRANSITIONS=…/transitions cargo test -p phosphor-isf gl_corpus -- --ignored --nocapture
    #[test]
    #[ignore]
    fn gl_corpus() {
        let Ok(dir) = std::env::var("PHOSPHOR_GLTRANSITIONS") else { return };
        let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "glsl")).collect();
        files.sort();
        let (mut ok, mut bad) = (0, 0);
        for f in &files {
            let name = f.file_stem().unwrap().to_string_lossy().to_string();
            let src = std::fs::read_to_string(f).unwrap();
            let result = super::to_isf(&src).and_then(|isf| crate::import(&name, &isf, None).map_err(|e| format!("{e:?}"))).and_then(|sh| match sh.unsupported {
                Some(u) => Err(u),
                None => crate::validate_wgsl(&sh.wgsl).map(|()| sh.is_transition),
            });
            match result {
                Ok(true) => ok += 1,
                Ok(false) => {
                    bad += 1;
                    println!("GLTR {name}: not seen as a transition");
                }
                Err(e) => {
                    bad += 1;
                    println!("GLTR {name}: {}", e.lines().next().unwrap_or(""));
                }
            }
        }
        println!("GLTR {ok} ok, {bad} failed of {}", files.len());
    }
}
