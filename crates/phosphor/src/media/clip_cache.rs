//! Clips from another drive (an external SSD, a network share: anything under /Volumes) are copied to this Mac the
//! first time they're opened, and opened from that copy afterwards: loading and looping read the Mac's own disk, and
//! a clip with a copy keeps playing if the drive is unplugged. A movie already playing from the drive moves to the
//! copy the moment it's finished (`copies_made`), and one whose drive goes first carries on from the copy or waits
//! for the drive (`media::drive`, the decoder's `read_packet`). Copies run on one thread of their own, at a capped
//! speed so they never starve playback from the same drive; the clips on the layers now (`set_current`) are copied
//! first, and a copy of any other clip gives way to them. The cache keeps the most recently used clips within its
//! cap (`cap_gb`: the user's, in Cache settings).

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// How much the cache may hold unless the user says otherwise (`set_cap_gb`).
pub const CAP_GB: u32 = 25;
static CAP: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(CAP_GB);

/// The most the cache may hold, in GB (Cache settings).
pub fn cap_gb() -> u32 {
    CAP.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn set_cap_gb(gb: u32) {
    CAP.store(gb.clamp(1, 500), std::sync::atomic::Ordering::Relaxed);
}

fn cap_bytes() -> u64 {
    (cap_gb() as u64) << 30
}
/// Free space a copy must leave on this Mac's disk (recordings, saves and macOS itself need room).
const KEEP_FREE: u64 = 30 << 30;
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
    /// Clips waiting for their copy, in the order they were asked for (`next_job` says which goes next).
    queue: VecDeque<String>,
    /// Waiting or being copied (a clip is asked for once until its copy is done or given up).
    pending: HashSet<String>,
    /// The clips on the layers now (`set_current`): copied first.
    current: HashSet<String>,
}

/// Wakes the copying thread when a clip is asked for.
static WORK: Condvar = Condvar::new();

/// How many copies have been finished since Phosphor started: a player reading a clip from its drive looks for its
/// copy only when this changes.
static MADE: AtomicU64 = AtomicU64::new(0);

/// Copies finished so far (`MADE`).
pub fn copies_made() -> u64 {
    MADE.load(Ordering::Relaxed)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Clips worth copying: on another volume, a real file (not an image sequence pattern or a link).
pub fn wanted(path: &str) -> bool {
    may_copy(path) && !boot_volume(path)
}

/// `wanted` going by the path alone (never touches a disk: safe on the UI thread).
fn may_copy(path: &str) -> bool {
    // (`is_supported` turns away "._" files: the metadata macOS writes beside files on some drives.)
    cfg!(target_os = "macos") && path.starts_with("/Volumes/") && !crate::media::sequence::is_sequence(path) && crate::media::is_movie(path) && crate::media::is_supported(path)
}

/// "/Volumes/Macintosh HD/…" is this Mac's own disk (a link to "/"), not another drive.
pub(super) fn boot_volume(path: &str) -> bool {
    let volume: PathBuf = Path::new(path).components().take(3).collect();
    std::fs::symlink_metadata(&volume).is_ok_and(|m| m.file_type().is_symlink())
}

static CACHE: OnceLock<Option<Mutex<Cache>>> = OnceLock::new();

fn cache() -> Option<&'static Mutex<Cache>> {
    CACHE
        .get_or_init(|| {
            let dir = crate::cache_store::folder("clips")?;
            let index: HashMap<String, Entry> = std::fs::read(dir.join("index.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            let worker_dir = dir.clone();
            let _ = std::thread::Builder::new().name("clip-cache".into()).spawn(move || work(&worker_dir));
            Some(Mutex::new(Cache { dir, index, queue: VecDeque::new(), pending: HashSet::new(), current: HashSet::new() }))
        })
        .as_ref()
}

/// The copying thread: one clip at a time, the most current first.
fn work(dir: &Path) {
    loop {
        let Some(c) = cache() else { return };
        let Ok(mut c) = c.lock() else { return };
        let path = loop {
            if let Some(path) = next_job(&c.queue, &c.current).and_then(|i| c.queue.remove(i)) {
                break path;
            }
            c = match WORK.wait(c) {
                Ok(c) => c,
                Err(_) => return,
            };
        };
        drop(c);
        copy_one(dir, &path);
    }
}

/// Which waiting clip to copy next: the first one asked for that's on a layer now, else the first one asked for.
fn next_job(queue: &VecDeque<String>, current: &HashSet<String>) -> Option<usize> {
    queue.iter().position(|p| current.contains(p)).or((!queue.is_empty()).then_some(0))
}

/// Should the copy of `path` stop and let another go first? When it isn't on a layer and one that is is waiting.
fn gives_way(path: &str, queue: &VecDeque<String>, current: &HashSet<String>) -> bool {
    !current.contains(path) && queue.iter().any(|p| current.contains(p))
}

/// Puts `path` in line for a copy (once).
fn ask(c: &mut Cache, path: &str) {
    if c.pending.insert(path.to_string()) {
        c.queue.push_back(path.to_string());
        WORK.notify_one();
    }
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
    ask(&mut c, path);
    match c.index.get_mut(path) {
        Some(e) if Path::new(&e.local).is_file() => {
            e.used = now_secs();
            e.local.clone()
        }
        _ => path.to_string(),
    }
}

/// Is there a copy of `path` on this computer (so it plays with its drive away)? Never touches the drive.
pub fn has(path: &str) -> bool {
    copy_of(path).is_some()
}

/// This computer's whole copy of `path`, if it has one. Never touches the drive.
pub fn copy_of(path: &str) -> Option<String> {
    if !may_copy(path) {
        return None;
    }
    let c = cache()?.lock().ok()?;
    c.index.get(path).map(|e| e.local.clone()).filter(|l| Path::new(l).is_file())
}

/// Asks for a copy of `path` (its drive came back while it played: copied now, so the next unplug finds it here).
/// Looks at the drive: not on the UI thread.
pub fn want(path: &str) {
    if wanted(path)
        && let Some(c) = cache()
        && let Ok(mut c) = c.lock()
    {
        ask(&mut c, path);
    }
}

/// The clips on the layers now (the show's current clips): their copies are made before any other clip's, and a
/// copy under way of a clip that isn't current gives way to them. Ones not copied yet are asked for. Called when the
/// layers' clips change; looks only at the paths (safe on the UI thread).
pub fn set_current(paths: impl IntoIterator<Item = String>) {
    let current: HashSet<String> = paths.into_iter().filter(|p| may_copy(p)).collect();
    // (No clip from another drive and none before: nothing to do, and the cache isn't even opened.)
    if current.is_empty() && cache_opened().is_none_or(|c| c.lock().is_ok_and(|c| c.current.is_empty())) {
        return;
    }
    let Some(c) = cache() else { return };
    let Ok(mut c) = c.lock() else { return };
    let now = now_secs();
    for p in &current {
        match c.index.get_mut(p) {
            // (In use: the cache keeps it over clips not played for longer.)
            Some(e) => e.used = now,
            None => ask(&mut c, p),
        }
    }
    c.current = current;
}

/// The cache if something already opened it.
fn cache_opened() -> Option<&'static Mutex<Cache>> {
    CACHE.get().and_then(Option::as_ref)
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

/// On the cache's thread: copies `path` unless an up-to-date copy is there, then trims the cache to its cap. A copy
/// of a clip that isn't on a layer stops when one that is is waiting, and goes back in line.
fn copy_one(dir: &Path, path: &str) {
    let finish = |log_fresh: bool| {
        if let Some(c) = cache()
            && let Ok(mut c) = c.lock()
        {
            c.pending.remove(path);
            // (Opened again: remember it was used, so the cache keeps the clips played most recently.)
            if log_fresh {
                save_index(&c);
            }
        }
    };
    // (Asked for by the clips on the layers without looking at the disk: the look is here.)
    if !wanted(path) {
        return finish(false);
    }
    // The drive mounted again under another name ("_FOOTAGE 1") holds the same clip.
    let Some(src) = Path::new(path).is_file().then(|| path.to_string()).or_else(|| super::drive::moved(path)) else {
        // The drive is gone: the copy (if any) is all there is. Asked for again when the clip is (`want`).
        return finish(false);
    };
    let Ok(meta) = std::fs::metadata(&src) else { return finish(false) };
    let (len, modified) = (meta.len(), meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs()));
    let fresh = cache().and_then(|c| c.lock().ok()).is_some_and(|c| c.index.get(path).is_some_and(|e| e.len == len && e.modified == modified && Path::new(&e.local).is_file()));
    // Never fill the disk: the copy must leave plenty free.
    let _ = std::fs::create_dir_all(dir);
    let room = crate::convert::free_space(&dir.to_string_lossy()).is_none_or(|free| free > len + KEEP_FREE);
    if fresh || !room || len == 0 || len > cap_bytes() / 4 {
        if !room && !fresh {
            log::info!(target: "phosphor::clip_cache", "not copied (this Mac's disk is nearly full): {path}");
        }
        return finish(fresh);
    }
    let ext = Path::new(path).extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| "mov".into());
    let local = dir.join(format!("{:016x}.{ext}", key(path)));
    let started = Instant::now();
    let keep_going = || cache().and_then(|c| c.lock().ok()).is_none_or(|c| !gives_way(path, &c.queue, &c.current));
    let copied = std::fs::create_dir_all(dir).map_err(|e| e.to_string()).and_then(|_| copy_capped(Path::new(&src), &local, keep_going));
    let Some(c) = cache() else { return };
    let Ok(mut c) = c.lock() else { return };
    let name = Path::new(path).file_name().map_or(path.into(), |n| n.to_string_lossy());
    match copied {
        Ok(Copied::Whole) => {
            c.pending.remove(path);
            log::info!(target: "phosphor::clip_cache", "copied to this Mac: {name} ({} MB in {:.1} s)", len >> 20, started.elapsed().as_secs_f32());
            c.index.insert(path.to_string(), Entry { local: local.to_string_lossy().into_owned(), len, modified, used: now_secs() });
            trim(&mut c);
            save_index(&c);
            MADE.fetch_add(1, Ordering::Relaxed);
        }
        // (Still pending: back in line, after the clips on the layers.)
        Ok(Copied::GaveWay) => {
            log::info!(target: "phosphor::clip_cache", "copy of {name} waits: clips playing now go first");
            c.queue.push_back(path.to_string());
        }
        Err(e) => {
            c.pending.remove(path);
            log::warn!(target: "phosphor::clip_cache", "couldn't copy {path} to this Mac: {e}");
        }
    }
}

/// How a copy ended.
#[derive(Debug, PartialEq)]
enum Copied {
    Whole,
    /// Stopped for a more current clip (nothing left behind).
    GaveWay,
}

/// A stable name for a path's copy.
fn key(path: &str) -> u64 {
    phosphor_engine::stable_hash(&[path.as_bytes()])
}

/// Copies `from` to `to` (through a `.part` file) at no more than `COPY_RATE`, asking `keep_going` between pieces.
fn copy_capped(from: &Path, to: &Path, keep_going: impl Fn() -> bool) -> Result<Copied, String> {
    let part = to.with_extension("part");
    let result = (|| {
        let mut src = std::fs::File::open(from).map_err(|e| e.to_string())?;
        let mut dst = std::fs::File::create(&part).map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 4 << 20];
        let start = Instant::now();
        let mut total: u64 = 0;
        loop {
            if !keep_going() {
                return Ok(Copied::GaveWay);
            }
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
        dst.flush().map_err(|e| e.to_string())?;
        Ok(Copied::Whole)
    })();
    match result {
        Ok(Copied::Whole) => std::fs::rename(&part, to).map(|()| Copied::Whole).map_err(|e| e.to_string()),
        other => {
            let _ = std::fs::remove_file(&part);
            other
        }
    }
}

/// Removes every copy (Cache settings › Clear). A clip playing from its copy keeps playing: the file goes when the
/// player lets go of it. Returns the bytes freed.
pub fn clear() -> u64 {
    drop_where(|_| true)
}

/// Removes the copies not opened for `secs` (the automatic clean-up). Returns the bytes freed.
pub fn drop_unused(secs: u64) -> u64 {
    let before = now_secs().saturating_sub(secs);
    drop_where(|e| e.used < before)
}

fn drop_where(gone: impl Fn(&Entry) -> bool) -> u64 {
    let Some(c) = cache() else { return 0 };
    let Ok(mut c) = c.lock() else { return 0 };
    let freed = drop_from(&mut c.index, gone);
    if freed > 0 {
        save_index(&c);
    }
    freed
}

fn drop_from(index: &mut HashMap<String, Entry>, gone: impl Fn(&Entry) -> bool) -> u64 {
    let mut freed = 0;
    index.retain(|_, e| {
        if gone(e) {
            let _ = std::fs::remove_file(&e.local);
            freed += e.len;
            false
        } else {
            true
        }
    });
    freed
}

/// Removes the least recently used copies until the cache fits its cap.
fn trim(c: &mut Cache) {
    trim_to(c, cap_bytes());
}

fn trim_to(c: &mut Cache, cap: u64) {
    let mut total: u64 = c.index.values().map(|e| e.len).sum();
    if total <= cap {
        return;
    }
    let mut by_age: Vec<(String, u64, u64)> = c.index.iter().map(|(k, e)| (k.clone(), e.used, e.len)).collect();
    by_age.sort_by_key(|(_, used, _)| *used);
    for (k, _, len) in by_age {
        if total <= cap {
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
        assert_eq!(copy_capped(&from, &to, || true), Ok(Copied::Whole));
        assert_eq!(std::fs::read(&to).unwrap(), data, "byte for byte");
        assert!(!to.with_extension("part").exists(), "no leftover part file");
        // Told to give way part of the way through: stops, leaves nothing behind (not even the old copy's place).
        let later = dir.join("later.mov");
        let pieces = std::cell::Cell::new(0);
        let once = || {
            pieces.set(pieces.get() + 1);
            pieces.get() < 2
        };
        assert_eq!(copy_capped(&from, &later, once), Ok(Copied::GaveWay));
        assert!(!later.exists() && !later.with_extension("part").exists(), "a copy that gave way leaves no file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clips_on_the_layers_are_copied_first() {
        let queue: VecDeque<String> = ["next in the bank", "playing", "also playing"].map(String::from).into();
        let mut current: HashSet<String> = ["playing", "also playing"].map(String::from).into();
        assert_eq!(next_job(&queue, &current), Some(1), "the first one asked for that's on a layer");
        assert!(gives_way("next in the bank", &queue, &current), "a clip that isn't playing stops for one that is");
        assert!(!gives_way("playing", &queue, &current), "a playing clip never stops for another playing one");
        current.clear();
        assert_eq!(next_job(&queue, &current), Some(0), "none on a layer: in the order asked for");
        assert!(!gives_way("next in the bank", &queue, &current));
        assert_eq!(next_job(&VecDeque::new(), &current), None);
    }

    #[test]
    fn the_layers_clips_are_known_without_looking_at_any_disk() {
        // (`set_current` runs on the UI thread: it goes by the path alone.)
        let on = cfg!(target_os = "macos");
        assert_eq!(may_copy("/Volumes/Gone Drive/a.mov"), on, "a drive that isn't even there");
        assert!(!may_copy("/Users/me/a.mov") && !may_copy("/Volumes/Gone Drive/still.png"));
        assert_eq!(copy_of("/Users/me/a.mov"), None, "nothing on this Mac's own disk is copied");
    }

    #[test]
    fn the_least_recently_used_go_first() {
        let dir = std::env::temp_dir();
        let mut c = Cache { dir, index: HashMap::new(), queue: VecDeque::new(), pending: HashSet::new(), current: HashSet::new() };
        let cap = 10 << 30;
        let half = cap / 2 + 1;
        for (k, used) in [("old", 1), ("mid", 2), ("new", 3)] {
            c.index.insert(k.into(), Entry { local: format!("/nonexistent/{k}"), len: half, modified: 0, used });
        }
        trim_to(&mut c, cap);
        assert!(c.index.contains_key("new") && !c.index.contains_key("old"), "{:?}", c.index.keys().collect::<Vec<_>>());
        assert!(c.index.values().map(|e| e.len).sum::<u64>() <= cap);
        // Not opened since 2: gone (the automatic clean-up); Clear takes the rest.
        c.index.insert("mid".into(), Entry { local: "/nonexistent/mid".into(), len: 5, modified: 0, used: 2 });
        assert_eq!(drop_from(&mut c.index, |e| e.used < 3), 5);
        assert!(c.index.contains_key("new") && !c.index.contains_key("mid"));
        assert_eq!(drop_from(&mut c.index, |_| true), half);
        assert!(c.index.is_empty());
    }
}
