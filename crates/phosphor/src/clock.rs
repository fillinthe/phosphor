//! The BPM clock: tap tempo, beat phase for modulation, and beat boundaries for quantised triggers.

pub struct Clock {
    pub bpm: f32,
    /// Lock tempo and beat to the audio beat detector.
    pub follow_audio: bool,
    /// Lock tempo (and, after a Start, the beat) to incoming MIDI clock.
    pub follow_midi: bool,
    /// Time of beat zero.
    anchor: f64,
    taps: Vec<f64>,
    last_beat: i64,
}

impl Clock {
    pub fn new(bpm: f32) -> Self {
        Self { bpm, follow_audio: false, follow_midi: true, anchor: 0.0, taps: Vec::new(), last_beat: 0 }
    }

    pub fn beats(&self, now: f64) -> f64 {
        (now - self.anchor) * self.bpm.max(1.0) as f64 / 60.0
    }

    /// 0..1 within the current beat.
    pub fn phase(&self, now: f64) -> f32 {
        self.beats(now).rem_euclid(1.0) as f32
    }

    /// Beat number within a 4-beat bar (0..3).
    pub fn beat_in_bar(&self, now: f64) -> usize {
        (self.beats(now).floor().rem_euclid(4.0)) as usize
    }

    /// True on the first frame of a new beat. Call once per frame.
    pub fn tick(&mut self, now: f64) -> bool {
        let beat = self.beats(now).floor() as i64;
        let crossed = beat != self.last_beat;
        self.last_beat = beat;
        crossed
    }

    /// Registers a tap; three or more taps within 2 s set the tempo and realign beat zero.
    pub fn tap(&mut self, now: f64) {
        self.taps.retain(|t| now - *t < 2.0);
        self.taps.push(now);
        if self.taps.len() >= 3 {
            let span = self.taps.last().unwrap() - self.taps[0];
            let interval = span / (self.taps.len() - 1) as f64;
            if interval > 0.1 {
                self.bpm = (60.0 / interval) as f32;
            }
        }
        self.anchor = now;
        self.last_beat = 0;
    }

    /// Called on each detected audio beat when following audio: eases the tempo toward the
    /// detected BPM and pulls the beat grid onto the kick.
    pub fn on_audio_beat(&mut self, now: f64, bpm: f32, confidence: f32) {
        if !self.follow_audio || confidence < 0.5 || bpm <= 0.0 {
            return;
        }
        self.bpm += (bpm - self.bpm) * 0.35;
        let beats = self.beats(now);
        let target = beats.round();
        // Shift beat zero so `now` lands on the nearest whole beat (halfway each time, no jumps).
        let error_secs = (beats - target) * 60.0 / self.bpm.max(1.0) as f64;
        self.anchor += error_secs * 0.5;
    }

    /// Follows MIDI clock (call once per frame): tempo from the tick rate; after a Start, beat zero sits on the
    /// Start and the beat count follows the ticks, so quantised launches land with the drum machine / DJ gear.
    pub fn follow_midi_clock(&mut self, now: f64, midi: &MidiClock) {
        if !self.follow_midi || !midi.active(now) {
            return;
        }
        if let Some(bpm) = midi.bpm() {
            self.bpm += (bpm - self.bpm) * 0.5;
        }
        if let Some((at, beats)) = midi.position() {
            self.anchor = at - beats * 60.0 / self.bpm.max(1.0) as f64;
        }
    }

    /// Realigns beat zero to now without changing the tempo.
    pub fn resync(&mut self, now: f64) {
        self.anchor = now;
        self.last_beat = 0;
    }
}

/// MIDI clock (24 ticks per beat) as it arrives: tempo from recent tick spacing, beat position since Start.
/// MIDI timecode (MTC) from quarter-frame messages: eight pieces make one full time; between them the time
/// runs on from the last full one while messages keep coming (it stops when they stop).
#[derive(Default)]
pub struct Mtc {
    pieces: [u8; 8],
    seen: u8,
    /// (timecode seconds, app time it was true).
    base: Option<(f64, f64)>,
    last: f64,
    pub fps: f64,
}

impl Mtc {
    pub fn quarter(&mut self, data: u8, t: f64) {
        let piece = (data >> 4) as usize & 7;
        self.pieces[piece] = data & 0x0f;
        self.seen |= 1 << piece;
        self.last = t;
        if piece == 7 && self.seen == 0xff {
            let p = &self.pieces;
            let fps = match (p[7] >> 1) & 3 {
                0 => 24.0,
                1 => 25.0,
                2 => 29.97,
                _ => 30.0,
            };
            let frames = (p[0] | (p[1] & 1) << 4) as f64;
            let secs = (p[2] | (p[3] & 3) << 4) as f64;
            let mins = (p[4] | (p[5] & 3) << 4) as f64;
            let hours = (p[6] | (p[7] & 1) << 4) as f64;
            // A full time takes two frames to send; it described the frame when the first piece left.
            let tc = hours * 3600.0 + mins * 60.0 + secs + (frames + 2.0) / fps;
            self.base = Some((tc, t));
            self.fps = fps;
            self.seen = 0;
        }
    }

    /// The timecode now (seconds), or None when no MTC has arrived in the last two seconds.
    pub fn time(&self, now: f64) -> Option<f64> {
        let (tc, at) = self.base?;
        if now - self.last > 2.0 {
            return None;
        }
        // Running while quarter frames keep arriving; frozen when they pause.
        let running = now - self.last < 0.25;
        Some(if running { tc + (now - at) } else { tc + (self.last - at) })
    }
}

#[cfg(test)]
mod mtc_tests {
    use super::Mtc;

    #[test]
    fn reads_quarter_frames() {
        let mut m = Mtc::default();
        // 01:02:03:04 at 25 fps.
        let (h, mi, s, f) = (1u8, 2u8, 3u8, 4u8);
        let pieces = [f & 15, f >> 4, s & 15, s >> 4, mi & 15, mi >> 4, h & 15, (h >> 4) | (1 << 1)];
        for (i, v) in pieces.iter().enumerate() {
            m.quarter(((i as u8) << 4) | v, 10.0 + i as f64 * 0.01);
        }
        let t = m.time(10.07).unwrap();
        assert!((t - (3723.0 + 6.0 / 25.0)).abs() < 1e-6, "{t}");
        assert!(m.time(13.0).is_none());
    }
}

#[derive(Default)]
pub struct MidiClock {
    times: std::collections::VecDeque<f64>,
    /// Ticks since the last Start (None until a Start: tempo only).
    ticks: Option<u64>,
    running: bool,
    last_tick: f64,
}

/// Ticks per beat in MIDI clock.
const PPQN: f64 = 24.0;

impl MidiClock {
    pub fn tick(&mut self, t: f64) {
        self.last_tick = t;
        self.times.push_back(t);
        while self.times.len() > 2 * PPQN as usize + 1 {
            self.times.pop_front();
        }
        if self.running
            && let Some(n) = &mut self.ticks
        {
            *n += 1;
        }
    }

    /// Start: beat one is now (the next tick is tick 0).
    pub fn start(&mut self, t: f64) {
        self.running = true;
        self.ticks = Some(0);
        self.last_tick = t;
    }

    pub fn resume(&mut self) {
        self.running = true;
    }

    pub fn stop(&mut self) {
        self.running = false;
    }

    /// Clock heard in the last half second.
    pub fn active(&self, now: f64) -> bool {
        !self.times.is_empty() && now - self.last_tick < 0.5
    }

    /// Tempo from the last two beats of ticks (needs half a beat of them).
    pub fn bpm(&self) -> Option<f32> {
        if self.times.len() < PPQN as usize / 2 {
            return None;
        }
        let span = self.times.back()? - self.times.front()?;
        let per_tick = span / (self.times.len() - 1) as f64;
        (per_tick > 0.0).then(|| (60.0 / (per_tick * PPQN)) as f32).filter(|b| (20.0..=400.0).contains(b))
    }

    /// (time of the last tick, beats since Start at that tick) while running after a Start.
    pub fn position(&self) -> Option<(f64, f64)> {
        let n = self.ticks.filter(|_| self.running)?;
        Some((self.last_tick, n.saturating_sub(1) as f64 / PPQN))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 120 BPM clock: tempo locks to 120 and, after a Start, beat 2 lands one beat (0.5 s) later.
    #[test]
    fn follows_midi_clock_tempo_and_start() {
        let mut midi = MidiClock::default();
        let mut clock = Clock::new(90.0);
        let tick = 60.0 / 120.0 / 24.0;
        midi.start(10.0);
        let mut t = 10.0;
        for _ in 0..49 {
            midi.tick(t);
            t += tick;
        }
        let now = t - tick;
        for _ in 0..8 {
            clock.follow_midi_clock(now, &midi);
        }
        assert!((midi.bpm().unwrap() - 120.0).abs() < 0.01);
        assert!((clock.bpm - 120.0).abs() < 0.5, "{}", clock.bpm);
        assert!((clock.beats(10.5) - 1.0).abs() < 0.02, "beat 2 at 10.5 s: {}", clock.beats(10.5));
        assert!(!midi.active(now + 1.0), "clock gone after half a second");
        clock.follow_midi = false;
        clock.bpm = 100.0;
        clock.follow_midi_clock(now, &midi);
        assert_eq!(clock.bpm, 100.0, "switched off");
    }
}
