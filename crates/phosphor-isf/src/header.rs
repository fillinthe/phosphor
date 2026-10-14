//! ISF JSON header: locate the leading `/*{ ... }*/` comment, repair the
//! JSON dialect found in the wild (trailing commas, `1.` literals, comments)
//! and extract inputs / passes / generators.

use crate::{IsfError, IsfInput, IsfInputKind};
use serde_json::Value;

/// Everything we read from the header.
#[derive(Debug, Default)]
pub(crate) struct Header {
    pub description: String,
    pub credit: String,
    pub categories: Vec<String>,
    pub inputs: Vec<IsfInput>,
    /// Number of entries in `PASSES` (0 when absent).
    pub passes: usize,
    /// Any pass declares `PERSISTENT`.
    pub persistent: bool,
    pub imported: bool,
    /// MadMapper `GENERATORS` of type `time_base`: (name, speed expression).
    pub time_bases: Vec<(String, String)>,
    /// Input types we cannot represent (first one wins).
    pub unsupported: Option<String>,
}

/// Splits `source` into (json text, GLSL body after the header comment).
pub(crate) fn split_header(source: &str) -> Result<(&str, &str), IsfError> {
    let start = source
        .find("/*")
        .ok_or_else(|| IsfError::Header("no /*{ ... }*/ header comment".into()))?;
    let after = &source[start + 2..];
    if !after.trim_start().starts_with('{') {
        return Err(IsfError::Header("first comment is not a JSON header".into()));
    }
    let end = after
        .find("*/")
        .ok_or_else(|| IsfError::Header("unterminated header comment".into()))?;
    Ok((&after[..end], &after[end + 2..]))
}

/// Repairs the relaxed JSON used by ISF/MadMapper headers so serde_json accepts it.
pub(crate) fn clean_json(raw: &str) -> String {
    let b = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match c {
            b'"' => {
                // Copy string literal verbatim (respecting escapes).
                let s = i;
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i = (i + 1).min(b.len());
                out.push_str(&raw[s..i]);
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b',' => {
                // Drop trailing commas before a closing bracket.
                let mut j = i + 1;
                loop {
                    while j < b.len() && (b[j] as char).is_whitespace() {
                        j += 1;
                    }
                    if b[j..].starts_with(b"//") {
                        while j < b.len() && b[j] != b'\n' {
                            j += 1;
                        }
                        continue;
                    }
                    break;
                }
                if j < b.len() && (b[j] == b']' || b[j] == b'}') {
                    i += 1;
                    continue;
                }
                out.push(',');
                i += 1;
            }
            b'.' => {
                // `1.` -> `1.0`, `.5` -> `0.5`
                let prev_digit = i > 0 && b[i - 1].is_ascii_digit();
                let next_digit = b.get(i + 1).is_some_and(|d| d.is_ascii_digit());
                if !prev_digit && next_digit {
                    out.push('0');
                }
                out.push('.');
                if prev_digit && !next_digit {
                    out.push('0');
                }
                i += 1;
            }
            _ => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    out
}

fn str_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

fn f32_of(v: Option<&Value>) -> Option<f32> {
    match v? {
        Value::Number(n) => n.as_f64().map(|f| f as f32),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn f32_array<const N: usize>(v: Option<&Value>, fallback: [f32; N]) -> [f32; N] {
    let mut out = fallback;
    if let Some(Value::Array(items)) = v {
        for (i, item) in items.iter().take(N).enumerate() {
            if let Some(f) = f32_of(Some(item)) {
                out[i] = f;
            }
        }
    }
    out
}

fn bool_of(v: Option<&Value>) -> Option<bool> {
    match v? {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => n.as_f64().map(|f| f != 0.0),
        Value::String(s) => match s.trim() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn parse_input(v: &Value) -> Result<IsfInput, String> {
    let name = v.get("NAME").map(str_of).unwrap_or_default();
    if name.is_empty() {
        return Err("input without NAME".into());
    }
    let label = v.get("LABEL").map(str_of).filter(|s| !s.is_empty()).unwrap_or_else(|| name.clone());
    let ty = v.get("TYPE").map(str_of).unwrap_or_default();
    let kind = match ty.as_str() {
        "float" => {
            let min = f32_of(v.get("MIN")).unwrap_or(0.0);
            let max = f32_of(v.get("MAX")).unwrap_or(1.0);
            let default = f32_of(v.get("DEFAULT")).unwrap_or(min).clamp(min.min(max), max.max(min));
            IsfInputKind::Float { min, max, default }
        }
        "bool" => IsfInputKind::Bool { default: bool_of(v.get("DEFAULT")).unwrap_or(false) },
        "event" => IsfInputKind::Event,
        "long" => {
            let values: Vec<i32> = match v.get("VALUES") {
                Some(Value::Array(a)) => a.iter().filter_map(|x| f32_of(Some(x))).map(|f| f as i32).collect(),
                _ => Vec::new(),
            };
            let labels: Vec<String> = match v.get("LABELS") {
                Some(Value::Array(a)) => a.iter().map(str_of).collect(),
                _ => Vec::new(),
            };
            let default = f32_of(v.get("DEFAULT")).map(|f| f as i32).or(values.first().copied()).unwrap_or(0);
            IsfInputKind::Long { values, labels, default }
        }
        "int" => {
            let min = f32_of(v.get("MIN")).unwrap_or(0.0) as i32;
            let max = f32_of(v.get("MAX")).unwrap_or(10.0) as i32;
            let default = f32_of(v.get("DEFAULT")).map(|f| f as i32).unwrap_or(min).clamp(min.min(max), max.max(min));
            IsfInputKind::Int { min, max, default }
        }
        "color" => IsfInputKind::Color { default: f32_array(v.get("DEFAULT"), [1.0, 1.0, 1.0, 1.0]) },
        "point2D" => {
            let min = f32_array(v.get("MIN"), [0.0, 0.0]);
            let max = f32_array(v.get("MAX"), [1.0, 1.0]);
            let default = f32_array(v.get("DEFAULT"), [0.5 * (min[0] + max[0]), 0.5 * (min[1] + max[1])]);
            IsfInputKind::Point2D { default, min, max }
        }
        "image" => IsfInputKind::Image,
        "audio" | "audioFFT" => IsfInputKind::Audio,
        other => return Err(format!("input type '{other}'")),
    };
    Ok(IsfInput { name, label, kind, offset: None, texture: None })
}

/// Extracts a speed expression for a MadMapper `time_base` generator: an input
/// name or a numeric literal.
fn time_base_speed(params: Option<&Value>) -> String {
    match params.and_then(|p| p.get("speed")) {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        Some(Value::Number(n)) => format!("{:?}", n.as_f64().unwrap_or(1.0) as f32),
        _ => "1.0".into(),
    }
}

pub(crate) fn parse_header(json: &str) -> Result<Header, IsfError> {
    let cleaned = clean_json(json);
    let root: Value = serde_json::from_str(&cleaned).map_err(|e| IsfError::Header(format!("invalid JSON: {e}")))?;
    let obj = root.as_object().ok_or_else(|| IsfError::Header("header is not an object".into()))?;
    let mut h = Header {
        description: obj.get("DESCRIPTION").map(str_of).unwrap_or_default(),
        credit: obj.get("CREDIT").map(str_of).unwrap_or_default(),
        ..Default::default()
    };
    match obj.get("CATEGORIES") {
        Some(Value::Array(a)) => h.categories = a.iter().map(str_of).filter(|s| !s.is_empty()).collect(),
        Some(Value::String(s)) => h.categories = vec![s.clone()],
        _ => {}
    }
    if let Some(Value::Array(inputs)) = obj.get("INPUTS") {
        for v in inputs {
            match parse_input(v) {
                Ok(input) => h.inputs.push(input),
                Err(reason) => {
                    h.unsupported.get_or_insert(reason);
                }
            }
        }
    }
    if let Some(Value::Array(passes)) = obj.get("PASSES") {
        h.passes = passes.len();
        h.persistent = passes.iter().any(|p| {
            p.get("PERSISTENT").is_some_and(|x| bool_of(Some(x)).unwrap_or(true))
        });
    }
    h.imported = obj.get("IMPORTED").is_some_and(|v| match v {
        Value::Object(m) => !m.is_empty(),
        Value::Array(a) => !a.is_empty(),
        _ => false,
    });
    if let Some(Value::Array(gens)) = obj.get("GENERATORS") {
        for g in gens {
            let name = g.get("NAME").map(str_of).unwrap_or_default();
            let ty = g.get("TYPE").map(str_of).unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            if ty == "time_base" {
                h.time_bases.push((name, time_base_speed(g.get("PARAMS"))));
            } else {
                h.unsupported.get_or_insert(format!("generator type '{ty}'"));
            }
        }
    }
    Ok(h)
}
