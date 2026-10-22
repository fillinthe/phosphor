//! MIDI timecode out: the show clock (or the time of day) as MTC at 25 fps to one MIDI output, for lighting desks,
//! DAWs and other video apps. A thread of its own sends four quarter-frame messages a frame; a full-frame message
//! goes first and after every jump.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Frames a second (MTC rate code 1).
const FPS: f64 = 25.0;
const RATE_CODE: u8 = 1;

/// Hours, minutes, seconds, frames of `secs` (wrapping at 24 hours).
pub fn hmsf(secs: f64) -> [u8; 4] {
    let frames = (secs.max(0.0) * FPS).floor() as u64;
    let f = frames % FPS as u64;
    let s = frames / FPS as u64;
    [((s / 3600) % 24) as u8, ((s / 60) % 60) as u8, (s % 60) as u8, f as u8]
}

/// The full-frame SysEx for a time (sent at the start and after a jump).
pub fn full_frame(t: [u8; 4]) -> [u8; 10] {
    [0xF0, 0x7F, 0x7F, 0x01, 0x01, (RATE_CODE << 5) | t[0], t[1], t[2], t[3], 0xF7]
}

/// Quarter-frame piece `k` (0..8) of a time: [0xF1, data].
pub fn quarter_frame(k: u8, t: [u8; 4]) -> [u8; 2] {
    let v = match k {
        0 => t[3] & 0x0F,
        1 => t[3] >> 4,
        2 => t[2] & 0x0F,
        3 => t[2] >> 4,
        4 => t[1] & 0x0F,
        5 => t[1] >> 4,
        6 => t[0] & 0x0F,
        _ => ((t[0] >> 4) & 1) | (RATE_CODE << 1),
    };
    [0xF1, (k << 4) | v]
}

/// Where the time is: at `at` it was `secs`, running (or held).
#[derive(Clone, Copy)]
struct Clock {
    at: Instant,
    secs: f64,
    running: bool,
}

pub struct MtcOut {
    clock: Arc<Mutex<Clock>>,
    stop: Arc<AtomicBool>,
    pub port: String,
}

impl MtcOut {
    pub fn start(port: &str, secs: f64) -> Result<Self, String> {
        let out = midir::MidiOutput::new("Phosphor timecode").map_err(|e| e.to_string())?;
        let p = out.ports().into_iter().find(|p| out.port_name(p).is_ok_and(|n| n == port)).ok_or_else(|| format!("{port} isn't connected"))?;
        let mut conn = out.connect(&p, "phosphor-mtc").map_err(|e| e.to_string())?;
        let clock = Arc::new(Mutex::new(Clock { at: Instant::now(), secs, running: true }));
        let stop = Arc::new(AtomicBool::new(false));
        let (c, s) = (clock.clone(), stop.clone());
        std::thread::Builder::new()
            .name("phosphor-mtc".into())
            .spawn(move || {
                // Quarter frames sent so far, counted from zero time (4 a frame).
                let mut sent: Option<i64> = None;
                while !s.load(Ordering::Relaxed) {
                    let Clock { at, secs, running } = *c.lock().unwrap();
                    let now = secs + if running { at.elapsed().as_secs_f64() } else { 0.0 };
                    let due = (now * FPS * 4.0).floor() as i64;
                    match sent {
                        // First, or the time jumped (a cue, a stop): the whole time at once.
                        Some(last) if (due - last).abs() <= 8 && running => {
                            for q in (last + 1)..=due {
                                // Each group of eight describes the time at its start (two frames).
                                let start = q - q.rem_euclid(8);
                                let t = hmsf(start as f64 / (FPS * 4.0));
                                let _ = conn.send(&quarter_frame(q.rem_euclid(8) as u8, t));
                            }
                        }
                        _ => {
                            let _ = conn.send(&full_frame(hmsf(now)));
                        }
                    }
                    sent = Some(due);
                    std::thread::sleep(Duration::from_secs_f64(1.0 / (FPS * 4.0)));
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { clock, stop, port: port.to_string() })
    }

    /// Where the time is now (call every frame); `running` false holds it.
    pub fn follow(&self, secs: f64, running: bool) {
        if let Ok(mut c) = self.clock.lock() {
            *c = Clock { at: Instant::now(), secs, running };
        }
    }
}

impl Drop for MtcOut {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timecode_messages_carry_the_time() {
        let t = hmsf(3661.0 + 12.0 / 25.0);
        assert_eq!(t, [1, 1, 1, 12]);
        assert_eq!(full_frame(t), [0xF0, 0x7F, 0x7F, 0x01, 0x01, 0x21, 1, 1, 12, 0xF7], "25 fps code in the hours byte");
        // Frames 12 = 0x0C: piece 0 = low nibble, piece 1 = high nibble; piece 7 has the rate.
        assert_eq!(quarter_frame(0, t), [0xF1, 0x0C]);
        assert_eq!(quarter_frame(1, t), [0xF1, 0x10]);
        assert_eq!(quarter_frame(7, t), [0xF1, 0x72]);
        // Phosphor's own MTC reader gets the time back from the eight pieces.
        let mut m = crate::clock::Mtc::default();
        for k in 0..8 {
            m.quarter(quarter_frame(k, t)[1], 0.0);
        }
        assert!((m.time(0.0).unwrap() - (3661.0 + 12.0 / 25.0)).abs() < 0.2, "{:?}", m.time(0.0));
    }
}
