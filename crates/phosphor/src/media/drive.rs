//! Clips on another drive (an external SSD, a network share: anything under /Volumes on a Mac) can go away while
//! they play: the drive is unplugged or unmounts. A movie's decoder and its sound ask here whether a read that
//! failed was that (rather than the end of the file or a damaged one), and where the file can be read now: this
//! Mac's copy (`clip_cache`), the file itself once its drive is back, or the same file on its drive mounted again
//! under a new name ("_FOOTAGE 1"). Everything here looks at the disk: call it from a worker thread (a decoder,
//! the sound, a movie opening), never the UI thread.

use std::path::Path;
use std::time::{Duration, Instant};

/// After a read error, how long macOS may take to take an unplugged drive's files away.
const SETTLE: Duration = Duration::from_secs(1);

/// Is `path` on another drive (one that can be unplugged), rather than this computer's own disk?
pub fn on_other_drive(path: &str) -> bool {
    (cfg!(target_os = "macos") && path.starts_with("/Volumes/") && !super::clip_cache::boot_volume(path)) || pretend::covers(path)
}

/// A read of `opened` (a file on another drive) failed (`io_error`) or came to its end: did its drive go away?
/// Only when the file is no longer there: a file that's still there and fails is damaged, and one that ends has
/// ended. After a read error the file gets `SETTLE` to go (the drive may still be on its way out).
pub fn went(opened: &str, io_error: bool) -> bool {
    gone_within(opened, if io_error { SETTLE } else { Duration::ZERO })
}

fn gone_within(opened: &str, wait: Duration) -> bool {
    let until = Instant::now() + wait;
    loop {
        if !Path::new(opened).is_file() {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Where a file can be read now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub path: String,
    /// It's this Mac's copy (it plays on whatever the drive does).
    pub copy: bool,
}

/// Where `path` can be read now: this Mac's copy of it, else the file where it was, else on its drive mounted
/// again under a new name. None while its drive is away and nothing was copied.
pub fn readable(path: &str) -> Option<Found> {
    readable_with(path, super::clip_cache::copy_of, Path::new("/Volumes"))
}

/// Where `path` (a file on another drive) is now if its drive was mounted again under a new name ("_FOOTAGE 1").
pub fn moved(path: &str) -> Option<String> {
    crate::project_files::relocated(path)
}

/// `readable`, with the copy lookup and the place drives are mounted given (tests).
fn readable_with(path: &str, copy: impl Fn(&str) -> Option<String>, volumes: &Path) -> Option<Found> {
    if let Some(c) = copy(path) {
        return Some(Found { path: c, copy: true });
    }
    let here = Path::new(path).is_file().then(|| path.to_string());
    here.or_else(|| crate::project_files::relocated_in(volumes, path)).map(|path| Found { path, copy: false })
}

/// Tests count folders of their own as other drives (a real one can't be unplugged in a test).
#[cfg(test)]
pub(crate) mod pretend {
    use std::sync::Mutex;

    static FOLDERS: Mutex<Vec<String>> = Mutex::new(Vec::new());

    pub(crate) fn add(folder: &std::path::Path) {
        FOLDERS.lock().unwrap().push(folder.to_string_lossy().into_owned());
    }

    pub(super) fn covers(path: &str) -> bool {
        FOLDERS.lock().unwrap().iter().any(|f| path.starts_with(f.as_str()))
    }
}

#[cfg(not(test))]
mod pretend {
    pub(super) fn covers(_: &str) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("phosphor-drive-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn only_a_file_that_is_gone_went_with_its_drive() {
        let dir = folder("went");
        let clip = dir.join("clip.mov");
        std::fs::write(&clip, b"movie").unwrap();
        let clip = clip.to_string_lossy().into_owned();
        // Still there: a read error is a damaged file (after waiting for it to go), an end is the end.
        let started = Instant::now();
        assert!(!went(&clip, true), "a file still there that fails is damaged, not unplugged");
        assert!(started.elapsed() >= SETTLE, "a read error gives the drive time to go");
        assert!(!went(&clip, false));
        std::fs::remove_file(&clip).unwrap();
        let started = Instant::now();
        assert!(went(&clip, true) && went(&clip, false));
        assert!(started.elapsed() < SETTLE, "a file already gone is known at once");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_clip_is_read_from_its_copy_then_its_drive_then_the_drive_under_a_new_name() {
        let volumes = folder("volumes");
        let saved = volumes.join("_FOOTAGE/loops/a.mov");
        let saved = saved.to_string_lossy().into_owned();
        let copy_dir = folder("copy");
        let copy = copy_dir.join("0123.mov").to_string_lossy().into_owned();
        let no_copy = |_: &str| None;
        let with_copy = |_: &str| Some(copy.clone());
        // The drive is away and nothing was copied: nowhere (it waits for the drive).
        assert_eq!(readable_with(&saved, no_copy, &volumes), None);
        // A copy plays whatever the drive does.
        assert_eq!(readable_with(&saved, with_copy, &volumes), Some(Found { path: copy.clone(), copy: true }));
        // The drive came back under a new name (macOS: the old name was still taken).
        std::fs::create_dir_all(volumes.join("_FOOTAGE 1/loops")).unwrap();
        std::fs::write(volumes.join("_FOOTAGE 1/loops/a.mov"), b"movie").unwrap();
        let moved = readable_with(&saved, no_copy, &volumes).expect("found on the renamed drive");
        assert!(!moved.copy && moved.path.contains("_FOOTAGE 1"), "{moved:?}");
        // Back under its own name: the file itself.
        std::fs::create_dir_all(volumes.join("_FOOTAGE/loops")).unwrap();
        std::fs::write(&saved, b"movie").unwrap();
        assert_eq!(readable_with(&saved, no_copy, &volumes), Some(Found { path: saved.clone(), copy: false }));
        assert_eq!(readable_with(&saved, with_copy, &volumes).map(|f| f.copy), Some(true), "the copy still comes first");
        let _ = std::fs::remove_dir_all(&volumes);
        let _ = std::fs::remove_dir_all(&copy_dir);
    }

    #[test]
    fn other_drives_are_the_ones_under_volumes() {
        assert_eq!(on_other_drive("/Volumes/WD Black/loops/a.mov"), cfg!(target_os = "macos"));
        assert!(!on_other_drive("/Users/me/Movies/a.mov"));
        let dir = folder("pretend");
        pretend::add(&dir);
        assert!(on_other_drive(&dir.join("a.mov").to_string_lossy()), "a test's own folder can stand in for a drive");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
