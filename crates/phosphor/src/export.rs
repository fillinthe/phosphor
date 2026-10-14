//! Export Show: packs the current project, every media file, the shaders it uses and its
//! mappings into a standalone copy of Phosphor.app that runs on another Mac on its own.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use phosphor_engine::{Composition, shaders};

use crate::show::{MANIFEST, Manifest, PROJECT, slug};

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub step: String,
    pub done: bool,
    pub error: Option<String>,
    /// The finished app (and zip, if made).
    pub output: Option<PathBuf>,
    pub zip: Option<PathBuf>,
    pub media_files: usize,
    pub bytes: u64,
}

pub type Shared = Arc<Mutex<Progress>>;

pub struct Request {
    pub comp: Composition,
    pub name: String,
    pub dest: PathBuf,
    pub zip: bool,
    pub autostart: bool,
    pub screens: Vec<(u64, String)>,
}

/// Starts the export on a background thread; the UI polls the returned progress.
pub fn start(req: Request) -> Shared {
    let p: Shared = Default::default();
    let progress = p.clone();
    std::thread::Builder::new()
        .name("phosphor-export".into())
        .spawn(move || {
            let result = run(req, &progress);
            let mut g = progress.lock().unwrap();
            g.done = true;
            match result {
                Ok(()) => g.step = "Done".into(),
                Err(e) => g.error = Some(e),
            }
        })
        .expect("export thread");
    p
}

fn step(p: &Shared, s: &str) {
    p.lock().unwrap().step = s.to_string();
}

/// Phosphor.app this binary lives in (or the installed / freshly built one when run from target/).
fn own_bundle() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let app = exe.parent().and_then(Path::parent).and_then(Path::parent).map(Path::to_path_buf);
    let candidates = [app, Some(PathBuf::from("dist/Phosphor.app")), Some(PathBuf::from("/Applications/Phosphor.app"))];
    candidates
        .into_iter()
        .flatten()
        .find(|p| p.extension().is_some_and(|e| e == "app") && p.join("Contents/Info.plist").is_file())
        .ok_or_else(|| "Couldn't find Phosphor.app to copy. Install it with scripts/bundle-mac.sh first.".into())
}

fn sh(cmd: &mut Command) -> Result<(), String> {
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

fn run(req: Request, p: &Shared) -> Result<(), String> {
    let src = own_bundle()?;
    let name = if req.name.trim().is_empty() { "Show".to_string() } else { req.name.trim().to_string() };
    let app = req.dest.join(format!("{name}.app"));
    if app.exists() {
        std::fs::remove_dir_all(&app).map_err(|e| format!("couldn't replace {}: {e}", app.display()))?;
    }
    step(p, "Copying Phosphor…");
    sh(Command::new("ditto").arg(&src).arg(&app))?;
    let show = app.join("Contents/Resources/Show");
    let media = show.join("media");
    std::fs::create_dir_all(&media).map_err(|e| e.to_string())?;

    // Media: copy each file once, under a unique name, and point the project at the copy.
    let mut comp = req.comp;
    let mut copied: HashMap<String, String> = HashMap::new();
    let mut names: HashSet<String> = HashSet::new();
    let mut failed: Vec<String> = Vec::new();
    let mut bytes = 0u64;
    let mut paths: Vec<String> = Vec::new();
    comp.map_paths(|path| {
        paths.push(path.to_string());
        path.to_string()
    });
    paths.sort();
    paths.dedup();
    let total = paths.len();
    for (i, path) in paths.iter().enumerate() {
        let src = Path::new(path);
        if !src.is_file() {
            failed.push(path.clone());
            continue;
        }
        let file = src.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| "media".into());
        let mut unique = file.clone();
        let mut n = 2;
        while !names.insert(unique.clone()) {
            let stem = Path::new(&file).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let ext = Path::new(&file).extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
            unique = format!("{stem} {n}{ext}");
            n += 1;
        }
        step(p, &format!("Copying media {}/{total}: {file}", i + 1));
        bytes += std::fs::copy(src, media.join(&unique)).map_err(|e| format!("copying {path}: {e}"))?;
        copied.insert(path.clone(), format!("media/{unique}"));
    }
    comp.map_paths(|path| copied.get(path).cloned().unwrap_or_else(|| path.to_string()));

    // Shaders the show uses: imported ISF files and your own Shader Editor effects.
    step(p, "Packing shaders…");
    let used = comp.used_shader_ids();
    let isf_dir = show.join("isf");
    let fx_dir = show.join("effects");
    for id in &used {
        if let Some(slug) = id.strip_prefix(shaders::ISF_EFFECT).or_else(|| id.strip_prefix(shaders::ISF_GENERATOR)) {
            if let Some(fs) = find_isf(slug) {
                std::fs::create_dir_all(&isf_dir).map_err(|e| e.to_string())?;
                let _ = std::fs::copy(&fs, isf_dir.join(format!("{slug}.fs")));
                let vs = fs.with_extension("vs");
                if vs.is_file() {
                    let _ = std::fs::copy(&vs, isf_dir.join(format!("{slug}.vs")));
                }
            }
        } else if let Some(slug) = id.strip_prefix(shaders::CUSTOM_EFFECT).or_else(|| id.strip_prefix(shaders::CUSTOM_GENERATOR)) {
            let file = crate::show::storage_dir().map(|d| d.join("effects").join(format!("{slug}.wgsl")));
            if let Some(file) = file.filter(|f| f.is_file()) {
                std::fs::create_dir_all(&fx_dir).map_err(|e| e.to_string())?;
                let _ = std::fs::copy(&file, fx_dir.join(format!("{slug}.wgsl")));
            }
        }
    }

    step(p, "Writing project…");
    std::fs::write(show.join(PROJECT), comp.to_json()).map_err(|e| e.to_string())?;
    let manifest = Manifest {
        name: name.clone(),
        exported: chrono_now(),
        phosphor_version: env!("CARGO_PKG_VERSION").into(),
        screens: req.screens,
        autostart: req.autostart,
    };
    std::fs::write(show.join(MANIFEST), serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    if !failed.is_empty() {
        let list = failed.join("\n");
        let _ = std::fs::write(show.join("missing-media.txt"), format!("These files were missing when the show was exported:\n{list}\n"));
    }

    // Its own name and identity, so it can sit next to Phosphor and keeps separate settings.
    step(p, "Signing…");
    let plist = app.join("Contents/Info.plist");
    for (key, value) in [
        ("CFBundleName", name.clone()),
        ("CFBundleDisplayName", name.clone()),
        ("CFBundleIdentifier", format!("co.fillintheblank.phosphor.show.{}", slug(&name))),
    ] {
        sh(Command::new("plutil").args(["-replace", key, "-string", &value]).arg(&plist))?;
    }
    let _ = Command::new("xattr").args(["-cr"]).arg(&app).output();
    sh(Command::new("codesign").args(["--force", "--deep", "--sign", "-"]).arg(&app))?;

    let mut zip = None;
    if req.zip {
        step(p, "Zipping…");
        let z = req.dest.join(format!("{name}.zip"));
        let _ = std::fs::remove_file(&z);
        sh(Command::new("ditto").args(["-c", "-k", "--sequesterRsrc", "--keepParent"]).arg(&app).arg(&z))?;
        zip = Some(z);
    }
    let mut g = p.lock().unwrap();
    g.output = Some(app);
    g.zip = zip;
    g.media_files = copied.len();
    g.bytes = bytes;
    if !failed.is_empty() {
        g.error = Some(format!("{} media file(s) were missing and aren't in the show (listed in missing-media.txt)", failed.len()));
    }
    Ok(())
}

fn find_isf(slug: &str) -> Option<PathBuf> {
    for dir in crate::isf_lib::folders() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().is_some_and(|x| x == "fs")
                && let Some(stem) = path.file_stem()
            {
                let s: String = stem.to_string_lossy().to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
                if s == slug {
                    return Some(path);
                }
            }
        }
    }
    None
}

/// Local date and time, e.g. "2026-10-14 15:02".
fn chrono_now() -> String {
    Command::new("date").arg("+%Y-%m-%d %H:%M").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}
