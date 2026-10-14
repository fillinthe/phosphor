//! Audio input analysis: overall level, three bands and a small spectrum, all 0..1 with a fast
//! attack and adjustable release. Feeds the `~` modulations and the Audio panel's meters.

#[derive(Clone, Copy, Debug, Default)]
pub struct AudioAnalysis {
    pub level: f32,
    pub low: f32,
    pub mid: f32,
    pub high: f32,
}

pub const FFT_SIZE: usize = 2048;
pub const SPECTRUM_BARS: usize = 32;

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

    use super::{AudioAnalysis, FFT_SIZE, SPECTRUM_BARS};

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
        /// Seconds for a meter to fall to ~37% after a peak.
        pub release: f32,
        fft: Arc<dyn Fft<f32>>,
        window: Vec<f32>,
        raw: Vec<f32>,
        scratch: Vec<Complex<f32>>,
        analysis: AudioAnalysis,
        spectrum: Vec<f32>,
        last_update: Option<web_time::Instant>,
        /// Analyse samples pushed from the network session instead of a device.
        network: bool,
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
                release: 0.15,
                fft: FftPlanner::new().plan_fft_forward(FFT_SIZE),
                window,
                raw: vec![0.0; FFT_SIZE],
                scratch: vec![Complex::default(); FFT_SIZE],
                analysis: AudioAnalysis::default(),
                spectrum: vec![0.0; SPECTRUM_BARS],
                last_update: None,
                network: false,
            };
            input.refresh_devices();
            input
        }

        pub fn devices(&self) -> &[String] {
            &self.devices
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

        /// Switches analysis to audio arriving from session viewers.
        pub fn use_network(&mut self) {
            self.close();
            self.network = true;
            self.current = Some("Session viewers".into());
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
            // Mixes all channels to mono and keeps only the newest samples.
            let push = move |data: &[f32]| {
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
            self.ring.lock().unwrap().clear();
            self.analysis = AudioAnalysis::default();
            self.spectrum.iter_mut().for_each(|v| *v = 0.0);
        }

        /// Analyses the newest samples; call once per frame.
        pub fn update(&mut self) -> AudioAnalysis {
            if self.stream.is_none() && !self.network {
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

            let fall = 1.0 - (-dt / self.release.max(0.01)).exp();
            let follow = |s: &mut f32, v: f32| *s = if v > *s { v } else { *s + (v - *s) * fall };
            follow(&mut self.analysis.level, targets[0]);
            follow(&mut self.analysis.low, targets[1]);
            follow(&mut self.analysis.mid, targets[2]);
            follow(&mut self.analysis.high, targets[3]);
            for (s, v) in self.spectrum.iter_mut().zip(bars) {
                follow(s, v);
            }
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
        pub release: f32,
    }

    impl AudioInput {
        pub fn new() -> Self {
            Self { gain: 1.0, release: 0.15 }
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
        pub fn refresh_devices(&mut self) {}
        pub fn open(&mut self, _name: Option<&str>) -> Result<(), String> {
            Err("Audio input in the browser is coming with network sessions".into())
        }
        pub fn close(&mut self) {}
        pub fn is_network(&self) -> bool {
            false
        }
        pub fn use_network(&mut self) {}
        pub fn push_external(&mut self, _samples: &[f32]) {}
        pub fn update(&mut self) -> AudioAnalysis {
            AudioAnalysis::default()
        }
    }
}
