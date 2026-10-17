//! Audio input analysis: overall level, three bands and a small spectrum, all 0..1 with a fast
//! attack and adjustable release. Feeds the `~` modulations and the Audio panel's meters.

#[derive(Clone, Copy, Debug, Default)]
pub struct AudioAnalysis {
    pub level: f32,
    pub low: f32,
    pub mid: f32,
    pub high: f32,
    /// A beat (kick) was detected this frame.
    pub beat: bool,
    /// 1 on a beat, decaying over ~150 ms.
    pub beat_env: f32,
    /// Estimated tempo (0 until a few beats were heard) and how steady it is (0..1).
    pub bpm: f32,
    pub confidence: f32,
}

/// Envelope follower: how fast a signal rises (attack) and falls (release), in seconds (0 attack = instant).
/// Used by the audio meters, so every audio-driven modulation can snap up on a kick and fade out slowly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Envelope {
    pub attack: f32,
    pub release: f32,
}

impl Default for Envelope {
    fn default() -> Self {
        Self { attack: 0.0, release: 0.15 }
    }
}

impl Envelope {
    /// Moves `s` toward `v` over `dt` seconds.
    pub fn follow(&self, s: &mut f32, v: f32, dt: f32) {
        let time = if v > *s { self.attack } else { self.release };
        *s = if time <= 0.001 { v } else { *s + (v - *s) * (1.0 - (-dt / time).exp()) };
    }
}


/// Kick/beat detection from low-frequency energy, plus a tempo estimate from the gaps between beats.
pub struct BeatTracker {
    /// Higher = needs a stronger kick (1.2 … 3).
    pub sensitivity: f32,
    avg: f32,
    prev: f32,
    since_last: f32,
    intervals: std::collections::VecDeque<f32>,
    env: f32,
    bpm: f32,
    confidence: f32,
}

impl Default for BeatTracker {
    fn default() -> Self {
        Self { sensitivity: 1.5, avg: 0.0, prev: 0.0, since_last: 0.0, intervals: Default::default(), env: 0.0, bpm: 0.0, confidence: 0.0 }
    }
}

impl BeatTracker {
    /// Feeds one analysis frame (`energy` = bass energy, `dt` seconds since the last call).
    pub fn process(&mut self, energy: f32, dt: f32) -> bool {
        self.since_last += dt;
        let rising = energy > self.prev;
        self.prev = energy;
        let floor = 1e-4;
        let beat = rising && energy > floor && energy > self.avg * self.sensitivity && self.since_last > 0.28;
        // ~1 s running average of the bass level.
        let a = 1.0 - (-dt / 1.0f32).exp();
        self.avg += (energy - self.avg) * a;
        if beat {
            if (0.25..2.0).contains(&self.since_last) {
                self.intervals.push_back(self.since_last);
                if self.intervals.len() > 16 {
                    self.intervals.pop_front();
                }
                self.estimate();
            }
            self.since_last = 0.0;
            self.env = 1.0;
        } else {
            self.env *= (-dt / 0.15f32).exp();
        }
        beat
    }

    /// The beat period is the typical gap between kicks; a missed kick shows as a double (or triple) gap, so gaps
    /// are divided by the whole number of typical gaps they span. Confidence = the share of gaps that agree.
    fn estimate(&mut self) {
        if self.intervals.len() < 4 {
            return;
        }
        let mut sorted: Vec<f32> = self.intervals.iter().copied().collect();
        sorted.sort_by(|a, b| a.total_cmp(b));
        // The typical gap: the median of the shorter half (long gaps are usually missed kicks).
        let base = sorted[sorted.len() / 3];
        let folded: Vec<f32> = sorted.iter().map(|iv| iv / (iv / base).round().max(1.0)).collect();
        let agreeing: Vec<f32> = folded.iter().copied().filter(|p| ((p - base) / base).abs() < 0.06).collect();
        self.confidence = agreeing.len() as f32 / folded.len() as f32;
        // The mean of the agreeing gaps, not the median: beat times are quantised to frames (16.7 ms at 60 fps),
        // so a median lands on a whole number of frames (128 BPM reads as 128.6); the mean averages that out.
        let mean = agreeing.iter().sum::<f32>() / agreeing.len().max(1) as f32;
        self.bpm = 60.0 / mean.max(0.1);
    }

    pub fn reset(&mut self) {
        *self = Self { sensitivity: self.sensitivity, ..Default::default() };
    }

    fn fill(&self, a: &mut AudioAnalysis, beat: bool) {
        a.beat = beat;
        a.beat_env = self.env;
        a.bpm = self.bpm;
        a.confidence = self.confidence;
    }
}

pub const FFT_SIZE: usize = 2048;
pub const SPECTRUM_BARS: usize = 32;
/// Spectrum bins handed to shaders (ISF `audioFFT`).
pub const FFT_BINS: usize = 256;
/// Waveform samples handed to shaders (ISF `audio`).
pub const WAVE_SAMPLES: usize = 512;

#[cfg(not(target_arch = "wasm32"))]
pub use native::AudioInput;
#[cfg(target_arch = "wasm32")]
pub use stub::AudioInput;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::collections::VecDeque;
    use std::f32::consts::TAU;
    use std::sync::{Arc, Mutex};

    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use rustfft::{Fft, FftPlanner, num_complex::Complex};

    use super::{AudioAnalysis, BeatTracker, FFT_SIZE, SPECTRUM_BARS, FFT_BINS, WAVE_SAMPLES};

    type Ring = Arc<Mutex<VecDeque<f32>>>;

    fn device_name(d: &cpal::Device) -> Option<String> {
        d.description().ok().map(|desc| desc.name().to_string())
    }

    pub struct AudioInput {
        stream: Option<cpal::Stream>,
        ring: Ring,
        sample_rate: u32,
        devices: Vec<String>,
        current: Option<String>,
        pub gain: f32,
        /// How the meters (and everything modulated by them) rise and fall.
        pub envelope: super::Envelope,
        fft: Arc<dyn Fft<f32>>,
        window: Vec<f32>,
        raw: Vec<f32>,
        scratch: Vec<Complex<f32>>,
        analysis: AudioAnalysis,
        spectrum: Vec<f32>,
        /// Fine spectrum for shaders (ISF `audioFFT`): log-spaced bins 30 Hz–16 kHz, 0..1.
        bins: Vec<f32>,
        last_update: Option<web_time::Instant>,
        /// Analyse samples pushed from the network session instead of a device.
        network: bool,
        /// Analysing the sound Phosphor plays (movie clips) instead of an input device.
        clips: bool,
        pub beats: BeatTracker,
        /// LTC (audio timecode) on the first two channels, decoded only while `ltc_on`.
        ltc: Arc<Mutex<Ltc>>,
        ltc_on: Arc<std::sync::atomic::AtomicBool>,
    }

    struct Ltc {
        decoders: [crate::ltc::LtcDecoder; 2],
        clock: crate::ltc::LtcClock,
    }

    impl Ltc {
        fn new(sample_rate: u32) -> Self {
            Self { decoders: [crate::ltc::LtcDecoder::new(sample_rate), crate::ltc::LtcDecoder::new(sample_rate)], clock: Default::default() }
        }
    }

    impl AudioInput {
        pub fn new() -> Self {
            let window = (0..FFT_SIZE).map(|i| 0.5 - 0.5 * (TAU * i as f32 / FFT_SIZE as f32).cos()).collect();
            let mut input = Self {
                stream: None,
                ring: Arc::new(Mutex::new(VecDeque::with_capacity(FFT_SIZE * 2))),
                sample_rate: 48_000,
                devices: Vec::new(),
                current: None,
                gain: 1.0,
                envelope: Default::default(),
                fft: FftPlanner::new().plan_fft_forward(FFT_SIZE),
                window,
                raw: vec![0.0; FFT_SIZE],
                scratch: vec![Complex::default(); FFT_SIZE],
                analysis: AudioAnalysis::default(),
                spectrum: vec![0.0; SPECTRUM_BARS],
                bins: vec![0.0; FFT_BINS],
                last_update: None,
                network: false,
                clips: false,
                beats: BeatTracker::default(),
                ltc: Arc::new(Mutex::new(Ltc::new(48_000))),
                ltc_on: Default::default(),
            };
            input.refresh_devices();
            input
        }

        pub fn devices(&self) -> &[String] {
            &self.devices
        }

        /// Decode LTC from the input (only while a cue list follows it).
        pub fn set_ltc(&self, on: bool) {
            self.ltc_on.store(on, std::sync::atomic::Ordering::Relaxed);
        }

        /// The audio timecode now (seconds), if LTC is coming in.
        pub fn ltc_time(&self) -> Option<f64> {
            self.ltc.lock().ok()?.clock.time()
        }

        pub fn current(&self) -> Option<&str> {
            self.current.as_deref()
        }

        pub fn is_open(&self) -> bool {
            self.stream.is_some() || self.network
        }

        pub fn is_network(&self) -> bool {
            self.network
        }

        pub fn is_open_or_clips(&self) -> bool {
            self.is_open() || self.clips
        }

        /// One line for the bench: where samples come from and how many are queued.
        pub fn debug_line(&self) -> String {
            let n = self.ring.lock().map(|q| q.len()).unwrap_or(0);
            format!("source={} ring={n} rate={} level={:.3}", self.current.as_deref().unwrap_or("default input"), self.sample_rate, self.analysis.level)
        }

        /// Switches analysis to audio arriving from session viewers.
        pub fn use_network(&mut self) {
            self.close();
            self.network = true;
            self.current = Some("Session viewers".into());
        }

        /// Switches analysis to the sound Phosphor itself plays (movie clips).
        pub fn use_clips(&mut self) {
            self.close();
            self.clips = true;
            self.current = Some("Clip sound".into());
        }

        pub fn is_clips(&self) -> bool {
            self.clips
        }

        /// Feeds the clip mix (mono at `rate` Hz) into the analyser.
        pub fn push_clips(&mut self, samples: &[f32], rate: u32) {
            if !self.clips || samples.is_empty() {
                return;
            }
            if rate > 0 && rate != self.sample_rate {
                self.sample_rate = rate;
                self.beats = BeatTracker::default();
            }
            let mut q = self.ring.lock().unwrap();
            q.extend(samples.iter().copied());
            while q.len() > FFT_SIZE * 2 {
                q.pop_front();
            }
        }

        /// Feeds viewer audio (mono, 48 kHz) into the analyser.
        pub fn push_external(&mut self, samples: &[f32]) {
            if !self.network {
                return;
            }
            let mut q = self.ring.lock().unwrap();
            q.extend(samples.iter().copied());
            while q.len() > FFT_SIZE * 2 {
                q.pop_front();
            }
        }

        pub fn spectrum(&self) -> &[f32] {
            &self.spectrum
        }

        /// Fine spectrum for shaders (`FFT_BINS` values, 0..1).
        pub fn bins(&self) -> &[f32] {
            &self.bins
        }

        /// The latest samples for shaders (`WAVE_SAMPLES` values, -1..1, gain applied).
        pub fn waveform(&self) -> Vec<f32> {
            let step = (FFT_SIZE / WAVE_SAMPLES).max(1);
            let g = self.gain;
            self.raw.iter().step_by(step).take(WAVE_SAMPLES).map(|v| (v * g).clamp(-1.0, 1.0)).collect()
        }

        pub fn analysis(&self) -> AudioAnalysis {
            self.analysis
        }

        pub fn refresh_devices(&mut self) {
            let host = cpal::default_host();
            self.devices = host.input_devices().map(|it| it.filter_map(|d| device_name(&d)).collect()).unwrap_or_default();
        }

        /// Starts listening on a device (`None` = system default input).
        pub fn open(&mut self, name: Option<&str>) -> Result<(), String> {
            self.close();
            if name == Some("Session viewers") {
                self.use_network();
                return Ok(());
            }
            let host = cpal::default_host();
            let device = match name {
                Some(n) => host
                    .input_devices()
                    .map_err(|e| e.to_string())?
                    .find(|d| device_name(d).as_deref() == Some(n))
                    .ok_or_else(|| format!("Audio device '{n}' not found"))?,
                None => host.default_input_device().ok_or("No audio input device available")?,
            };
            let supported = device.default_input_config().map_err(|e| e.to_string())?;
            let channels = supported.channels().max(1) as usize;
            self.sample_rate = supported.sample_rate();
            let config: cpal::StreamConfig = supported.config();
            let ring = self.ring.clone();
            *self.ltc.lock().unwrap() = Ltc::new(self.sample_rate);
            let (ltc, ltc_on) = (self.ltc.clone(), self.ltc_on.clone());
            // Mixes all channels to mono and keeps only the newest samples.
            let push = move |data: &[f32]| {
                if ltc_on.load(std::sync::atomic::Ordering::Relaxed)
                    && let Ok(mut l) = ltc.lock()
                {
                    let l = &mut *l;
                    for (ch, d) in l.decoders.iter_mut().enumerate().take(channels) {
                        d.push(data.iter().skip(ch).step_by(channels).copied());
                        l.clock.update(d);
                    }
                }
                let mut q = ring.lock().unwrap();
                for frame in data.chunks(channels) {
                    q.push_back(frame.iter().sum::<f32>() / frame.len() as f32);
                }
                while q.len() > FFT_SIZE * 2 {
                    q.pop_front();
                }
            };
            let err_fn = |e: cpal::Error| log::warn!("audio input: {e}");
            let stream = match supported.sample_format() {
                cpal::SampleFormat::F32 => device.build_input_stream(config, move |d: &[f32], _| push(d), err_fn, None),
                cpal::SampleFormat::I16 => device.build_input_stream(
                    config,
                    move |d: &[i16], _| push(&d.iter().map(|s| *s as f32 / 32768.0).collect::<Vec<_>>()),
                    err_fn,
                    None,
                ),
                cpal::SampleFormat::U16 => device.build_input_stream(
                    config,
                    move |d: &[u16], _| push(&d.iter().map(|s| (*s as f32 - 32768.0) / 32768.0).collect::<Vec<_>>()),
                    err_fn,
                    None,
                ),
                other => return Err(format!("Unsupported audio sample format {other:?}")),
            }
            .map_err(|e| e.to_string())?;
            stream.play().map_err(|e| e.to_string())?;
            self.stream = Some(stream);
            self.current = name.map(str::to_string);
            Ok(())
        }

        pub fn close(&mut self) {
            self.stream = None;
            self.network = false;
            self.clips = false;
            self.ring.lock().unwrap().clear();
            self.analysis = AudioAnalysis::default();
            self.spectrum.iter_mut().for_each(|v| *v = 0.0);
            self.beats.reset();
        }

        /// Analyses the newest samples; call once per frame.
        pub fn update(&mut self) -> AudioAnalysis {
            if self.stream.is_none() && !self.network && !self.clips {
                return AudioAnalysis::default();
            }
            let now = web_time::Instant::now();
            let dt = self.last_update.map_or(1.0 / 60.0, |t| (now - t).as_secs_f32()).clamp(0.001, 0.1);
            self.last_update = Some(now);
            {
                let q = self.ring.lock().unwrap();
                if q.len() < FFT_SIZE {
                    return self.analysis;
                }
                let start = q.len() - FFT_SIZE;
                for (i, v) in self.raw.iter_mut().enumerate() {
                    *v = q[start + i];
                }
            }
            let rms = (self.raw.iter().map(|v| v * v).sum::<f32>() / FFT_SIZE as f32).sqrt();
            for (i, c) in self.scratch.iter_mut().enumerate() {
                *c = Complex::new(self.raw[i] * self.window[i], 0.0);
            }
            self.fft.process(&mut self.scratch);

            // Windowed magnitude ≈ amplitude · N/4, so this maps a full-scale sine to 1.0.
            let scale = 4.0 / FFT_SIZE as f32;
            let bin_hz = self.sample_rate as f32 / FFT_SIZE as f32;
            let band = |lo: f32, hi: f32| -> f32 {
                let a = ((lo / bin_hz) as usize).clamp(1, FFT_SIZE / 2 - 1);
                let b = ((hi / bin_hz) as usize).clamp(a + 1, FFT_SIZE / 2);
                self.scratch[a..b].iter().map(|c| c.norm()).sum::<f32>() / (b - a) as f32 * scale
            };
            let gain = self.gain * 6.0;
            let shape = |v: f32| (v * gain).max(0.0).sqrt().min(1.0);
            let targets = [shape(rms * 1.414), shape(band(20.0, 250.0)), shape(band(250.0, 2000.0)), shape(band(2000.0, 8000.0))];
            let bars: Vec<f32> = (0..SPECTRUM_BARS)
                .map(|i| {
                    let f = |k: usize| 30.0 * (16_000.0f32 / 30.0).powf(k as f32 / SPECTRUM_BARS as f32);
                    shape(band(f(i), f(i + 1)))
                })
                .collect();

            for i in 0..FFT_BINS {
                let f = |k: usize| 30.0 * (16_000.0f32 / 30.0).powf(k as f32 / FFT_BINS as f32);
                self.bins[i] = shape(band(f(i), f(i + 1)));
            }
            let env = self.envelope;
            let follow = |s: &mut f32, v: f32| env.follow(s, v, dt);
            follow(&mut self.analysis.level, targets[0]);
            follow(&mut self.analysis.low, targets[1]);
            follow(&mut self.analysis.mid, targets[2]);
            follow(&mut self.analysis.high, targets[3]);
            for (s, v) in self.spectrum.iter_mut().zip(bars) {
                follow(s, v);
            }
            // Kick detection works on the raw bass energy (independent of Gain).
            let kick = band(40.0, 130.0);
            let hit = self.beats.process(kick * kick, dt);
            self.beats.fill(&mut self.analysis, hit);
            self.analysis
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod stub {
    use super::AudioAnalysis;

    /// Browser audio input arrives with the WebRTC work; until then the panel shows silence.
    pub struct AudioInput {
        pub gain: f32,
        pub envelope: super::Envelope,
        pub beats: super::BeatTracker,
    }

    impl AudioInput {
        pub fn new() -> Self {
            Self { gain: 1.0, envelope: Default::default(), beats: Default::default() }
        }
        pub fn devices(&self) -> &[String] {
            &[]
        }
        pub fn current(&self) -> Option<&str> {
            None
        }
        pub fn is_open(&self) -> bool {
            false
        }
        pub fn spectrum(&self) -> &[f32] {
            &[]
        }
        pub fn bins(&self) -> &[f32] {
            &[]
        }
        pub fn waveform(&self) -> Vec<f32> {
            Vec::new()
        }
        pub fn analysis(&self) -> AudioAnalysis {
            AudioAnalysis::default()
        }
        pub fn refresh_devices(&mut self) {}
        pub fn open(&mut self, _name: Option<&str>) -> Result<(), String> {
            Err("Audio input in the browser is coming with network sessions".into())
        }
        pub fn close(&mut self) {}
        pub fn is_clips(&self) -> bool {
            false
        }
        pub fn debug_line(&self) -> String {
            String::new()
        }
        pub fn use_clips(&mut self) {}
        pub fn push_clips(&mut self, _samples: &[f32], _rate: u32) {}
        pub fn is_network(&self) -> bool {
            false
        }
        pub fn use_network(&mut self) {}
        pub fn set_ltc(&self, _on: bool) {}
        pub fn ltc_time(&self) -> Option<f64> {
            None
        }
        pub fn push_external(&mut self, _samples: &[f32]) {}
        pub fn update(&mut self) -> AudioAnalysis {
            AudioAnalysis::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BeatTracker;

    /// A kick every 0.5 s (120 BPM) at 60 fps should be found and timed.
    #[test]
    fn detects_120_bpm() {
        let mut t = BeatTracker::default();
        let dt = 1.0 / 60.0;
        let mut hits = 0;
        let mut a = super::AudioAnalysis::default();
        for f in 0..(60 * 12) {
            let time = f as f32 * dt;
            let phase = (time % 0.5) / 0.5;
            // Sharp attack, quick decay, with a bit of background.
            let energy = 0.02 + if phase < 0.12 { 1.0 - phase / 0.12 } else { 0.0 };
            let hit = t.process(energy, dt);
            t.fill(&mut a, hit);
            hits += hit as i32;
        }
        assert!((22..=26).contains(&hits), "hits {hits}");
        assert!((a.bpm - 120.0).abs() < 3.0, "bpm {}", a.bpm);
        assert!(a.confidence > 0.8, "confidence {}", a.confidence);
    }
}

#[cfg(test)]
mod envelope_tests {
    use super::Envelope;

    #[test]
    fn attack_and_release_shape_the_signal() {
        let mut s = 0.0;
        Envelope::default().follow(&mut s, 1.0, 0.016);
        assert_eq!(s, 1.0, "no attack: instant");
        let slow = Envelope { attack: 0.2, release: 0.5 };
        let mut s = 0.0;
        slow.follow(&mut s, 1.0, 0.2);
        assert!((s - 0.632).abs() < 0.01, "one attack time reaches ~63%: {s}");
        slow.follow(&mut s, 0.0, 0.5);
        assert!((s - 0.632 * 0.368).abs() < 0.01, "one release time falls to ~37%: {s}");
    }
}
