//! MIDI clock out: 24 pulses a beat to one MIDI output, locked to Phosphor's beat (drum machines, sequencers, other
//! apps follow the tempo). A thread of its own sends the pulses on time; the app tells it where the beat is each frame.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Where the beat is: at `at`, `beats` beats had gone by, going at `bpm`.
#[derive(Clone, Copy)]
struct Beat {
    at: Instant,
    beats: f64,
    bpm: f64,
}

pub struct ClockOut {
    beat: Arc<Mutex<Beat>>,
    stop: Arc<AtomicBool>,
    /// The output it sends to.
    pub port: String,
}

const PPQ: f64 = 24.0;

impl ClockOut {
    /// Starts sending to the MIDI output named `port` (a Start first, then the pulses).
    pub fn start(port: &str, beats: f64, bpm: f64) -> Result<Self, String> {
        let out = midir::MidiOutput::new("Phosphor clock").map_err(|e| e.to_string())?;
        let p = out.ports().into_iter().find(|p| out.port_name(p).is_ok_and(|n| n == port)).ok_or_else(|| format!("{port} isn't connected"))?;
        let mut conn = out.connect(&p, "phosphor-clock").map_err(|e| e.to_string())?;
        let beat = Arc::new(Mutex::new(Beat { at: Instant::now(), beats, bpm }));
        let stop = Arc::new(AtomicBool::new(false));
        let (b, s) = (beat.clone(), stop.clone());
        std::thread::Builder::new()
            .name("phosphor-midi-clock".into())
            .spawn(move || {
                let _ = conn.send(&[0xFA]);
                let mut sent: Option<i64> = None;
                while !s.load(Ordering::Relaxed) {
                    let Beat { at, beats, bpm } = *b.lock().unwrap();
                    let now = beats + at.elapsed().as_secs_f64() * bpm / 60.0;
                    let due = (now * PPQ).floor() as i64;
                    let last = *sent.get_or_insert(due - 1);
                    // Way behind or ahead (a jump in the beat): carry on from here instead of a burst.
                    if (due - last).abs() > 48 {
                        sent = Some(due);
                    } else {
                        for _ in last..due {
                            let _ = conn.send(&[0xF8]);
                        }
                        if due > last {
                            sent = Some(due);
                        }
                    }
                    // Until the next pulse (a millisecond at least, so the thread never spins).
                    let next = ((due + 1) as f64 / PPQ - now) * 60.0 / bpm.max(1.0);
                    std::thread::sleep(Duration::from_secs_f64(next.clamp(0.001, 0.02)));
                }
                let _ = conn.send(&[0xFC]);
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { beat, stop, port: port.to_string() })
    }

    /// Where Phosphor's beat is now (call every frame).
    pub fn follow(&self, beats: f64, bpm: f64) {
        if let Ok(mut b) = self.beat.lock() {
            *b = Beat { at: Instant::now(), beats, bpm };
        }
    }
}

impl Drop for ClockOut {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Every MIDI output on the system, by name.
pub fn outputs() -> Vec<String> {
    midir::MidiOutput::new("Phosphor").map(|o| o.ports().iter().filter_map(|p| o.port_name(p).ok()).collect()).unwrap_or_default()
}
