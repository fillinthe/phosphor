//! Export Show: packs the current project, every media file, the shaders it uses and its
//! mappings into a standalone copy of Phosphor that runs on another computer on its own: a
//! "<Name>.app" on the Mac, a "<Name>" folder with "<Name>.exe" on Windows.

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
    pub lock: Option<String>,
    pub screens: Vec<(u64, String)>,
    pub layout: Option<String>,
}

/// What an export would pack: (files, bytes, missing). Reads file sizes, so call it once, not per frame.
pub fn summary(comp: &Composition) -> (usize, u64, usize) {
    let files = comp.media_paths();
    let parts = |f: &String| if crate::media::sequence::is_sequence(f) { crate::media::sequence::members(f) } else { vec![PathBuf::from(f)] };
    let missing = files.iter().filter(|f| parts(f).iter().all(|p| !p.is_file())).count();
    let bytes = files.iter().flat_map(parts).filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len()).sum();
    (files.len(), bytes, missing)
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

/// Windows (and other folder installs): copies the folder Phosphor.exe runs from into `dest/<name>/`, the exe
/// renamed to "<name>.exe" (update leftovers skipped). Returns (folder, its Show folder).
fn copy_folder_app(src_dir: &Path, exe: &Path, dest: &Path, name: &str) -> Result<(PathBuf, PathBuf), String> {
    let out = dest.join(name);
    if out.exists() {
        std::fs::remove_dir_all(&out).map_err(|e| format!("couldn't replace {}: {e}", out.display()))?;
    }
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let ext = exe.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    for entry in std::fs::read_dir(src_dir).map_err(|e| e.to_string())?.flatten() {
        let from = entry.path();
        let file = entry.file_name().to_string_lossy().into_owned();
        if !from.is_file() || file.ends_with(".old") {
            continue;
        }
        let to = if from == exe { out.join(format!("{name}{ext}")) } else { out.join(&file) };
        std::fs::copy(&from, &to).map_err(|e| format!("copying {file}: {e}"))?;
    }
    let show = out.join("Show");
    Ok((out, show))
}

fn run(req: Request, p: &Shared) -> Result<(), String> {
    let name = if req.name.trim().is_empty() { "Show".to_string() } else { req.name.trim().to_string() };
    step(p, "Copying Phosphor…");
    let (app, show) = if cfg!(target_os = "macos") {
        let src = own_bundle()?;
        let app = req.dest.join(format!("{name}.app"));
        if app.exists() {
            std::fs::remove_dir_all(&app).map_err(|e| format!("couldn't replace {}: {e}", app.display()))?;
        }
        sh(Command::new("ditto").arg(&src).arg(&app))?;
        let show = app.join("Contents/Resources/Show");
        (app, show)
    } else {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        copy_folder_app(exe.parent().ok_or("Can't tell where Phosphor is installed")?, &exe, &req.dest, &name)?
    };
    let media = show.join("media");
    std::fs::create_dir_all(&media).map_err(|e| e.to_string())?;

    // Media: copy each file once, under a unique name, and point the project at the copy.
    let mut comp = req.comp;
    let mut copied: HashMap<String, String> = HashMap::new();
    let mut names: HashSet<String> = HashSet::new();
    let mut failed: Vec<String> = Vec::new();
    let mut bytes = 0u64;
    let paths = comp.media_paths();
    let total = paths.len();
    for (i, path) in paths.iter().enumerate() {
        // An image sequence: all its frames into a folder of their own.
        if crate::media::sequence::is_sequence(path) {
            let frames = crate::media::sequence::members(path);
            if frames.is_empty() {
                failed.push(path.clone());
                continue;
            }
            let mut folder = crate::media::sequence::label(path);
            let mut n = 2;
            while !names.insert(folder.clone()) {
                folder = format!("{} {n}", crate::media::sequence::label(path));
                n += 1;
            }
            let dir = media.join(&folder);
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            step(p, &format!("Copying media {}/{total}: {folder} ({} frames)", i + 1, frames.len()));
            for f in &frames {
                bytes += std::fs::copy(f, dir.join(f.file_name().unwrap_or_default())).map_err(|e| format!("copying {}: {e}", f.display()))?;
            }
            let pattern = Path::new(path).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
            copied.insert(path.clone(), format!("media/{folder}/{pattern}"));
            continue;
        }
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
        layout: req.layout,
        lock: req.lock,
    };
    std::fs::write(show.join(MANIFEST), serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    if !failed.is_empty() {
        let list = failed.join("\n");
        let _ = std::fs::write(show.join("missing-media.txt"), format!("These files were missing when the show was exported:\n{list}\n"));
    }

    // Mac: its own name and identity, so it can sit next to Phosphor and keeps separate settings. (On Windows
    // the exe's own name does that; settings are kept per show by `show::storage_name`.)
    if cfg!(target_os = "macos") {
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
    }

    let mut zip = None;
    if req.zip {
        step(p, "Zipping…");
        let z = req.dest.join(format!("{name}.zip"));
        let _ = std::fs::remove_file(&z);
        if cfg!(target_os = "macos") {
            sh(Command::new("ditto").args(["-c", "-k", "--sequesterRsrc", "--keepParent"]).arg(&app).arg(&z))?;
        } else {
            // Windows' tar writes zips (-a picks the format from the name).
            sh(Command::new("tar").arg("-a").arg("-c").arg("-f").arg(&z).arg("-C").arg(&req.dest).arg(&name))?;
        }
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
                if phosphor_engine::custom::slug(&stem.to_string_lossy()) == slug {
                    return Some(path);
                }
            }
        }
    }
    None
}

/// Local date and time, e.g. "2026-10-14 15:02" (UTC where there's no `date` command).
fn chrono_now() -> String {
    let local = Command::new("date").arg("+%Y-%m-%d %H:%M").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|s| s.len() == 16);
    local.unwrap_or_else(|| {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
        let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
        // Days since 1970 to a civil date (Howard Hinnant's algorithm).
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + i64::from(m <= 2);
        format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", rem / 3600, rem % 3600 / 60)
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn folder_app_renames_the_exe_and_skips_update_leftovers() {
        let root = std::env::temp_dir().join(format!("phosphor-export-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (install, dest) = (root.join("install"), root.join("out"));
        std::fs::create_dir_all(&install).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        let exe = install.join("Phosphor.exe");
        for (f, body) in [("Phosphor.exe", "exe"), ("avcodec-62.dll", "dll"), ("Phosphor.exe.old", "old")] {
            std::fs::write(install.join(f), body).unwrap();
        }
        let (out, show) = super::copy_folder_app(&install, &exe, &dest, "Club Night").unwrap();
        assert_eq!(out, dest.join("Club Night"));
        assert_eq!(show, out.join("Show"));
        assert_eq!(std::fs::read_to_string(out.join("Club Night.exe")).unwrap(), "exe");
        assert!(out.join("avcodec-62.dll").is_file());
        assert!(!out.join("Phosphor.exe").exists() && !out.join("Phosphor.exe.old").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn date_looks_like_a_date() {
        let d = super::chrono_now();
        assert!(d.starts_with("20") && d.len() >= 16, "{d}");
    }
}
