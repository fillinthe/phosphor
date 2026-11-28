//! A movie whose file is on another drive, when the drive goes and comes back (`media::drive`): the decoder reads
//! every packet through `read_packet`, which tells a drive that went from the end of the file or a damaged one,
//! carries on from this Mac's copy (or waits for the drive, then carries on from there) exactly where it was, and
//! moves to the copy by itself as soon as one is finished, so the drive can go without a hitch.

use super::*;

use crate::media::clip_cache;

/// The error a decoder ends with when its player closed while it waited for a drive (nobody hears it).
pub(super) const CLOSED: &str = "closed";

/// How often a movie waiting for its drive looks for it again.
const LOOK_AGAIN: std::time::Duration = std::time::Duration::from_secs(1);

impl Decoder {
    /// The next packet of the picture's stream; None at the end of the file. A file on another drive that stops
    /// answering because its drive went (unplugged, unmounted) is never taken for the end or for a damaged file: it
    /// carries on from this Mac's copy, or waits for the drive (`find_again`). A copy finished meanwhile is moved to
    /// first (`move_to_copy`).
    pub(super) fn read_packet(&mut self) -> Result<Option<ff::Packet>, String> {
        self.move_to_copy()?;
        let mut read_errors = 0;
        loop {
            let mut pkt = ff::Packet::empty();
            match pkt.read(&mut self.input) {
                Ok(()) if pkt.stream() == self.stream => {
                    if let Some(through) = self.skip_through {
                        match pkt.dts() {
                            // Given to the decoder already, from the file before.
                            Some(d) if d <= through => {
                                self.skip_landed = true;
                                continue;
                            }
                            // The first packet is already past the place: some would be missed.
                            _ if !self.skip_landed => {
                                self.skip_through = None;
                                self.reseek()?;
                                continue;
                            }
                            _ => self.skip_through = None,
                        }
                    }
                    self.read_since_jump = true;
                    self.last_dts = pkt.dts();
                    return Ok(Some(pkt));
                }
                Ok(()) => {}
                Err(ff::Error::Other { errno }) if errno == libc::EAGAIN => {}
                Err(e) => {
                    let end = matches!(e, ff::Error::Eof) && !self.read_failed();
                    // (A read that fails for another reason than the end is tried again a few times: a hiccup on
                    // a busy disk is not the end of the movie. A file already gone isn't waited for.)
                    if !end && read_errors < 8 && !(self.remote && drive::went(&self.opened, false)) {
                        read_errors += 1;
                        log::debug!("{}: read error ({e})", self.path);
                        std::thread::sleep(std::time::Duration::from_millis(10));
                        continue;
                    }
                    if self.remote && drive::went(&self.opened, !end) {
                        let (input, at) = self.find_again()?;
                        self.carry_on_in(input, at)?;
                        read_errors = 0;
                        continue;
                    }
                    return Ok(None);
                }
            }
        }
    }

    /// The file's reads have failed (FFmpeg reports a failed read as the end of the file, and keeps the error).
    fn read_failed(&mut self) -> bool {
        // SAFETY: reading the I/O context's error field of the input this thread owns (null for inputs with none).
        unsafe {
            let pb = (*self.input.as_mut_ptr()).pb;
            !pb.is_null() && (*pb).error < 0
        }
    }

    fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// The file's drive went away: where the movie can be read again, opened and checked to be the same movie. This
    /// Mac's copy at once; else the drive coming back (under its own name or a new one), looked for every
    /// `LOOK_AGAIN` meanwhile, the player holding its picture and its place and the layer saying why. Waits on this
    /// thread, never the UI's; Err(`CLOSED`) when the player closes meanwhile.
    pub(super) fn find_again(&mut self) -> Result<(ff::format::context::Input, String), String> {
        let shared = self.shared.clone();
        let mut waited = false;
        // (Found again and failed at once: the drive is still going. That place isn't tried again straight away.)
        let mut just_failed = self.found_at.is_some_and(|t| t.elapsed() < LOOK_AGAIN);
        loop {
            let found = drive::readable(&self.path).filter(|f| !(just_failed && f.path == self.opened));
            just_failed = false;
            if let Some(found) = found {
                match super::open_file(&found.path).map_err(err).and_then(|i| self.same_movie(&i).map(|()| i)) {
                    Ok(input) => {
                        self.found_at = Some(std::time::Instant::now());
                        if let Some(sh) = &shared {
                            sh.waiting.store(false, Ordering::Relaxed);
                        }
                        if found.copy {
                            log::warn!("{}: its drive went away: playing on from this Mac's copy", self.name());
                        } else {
                            log::warn!("{}: its drive is back: playing on", self.name());
                            // Copied now, so the next unplug finds it here.
                            clip_cache::want(&self.path);
                        }
                        return Ok((input, found.path));
                    }
                    Err(e) => log::debug!("{}: can't read it yet at {} ({e})", self.name(), found.path),
                }
            }
            let Some(sh) = &shared else { return Err("its drive was unplugged".into()) };
            if !waited {
                waited = true;
                sh.waiting.store(true, Ordering::Relaxed);
                log::warn!("{}: its drive was unplugged and this Mac has no copy of it: it plays on when the drive is back", self.name());
            }
            let ctl = sh.ctl.lock().map_err(|_| CLOSED.to_string())?;
            if ctl.quit {
                return Err(CLOSED.into());
            }
            let (ctl, _) = sh.cv.wait_timeout(ctl, LOOK_AGAIN).map_err(|_| CLOSED.to_string())?;
            if ctl.quit {
                return Err(CLOSED.into());
            }
        }
    }

    /// Is `input` another copy of the movie being played (the same picture stream, codec, size and timing)? A
    /// different file under the same name isn't carried on in.
    fn same_movie(&self, input: &ff::format::context::Input) -> Result<(), String> {
        let looks = |i: &ff::format::context::Input| {
            i.stream(self.stream).map(|st| {
                let p = st.parameters();
                // SAFETY: plain fields of the stream's codec parameters.
                let (w, h) = unsafe { ((*p.as_ptr()).width, (*p.as_ptr()).height) };
                (p.id(), w, h, st.time_base())
            })
        };
        match (looks(input), looks(&self.input)) {
            (Some(a), Some(b)) if a == b => Ok(()),
            _ => Err("not the same movie".into()),
        }
    }

    /// Goes on reading from `input` (another copy of this movie, at `at`) exactly where this one was: the decoder
    /// keeps everything it has; the packets already given to it are skipped (`read_packet`), so no frame is shown
    /// twice or missed and nothing is decoded again. A file whose packets don't say their time is jumped in instead.
    fn carry_on_in(&mut self, mut input: ff::format::context::Input, at: String) -> Result<(), String> {
        let place = match (self.read_since_jump, self.last_dts) {
            // Nothing read since the last jump: make the same jump here.
            (false, _) => Some(self.jump_ts),
            (true, Some(dts)) => Some(dts),
            (true, None) => None,
        };
        // (A seek that fails leaves the file at its start: skipping the packets up to the place still finds it.)
        if let Some(ts) = place {
            // SAFETY: seeking the demuxer we own, on this thread.
            unsafe { ffi::av_seek_frame(input.as_mut_ptr(), self.stream as i32, ts, ffi::AVSEEK_FLAG_BACKWARD) };
        }
        self.input = input;
        self.remote = drive::on_other_drive(&at);
        self.opened = at;
        self.skip_through = None;
        match (self.read_since_jump, self.last_dts) {
            (true, Some(dts)) => {
                self.skip_through = Some(dts);
                self.skip_landed = false;
            }
            (true, None) => self.reseek()?,
            (false, _) => {}
        }
        Ok(())
    }

    /// Plays on in `input` (another copy, at `at`) from the last jump, the decoder starting over (it was told the
    /// file had ended).
    pub(super) fn start_over_in(&mut self, input: ff::format::context::Input, at: String) -> Result<(), String> {
        self.input = input;
        self.remote = drive::on_other_drive(&at);
        self.opened = at;
        self.skip_through = None;
        let t = self.jump_t;
        self.seek_to(t)
    }

    /// Carrying on in another copy couldn't find the place packet by packet: jump there instead (the decoder starts
    /// over from the keyframe before; the frames up to the last one decoded are decoded again, not shown).
    fn reseek(&mut self) -> Result<(), String> {
        self.reseeks += 1;
        let target = self.seek_target.unwrap_or(self.last_rel + self.default_dur).max(0.0);
        self.seek_to(target)?;
        // (A group being decoded for reverse play keeps its frames by time itself.)
        if self.rev_job.is_some() {
            self.seek_target = None;
        }
        Ok(())
    }

    /// A copy of this clip finished on this Mac while it played from its drive: carry on from the copy, between two
    /// packets (nothing is decoded again), so the drive can go without even a hitch.
    fn move_to_copy(&mut self) -> Result<(), String> {
        if !self.remote {
            return Ok(());
        }
        let made = clip_cache::copies_made();
        if self.copies_seen == Some(made) {
            return Ok(());
        }
        self.copies_seen = Some(made);
        let Some(copy) = clip_cache::copy_of(&self.path) else { return Ok(()) };
        match super::open_file(&copy).map_err(err).and_then(|i| self.same_movie(&i).map(|()| i)) {
            Ok(input) => {
                self.carry_on_in(input, copy)?;
                log::info!("{}: playing from this Mac's copy now", self.name());
            }
            Err(e) => log::debug!("{}: its copy won't open ({e})", self.name()),
        }
        Ok(())
    }
}

#[cfg(test)]
mod copy_tests {
    use super::super::Decoder;

    /// Media times of every frame decoded until the end (or `stop` frames).
    fn frames(d: &mut Decoder, stop: usize) -> Vec<f64> {
        let mut out = Vec::new();
        while out.len() < stop && d.next_frame().unwrap() {
            out.push(d.timing().0);
        }
        out
    }

    /// Moving to another copy of the file mid-play (its drive went, or this Mac's copy finished) carries on exactly
    /// where it was: no frame twice, none missed, nothing decoded again.
    #[test]
    fn moving_to_another_copy_carries_on_exactly_where_it_was() {
        let dir = std::env::temp_dir().join(format!("phosphor-carry-on-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["h264.mp4", "hevc.mov", "av1.mkv"] {
            let src = format!("{}/../../media/test/{name}", env!("CARGO_MANIFEST_DIR"));
            if !std::path::Path::new(&src).exists() {
                continue;
            }
            let copy = dir.join(name).to_string_lossy().into_owned();
            std::fs::copy(&src, &copy).unwrap();
            let whole = frames(&mut Decoder::open(&src).unwrap(), usize::MAX);
            for at in [1, 17, whole.len() / 2] {
                let mut d = Decoder::open(&src).unwrap();
                let mut seen = frames(&mut d, at);
                let input = super::super::super::open_file(&copy).unwrap();
                d.same_movie(&input).unwrap();
                d.carry_on_in(input, copy.clone()).unwrap();
                assert_eq!(d.opened, copy);
                seen.extend(frames(&mut d, usize::MAX));
                assert_eq!(seen, whole, "{name}: moved after {at} frames");
                // (Files whose packets say their time move between two packets; others jump, decoding again.)
                if !name.ends_with(".mkv") {
                    assert_eq!(d.reseeks, 0, "{name}: moved without decoding anything again");
                }
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Another movie under the same name isn't carried on in.
    #[test]
    fn a_different_movie_is_not_another_copy() {
        let base = format!("{}/../../media/test/", env!("CARGO_MANIFEST_DIR"));
        let (a, b) = (format!("{base}h264.mp4"), format!("{base}hevc.mov"));
        if !std::path::Path::new(&a).exists() || !std::path::Path::new(&b).exists() {
            return;
        }
        let d = Decoder::open(&a).unwrap();
        assert!(d.same_movie(&super::super::super::open_file(&a).unwrap()).is_ok());
        assert!(d.same_movie(&super::super::super::open_file(&b).unwrap()).is_err());
    }
}
