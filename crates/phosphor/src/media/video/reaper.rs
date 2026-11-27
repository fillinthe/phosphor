//! One thread that waits for worker threads to end (movie decoders, clip sound, NDI receivers) once their owners
//! are dropped. Each owner tells its thread to stop and hands the handle over here: dropping a player used to wait
//! on the UI thread for its threads (up to 60 ms each for the picture and the sound, unbounded for NDI), a hitch at
//! every clip change.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

/// How often threads that haven't ended yet are looked at again.
const LOOK_AGAIN: Duration = Duration::from_millis(25);

static REAPER: OnceLock<Option<Mutex<Sender<JoinHandle<()>>>>> = OnceLock::new();

/// Waits for `thread` (already told to stop) on the reaper thread. A thread that never ends (stuck reading a pulled
/// drive) just stays in its list; nothing else waits for it. If the reaper can't start, waits here briefly instead.
pub fn join_later(thread: JoinHandle<()>) {
    let reaper = REAPER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new().name("phosphor-reaper".into()).spawn(move || reap(&rx)).ok().map(|_| Mutex::new(tx))
    });
    let sent = match reaper {
        Some(tx) => tx.lock().unwrap_or_else(|e| e.into_inner()).send(thread).map_err(|e| e.0),
        None => Err(thread),
    };
    if let Err(thread) = sent {
        super::join_briefly(thread);
    }
}

/// The reaper: joins each thread once it has ended, sleeping when there's nothing to wait for.
fn reap(rx: &Receiver<JoinHandle<()>>) {
    let mut waiting: Vec<JoinHandle<()>> = Vec::new();
    loop {
        let next = if waiting.is_empty() { rx.recv().map_err(|_| RecvTimeoutError::Disconnected) } else { rx.recv_timeout(LOOK_AGAIN) };
        match next {
            Ok(t) => waiting.push(t),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let (done, still): (Vec<_>, Vec<_>) = waiting.into_iter().partition(JoinHandle::is_finished);
        waiting = still;
        for t in done {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn handing_over_a_thread_never_waits_for_it() {
        let stop = Arc::new(AtomicBool::new(false));
        let ended = Arc::new(AtomicBool::new(false));
        let (s, e) = (stop.clone(), ended.clone());
        let slow = std::thread::spawn(move || {
            while !s.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
            e.store(true, Ordering::Relaxed);
        });
        stop.store(true, Ordering::Relaxed);
        let started = std::time::Instant::now();
        super::join_later(slow);
        assert!(started.elapsed() < std::time::Duration::from_millis(50), "{:?}", started.elapsed());
        // It still ends, on its own time.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !ended.load(Ordering::Relaxed) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(ended.load(Ordering::Relaxed));
        // A stuck one doesn't hold up the next.
        super::join_later(std::thread::spawn(|| std::thread::sleep(std::time::Duration::from_secs(3))));
        let quick = std::time::Instant::now();
        super::join_later(std::thread::spawn(|| {}));
        assert!(quick.elapsed() < std::time::Duration::from_millis(50));
    }
}
