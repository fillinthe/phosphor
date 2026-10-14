//! The BPM clock: tap tempo, beat phase for modulation, and beat boundaries for quantised triggers.

pub struct Clock {
    pub bpm: f32,
    /// Lock tempo and beat to the audio beat detector.
    pub follow_audio: bool,
    /// Time of beat zero.
    anchor: f64,
    taps: Vec<f64>,
    last_beat: i64,
}

impl Clock {
    pub fn new(bpm: f32) -> Self {
        Self { bpm, follow_audio: false, anchor: 0.0, taps: Vec::new(), last_beat: 0 }
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

    /// Realigns beat zero to now without changing the tempo.
    pub fn resync(&mut self, now: f64) {
        self.anchor = now;
        self.last_beat = 0;
    }
}
