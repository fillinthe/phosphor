//! Everything Phosphor keeps on this computer to be quick and to work with a drive away: copies of clips from other
//! drives (`media::clip_cache`, the big one), thumbnails (`thumbs`), project pictures and project copies. This is
//! its housekeeping: how much there is, clearing it, and the clean-up at launch of what hasn't been used for a
//! while (Cache settings, in the brightness menu).

use std::path::{Path, PathBuf};
use std::sync::Mutex;
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

fn dir() -> Option<PathBuf> {
    Some(crate::show::storage_dir()?.join("cache"))
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
            let bytes = dir().map_or(0, |d| size_of(&d));
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
