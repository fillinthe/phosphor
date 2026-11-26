//! Project saves off the UI thread. A save makes a backup of the file that's there, writes the new one whole and
//! keeps this computer's copy; on a slow or sleeping drive any of those can take many seconds (a report,
//! 2026-11-26: a backup copy sat 20 s in the file system and the watchdog restarted Phosphor), so one writer thread
//! of its own does them in order and says how each went. Saves are never dropped: when several for the same file
//! wait, only the newest is written.

use std::path::Path;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

/// One save asked for.
pub struct Job {
    pub path: String,
    pub json: String,
    /// Made by autosave (its backups are kept apart).
    pub auto: bool,
    /// Unix seconds when asked (names the backup).
    pub secs: u64,
}

/// How a save went.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// Written; the backup's error if one couldn't be made (the save still went ahead).
    Saved { backup: Option<String> },
    /// The file's folder isn't there (its drive unplugged): kept on this computer instead.
    KeptHere,
    Failed(String),
}

/// A finished save: which file, the hash of what was written, and how it went.
pub struct Done {
    pub path: String,
    pub hash: u64,
    pub auto: bool,
    pub outcome: Outcome,
}

/// Does one save, the way Save always has (runs on the writer thread; tests call it directly).
pub fn save_now(job: &Job) -> Outcome {
    let path = Path::new(&job.path);
    // Its folder isn't there (the drive unplugged): the work goes into this computer's copy, and nobody is
    // stopped. The next save with the drive back writes the file.
    let away = !path.parent().is_some_and(|d| d.as_os_str().is_empty() || d.exists());
    if away {
        return match crate::project_files::keep_copy(&job.path, &job.json, true) {
            Ok(()) => Outcome::KeptHere,
            Err(e) => Outcome::Failed(e.to_string()),
        };
    }
    // A backup that can't be made (a read-only backups folder, a full drive) is said, but never stops the save.
    let backup = crate::backups::back_up(path, job.secs, job.auto).err().map(|e| e.to_string());
    match crate::project_files::write_whole(path, &job.json) {
        Ok(()) => {
            let _ = crate::project_files::keep_copy(&job.path, &job.json, false);
            Outcome::Saved { backup }
        }
        Err(e) => Outcome::Failed(e.to_string()),
    }
}

/// The writer thread and what it has handed back.
#[derive(Default)]
pub struct ProjectSaver {
    worker: Option<(Sender<Job>, Receiver<Done>, std::thread::JoinHandle<()>)>,
    /// Results still to be read after the writer stopped (quit, or no thread could be started).
    leftover: Option<Receiver<Done>>,
    /// Saves handed over and not yet back: (path, hash, when asked).
    pending: Vec<(String, u64, Instant)>,
    /// What a save does (None = `save_now`; tests give their own, so nothing lands in the settings folder).
    write: Option<fn(&Job) -> Outcome>,
    /// The save (by when it was asked) already said to be slow.
    told: Option<Instant>,
}

impl ProjectSaver {
    /// Hands a save to the writer (started on first use).
    pub fn save(&mut self, job: Job, hash: u64) {
        if self.worker.is_none() {
            let (tx, rx) = std::sync::mpsc::channel::<Job>();
            let (done_tx, done_rx) = std::sync::mpsc::channel::<Done>();
            let write = self.write.unwrap_or(save_now);
            let Ok(handle) = std::thread::Builder::new().name("project-save".into()).spawn(move || run(&rx, &done_tx, write)) else {
                // No thread to be had: saved here and now, as before.
                let outcome = write(&job);
                let (done_tx, done_rx) = std::sync::mpsc::channel();
                let _ = done_tx.send(Done { path: job.path.clone(), hash, auto: job.auto, outcome });
                self.pending.push((job.path, hash, Instant::now()));
                self.worker = None;
                self.leftover = Some(done_rx);
                return;
            };
            self.worker = Some((tx, done_rx, handle));
        }
        self.pending.push((job.path.clone(), hash, Instant::now()));
        if let Some((tx, _, _)) = &self.worker {
            let _ = tx.send(job);
        }
    }

    /// Saves finished since last asked (call once a frame).
    pub fn take_done(&mut self) -> Vec<Done> {
        let mut out = Vec::new();
        if let Some(rx) = &self.leftover {
            out.extend(rx.try_iter());
        }
        if let Some((_, rx, _)) = &self.worker {
            out.extend(rx.try_iter());
        }
        for d in &out {
            // Saves for this file asked before (or as) this one are settled by it.
            if let Some(i) = self.pending.iter().rposition(|(p, h, _)| *p == d.path && *h == d.hash) {
                let mut k = 0;
                self.pending.retain(|(p, _, _)| {
                    let keep = !(k <= i && *p == d.path);
                    k += 1;
                    keep
                });
            }
        }
        out
    }

    /// A save of `path` with this content is on its way (so autosave doesn't ask again).
    pub fn writing(&self, path: &str, hash: u64) -> bool {
        self.pending.iter().any(|(p, h, _)| p == path && *h == hash)
    }

    /// The file of a save that has now taken longer than `after`: once per save (to say it's still going).
    pub fn newly_slow(&mut self, after: Duration) -> Option<String> {
        let (path, _, at) = self.pending.first()?;
        if at.elapsed() < after || self.told == Some(*at) {
            return None;
        }
        self.told = Some(*at);
        Some(path.clone())
    }

    /// Waits up to `limit` for every save handed over to be written (quit, bench). False when time ran out.
    pub fn finish(&mut self, limit: Duration) -> bool {
        let Some((tx, rx, handle)) = self.worker.take() else { return true };
        drop(tx);
        let until = Instant::now() + limit;
        while !handle.is_finished() {
            if Instant::now() >= until {
                // (The thread goes on by itself; the process may end under it.)
                self.worker = None;
                self.leftover = Some(rx);
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = handle.join();
        self.leftover = Some(rx);
        true
    }
}

/// The writer: takes everything waiting, keeps the newest save per file (in the order first asked), writes them.
fn run(rx: &Receiver<Job>, done: &Sender<Done>, write: fn(&Job) -> Outcome) {
    while let Ok(first) = rx.recv() {
        let mut jobs = vec![first];
        for job in rx.try_iter() {
            match jobs.iter_mut().find(|j| j.path == job.path) {
                Some(j) => *j = job,
                None => jobs.push(job),
            }
        }
        for job in jobs {
            let outcome = write(&job);
            let hash = crate::app::hash_text(&job.json);
            if done.send(Done { path: job.path, hash, auto: job.auto, outcome }).is_err() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("phosphor-project-save-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn saves_are_written_in_order_and_the_newest_wins() {
        let d = dir("order");
        let path = d.join("Show.phosphor").to_string_lossy().into_owned();
        fn write(job: &Job) -> Outcome {
            match crate::project_files::write_whole(Path::new(&job.path), &job.json) {
                Ok(()) => Outcome::Saved { backup: None },
                Err(e) => Outcome::Failed(e.to_string()),
            }
        }
        let mut saver = ProjectSaver { write: Some(write), ..Default::default() };
        for (k, text) in ["{\"v\":1}", "{\"v\":2}", "{\"v\":3}"].iter().enumerate() {
            let hash = crate::app::hash_text(text);
            saver.save(Job { path: path.clone(), json: (*text).into(), auto: false, secs: 1_000 + k as u64 }, hash);
            assert!(saver.writing(&path, hash));
        }
        assert!(saver.finish(Duration::from_secs(10)));
        let done = saver.take_done();
        assert!(!done.is_empty() && done.iter().all(|d| matches!(d.outcome, Outcome::Saved { backup: None })));
        assert_eq!(done.last().unwrap().hash, crate::app::hash_text("{\"v\":3}"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"v\":3}");
        assert!(!saver.writing(&path, crate::app::hash_text("{\"v\":3}")), "nothing left on its way");
        let _ = std::fs::remove_dir_all(&d);
    }
}
