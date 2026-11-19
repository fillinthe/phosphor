//! Packed shows: one `.phosphorpack` file (a zip) with the project, every media file and the shaders it uses, its
//! paths made relative, to carry a show to another computer. Opening one unpacks it into a folder and opens the
//! project from there with every media link working. Media is stored as it is (video doesn't shrink) and streamed
//! a megabyte at a time, so a pack can be as big as the disk allows.
//!
//! What goes in comes from `export::plan`, the same walk Export Show uses.

use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use phosphor_engine::Composition;

use crate::job::{self, Shared};
use crate::show::{MANIFEST, Manifest, PROJECT};

/// The file extension (no dot).
pub const EXT: &str = "phosphorpack";

/// Is this a packed show (by its name)?
pub fn is_pack(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case(EXT))
}

/// Where shows that arrive (sent by another Phosphor) are unpacked: Documents/Phosphor Shows.
pub fn shows_dir() -> PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    home.join("Documents").join("Phosphor Shows")
}

/// A name that's free in `dir`: `name`, else "name 2", "name 3"…
pub fn free_path(dir: &Path, name: &str, ext: &str) -> PathBuf {
    let with = |n: &str| if ext.is_empty() { dir.join(n) } else { dir.join(format!("{n}.{ext}")) };
    let mut path = with(name);
    let mut n = 2;
    while path.exists() {
        path = with(&format!("{name} {n}"));
        n += 1;
    }
    path
}

/// The folder a pack unpacks into when nobody picks one: beside it, named after it.
pub fn folder_beside(pack: &Path) -> PathBuf {
    let stem = pack.file_stem().map_or_else(|| "Show".into(), |s| s.to_string_lossy().into_owned());
    free_path(pack.parent().unwrap_or(Path::new(".")), &stem, "")
}

/// A show's name as a file name (no slashes or other characters disks refuse).
pub fn file_name(name: &str) -> String {
    let clean: String = name.trim().chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { '-' } else { c }).collect();
    let clean = clean.trim_matches(|c: char| c == '.' || c.is_whitespace()).to_string();
    if clean.is_empty() { "Show".into() } else { clean }
}

/// Packs `comp` into `dest` (written whole: a canceled or failed pack leaves nothing behind). Progress counts the
/// bytes of the files packed; `output` = the pack.
pub fn write(mut comp: Composition, name: &str, dest: &Path, p: &Shared) -> Result<(), String> {
    use zip::write::SimpleFileOptions;
    job::step(p, "Looking at the project…");
    let plan = crate::export::plan(&comp);
    p.lock().unwrap().total = plan.bytes();
    let part = dest.with_file_name(format!(".{}.part", dest.file_name().unwrap_or_default().to_string_lossy()));
    let written = (|| {
        let file = std::fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?;
        let mut zip = zip::ZipWriter::new(BufWriter::with_capacity(1 << 20, file));
        let total = plan.files.len();
        for (i, (src, rel)) in plan.files.iter().enumerate() {
            job::step(p, format!("Packing {}/{total}: {rel}", i + 1));
            let mut from = std::fs::File::open(src).map_err(|e| format!("{}: {e}", src.display()))?;
            let size = from.metadata().map(|m| m.len()).unwrap_or(0);
            // Stored, not compressed: video and pictures are compressed already. Zip64 for files of 4 GB or more.
            let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored).large_file(size >= u32::MAX as u64);
            zip.start_file(rel.as_str(), options).map_err(|e| e.to_string())?;
            job::copy(&mut from, &mut zip, p)?;
        }
        job::step(p, "Writing the project…");
        plan.point_at_copies(&mut comp);
        let small = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file(PROJECT, small).map_err(|e| e.to_string())?;
        zip.write_all(comp.to_json().as_bytes()).map_err(|e| e.to_string())?;
        let manifest = Manifest { name: name.to_string(), exported: crate::export::chrono_now(), phosphor_version: env!("CARGO_PKG_VERSION").into(), ..Default::default() };
        zip.start_file(MANIFEST, small).map_err(|e| e.to_string())?;
        zip.write_all(serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?.as_bytes()).map_err(|e| e.to_string())?;
        if !plan.missing.is_empty() {
            zip.start_file(MISSING, small).map_err(|e| e.to_string())?;
            zip.write_all(format!("These files were missing when the show was packed:\n{}\n", plan.missing.join("\n")).as_bytes()).map_err(|e| e.to_string())?;
        }
        let out = zip.finish().map_err(|e| e.to_string())?;
        let file = out.into_inner().map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())
    })();
    if let Err(e) = written.and_then(|()| std::fs::rename(&part, dest).map_err(|e| e.to_string())) {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    let mut g = p.lock().unwrap();
    g.output = Some(dest.to_path_buf());
    g.media_files = plan.media;
    if !plan.missing.is_empty() {
        g.error = Some(crate::ui::plural(plan.missing.len(), "{0} media file was missing and isn't in the pack", "{0} media files were missing and aren't in the pack"));
    }
    Ok(())
}

const MISSING: &str = "missing-media.txt";

/// Unpacks `pack` into the folder `into` (made; it should be new or empty) and writes the project there as
/// "<name>.phosphor" with its media paths pointing at the unpacked files. Shaders the show brought that this
/// computer doesn't have go into its shader folders. `output` = the project file to open.
pub fn unpack(pack: &Path, into: &Path, p: &Shared) -> Result<PathBuf, String> {
    let fresh = !into.exists();
    let result = unpack_into(pack, into, p);
    // A canceled or failed unpack takes away the folder it made (never one that was there before).
    if result.is_err() && fresh {
        let _ = std::fs::remove_dir_all(into);
    }
    result
}

fn unpack_into(pack: &Path, into: &Path, p: &Shared) -> Result<PathBuf, String> {
    job::step(p, "Opening the pack…");
    let file = std::fs::File::open(pack).map_err(|e| format!("{}: {e}", pack.display()))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file)).map_err(|e| format!("Not a packed show ({e})"))?;
    zip.by_name(PROJECT).map_err(|_| "Not a packed show: there's no project in it".to_string())?;
    let total: u64 = (0..zip.len()).filter_map(|i| zip.by_index_raw(i).ok().map(|f| f.size())).sum();
    p.lock().unwrap().total = total;
    std::fs::create_dir_all(into).map_err(|e| format!("{}: {e}", into.display()))?;
    let count = zip.len();
    for i in 0..count {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        // Only names that stay inside the folder ("../" or absolute paths are skipped, never written).
        let Some(rel) = entry.enclosed_name() else { continue };
        let to = into.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&to).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        job::step(p, format!("Unpacking {}/{count}: {}", i + 1, rel.display()));
        let out = std::fs::File::create(&to).map_err(|e| format!("{}: {e}", to.display()))?;
        let mut out = BufWriter::with_capacity(1 << 20, out);
        job::copy(&mut entry, &mut out, p)?;
        out.flush().map_err(|e| e.to_string())?;
    }
    job::step(p, "Opening the project…");
    let manifest: Manifest = std::fs::read_to_string(into.join(MANIFEST)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let json = std::fs::read_to_string(into.join(PROJECT)).map_err(|e| e.to_string())?;
    let mut comp = Composition::from_json(&json).map_err(|e| e.to_string())?;
    point_into(&mut comp, into);
    let name = if manifest.name.trim().is_empty() { into.file_name().map_or_else(|| "Show".into(), |n| n.to_string_lossy().into_owned()) } else { file_name(&manifest.name) };
    let project = free_path(into, &name, "phosphor");
    crate::project_files::write_whole(&project, comp.to_json()).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(into.join(PROJECT));
    let _ = std::fs::remove_file(into.join(MANIFEST));
    install_shaders(into);
    p.lock().unwrap().output = Some(project.clone());
    Ok(project)
}

/// The project's relative media paths (as packed) made whole, inside `dir`.
pub fn point_into(comp: &mut Composition, dir: &Path) {
    comp.map_paths(|p| if Path::new(p).is_relative() { dir.join(p).to_string_lossy().into_owned() } else { p.to_string() });
}

/// Shaders a show brought (isf/, effects/ in its folder) go into this computer's shader folders, unless one of
/// that name is there already (yours is kept). Returns how many were added.
pub fn install_shaders(dir: &Path) -> usize {
    let Some(storage) = crate::show::storage_dir() else { return 0 };
    let mut added = 0;
    for kind in ["isf", "effects"] {
        let Ok(entries) = std::fs::read_dir(dir.join(kind)) else { continue };
        // Decided for every file before any is copied (a .vs goes with its .fs, whichever comes first).
        let files: Vec<(PathBuf, bool)> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|f| f.is_file())
            .map(|f| {
                let have = if kind == "isf" {
                    // An ISF shader or LUT of that name anywhere in the shader folders.
                    let stem = f.file_stem().map(|s| phosphor_engine::custom::slug(&s.to_string_lossy())).unwrap_or_default();
                    let lut = f.extension().is_some_and(|e| e.eq_ignore_ascii_case("cube"));
                    !crate::export::find_isf(&if lut { format!("lut_{stem}") } else { stem }).is_empty()
                } else {
                    f.file_name().is_some_and(|n| storage.join(kind).join(n).exists())
                };
                (f, have)
            })
            .collect();
        for (f, have) in files {
            let Some(file) = f.file_name() else { continue };
            let to = storage.join(kind).join(file);
            if have {
                continue;
            }
            if std::fs::create_dir_all(storage.join(kind)).and_then(|()| std::fs::copy(&f, &to)).is_ok() {
                added += 1;
            }
        }
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;
    use phosphor_engine::SourceSpec;

    fn wait(p: &Shared) {
        while !p.lock().unwrap().done {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn a_show_packs_and_unpacks_with_its_media_found() {
        let root = std::env::temp_dir().join(format!("phosphor-pack-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (a, b) = (root.join("here/clips"), root.join("there/clips"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        // Two files with the same name in different folders, one big enough to take several chunks.
        let movie = a.join("loop.mov");
        let big: Vec<u8> = (0..(3 << 20) + 17).map(|i| (i % 251) as u8).collect();
        std::fs::write(&movie, &big).unwrap();
        let other = b.join("loop.mov");
        std::fs::write(&other, b"second").unwrap();
        let picture = a.join("still.png");
        std::fs::write(&picture, b"png").unwrap();
        let mut comp = Composition::default();
        let l = comp.add_layer();
        l.source = SourceSpec::Movie { name: "loop".into(), path: Some(movie.to_string_lossy().into_owned()) };
        let l = comp.add_layer();
        l.source = SourceSpec::Movie { name: "loop 2".into(), path: Some(other.to_string_lossy().into_owned()) };
        comp.add_media("still.png", &picture.to_string_lossy());
        comp.add_media("gone.mov", "/nowhere/gone.mov");

        let pack = root.join("Club Night.phosphorpack");
        let p: Shared = Default::default();
        write(comp.clone(), "Club Night", &pack, &p).unwrap();
        assert!(pack.is_file() && !root.join(".Club Night.phosphorpack.part").exists());
        assert_eq!(p.lock().unwrap().media_files, 3);
        assert!(p.lock().unwrap().error.as_deref().is_some_and(|e| e.contains("1 media file")), "the missing file is said");

        // Unpacked somewhere else, with the originals gone: every path points at the unpacked copies.
        std::fs::remove_dir_all(root.join("here")).unwrap();
        std::fs::remove_dir_all(root.join("there")).unwrap();
        let into = folder_beside(&pack);
        assert_eq!(into, root.join("Club Night"));
        let p: Shared = Default::default();
        let project = unpack(&pack, &into, &p).unwrap();
        assert_eq!(project, into.join("Club Night.phosphor"));
        let back = Composition::from_json(&std::fs::read_to_string(&project).unwrap()).unwrap();
        let paths = back.media_paths();
        let found: Vec<&String> = paths.iter().filter(|f| Path::new(f).is_file()).collect();
        assert_eq!(found.len(), 3, "{paths:?}");
        assert!(paths.iter().any(|f| f == "/nowhere/gone.mov"), "a missing file keeps its old path");
        let movies: Vec<Vec<u8>> = found.iter().filter(|f| f.ends_with(".mov")).map(|f| std::fs::read(f).unwrap()).collect();
        assert!(movies.contains(&big) && movies.contains(&b"second".to_vec()), "both same-named files arrive whole");
        assert!(into.join(MISSING).is_file());
        assert!(p.lock().unwrap().bytes >= big.len() as u64);
        // Unpacking again goes into a folder of its own.
        assert_eq!(folder_beside(&pack), root.join("Club Night 2"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_canceled_pack_leaves_nothing() {
        let root = std::env::temp_dir().join(format!("phosphor-pack-cancel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let f = root.join("a.mov");
        std::fs::write(&f, vec![1u8; 2 << 20]).unwrap();
        let mut comp = Composition::default();
        comp.add_media("a.mov", &f.to_string_lossy());
        let pack = root.join("x.phosphorpack");
        let p = job::spawn("pack-test", {
            let pack = pack.clone();
            move |p| {
                job::cancel(p);
                write(comp, "x", &pack, p)
            }
        });
        wait(&p);
        assert_eq!(p.lock().unwrap().error.as_deref(), Some(job::CANCELED));
        assert!(!pack.exists());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1, "no part file left");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn names_never_escape_the_folder() {
        assert_eq!(file_name("a/b:c"), "a-b-c");
        assert_eq!(file_name("  ..  "), "Show");
        // A pack holding "../evil" writes nothing outside the folder.
        let root = std::env::temp_dir().join(format!("phosphor-pack-slip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let pack = root.join("bad.phosphorpack");
        let mut z = zip::ZipWriter::new(std::fs::File::create(&pack).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        z.start_file("../evil.txt", o).unwrap();
        z.write_all(b"x").unwrap();
        z.start_file(PROJECT, o).unwrap();
        z.write_all(Composition::default().to_json().as_bytes()).unwrap();
        z.finish().unwrap();
        let into = root.join("in");
        unpack(&pack, &into, &Default::default()).unwrap();
        assert!(!root.join("evil.txt").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
