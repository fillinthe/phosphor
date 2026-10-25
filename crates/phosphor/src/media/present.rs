//! "Is this file there?" for code that draws every frame: answers from a cache and checks the disk on a thread of
//! its own (at most every few seconds per file), so a slow or unplugged drive never holds up the show.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// How old an answer may get before the file is looked at again.
const FRESH: Duration = Duration::from_secs(5);

struct Cache {
    known: HashMap<String, (bool, Instant)>,
    asked: HashSet<String>,
    tx: Sender<String>,
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let (tx, rx) = channel::<String>();
        let _ = std::thread::Builder::new().name("file-present".into()).spawn(move || {
            for path in rx {
                let here = std::path::Path::new(&path).exists() || super::sequence::exists(&path);
                if let Some(c) = CACHE_REF.get()
                    && let Ok(mut c) = c.lock()
                {
                    c.asked.remove(&path);
                    c.known.insert(path, (here, Instant::now()));
                }
            }
        });
        Mutex::new(Cache { known: HashMap::new(), asked: HashSet::new(), tx })
    })
}

/// The same cache, for the checking thread (set once `cache()` exists).
static CACHE_REF: OnceLock<&'static Mutex<Cache>> = OnceLock::new();

/// Is the file at `path` there? `None` until it has been looked at once. Never touches the disk itself.
pub fn file_present(path: &str) -> Option<bool> {
    let c = cache();
    let _ = CACHE_REF.set(c);
    let Ok(mut c) = c.lock() else { return None };
    let known = c.known.get(path).copied();
    let stale = known.is_none_or(|(_, at)| at.elapsed() > FRESH);
    if stale && !c.asked.contains(path) {
        c.asked.insert(path.to_string());
        let _ = c.tx.send(path.to_string());
    }
    known.map(|(here, _)| here)
}

#[cfg(test)]
mod tests {
    #[test]
    fn answers_come_from_a_thread() {
        let here = std::env::temp_dir().join(format!("phosphor-present-{}", std::process::id()));
        std::fs::write(&here, b"x").unwrap();
        let p = here.to_string_lossy().into_owned();
        let gone = format!("{p}-not-there");
        let t = std::time::Instant::now();
        let (mut a, mut b) = (None, None);
        while (a.is_none() || b.is_none()) && t.elapsed() < std::time::Duration::from_secs(5) {
            a = super::file_present(&p);
            b = super::file_present(&gone);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let _ = std::fs::remove_file(&here);
        assert_eq!((a, b), (Some(true), Some(false)));
    }
}
