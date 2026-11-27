//! Project saves off the UI thread. A save makes a backup of the file that's there, writes the new one whole and
//! keeps this computer's copy; on a slow or sleeping drive any of those can take many seconds (a report,
//! 2026-11-26: a backup copy sat 20 s in the file system and the watchdog restarted Phosphor), so one writer thread
//! of its own does them in order and says how each went. Saves are never dropped: when several for the same file
//! wait, only the newest is written. The UI thread hands over the project itself (a shared copy, `State::shared_comp`):
//! its text and hash are made here, so neither Save nor autosave builds a show's JSON on the UI thread.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

/// One save asked for (`ProjectSaver::save`).
pub struct Request {
    pub path: String,
    /// The project as it was when asked (shared, never copied for this); its text is `Composition::to_json`, as Save
    /// has always written it.
    pub project: Arc<phosphor_engine::Composition>,
    /// Made by autosave (its backups are kept apart).
    pub auto: bool,
    /// Unix seconds when asked (names the backup).
    pub secs: u64,
    /// `History::generation` when asked: the project is saved as of then (handed back in `Done`).
    pub generation: u64,
    /// Not written when its text hashes to this (autosave: what was last saved; an unchanged project is left alone).
    pub unless: Option<u64>,
}

/// One save being written: its text made.
pub struct Job {
    pub path: String,
    pub json: String,
    pub auto: bool,
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
    /// Nothing written: an autosave of a project that hasn't changed since it was saved (`Request::unless`).
    Unchanged,
}

/// A finished save: which file, the hash of what was written (or found unchanged), the project's generation it
/// holds, and how it went.
pub struct Done {
    pub path: String,
    pub hash: u64,
    pub generation: u64,
    pub auto: bool,
    pub outcome: Outcome,
    /// Which `save` it answers (and every earlier one for the same file it replaced).
    id: u64,
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
    worker: Option<(Sender<(u64, Request)>, Receiver<Done>, std::thread::JoinHandle<()>)>,
    /// Results still to be read after the writer stopped (quit, or no thread could be started).
    leftover: Option<Receiver<Done>>,
    /// Saves handed over and not yet back: (path, id, when asked).
    pending: Vec<(String, u64, Instant)>,
    /// The id of the next save.
    next_id: u64,
    /// What a save does (None = `save_now`; tests give their own, so nothing lands in the settings folder).
    write: Option<fn(&Job) -> Outcome>,
    /// The save (by when it was asked) already said to be slow.
    told: Option<Instant>,
}

impl ProjectSaver {
    /// Hands a save to the writer (started on first use).
    pub fn save(&mut self, request: Request) {
        let id = self.next_id;
        self.next_id += 1;
        self.pending.push((request.path.clone(), id, Instant::now()));
        if self.worker.is_none() {
            let (tx, rx) = std::sync::mpsc::channel::<(u64, Request)>();
            let (done_tx, done_rx) = std::sync::mpsc::channel::<Done>();
            let write = self.write.unwrap_or(save_now);
            match std::thread::Builder::new().name("project-save".into()).spawn(move || run(&rx, &done_tx, write)) {
                Ok(handle) => self.worker = Some((tx, done_rx, handle)),
                Err(_) => {
                    // No thread to be had: saved here and now, as before.
                    let (done_tx, done_rx) = std::sync::mpsc::channel();
                    let _ = done_tx.send(write_one(id, request, &mut HashMap::new(), write));
                    self.leftover = Some(done_rx);
                    return;
                }
            }
        }
        if let Some((tx, _, _)) = &self.worker {
            let _ = tx.send((id, request));
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
            if let Some(i) = self.pending.iter().rposition(|(p, id, _)| *p == d.path && *id == d.id) {
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

/// Makes one request's text and writes it, unless it's an autosave of what that file already holds (`unless`, or
/// what this writer last wrote there). `written` = the hash last written per file.
fn write_one(id: u64, r: Request, written: &mut HashMap<String, u64>, write: fn(&Job) -> Outcome) -> Done {
    let json = r.project.to_json();
    let hash = crate::app::hash_text(&json);
    let unchanged = r.unless == Some(hash) || (r.auto && written.get(&r.path) == Some(&hash));
    let outcome = if unchanged {
        Outcome::Unchanged
    } else {
        let job = Job { path: r.path.clone(), json, auto: r.auto, secs: r.secs };
        let outcome = write(&job);
        if matches!(outcome, Outcome::Saved { .. } | Outcome::KeptHere) {
            written.insert(r.path.clone(), hash);
        }
        outcome
    };
    Done { path: r.path, hash, generation: r.generation, auto: r.auto, outcome, id }
}

/// The writer: takes everything waiting, keeps the newest save per file (in the order first asked), writes them.
fn run(rx: &Receiver<(u64, Request)>, done: &Sender<Done>, write: fn(&Job) -> Outcome) {
    let mut written = HashMap::new();
    while let Ok(first) = rx.recv() {
        let mut jobs = vec![first];
        for (id, r) in rx.try_iter() {
            match jobs.iter_mut().find(|(_, j)| j.path == r.path) {
                Some(j) => *j = (id, r),
                None => jobs.push((id, r)),
            }
        }
        for (id, r) in jobs {
            if done.send(write_one(id, r, &mut written, write)).is_err() {
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

    fn write(job: &Job) -> Outcome {
        match crate::project_files::write_whole(Path::new(&job.path), &job.json) {
            Ok(()) => Outcome::Saved { backup: None },
            Err(e) => Outcome::Failed(e.to_string()),
        }
    }

    /// A project told apart by its notes.
    fn show(notes: &str) -> Arc<phosphor_engine::Composition> {
        Arc::new(phosphor_engine::Composition { notes: notes.into(), ..Default::default() })
    }

    fn hash(notes: &str) -> u64 {
        crate::app::hash_text(&show(notes).to_json())
    }

    fn text(path: &str, notes: &str, auto: bool, unless: Option<u64>) -> Request {
        Request { path: path.into(), project: show(notes), auto, secs: 1_000, generation: 7, unless }
    }

    #[test]
    fn saves_are_written_in_order_and_the_newest_wins() {
        let d = dir("order");
        let path = d.join("Show.phosphor").to_string_lossy().into_owned();
        let mut saver = ProjectSaver { write: Some(write), ..Default::default() };
        for (k, t) in ["1", "2", "3"].iter().enumerate() {
            saver.save(Request { secs: 1_000 + k as u64, ..text(&path, t, false, None) });
            assert!(!saver.pending.is_empty());
        }
        assert!(saver.finish(Duration::from_secs(10)));
        let done = saver.take_done();
        assert!(!done.is_empty() && done.iter().all(|d| matches!(d.outcome, Outcome::Saved { backup: None })));
        assert_eq!(done.last().unwrap().hash, hash("3"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), show("3").to_json());
        assert!(saver.pending.is_empty(), "nothing left on its way");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The UI thread hands over the project; its text (the same as Save always wrote) and hash are made by the writer,
    /// and the generation it was asked at comes back with the answer.
    #[test]
    fn the_project_is_turned_into_text_on_the_writer() {
        let d = dir("project");
        let path = d.join("Show.phosphor").to_string_lossy().into_owned();
        let mut comp = phosphor_engine::Composition::opening();
        comp.notes = "doors at nine".into();
        let mut saver = ProjectSaver { write: Some(write), ..Default::default() };
        saver.save(Request { path: path.clone(), project: Arc::new(comp.clone()), auto: false, secs: 1, generation: 42, unless: None });
        assert!(saver.finish(Duration::from_secs(10)));
        let done = saver.take_done();
        assert_eq!(done.len(), 1);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), comp.to_json());
        assert_eq!((done[0].hash, done[0].generation), (crate::app::hash_text(&comp.to_json()), 42));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Autosave leaves a file alone when the project hasn't changed since it was saved (as when the UI thread made the
    /// hash itself), and a manual save always writes.
    #[test]
    fn an_unchanged_autosave_writes_nothing() {
        let d = dir("unchanged");
        let path = d.join("Show.phosphor").to_string_lossy().into_owned();
        let saved = hash("1");
        let mut saver = ProjectSaver { write: Some(write), ..Default::default() };
        // Unchanged since the last save: not written (the file isn't even made).
        saver.save(text(&path, "1", true, Some(saved)));
        assert!(saver.finish(Duration::from_secs(10)));
        let done = saver.take_done();
        assert_eq!(done[0].outcome, Outcome::Unchanged);
        assert_eq!((done[0].hash, done[0].generation), (saved, 7));
        assert!(!Path::new(&path).exists());
        // Changed: written.
        saver.save(text(&path, "2", true, Some(saved)));
        assert!(saver.finish(Duration::from_secs(10)));
        assert!(matches!(saver.take_done()[0].outcome, Outcome::Saved { .. }));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), show("2").to_json());
        // A manual save of the same text is written anyway.
        std::fs::remove_file(&path).unwrap();
        saver.save(text(&path, "2", false, None));
        assert!(saver.finish(Duration::from_secs(10)));
        assert!(matches!(saver.take_done()[0].outcome, Outcome::Saved { .. }));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), show("2").to_json());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// One writer remembers what it wrote: an autosave of exactly that is left alone.
    #[test]
    fn an_autosave_of_what_was_just_written_is_left_alone() {
        let mut written = HashMap::new();
        let first = write_one(0, text("/nowhere/Show.phosphor", "2", false, None), &mut written, |_| Outcome::Saved { backup: None });
        assert!(matches!(first.outcome, Outcome::Saved { .. }));
        let again = write_one(1, text("/nowhere/Show.phosphor", "2", true, Some(0)), &mut written, |_| panic!("not written"));
        assert_eq!(again.outcome, Outcome::Unchanged);
        let other = write_one(2, text("/nowhere/Show.phosphor", "3", true, Some(0)), &mut written, |_| Outcome::Saved { backup: None });
        assert!(matches!(other.outcome, Outcome::Saved { .. }));
    }
}
