//! Clips from another drive (an external SSD, a network share: anything under /Volumes) are copied to this Mac the
//! first time they're opened, and opened from that copy afterwards: loading and looping read the Mac's own disk, and
//! a clip already copied keeps playing if the drive is unplugged (a player that loses its file reopens it, and the
//! reopen lands on the copy). Copies run on one thread of their own, at a capped speed so they never starve
//! playback from the same drive; the cache keeps the most recently used clips within `CAP_BYTES`.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// How much the cache may hold.
pub const CAP_BYTES: u64 = 25 << 30;
/// Copy speed (bytes per second): well under what an external SSD gives, so playback from it keeps its share.
const COPY_RATE: u64 = 150 << 20;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    /// The copy, in the cache folder.
    local: String,
    len: u64,
    /// The original's modification time (seconds) when copied: a changed original is copied again.
    modified: u64,
    /// Last opened (seconds since 1970): the least recently used go first when the cache is full.
    used: u64,
}

struct Cache {
    dir: PathBuf,
    index: HashMap<String, Entry>,
    pending: HashSet<String>,
    tx: Sender<String>,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Clips worth copying: on another volume, a real file (not an image sequence pattern or a link).
pub fn wanted(path: &str) -> bool {
    // (`is_supported` turns away "._" files: the metadata macOS writes beside files on some drives.)
    cfg!(target_os = "macos") && path.starts_with("/Volumes/") && !crate::media::sequence::is_sequence(path) && crate::media::is_movie(path) && crate::media::is_supported(path) && !boot_volume(path)
}

/// "/Volumes/Macintosh HD/…" is this Mac's own disk (a link to "/"), not another drive.
fn boot_volume(path: &str) -> bool {
    let volume: PathBuf = Path::new(path).components().take(3).collect();
    std::fs::symlink_metadata(&volume).is_ok_and(|m| m.file_type().is_symlink())
}

fn cache() -> Option<&'static Mutex<Cache>> {
    static CACHE: OnceLock<Option<Mutex<Cache>>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let dir = crate::show::storage_dir()?.join("cache").join("clips");
            let index: HashMap<String, Entry> = std::fs::read(dir.join("index.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            let (tx, rx) = channel::<String>();
            let worker_dir = dir.clone();
            let _ = std::thread::Builder::new().name("clip-cache".into()).spawn(move || {
                for path in rx {
                    copy_one(&worker_dir, &path);
                }
            });
            Some(Mutex::new(Cache { dir, index, pending: HashSet::new(), tx }))
        })
        .as_ref()
}

/// The file to open for `path`: the Mac's copy when there is one (never touches the original's drive), else `path`.
/// Also asks for a copy of a clip that should have one. Called wherever a movie is opened (`video::open_input`).
pub fn resolve(path: &str) -> String {
    if !wanted(path) {
        return path.to_string();
    }
    let Some(c) = cache() else { return path.to_string() };
    let Ok(mut c) = c.lock() else { return path.to_string() };
    // Checked again (in the background) each time it's opened: a changed original is copied again.
    if c.pending.insert(path.to_string()) {
        let _ = c.tx.send(path.to_string());
    }
    match c.index.get_mut(path) {
        Some(e) if Path::new(&e.local).is_file() => {
            e.used = now_secs();
            e.local.clone()
        }
        _ => path.to_string(),
    }
}

/// Drops the copy of `path` (the file was edited: Reload).
pub fn forget(path: &str) {
    if let Some(c) = cache()
        && let Ok(mut c) = c.lock()
        && let Some(e) = c.index.remove(path)
    {
        let _ = std::fs::remove_file(&e.local);
        save_index(&c);
    }
}

fn save_index(c: &Cache) {
    let _ = std::fs::create_dir_all(&c.dir);
    let tmp = c.dir.join("index.json.part");
    if std::fs::write(&tmp, serde_json::to_vec(&c.index).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(&tmp, c.dir.join("index.json"));
    }
}

/// On the cache's thread: copies `path` unless an up-to-date copy is there, then trims the cache to its cap.
fn copy_one(dir: &Path, path: &str) {
    let done = |c: &mut Cache| {
        c.pending.remove(path);
    };
    let Ok(meta) = std::fs::metadata(path) else {
        // The drive is gone: the copy (if any) is all there is.
        if let Some(c) = cache()
            && let Ok(mut c) = c.lock()
        {
            done(&mut c);
        }
        return;
    };
    let (len, modified) = (meta.len(), meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs()));
    let fresh = cache().and_then(|c| c.lock().ok()).is_some_and(|c| c.index.get(path).is_some_and(|e| e.len == len && e.modified == modified && Path::new(&e.local).is_file()));
    if fresh || len == 0 || len > CAP_BYTES / 4 {
        if let Some(c) = cache()
            && let Ok(mut c) = c.lock()
        {
            done(&mut c);
        }
        return;
    }
    let ext = Path::new(path).extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| "mov".into());
    let local = dir.join(format!("{:016x}.{ext}", key(path)));
    let started = Instant::now();
    let copied = std::fs::create_dir_all(dir).map_err(|e| e.to_string()).and_then(|_| copy_capped(Path::new(path), &local));
    let Some(c) = cache() else { return };
    let Ok(mut c) = c.lock() else { return };
    done(&mut c);
    match copied {
        Ok(()) => {
            log::info!(target: "phosphor::clip_cache", "copied to this Mac: {} ({} MB in {:.1} s)", Path::new(path).file_name().map_or(path.into(), |n| n.to_string_lossy()), len >> 20, started.elapsed().as_secs_f32());
            c.index.insert(path.to_string(), Entry { local: local.to_string_lossy().into_owned(), len, modified, used: now_secs() });
            trim(&mut c);
            save_index(&c);
        }
        Err(e) => log::warn!(target: "phosphor::clip_cache", "couldn't copy {path} to this Mac: {e}"),
    }
}

/// A stable name for a path's copy.
fn key(path: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    h.finish()
}

/// Copies `from` to `to` (through a `.part` file) at no more than `COPY_RATE`.
fn copy_capped(from: &Path, to: &Path) -> Result<(), String> {
    let part = to.with_extension("part");
    let result = (|| {
        let mut src = std::fs::File::open(from).map_err(|e| e.to_string())?;
        let mut dst = std::fs::File::create(&part).map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 4 << 20];
        let start = Instant::now();
        let mut total: u64 = 0;
        loop {
            let n = src.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            dst.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            total += n as u64;
            // Ahead of the allowed rate: wait until the clock catches up.
            let due = Duration::from_secs_f64(total as f64 / COPY_RATE as f64);
            if let Some(wait) = due.checked_sub(start.elapsed()) {
                std::thread::sleep(wait);
            }
        }
        dst.flush().map_err(|e| e.to_string())
    })();
    match result {
        Ok(()) => std::fs::rename(&part, to).map_err(|e| e.to_string()),
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(e)
        }
    }
}

/// Removes the least recently used copies until the cache fits its cap.
fn trim(c: &mut Cache) {
    let mut total: u64 = c.index.values().map(|e| e.len).sum();
    if total <= CAP_BYTES {
        return;
    }
    let mut by_age: Vec<(String, u64, u64)> = c.index.iter().map(|(k, e)| (k.clone(), e.used, e.len)).collect();
    by_age.sort_by_key(|(_, used, _)| *used);
    for (k, _, len) in by_age {
        if total <= CAP_BYTES {
            break;
        }
        if let Some(e) = c.index.remove(&k) {
            let _ = std::fs::remove_file(&e.local);
            total -= len;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_movies_on_other_drives_are_copied() {
        let on = cfg!(target_os = "macos");
        assert_eq!(wanted("/Volumes/WD Black/loops/a.mov"), on);
        assert!(!wanted("/Users/me/Movies/a.mov"), "already on this Mac");
        assert!(!wanted("/Volumes/WD Black/still.png"), "pictures load whole anyway");
        assert!(!wanted("/Volumes/WD Black/seq/frame_%04d.png"), "sequences are many files");
        assert!(!wanted("/Volumes/WD Black/loops/._a.mov"), "macOS's metadata files aren't clips");
    }

    #[test]
    fn a_copy_is_made_at_a_capped_speed_and_whole() {
        let dir = std::env::temp_dir().join(format!("phosphor-clipcache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let from = dir.join("clip.mov");
        let data: Vec<u8> = (0..(3u32 << 20)).map(|i| (i % 251) as u8).collect();
        std::fs::write(&from, &data).unwrap();
        let to = dir.join("copy.mov");
        copy_capped(&from, &to).unwrap();
        assert_eq!(std::fs::read(&to).unwrap(), data, "byte for byte");
        assert!(!to.with_extension("part").exists(), "no leftover part file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_least_recently_used_go_first() {
        let (tx, _rx) = channel();
        let dir = std::env::temp_dir();
        let mut c = Cache { dir, index: HashMap::new(), pending: HashSet::new(), tx };
        let half = CAP_BYTES / 2 + 1;
        for (k, used) in [("old", 1), ("mid", 2), ("new", 3)] {
            c.index.insert(k.into(), Entry { local: format!("/nonexistent/{k}"), len: half, modified: 0, used });
        }
        trim(&mut c);
        assert!(c.index.contains_key("new") && !c.index.contains_key("old"), "{:?}", c.index.keys().collect::<Vec<_>>());
        assert!(c.index.values().map(|e| e.len).sum::<u64>() <= CAP_BYTES);
    }
}
