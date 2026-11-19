//! Export Show: packs the current project, every media file, the shaders it uses and its
//! mappings into a standalone copy of Phosphor that runs on another computer on its own: a
//! "<Name>.app" on the Mac, a "<Name>" folder with "<Name>.exe" on Windows.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use phosphor_engine::{Composition, shaders};

use crate::show::{MANIFEST, Manifest, PROJECT, slug};

pub use crate::job::Shared;

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
    crate::job::spawn("phosphor-export", move |p| run(req, p))
}

fn step(p: &Shared, s: &str) {
    crate::job::step(p, s);
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
        sh(crate::os::command("ditto").arg(&src).arg(&app))?;
        let show = app.join("Contents/Resources/Show");
        (app, show)
    } else {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        copy_folder_app(exe.parent().ok_or("Can't tell where Phosphor is installed")?, &exe, &req.dest, &name)?
    };
    // Media and shaders: each file once, under a unique name, and the project pointed at the copies.
    let plan = plan(&req.comp);
    p.lock().unwrap().total = plan.bytes();
    let total = plan.files.len();
    for (i, (src, rel)) in plan.files.iter().enumerate() {
        step(p, &format!("Copying {}/{total}: {}", i + 1, rel));
        let to = show.join(rel);
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let n = std::fs::copy(src, &to).map_err(|e| format!("copying {}: {e}", src.display()))?;
        p.lock().unwrap().bytes += n;
    }
    let mut comp = req.comp;
    plan.point_at_copies(&mut comp);
    let failed = plan.missing.clone();

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
            sh(crate::os::command("plutil").args(["-replace", key, "-string", &value]).arg(&plist))?;
        }
        let _ = crate::os::command("xattr").args(["-cr"]).arg(&app).output();
        sh(crate::os::command("codesign").args(["--force", "--deep", "--sign", "-"]).arg(&app))?;
    }

    let mut zip = None;
    if req.zip {
        step(p, "Zipping…");
        let z = req.dest.join(format!("{name}.zip"));
        let _ = std::fs::remove_file(&z);
        if cfg!(target_os = "macos") {
            sh(crate::os::command("ditto").args(["-c", "-k", "--sequesterRsrc", "--keepParent"]).arg(&app).arg(&z))?;
        } else {
            // Windows' tar writes zips (-a picks the format from the name).
            sh(crate::os::command("tar").arg("-a").arg("-c").arg("-f").arg(&z).arg("-C").arg(&req.dest).arg(&name))?;
        }
        zip = Some(z);
    }
    let mut g = p.lock().unwrap();
    g.output = Some(app);
    g.zip = zip;
    g.media_files = plan.media;
    if !failed.is_empty() {
        g.error = Some(crate::ui::plural(failed.len(), "{0} media file was missing and isn't in the show (listed in missing-media.txt)", "{0} media files were missing and aren't in the show (listed in missing-media.txt)"));
    }
    Ok(())
}

/// What a show carries: every file it needs, where each goes inside the show ("media/clip.mov", "isf/glow.fs"),
/// and how the project's paths change to point there. One walk over the project for Export Show and packed shows.
#[derive(Default)]
pub struct Plan {
    /// (file on this computer, path inside the show; always "/"-separated).
    pub files: Vec<(PathBuf, String)>,
    /// Project path → its copy's path inside the show (an image sequence keeps its pattern).
    pub paths: HashMap<String, String>,
    /// Media files that couldn't be found (left out).
    pub missing: Vec<String>,
    /// How many media files (a sequence counts once).
    pub media: usize,
}

impl Plan {
    /// Bytes to copy (reads each file's size).
    pub fn bytes(&self) -> u64 {
        self.files.iter().filter_map(|(f, _)| std::fs::metadata(f).ok()).map(|m| m.len()).sum()
    }

    /// Points the project's media at the copies (paths relative to the show's folder).
    pub fn point_at_copies(&self, comp: &mut Composition) {
        comp.map_paths(|path| self.paths.get(path).cloned().unwrap_or_else(|| path.to_string()));
    }
}

/// Works out the `Plan` for a project: its media (each under a unique name, sequences in a folder of their own) and
/// the shaders it uses that aren't built in (imported ISF / LUT files, your own Shader Editor effects).
pub fn plan(comp: &Composition) -> Plan {
    let mut plan = Plan::default();
    // Names already taken in media/ (lower case: Mac and Windows disks don't tell "A.mov" from "a.mov").
    let mut names: HashSet<String> = HashSet::new();
    let mut unique = |wanted: String, ext: &str| {
        let mut name = format!("{wanted}{ext}");
        let mut n = 2;
        while !names.insert(name.to_lowercase()) {
            name = format!("{wanted} {n}{ext}");
            n += 1;
        }
        name
    };
    for path in comp.media_paths() {
        // An image sequence: all its frames into a folder of their own.
        if crate::media::sequence::is_sequence(&path) {
            let frames = crate::media::sequence::members(&path);
            if frames.is_empty() {
                plan.missing.push(path);
                continue;
            }
            let folder = unique(crate::media::sequence::label(&path), "");
            for f in frames {
                let rel = format!("media/{folder}/{}", f.file_name().unwrap_or_default().to_string_lossy());
                plan.files.push((f, rel));
            }
            let pattern = Path::new(&path).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
            plan.paths.insert(path, format!("media/{folder}/{pattern}"));
            plan.media += 1;
            continue;
        }
        let src = PathBuf::from(&path);
        if !src.is_file() {
            plan.missing.push(path);
            continue;
        }
        let stem = src.file_stem().map_or_else(|| "media".into(), |s| s.to_string_lossy().into_owned());
        let ext = src.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        let rel = format!("media/{}", unique(stem, &ext));
        plan.paths.insert(path, rel.clone());
        plan.files.push((src, rel));
        plan.media += 1;
    }
    for id in comp.used_shader_ids() {
        if let Some(slug) = id.strip_prefix(shaders::ISF_EFFECT).or_else(|| id.strip_prefix(shaders::ISF_GENERATOR)) {
            for f in find_isf(slug) {
                let rel = format!("isf/{}", f.file_name().unwrap_or_default().to_string_lossy());
                plan.files.push((f, rel));
            }
        } else if let Some(slug) = id.strip_prefix(shaders::CUSTOM_EFFECT).or_else(|| id.strip_prefix(shaders::CUSTOM_GENERATOR)).or_else(|| id.strip_prefix(shaders::CUSTOM_BLEND)) {
            let file = crate::show::storage_dir().map(|d| d.join("effects").join(format!("{slug}.wgsl")));
            if let Some(file) = file.filter(|f| f.is_file()) {
                plan.files.push((file, format!("effects/{slug}.wgsl")));
            }
        }
    }
    // (One shader used as an effect and a generator is still one file.)
    let mut seen = HashSet::new();
    plan.files.retain(|(_, rel)| seen.insert(rel.clone()));
    plan
}

/// The file(s) behind an ISF id's slug anywhere in the shader folders: the `.fs` (and its `.vs`), a `.glsl`, or a
/// LUT's `.cube` (`lut_<slug>`). Empty when nothing is found.
pub fn find_isf(slug: &str) -> Vec<PathBuf> {
    for dir in crate::isf_lib::folders() {
        for path in crate::isf_lib::shader_files(dir) {
            let (Some(stem), Some(ext)) = (path.file_stem(), path.extension()) else { continue };
            let own = phosphor_engine::custom::slug(&stem.to_string_lossy());
            let hit = if ext == "cube" { slug.strip_prefix("lut_") == Some(own.as_str()) } else { own == slug };
            if hit {
                let vs = path.with_extension("vs");
                return std::iter::once(path).chain(vs.is_file().then_some(vs)).collect();
            }
        }
    }
    Vec::new()
}

/// Local date and time, e.g. "2026-10-14 15:02" (UTC where there's no `date` command).
pub(crate) fn chrono_now() -> String {
    let local = crate::os::command("date").arg("+%Y-%m-%d %H:%M").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|s| s.len() == 16);
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
