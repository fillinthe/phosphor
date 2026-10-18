//! Post-processes naga's WGSL output into Phosphor's binding contract:
//! strips the uniform/texture/sampler declarations (Phosphor's prelude owns
//! them), renames the globals to `u` / `samp` / `tex0..2`, prefixes every other
//! top-level name with the entry name and replaces naga's `@fragment fn main`
//! wrapper with `fn <entry>(in: VOut) -> @location(0) vec4f`.

use crate::IsfError;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Struct,
    Const,
    Var,
    Fn,
    Other,
}

#[derive(Debug)]
struct Item<'a> {
    kind: Kind,
    name: String,
    /// `@group(g) @binding(b)` if present.
    binding: Option<u32>,
    entry: bool,
    text: &'a str,
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Splits naga's output into top-level items.
fn scan_items(src: &str) -> Vec<Item<'_>> {
    let b = src.as_bytes();
    let mut items = Vec::new();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && (b[i] as char).is_whitespace() {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        let start = i;
        let mut binding = None;
        let mut entry = false;
        // Attributes.
        while i < b.len() && b[i] == b'@' {
            let a = i + 1;
            let mut e = a;
            while e < b.len() && is_ident(b[e]) {
                e += 1;
            }
            let attr = &src[a..e];
            i = e;
            if i < b.len() && b[i] == b'(' {
                let mut depth = 0;
                let args_start = i + 1;
                while i < b.len() {
                    match b[i] {
                        b'(' => depth += 1,
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
                let args = &src[args_start..i];
                i += 1;
                if attr == "binding" {
                    binding = args.trim().parse().ok();
                }
            }
            if attr == "fragment" || attr == "vertex" || attr == "compute" {
                entry = true;
            }
            while i < b.len() && (b[i] as char).is_whitespace() {
                i += 1;
            }
        }
        // Keyword.
        let kw_start = i;
        while i < b.len() && is_ident(b[i]) {
            i += 1;
        }
        let kw = &src[kw_start..i];
        let kind = match kw {
            "struct" => Kind::Struct,
            "const" => Kind::Const,
            "var" => Kind::Var,
            "fn" => Kind::Fn,
            _ => Kind::Other,
        };
        // Optional `<...>` after var, then the name.
        while i < b.len() && (b[i] as char).is_whitespace() {
            i += 1;
        }
        if i < b.len() && b[i] == b'<' {
            while i < b.len() && b[i] != b'>' {
                i += 1;
            }
            i += 1;
            while i < b.len() && (b[i] as char).is_whitespace() {
                i += 1;
            }
        }
        let name_start = i;
        while i < b.len() && is_ident(b[i]) {
            i += 1;
        }
        let name = src[name_start..i].to_string();
        // Find the end: `;` at depth 0 for const/var, matching `}` for struct/fn.
        let mut depth = 0i32;
        let mut seen_brace = false;
        while i < b.len() {
            match b[i] {
                b'{' => {
                    depth += 1;
                    seen_brace = true;
                }
                b'}' => {
                    depth -= 1;
                    if depth == 0 && seen_brace {
                        i += 1;
                        break;
                    }
                }
                b';' if depth == 0 && !seen_brace => {
                    i += 1;
                    break;
                }
                _ => {}
            }
            i += 1;
        }
        items.push(Item { kind, name, binding, entry, text: &src[start..i] });
    }
    items
}

/// Extracts the parameter list `(…)` of a function item.
fn fn_params(text: &str) -> &str {
    let open = text.find('(').unwrap_or(0);
    let mut depth = 0;
    for (k, c) in text[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return &text[open + 1..open + k];
                }
            }
            _ => {}
        }
    }
    ""
}

/// Applies `map` to identifier tokens of `text`, skipping member accesses
/// (`.name`) and struct member declarations.
fn rename_tokens(text: &str, map: &HashMap<String, String>, is_struct: bool) -> String {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len() + 64);
    let mut i = 0;
    let mut brace = 0i32;
    let mut prev_sig = b' ';
    while i < b.len() {
        let c = b[i];
        if is_ident(c) {
            let s = i;
            while i < b.len() && is_ident(b[i]) {
                i += 1;
            }
            let tok = &text[s..i];
            let member_decl = is_struct && brace > 0 && matches!(prev_sig, b'{' | b',' | b')');
            if prev_sig != b'.' && !member_decl && !c.is_ascii_digit() {
                if let Some(r) = map.get(tok) {
                    out.push_str(r);
                    prev_sig = b'a';
                    continue;
                }
            }
            out.push_str(tok);
            prev_sig = b'a';
            continue;
        }
        match c {
            b'{' => brace += 1,
            b'}' => brace -= 1,
            _ => {}
        }
        if !(c as char).is_whitespace() {
            prev_sig = c;
        }
        out.push(c as char);
        i += 1;
    }
    out
}

/// Rewrites naga's module text. `entry` is the fragment entry name to emit.
pub(crate) fn finalize(naga_wgsl: &str, entry: &str) -> Result<String, IsfError> {
    let items = scan_items(naga_wgsl);
    let mut map: HashMap<String, String> = HashMap::new();
    let mut drop_structs: Vec<String> = Vec::new();
    let mut entry_item: Option<&Item> = None;

    for it in &items {
        if it.entry {
            entry_item = Some(it);
            continue;
        }
        match (it.kind, it.binding) {
            (Kind::Var, Some(bind)) => {
                let contract = match bind {
                    0 => "u".to_string(),
                    1 => "samp".to_string(),
                    2 => "samp_mirror".to_string(),
                    3 => "tex0".to_string(),
                    4 => "tex1".to_string(),
                    5 => "tex2".to_string(),
                    // Multi-pass buffers: `buf0`… in the shared prelude.
                    b if (crate::BUFFER_BINDING as u32..(crate::BUFFER_BINDING + crate::MAX_BUFFERS) as u32).contains(&b) => {
                        format!("buf{}", b - crate::BUFFER_BINDING as u32)
                    }
                    _ => return Err(IsfError::Wgsl(format!("unexpected binding {bind}"))),
                };
                map.insert(it.name.clone(), contract);
                if bind == 0 {
                    // `var<uniform> NAME: TYPE;`
                    if let Some(ty) = it.text.rsplit(':').next() {
                        drop_structs.push(ty.trim().trim_end_matches(';').trim().to_string());
                    }
                }
            }
            (Kind::Other, _) => {}
            _ => {
                if !it.name.is_empty() {
                    map.insert(it.name.clone(), format!("{entry}_{}", it.name));
                }
            }
        }
    }

    let entry_item = entry_item.ok_or_else(|| IsfError::Wgsl("naga output has no fragment entry point".into()))?;
    // Return type struct of the wrapper is dropped too.
    let sig = entry_item.text;
    if let Some(arrow) = sig.find("->") {
        let ret = sig[arrow + 2..].split('{').next().unwrap_or("").trim();
        drop_structs.push(ret.to_string());
    }

    // Parameter name -> Phosphor expression, by attribute.
    let mut param_src: HashMap<String, String> = HashMap::new();
    for p in fn_params(sig).split(',') {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        let Some((decl, _ty)) = p.rsplit_once(':') else { continue };
        let name = decl.split_whitespace().last().unwrap_or("").to_string();
        let expr = if decl.contains("@builtin(position)") {
            "in.pos"
        } else if decl.contains("@location(0)") {
            "in.uv"
        } else if decl.contains("@builtin(front_facing)") {
            "true"
        } else {
            return Err(IsfError::Wgsl(format!("unsupported entry parameter `{p}`")));
        };
        param_src.insert(name, expr.to_string());
    }

    // Walk the wrapper body: `global = param;`, `main_1();`, `let _e = out; return S(_e);`
    let body_start = sig.find('{').map(|k| k + 1).unwrap_or(0);
    let body = &sig[body_start..sig.len().saturating_sub(1)];
    let mut assigns: Vec<(String, String)> = Vec::new();
    let mut calls: Vec<String> = Vec::new();
    let mut lets: HashMap<String, String> = HashMap::new();
    let mut ret_expr: Option<String> = None;
    for stmt in body.split(';') {
        let s = stmt.trim();
        if s.is_empty() {
            continue;
        }
        if let Some(rest) = s.strip_prefix("let ") {
            if let Some((l, r)) = rest.split_once('=') {
                lets.insert(l.trim().to_string(), r.trim().to_string());
            }
        } else if let Some(rest) = s.strip_prefix("return") {
            let inner = rest.trim();
            let arg = inner.split_once('(').map(|(_, a)| a.trim_end_matches(')').trim()).unwrap_or(inner);
            let arg = arg.split(',').next().unwrap_or("").trim();
            let resolved = lets.get(arg).cloned().unwrap_or_else(|| arg.to_string());
            ret_expr = Some(resolved);
        } else if s.ends_with(')') && !s.contains('=') {
            calls.push(s.to_string());
        } else if let Some((l, r)) = s.split_once('=') {
            assigns.push((l.trim().to_string(), r.trim().to_string()));
        }
    }
    let ret_expr = ret_expr.ok_or_else(|| IsfError::Wgsl("fragment wrapper has no return".into()))?;

    let mut out = String::new();
    for it in &items {
        if it.entry || (it.kind == Kind::Var && it.binding.is_some()) {
            continue;
        }
        if it.kind == Kind::Struct && drop_structs.contains(&it.name) {
            continue;
        }
        out.push_str(&rename_tokens(it.text, &map, it.kind == Kind::Struct));
        out.push_str("\n\n");
    }
    out.push_str(&format!("@fragment\nfn {entry}(in: VOut) -> @location(0) vec4f {{\n"));
    for (l, r) in &assigns {
        let src = param_src
            .get(r)
            .cloned()
            .ok_or_else(|| IsfError::Wgsl(format!("unexpected wrapper assignment `{l} = {r}`")))?;
        out.push_str(&format!("    {} = {src};\n", map.get(l).unwrap_or(l)));
    }
    for c in &calls {
        out.push_str(&format!("    {};\n", rename_tokens(c, &map, false)));
    }
    out.push_str(&format!("    return {};\n}}\n", rename_tokens(&ret_expr, &map, false)));
    Ok(out)
}

/// Rewrites naga's vertex-stage output into `@vertex fn <vs_entry>(@builtin(vertex_index) vi: u32) -> VOut`,
/// with the same binding renames as `finalize` and every other name prefixed by `vs_entry`.
pub(crate) fn finalize_vertex(naga_wgsl: &str, vs_entry: &str) -> Result<String, IsfError> {
    let items = scan_items(naga_wgsl);
    let mut map: HashMap<String, String> = HashMap::new();
    let mut drop_structs: Vec<String> = Vec::new();
    let mut entry_item: Option<&Item> = None;
    for it in &items {
        if it.entry {
            entry_item = Some(it);
            continue;
        }
        match (it.kind, it.binding) {
            (Kind::Var, Some(0)) => {
                map.insert(it.name.clone(), "u".to_string());
                if let Some(ty) = it.text.rsplit(':').next() {
                    drop_structs.push(ty.trim().trim_end_matches(';').trim().to_string());
                }
            }
            (Kind::Var, Some(bind)) => return Err(IsfError::Wgsl(format!("vertex shader binds {bind}; only the uniforms are allowed there"))),
            (Kind::Other, _) => {}
            _ => {
                if !it.name.is_empty() {
                    map.insert(it.name.clone(), format!("{vs_entry}_{}", it.name));
                }
            }
        }
    }
    let entry_item = entry_item.ok_or_else(|| IsfError::Wgsl("naga output has no vertex entry point".into()))?;
    let sig = entry_item.text;
    // The output struct: which constructor argument is the position and which the uv.
    let ret_name = sig.find("->").map(|a| sig[a + 2..].split('{').next().unwrap_or("").trim().to_string()).unwrap_or_default();
    drop_structs.push(ret_name.clone());
    let ret_struct = items.iter().find(|it| it.kind == Kind::Struct && it.name == ret_name).ok_or_else(|| IsfError::Wgsl("vertex output struct not found".into()))?;
    let fields: Vec<&str> = ret_struct.text[ret_struct.text.find('{').map_or(0, |k| k + 1)..].trim_end_matches('}').split(',').map(str::trim).filter(|f| !f.is_empty()).collect();
    let pos_index = fields.iter().position(|f| f.contains("@builtin(position)")).ok_or_else(|| IsfError::Wgsl("vertex output has no position".into()))?;
    let uv_index = fields.iter().position(|f| f.contains("@location(0)")).ok_or_else(|| IsfError::Wgsl("vertex output has no location 0 (phosphor_uv)".into()))?;
    // Parameters: the vertex index.
    let mut param_src: HashMap<String, String> = HashMap::new();
    for p in fn_params(sig).split(',') {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        let Some((decl, _ty)) = p.rsplit_once(':') else { continue };
        let name = decl.split_whitespace().last().unwrap_or("").to_string();
        if decl.contains("@builtin(vertex_index)") {
            param_src.insert(name, "vi".to_string());
        } else {
            return Err(IsfError::Wgsl(format!("unsupported vertex entry parameter `{p}`")));
        }
    }
    let body_start = sig.find('{').map(|k| k + 1).unwrap_or(0);
    let body = &sig[body_start..sig.len().saturating_sub(1)];
    let mut assigns: Vec<(String, String)> = Vec::new();
    let mut calls: Vec<String> = Vec::new();
    let mut lets: HashMap<String, String> = HashMap::new();
    let mut ret_args: Vec<String> = Vec::new();
    for stmt in body.split(';') {
        let s = stmt.trim();
        if s.is_empty() {
            continue;
        }
        if let Some(rest) = s.strip_prefix("let ") {
            if let Some((l, r)) = rest.split_once('=') {
                lets.insert(l.trim().to_string(), r.trim().to_string());
            }
        } else if let Some(rest) = s.strip_prefix("return") {
            let inner = rest.trim();
            let args = inner.split_once('(').map(|(_, a)| a.trim_end_matches(')').to_string()).unwrap_or_default();
            ret_args = args.split(',').map(|a| a.trim()).filter(|a| !a.is_empty()).map(|a| lets.get(a).cloned().unwrap_or_else(|| a.to_string())).collect();
        } else if s.ends_with(')') && !s.contains('=') {
            calls.push(s.to_string());
        } else if let Some((l, r)) = s.split_once('=') {
            assigns.push((l.trim().to_string(), r.trim().to_string()));
        }
    }
    let (pos_expr, uv_expr) = match (ret_args.get(pos_index), ret_args.get(uv_index)) {
        (Some(p), Some(u)) => (p.clone(), u.clone()),
        _ => return Err(IsfError::Wgsl("vertex wrapper return doesn't match its struct".into())),
    };
    let mut out = String::new();
    for it in &items {
        if it.entry || (it.kind == Kind::Var && it.binding.is_some()) {
            continue;
        }
        if it.kind == Kind::Struct && drop_structs.contains(&it.name) {
            continue;
        }
        out.push_str(&rename_tokens(it.text, &map, it.kind == Kind::Struct));
        out.push_str("\n\n");
    }
    out.push_str(&format!("@vertex\nfn {vs_entry}(@builtin(vertex_index) vi: u32) -> VOut {{\n"));
    for (l, r) in &assigns {
        let src = param_src.get(r).cloned().ok_or_else(|| IsfError::Wgsl(format!("unexpected vertex wrapper assignment `{l} = {r}`")))?;
        out.push_str(&format!("    {} = {src};\n", map.get(l).unwrap_or(l)));
    }
    for c in &calls {
        out.push_str(&format!("    {};\n", rename_tokens(c, &map, false)));
    }
    out.push_str(&format!("    return VOut({}, {});\n}}\n", rename_tokens(&pos_expr, &map, false), rename_tokens(&uv_expr, &map, false)));
    Ok(out)
}
