//! A job on a thread of its own that the screen watches: what it's doing, bytes done of how many, a Cancel the
//! screen can press, and what it made. Export Show, packed shows and shows sent between Phosphors use it.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub step: String,
    pub done: bool,
    pub error: Option<String>,
    /// What it made (the exported app, the packed file, the opened project).
    pub output: Option<PathBuf>,
    /// Export Show: the zip, if made.
    pub zip: Option<PathBuf>,
    pub media_files: usize,
    /// Bytes done so far, of `total` (0 = not known).
    pub bytes: u64,
    pub total: u64,
    /// Set by the screen: the job stops at its next chunk.
    pub cancel: bool,
}

impl Progress {
    /// How far along (0..1), when the total is known.
    pub fn fraction(&self) -> Option<f32> {
        (self.total > 0).then(|| (self.bytes as f64 / self.total as f64).clamp(0.0, 1.0) as f32)
    }
}

pub type Shared = Arc<Mutex<Progress>>;

/// What a canceled job ends with (said nowhere as an error).
pub const CANCELED: &str = "Canceled";

/// Runs `work` on a thread named `name`; the returned progress says when it's done (and how it went).
pub fn spawn(name: &str, work: impl FnOnce(&Shared) -> Result<(), String> + Send + 'static) -> Shared {
    let p: Shared = Default::default();
    let progress = p.clone();
    let started = std::thread::Builder::new().name(name.into()).spawn(move || {
        let result = work(&progress);
        let mut g = progress.lock().unwrap();
        g.done = true;
        // (Done: the last step stays, e.g. "Sent to …".)
        if let Err(e) = result {
            g.error = Some(e);
        }
    });
    if let Err(e) = started {
        let mut g = p.lock().unwrap();
        g.done = true;
        g.error = Some(e.to_string());
    }
    p
}

pub fn step(p: &Shared, s: impl Into<String>) {
    p.lock().unwrap().step = s.into();
}

pub fn canceled(p: &Shared) -> bool {
    p.lock().unwrap().cancel
}

pub fn cancel(p: &Shared) {
    p.lock().unwrap().cancel = true;
}

/// Copies `from` into `to` a megabyte at a time, counting into the progress; stops (Err(CANCELED)) when canceled.
pub fn copy(from: &mut impl Read, to: &mut impl Write, p: &Shared) -> Result<u64, String> {
    let mut buf = vec![0u8; 1 << 20];
    let mut n = 0u64;
    loop {
        let got = match from.read(&mut buf) {
            Ok(0) => return Ok(n),
            Ok(got) => got,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.to_string()),
        };
        to.write_all(&buf[..got]).map_err(|e| e.to_string())?;
        n += got as u64;
        let mut g = p.lock().unwrap();
        g.bytes += got as u64;
        if g.cancel {
            return Err(CANCELED.into());
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_copy_counts_and_stops_when_canceled() {
        let p = super::spawn("job-test", |_| Ok(()));
        while !p.lock().unwrap().done {
            std::thread::yield_now();
        }
        let p: super::Shared = Default::default();
        let data = vec![7u8; 3 << 20];
        let mut out = Vec::new();
        assert_eq!(super::copy(&mut &data[..], &mut out, &p).unwrap(), data.len() as u64);
        assert_eq!(p.lock().unwrap().bytes, data.len() as u64);
        super::cancel(&p);
        assert_eq!(super::copy(&mut &data[..], &mut Vec::new(), &p).unwrap_err(), super::CANCELED);
    }
}
