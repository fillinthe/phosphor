//! Phosphor's own sound (the movie-sound mix) into a stream or recording: FFmpeg reads it as raw stereo floats from a
//! local socket (`-f f32le -ar 48000 -ac 2 -i tcp://127.0.0.1:<port>`), fed on the clock. While nothing plays it sends
//! silence, so the audio never holds the stream up; the mix is resampled to 48 kHz when the Mac's output runs at
//! another rate, and never more than `MAX_LAG` behind (older sound is dropped rather than drifting).

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::media::video::sound;

/// What FFmpeg is told the sound is.
pub const RATE: u32 = 48_000;
/// At most this much mixed sound waits to be sent (seconds).
const MAX_LAG: f64 = 0.25;
const TICK: Duration = Duration::from_millis(10);

/// The FFmpeg input arguments for a feed on `port`.
pub fn input_args(port: u16) -> Vec<String> {
    ["-thread_queue_size", "1024", "-f", "f32le", "-ar", &RATE.to_string(), "-ac", "2", "-i", &format!("tcp://127.0.0.1:{port}")].map(String::from).to_vec()
}

/// The feed for one stream: dropping it stops the sound (FFmpeg then sees the end of its audio input).
pub struct ProgramFeed {
    _stop: mpsc::Sender<()>,
}

impl ProgramFeed {
    /// Listens on a free local port (pass it to `input_args`); FFmpeg connects once it starts. The sound starts
    /// `delay` seconds late: the picture takes that long to reach FFmpeg (rendered, read back, piped).
    pub fn start(delay: f64) -> std::io::Result<(ProgramFeed, u16)> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let (tx, rx) = mpsc::channel::<()>();
        sound::set_program_tap(true);
        std::thread::Builder::new().name("stream sound".into()).spawn(move || {
            if let Some(conn) = accept(&listener, &rx) {
                feed(conn, &rx, delay);
            }
            sound::set_program_tap(false);
        })?;
        Ok((ProgramFeed { _stop: tx }, port))
    }
}

fn stopped(rx: &mpsc::Receiver<()>) -> bool {
    matches!(rx.try_recv(), Err(mpsc::TryRecvError::Disconnected))
}

/// Waits (up to 15 s) for FFmpeg to connect, unless the stream stops first.
fn accept(listener: &TcpListener, rx: &mpsc::Receiver<()>) -> Option<TcpStream> {
    listener.set_nonblocking(true).ok()?;
    let until = Instant::now() + Duration::from_secs(15);
    while Instant::now() < until && !stopped(rx) {
        match listener.accept() {
            Ok((conn, _)) => {
                conn.set_nonblocking(false).ok()?;
                let _ = conn.set_nodelay(true);
                return Some(conn);
            }
            Err(_) => std::thread::sleep(TICK),
        }
    }
    None
}

fn feed(mut conn: TcpStream, rx: &mpsc::Receiver<()>, delay: f64) {
    let start = Instant::now();
    let mut sent: u64 = 0;
    // The delay is silence before the first sound: everything after it lands that much later.
    let mut pending: std::collections::VecDeque<f32> = std::iter::repeat_n(0.0, (delay.max(0.0) * RATE as f64) as usize * 2).collect();
    let mut resampler = Resampler::default();
    let mut bytes: Vec<u8> = Vec::new();
    sound::take_program();
    while !stopped(rx) {
        std::thread::sleep(TICK);
        let mixed = sound::take_program();
        let from = sound::stats().0;
        pending.extend(resampler.run(&mixed, if from == 0 { RATE } else { from }, RATE));
        let due = (start.elapsed().as_secs_f64() * RATE as f64) as u64;
        let frames = due.saturating_sub(sent) as usize;
        // Too far behind (the mix ran ahead of the clock): drop the oldest so sound stays with the picture.
        let max = (((MAX_LAG + delay) * RATE as f64) as usize + frames) * 2;
        while pending.len() > max {
            pending.drain(..2);
        }
        bytes.clear();
        for _ in 0..frames * 2 {
            bytes.extend_from_slice(&pending.pop_front().unwrap_or(0.0).to_le_bytes());
        }
        if conn.write_all(&bytes).is_err() {
            return;
        }
        sent += frames as u64;
    }
}

/// Linear stereo resampling that carries its position across blocks.
#[derive(Default)]
pub struct Resampler {
    /// Position in the input, relative to the start of the next block (can be just before it: -1..0 = after `last`).
    pos: f64,
    last: [f32; 2],
}

impl Resampler {
    pub fn run(&mut self, input: &[f32], from: u32, to: u32) -> Vec<f32> {
        if from == to {
            return input.to_vec();
        }
        let n = input.len() / 2;
        let step = from as f64 / to as f64;
        let at = |i: isize, c: usize| if i < 0 { self.last[c] } else { input[i as usize * 2 + c] };
        let mut out = Vec::with_capacity((n as f64 / step) as usize * 2 + 4);
        while self.pos < n as f64 - 1.0 {
            let i = self.pos.floor();
            let f = (self.pos - i) as f32;
            for c in 0..2 {
                let (a, b) = (at(i as isize, c), at(i as isize + 1, c));
                out.push(a + (b - a) * f);
            }
            self.pos += step;
        }
        if n > 0 {
            self.last = [input[(n - 1) * 2], input[(n - 1) * 2 + 1]];
            self.pos -= n as f64;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampling_keeps_the_rate_and_the_shape() {
        // One second of a 441 Hz sine at 44.1 kHz in 10 ms blocks → about one second at 48 kHz, still a clean sine.
        let mut r = Resampler::default();
        let mut out = Vec::new();
        for block in 0..100 {
            let input: Vec<f32> = (0..441).flat_map(|i| {
                let v = ((block * 441 + i) as f32 / 44_100.0 * 441.0 * std::f32::consts::TAU).sin();
                [v, -v]
            }).collect();
            out.extend(r.run(&input, 44_100, 48_000));
        }
        let frames = out.len() / 2;
        assert!((frames as i64 - 48_000).abs() < 3, "{frames}");
        for (k, lr) in out.chunks_exact(2).enumerate().skip(10).step_by(97) {
            let want = (k as f32 / 48_000.0 * 441.0 * std::f32::consts::TAU).sin();
            assert!((lr[0] - want).abs() < 0.02, "frame {k}: {} vs {want}", lr[0]);
            assert_eq!(lr[1], -lr[0]);
        }
    }
}
