//! Everything Phosphor keeps on this computer to be quick and to work with a drive away: copies of clips from other
//! drives (`media::clip_cache`, the big one), thumbnails (`thumbs`), project pictures and project copies. This is
//! its housekeeping: where it lives, how much there is, clearing it, and the clean-up at launch of what hasn't been
//! used for a while (Cache settings, in the brightness menu).
//!
//! Clip copies and thumbnails (up to 25 GB, all made again from your files) live in the system's cache folder:
//! ~/Library/Caches/Phosphor, %LOCALAPPDATA%\Phosphor\cache, $XDG_CACHE_HOME/phosphor. They used to be in the
//! settings folder, which Time Machine backs up and a Windows roaming profile carries from PC to PC; the first run
//! moves them over (`folder`). Project copies and pictures stay with the settings: they're small, and they are what
//! opens a project whose drive is away.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime};

/// Days a clip copy or thumbnail may go unused before the clean-up at launch removes it, unless the user says
/// otherwise (0 = never).
pub const DAYS: u32 = 30;
static KEEP_DAYS: AtomicU32 = AtomicU32::new(DAYS);

pub fn days() -> u32 {
    KEEP_DAYS.load(Ordering::Relaxed)
}

pub fn set_days(days: u32) {
    KEEP_DAYS.store(days.min(3650), Ordering::Relaxed);
}

/// The settings folder's cache: project copies and pictures (and, before, everything).
fn settings_cache() -> Option<PathBuf> {
    Some(crate::show::storage_dir()?.join("cache"))
}

/// This system's folder for caches, one per settings folder (shows and profiles have their own).
fn system_cache() -> Option<PathBuf> {
    let name = crate::show::storage_name();
    let home = std::env::home_dir;
    if cfg!(target_os = "macos") {
        Some(home()?.join("Library").join("Caches").join(name.replace(|c: char| c.is_ascii_whitespace(), "-")))
    } else if cfg!(windows) {
        Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join(name).join("cache"))
    } else {
        let base = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).or_else(|| home().map(|h| h.join(".cache")))?;
        Some(base.join(name.to_lowercase().replace(|c: char| c.is_ascii_whitespace(), "")))
    }
}

/// The folders that move to the system's cache.
const MOVED: [&str; 2] = ["clips", "thumbs"];

/// Where cache folder `name` ("clips", "thumbs") is: the system's cache folder. The first call moves it there from
/// the settings folder (a rename: nothing is copied). If it can't be moved (another drive), it stays where it was and
/// is used there. Old leftovers beside a folder that already moved are removed on a thread of their own.
pub fn folder(name: &str) -> Option<PathBuf> {
    static FOLDERS: OnceLock<Vec<(&'static str, PathBuf)>> = OnceLock::new();
    let all = FOLDERS.get_or_init(|| {
        let (Some(old), Some(new)) = (settings_cache(), system_cache()) else {
            return settings_cache().map(|old| MOVED.iter().map(|n| (*n, old.join(n))).collect()).unwrap_or_default();
        };
        MOVED.iter().map(|n| (*n, move_once(&old.join(n), &new.join(n)))).collect()
    });
    all.iter().find(|(n, _)| *n == name).map(|(_, p)| p.clone())
}

/// Moves `old` to `new` if only `old` is there; returns the folder to use.
fn move_once(old: &Path, new: &Path) -> PathBuf {
    if !old.exists() {
        return new.to_path_buf();
    }
    if new.exists() {
        // Moved before; what's left in the old place is stale (a run of an older version since).
        let stale = old.to_path_buf();
        let _ = std::thread::Builder::new().name("cache-old".into()).spawn(move || {
            let bytes = size_of(&stale);
            if std::fs::remove_dir_all(&stale).is_ok() {
                log::info!("cache: removed {} left in the settings folder ({})", size_text(bytes), stale.display());
            }
        });
        return new.to_path_buf();
    }
    let moved = new.parent().is_some_and(|p| std::fs::create_dir_all(p).is_ok()) && std::fs::rename(old, new).is_ok();
    if moved {
        log::info!("cache: moved {} to {}", old.display(), new.display());
        new.to_path_buf()
    } else {
        log::warn!("cache: couldn't move {} to {}: kept where it is", old.display(), new.display());
        old.to_path_buf()
    }
}

/// Finds the cache's folders now, off the UI thread (the first look moves them from the settings folder).
pub fn prepare() {
    let _ = std::thread::Builder::new().name("cache-folders".into()).spawn(|| {
        let _ = folder("clips");
    });
}

/// The size of everything under `dir`.
fn size_of(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries.flatten().map(|e| match e.metadata() {
        Ok(m) if m.is_dir() => size_of(&e.path()),
        Ok(m) => m.len(),
        Err(_) => 0,
    }).sum()
}

/// (bytes, when it was measured); None = not measured yet.
static USAGE: Mutex<Option<(u64, Instant)>> = Mutex::new(None);
static MEASURING: AtomicBool = AtomicBool::new(false);

/// How much the cache holds, measured on a thread of its own at most every few seconds (a settings row draws every
/// frame). None until the first measure is in.
pub fn usage() -> Option<u64> {
    let known = *USAGE.lock().ok()?;
    if known.is_none_or(|(_, at)| at.elapsed() > Duration::from_secs(5)) && !MEASURING.swap(true, Ordering::Relaxed) {
        let _ = std::thread::Builder::new().name("cache-size".into()).spawn(|| {
            // The settings folder's cache, and the moved folders (one that couldn't move is in there: counted once).
            let kept = settings_cache();
            let elsewhere: u64 = MOVED.iter().filter_map(|n| folder(n)).filter(|d| kept.as_ref().is_none_or(|k| !d.starts_with(k))).map(|d| size_of(&d)).sum();
            let bytes = kept.map_or(0, |k| size_of(&k)) + elsewhere;
            if let Ok(mut u) = USAGE.lock() {
                *u = Some((bytes, Instant::now()));
            }
            MEASURING.store(false, Ordering::Relaxed);
        });
    }
    known.map(|(bytes, _)| bytes)
}

fn forget_usage() {
    if let Ok(mut u) = USAGE.lock() {
        *u = None;
    }
}

/// "25.1 GB", "104 MB".
pub fn size_text(bytes: u64) -> String {
    let mb = bytes as f64 / (1 << 20) as f64;
    if mb >= 1000.0 { format!("{:.1} GB", mb / 1024.0) } else { format!("{mb:.0} MB") }
}

/// Clears the clip copies and the thumbnails (on a thread: it's files to delete). Project copies and pictures stay:
/// they're what opens a project with its drive away, and they're small.
pub fn clear() {
    let _ = std::thread::Builder::new().name("cache-clear".into()).spawn(|| {
        let freed = crate::media::clip_cache::clear();
        let thumbs = crate::thumbs::cache_dir().map_or(0, |d| remove_older(&d, None));
        log::info!("cache cleared: {} of clip copies, {} of thumbnails", size_text(freed), size_text(thumbs));
        forget_usage();
    });
}

/// The clean-up at launch: clip copies and thumbnails not used for `days()` go (0 = never).
pub fn sweep() {
    let days = days();
    if days == 0 {
        return;
    }
    let _ = std::thread::Builder::new().name("cache-sweep".into()).spawn(move || {
        let age = Duration::from_secs(days as u64 * 86_400);
        let clips = crate::media::clip_cache::drop_unused(age.as_secs());
        let thumbs = crate::thumbs::cache_dir().map_or(0, |d| remove_older(&d, Some(age)));
        if clips + thumbs > 0 {
            log::info!("cache clean-up: {} of clip copies and {} of thumbnails not used for {days} days", size_text(clips), size_text(thumbs));
        }
    });
}

/// Removes the files in `dir` (and its folders) last used longer ago than `age` (None = all). Returns the bytes freed.
fn remove_older(dir: &Path, age: Option<Duration>) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    let now = SystemTime::now();
    let mut freed = 0;
    for e in entries.flatten() {
        let Ok(m) = e.metadata() else { continue };
        if m.is_dir() {
            freed += remove_older(&e.path(), age);
            continue;
        }
        let old = age.is_none_or(|a| m.modified().ok().and_then(|t| now.duration_since(t).ok()).is_some_and(|d| d > a));
        if old && std::fs::remove_file(e.path()).is_ok() {
            freed += m.len();
        }
    }
    freed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cache_folder_moves_once_and_stays_put_when_it_cant() {
        let root = std::env::temp_dir().join(format!("phosphor-cache-move-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (old, new) = (root.join("settings/cache/thumbs"), root.join("Caches/Phosphor/thumbs"));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("a.png"), b"12").unwrap();
        assert_eq!(move_once(&old, &new), new);
        assert!(!old.exists() && std::fs::read(new.join("a.png")).unwrap() == b"12", "moved whole");
        // Nothing to move: the new place.
        assert_eq!(move_once(&old, &new), new);
        // Left behind by an older version after the move: the new place wins, the old one goes.
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("b.png"), b"3").unwrap();
        assert_eq!(move_once(&old, &new), new);
        let until = std::time::Instant::now() + Duration::from_secs(5);
        while old.exists() && std::time::Instant::now() < until {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!old.exists() && new.join("a.png").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_what_went_unused_is_removed() {
        let dir = std::env::temp_dir().join(format!("phosphor-cache-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("last")).unwrap();
        std::fs::write(dir.join("new.png"), b"12345").unwrap();
        std::fs::write(dir.join("old.png"), b"123").unwrap();
        std::fs::write(dir.join("last").join("note"), b"1").unwrap();
        let long_ago = SystemTime::now() - Duration::from_secs(40 * 86_400);
        std::fs::File::options().write(true).open(dir.join("old.png")).unwrap().set_modified(long_ago).unwrap();
        assert_eq!(size_of(&dir), 9);
        assert_eq!(remove_older(&dir, Some(Duration::from_secs(30 * 86_400))), 3);
        assert!(dir.join("new.png").exists() && !dir.join("old.png").exists());
        // Clear takes the rest, folders within included.
        assert_eq!(remove_older(&dir, None), 6);
        assert_eq!(size_of(&dir), 0);
        assert_eq!(size_text(104 << 20), "104 MB");
        assert_eq!(size_text(25 << 30), "25.0 GB");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
