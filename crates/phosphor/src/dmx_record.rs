//! Record the LED / DMX output and play it back (VDMX / MadMapper's DMX recorder): each frame of universes that went
//! out, with its time. Playback sends them again in time, looping if asked, instead of the live picture. Takes save
//! to a small binary file (`.phdmx`).

use std::collections::BTreeMap;

pub type Universes = BTreeMap<u16, [u8; 512]>;

/// A recording: (seconds from its start, the universes sent then).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Take {
    pub frames: Vec<(f64, Universes)>,
}

const MAGIC: &[u8; 6] = b"PHDMX1";

impl Take {
    pub fn length(&self) -> f64 {
        self.frames.last().map_or(0.0, |f| f.0)
    }

    /// The frame showing `t` seconds in (the last one at or before it).
    pub fn at(&self, t: f64) -> Option<&Universes> {
        let i = self.frames.partition_point(|f| f.0 <= t);
        self.frames.get(i.saturating_sub(1)).map(|f| &f.1)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        for (t, unis) in &self.frames {
            out.extend(t.to_le_bytes());
            out.extend((unis.len() as u16).to_le_bytes());
            for (u, data) in unis {
                out.extend(u.to_le_bytes());
                out.extend(data);
            }
        }
        out
    }

    pub fn from_bytes(b: &[u8]) -> Result<Self, String> {
        let rest = b.strip_prefix(MAGIC.as_slice()).ok_or("not a Phosphor DMX recording")?;
        let mut take = Take::default();
        let mut p = rest;
        let take_n = |n: usize, p: &mut &[u8]| -> Result<Vec<u8>, String> {
            if p.len() < n {
                return Err("the recording is cut short".into());
            }
            let (a, b) = p.split_at(n);
            *p = b;
            Ok(a.to_vec())
        };
        while !p.is_empty() {
            let t = f64::from_le_bytes(take_n(8, &mut p)?.try_into().unwrap());
            let count = u16::from_le_bytes(take_n(2, &mut p)?.try_into().unwrap());
            let mut unis = Universes::new();
            for _ in 0..count {
                let u = u16::from_le_bytes(take_n(2, &mut p)?.try_into().unwrap());
                unis.insert(u, take_n(512, &mut p)?.try_into().unwrap());
            }
            take.frames.push((t, unis));
        }
        Ok(take)
    }
}

/// Recording, playing back, or neither.
#[derive(Default)]
pub struct DmxRecorder {
    /// Recording since this app time.
    recording: Option<(f64, Take)>,
    /// Playing this take since this app time.
    playing: Option<(f64, Take)>,
    pub looping: bool,
    /// The last finished recording (Play / Save use it).
    pub take: Option<Take>,
}

impl DmxRecorder {
    pub fn start_recording(&mut self, now: f64) {
        self.playing = None;
        self.recording = Some((now, Take::default()));
    }

    pub fn stop(&mut self) {
        if let Some((_, take)) = self.recording.take() {
            self.take = Some(take);
        }
        self.playing = None;
    }

    pub fn play(&mut self, now: f64) {
        self.recording = None;
        self.playing = self.take.clone().filter(|t| !t.frames.is_empty()).map(|t| (now, t));
    }

    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    pub fn is_playing(&self) -> bool {
        self.playing.is_some()
    }

    /// Seconds recorded or played so far.
    pub fn elapsed(&self, now: f64) -> f64 {
        self.recording.as_ref().or(self.playing.as_ref()).map_or(0.0, |(s, _)| now - s)
    }

    /// A frame that just went out live (kept while recording).
    pub fn sent(&mut self, now: f64, unis: &Universes) {
        if let Some((start, take)) = &mut self.recording {
            take.frames.push((now - *start, unis.clone()));
        }
    }

    /// While playing: the recorded frame for now (None = play live). Stops at the end unless looping.
    pub fn playback(&mut self, now: f64) -> Option<Universes> {
        let (start, take) = self.playing.as_ref()?;
        let len = take.length().max(1.0 / 60.0);
        let mut t = now - start;
        if t > len {
            if !self.looping {
                self.playing = None;
                return None;
            }
            t %= len;
        }
        take.at(t).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(v: u8) -> Universes {
        let mut u = Universes::new();
        u.insert(0, [v; 512]);
        u
    }

    #[test]
    fn records_plays_back_and_saves() {
        let mut r = DmxRecorder::default();
        r.start_recording(10.0);
        for (k, t) in [10.0, 10.5, 11.0].into_iter().enumerate() {
            r.sent(t, &frame(k as u8 * 100));
        }
        r.stop();
        r.play(20.0);
        assert_eq!(r.playback(20.6).unwrap()[&0][0], 100, "the frame from 0.5 s in");
        assert_eq!(r.playback(21.2), None, "ends");
        r.looping = true;
        r.play(30.0);
        assert_eq!(r.playback(31.25).unwrap()[&0][0], 0, "loops round");
        let take = r.take.clone().unwrap();
        assert_eq!(Take::from_bytes(&take.to_bytes()), Ok(take));
        assert!(Take::from_bytes(b"nope").is_err());
        assert!(Take::from_bytes(b"PHDMX1\x00\x00").is_err(), "cut short");
    }
}
