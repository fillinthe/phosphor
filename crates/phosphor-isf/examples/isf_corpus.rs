//! Imports every ISF / MadMapper shader found on this machine, validates the
//! produced WGSL with naga and prints a summary table.
//!
//! Usage: `cargo run -p phosphor-isf --example isf_corpus [--dump DIR] [extra dirs...]`

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const DEFAULT_DIRS: &[&str] = &[
    "/Library/Graphics/ISF",
    "~/Library/Graphics/ISF",
    "/Applications/MadMapper 6.1.7.app/Contents/Resources/Materials",
    "/Applications/MadMapper 6.1.7.app/Contents/Resources/FactorySurface2DFX",
    "/Applications/MadMapper 6.1.7.app/Contents/Resources/FactoryGenerators",
];

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|e| e == "fs") {
            out.push(p);
        }
    }
}

/// Collapses an error message into a class: first line, quoted names and numbers removed.
fn error_class(msg: &str) -> String {
    let first = msg.lines().find(|l| l.contains("error") || !l.trim().is_empty()).unwrap_or("");
    let first = first.trim_start_matches("error:").trim();
    let mut out = String::new();
    let mut in_quote = false;
    for c in first.chars() {
        match c {
            '`' | '\'' | '"' => {
                in_quote = !in_quote;
                if !in_quote {
                    out.push_str("<x>");
                }
            }
            _ if in_quote => {}
            c if c.is_ascii_digit() => {}
            c => out.push(c),
        }
    }
    let out = out.trim().to_string();
    if out.len() > 90 { out[..90].to_string() } else { out }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut dump: Option<PathBuf> = None;
    let mut dirs: Vec<String> = Vec::new();
    while let Some(a) = args.next() {
        if a == "--dump" {
            dump = args.next().map(PathBuf::from);
        } else {
            dirs.push(a);
        }
    }
    if dirs.is_empty() {
        dirs = DEFAULT_DIRS.iter().map(|s| s.to_string()).collect();
    }
    let home = std::env::var("HOME").unwrap_or_default();

    let mut total = 0usize;
    let mut ok = 0usize;
    let mut per_dir: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut unsupported: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut failed: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut module = phosphor_isf::test_prelude();

    for dir in &dirs {
        let dir = PathBuf::from(dir.replace('~', &home));
        let mut files = Vec::new();
        collect(&dir, &mut files);
        let label = dir.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        for path in files {
            total += 1;
            let entry = per_dir.entry(label.clone()).or_default();
            entry.0 += 1;
            let fname = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let Ok(fs) = std::fs::read_to_string(&path) else {
                failed.entry("unreadable file".into()).or_default().push(fname);
                continue;
            };
            let vs = std::fs::read_to_string(path.with_extension("vs")).ok();
            let slug = format!("{}_{}_{}", total, label, path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
            match phosphor_isf::import(&slug, &fs, vs.as_deref()) {
                Err(e) => {
                    let (class, msg) = match &e {
                        phosphor_isf::IsfError::Header(m) => ("header: ".to_string(), m.as_str()),
                        phosphor_isf::IsfError::Glsl(m) => ("glsl: ".to_string(), m.as_str()),
                        phosphor_isf::IsfError::Wgsl(m) => ("wgsl: ".to_string(), m.as_str()),
                    };
                    failed.entry(format!("{class}{}", error_class(msg))).or_default().push(fname);
                }
                Ok(shader) => {
                    if let Some(reason) = shader.unsupported {
                        unsupported.entry(reason).or_default().push(fname);
                        continue;
                    }
                    match phosphor_isf::validate_wgsl(&shader.wgsl) {
                        Ok(()) => {
                            ok += 1;
                            entry.1 += 1;
                            module.push_str(&shader.wgsl);
                            if let Some(d) = &dump {
                                let _ = std::fs::create_dir_all(d);
                                let _ = std::fs::write(d.join(format!("{slug}.wgsl")), &shader.wgsl);
                            }
                        }
                        Err(e) => {
                            failed.entry(format!("validate: {}", error_class(&e))).or_default().push(fname);
                            if let Some(d) = &dump {
                                let _ = std::fs::create_dir_all(d);
                                let _ = std::fs::write(d.join(format!("{slug}.FAILED.wgsl")), format!("{e}\n\n{}", shader.wgsl));
                            }
                        }
                    }
                }
            }
        }
    }

    // Every converted shader concatenated into a single module must still validate (name collisions).
    let concat = match naga::front::wgsl::parse_str(&module) {
        Ok(m) => naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
            .validate(&m)
            .map(|_| ())
            .map_err(|e| e.emit_to_string(&module)),
        Err(e) => Err(e.emit_to_string(&module)),
    };

    let n_unsup: usize = unsupported.values().map(Vec::len).sum();
    let n_fail: usize = failed.values().map(Vec::len).sum();
    println!("ISF corpus: {total} shaders");
    println!("  converted + validated: {ok} ({:.1}%)", 100.0 * ok as f64 / total.max(1) as f64);
    println!("  unsupported:           {n_unsup}");
    println!("  failed:                {n_fail}");
    println!("  concatenated module:   {}", match &concat { Ok(()) => "validates".to_string(), Err(e) => format!("FAILS\n{e}") });
    println!("\nPer folder (ok/total):");
    for (d, (t, o)) in &per_dir {
        println!("  {d:<22} {o:>4}/{t:<4} ({:.1}%)", 100.0 * *o as f64 / (*t).max(1) as f64);
    }
    let mut unsup: Vec<_> = unsupported.iter().collect();
    unsup.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    println!("\nUnsupported by reason:");
    for (r, files) in unsup {
        println!("  {:>4}  {r}  e.g. {}", files.len(), files.iter().take(3).cloned().collect::<Vec<_>>().join(" | "));
    }
    let mut fails: Vec<_> = failed.iter().collect();
    fails.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    println!("\nFailed by error class:");
    for (r, files) in fails {
        println!("  {:>4}  {r}  e.g. {}", files.len(), files.iter().take(3).cloned().collect::<Vec<_>>().join(" | "));
    }
}
