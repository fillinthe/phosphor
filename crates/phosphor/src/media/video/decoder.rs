//! The decoder thread: FFmpeg demux + decode (VideoToolbox when it can), seeks, loop regions, reverse and
//! ping-pong, and the presentation clock the player follows.

use super::*;

/// Decoder state owned by the background thread.
pub(super) struct Decoder {
    /// Hap played on the GPU: the current frame's blocks (None = FFmpeg decodes pixels).
    hap: Option<Vec<u8>>,
    hap_format: phosphor_engine::CompressedFormat,
    /// Hap Q Alpha: the current frame's alpha blocks (empty for the other flavours).
    hap_alpha: Vec<u8>,
    path: String,
    input: ff::format::context::Input,
    stream: usize,
    tb: f64,
    start_ts: i64,
    default_dur: f64,
    pub(super) duration: Option<f64>,
    pub(super) codec: String,
    pub(super) dec: ff::decoder::Video,
    /// VideoToolbox was attached; cleared if it fails before the first frame.
    hw: bool,
    /// Every frame is a keyframe (ProRes, Hap, DXV, MJPEG, DNxHD…): reverse can jump straight to any frame.
    intra: bool,
    frame: ff::frame::Video,
    mapped: ff::frame::Video,
    scaler: Scaler,
    eof_sent: bool,
    /// First pts of the file, subtracted so playback starts at 0.
    origin: Option<i64>,
    last_rel: f64,
    /// Longest media time seen (the movie's length when the container doesn't say).
    loop_len: f64,
    /// Presentation time of the next frame sent. Grows by each frame's duration whichever way the
    /// movie plays, so loops, reverse and ping-pong all look like one continuous stream to the player.
    out_clock: Option<f64>,
    /// Times the movie has looped or turned around (0 = first pass).
    passes: u32,
    /// Media time of the last frame sent.
    last_sent: f64,
    /// Playing backwards: frames before `rev_end` are decoded a group at a time into `rev_buf`
    /// and sent from the back. Each keyframe group is decoded once (as much of it as `rev_cap` frames
    /// hold), and the group before it is decoded into `rev_next` a few frames at a time while this
    /// one plays, so backwards costs about what forwards does and never waits on a whole group.
    reversing: bool,
    rev_end: f64,
    rev_buf: Vec<(f64, f64, VideoFrame)>,
    rev_next: Vec<(f64, f64, VideoFrame)>,
    rev_job: Option<RevJob>,
    /// Ping-pong: the last frames of the forward pass (cheap handles), so turning around plays backwards
    /// straight from them, without decoding.
    tail: Vec<(f64, f64, VideoFrame)>,
    frames_in_loop: u64,
    skips: u32,
    errors: u32,
    live: bool,
    /// Tag for frames sent (the seek they belong to).
    generation: u64,
    /// After a seek: decode and drop frames until this media time.
    seek_target: Option<f64>,
    /// Live pacing: (first pts, when it arrived). Inputs that run faster than real time (test
    /// sources, recorded streams) are slowed to their timestamps; cameras already are.
    live_start: Option<(f64, std::time::Instant)>,
}

impl Decoder {
    pub(super) fn open(path: &str) -> Result<Self, String> {
        let input = super::open_input(path).map_err(|e| format!("can't open ({e})"))?;
        Self::from_input(path, input, false)
    }

    pub(super) fn open_live(format: &str, url: &str, options: &[(&str, &str)]) -> Result<Self, String> {
        static DEVICES: Once = Once::new();
        DEVICES.call_once(ff::device::register_all);
        let mut dict = ff::Dictionary::new();
        for (k, v) in options {
            dict.set(k, v);
        }
        let ctx = if format.is_empty() {
            ff::format::input_with_dictionary(&url, dict).map_err(|e| format!("can't open ({e})"))?
        } else {
            let fmt = ff::device::input::video()
                .find(|f| match f {
                    ff::Format::Input(i) => i.name() == format,
                    _ => false,
                })
                .ok_or_else(|| format!("input type \"{format}\" isn't available"))?;
            match ff::format::open_with(&url, &fmt, dict).map_err(|e| format!("can't open ({e})"))? {
                ff::format::context::Context::Input(i) => i,
                _ => return Err("not an input".into()),
            }
        };
        Self::from_input(url, ctx, true)
    }

    fn from_input(path: &str, input: ff::format::context::Input, live: bool) -> Result<Self, String> {
        let st = input
            .streams()
            .best(ff::media::Type::Video)
            .ok_or("no video stream")?;
        let stream = st.index();
        let tb = f64::from(st.time_base());
        let tb = if tb > 0.0 { tb } else { 1.0 / 90_000.0 };
        let fps = f64::from(st.avg_frame_rate());
        let default_dur = if fps > 0.0 && fps < 1000.0 {
            1.0 / fps
        } else {
            1.0 / 30.0
        };
        let duration = if input.duration() > 0 {
            Some(input.duration() as f64 / f64::from(ff::ffi::AV_TIME_BASE))
        } else if st.duration() > 0 {
            Some(st.duration() as f64 * tb)
        } else {
            None
        };
        let start_ts = if st.start_time() != NOPTS {
            st.start_time()
        } else {
            0
        };
        let (dec, codec, hw) = open_decoder(&input, stream, true)?;
        // SAFETY: a static table lookup by codec id.
        let intra = unsafe {
            let d = ffi::avcodec_descriptor_get(dec.id().into());
            !d.is_null() && (*d).props & ffi::AV_CODEC_PROP_INTRA_ONLY != 0
        };
        // Hap goes to the GPU as it is stored (blocks), skipping FFmpeg's CPU texture decoder.
        let hap = (gpu_hap() && st_is_hap(&input, stream)).then(Vec::new);
        if hap.is_none() && input.stream(stream).is_some_and(|s| unsafe { (*s.parameters().as_ptr()).codec_tag } == u32::from_le_bytes(super::HAP_R)) {
            return Err("Hap R plays only on the graphics card (PHOSPHOR_CPU_HAP is set, or the GPU has no BC7)".into());
        }
        Ok(Self {
            hap,
            hap_format: phosphor_engine::CompressedFormat::Bc1,
            hap_alpha: Vec::new(),
            path: path.to_string(),
            input,
            stream,
            tb,
            start_ts,
            default_dur,
            duration,
            codec,
            dec,
            hw,
            intra,
            frame: ff::frame::Video::empty(),
            mapped: ff::frame::Video::empty(),
            scaler: Scaler::default(),
            eof_sent: false,
            origin: None,
            last_rel: -1.0,
            loop_len: 0.0,
            out_clock: None,
            passes: 0,
            last_sent: 0.0,
            reversing: false,
            rev_end: 0.0,
            rev_buf: Vec::new(),
            rev_next: Vec::new(),
            rev_job: None,
            tail: Vec::new(),
            frames_in_loop: 0,
            skips: 0,
            errors: 0,
            live,
            live_start: None,
            generation: 0,
            seek_target: None,
        })
    }

    pub(super) fn run(&mut self, sh: &Shared, tx: &SyncSender<Msg>) -> Result<(), String> {
        loop {
            let (seek, mode) = match sh.ctl.lock() {
                Ok(mut c) if !c.quit => (c.seek.take(), c.mode),
                _ => return Ok(()),
            };
            if let Some((t, generation)) = seek {
                self.generation = generation;
                self.out_clock = None;
                let turning = mode.direction == PlayDirection::Reverse && !self.live && !self.reversing;
                // Turning around where we are: the frames just shown play backwards straight away (no wait for
                // decoding); the group before them is decoded while they play.
                let mut kept = if turning { std::mem::take(&mut self.tail) } else { Vec::new() };
                self.drop_reverse(sh);
                self.reversing = mode.direction == PlayDirection::Reverse && !self.live;
                if self.reversing {
                    // Include the frame on screen at `t`; frames decoded past it were never seen.
                    while kept.last().is_some_and(|f| f.0 > t + 1e-4) {
                        if let Some((_, _, f)) = kept.pop() {
                            sh.recycle(f);
                        }
                    }
                    match kept.first() {
                        Some(first) if kept.last().is_some_and(|l| t - l.0 < 0.1) => {
                            self.rev_end = first.0;
                            self.rev_buf = kept;
                            let (lo, _) = self.bounds(mode);
                            self.start_rev_job(self.rev_end, lo)?;
                        }
                        _ => {
                            for (_, _, f) in kept {
                                sh.recycle(f);
                            }
                            self.rev_end = t + 1e-4;
                        }
                    }
                } else {
                    for (_, _, f) in kept {
                        sh.recycle(f);
                    }
                    self.seek_to(t)?;
                }
            }
            let (lo, hi) = self.bounds(mode);
            if self.reversing {
                if !self.reverse_step(sh, tx, mode, lo, hi)? {
                    return Ok(());
                }
                continue;
            }
            let more = match self.next_frame() {
                Ok(more) => more,
                Err(e) if self.hw && self.origin.is_none() => {
                    self.fallback_to_software(&e, sh)?;
                    continue;
                }
                Err(e) => return Err(e),
            };
            if !more && self.live {
                return Err("the input stopped (unplugged or closed?)".into());
            }
            if !more {
                if self.frames_in_loop == 0 && self.hw && self.origin.is_none() {
                    self.fallback_to_software("no frames", sh)?;
                    continue;
                }
                if self.frames_in_loop == 0 {
                    return Err("no decodable video frames".into());
                }
                // Single-frame files (stills) never need re-decoding.
                let still = self.frames_in_loop == 1 && self.passes == 0 && mode == PlayMode::default();
                if !self.at_end(sh, tx, mode, lo, hi, still)? {
                    return Ok(());
                }
                continue;
            }
            sh.decoded.fetch_add(1, Ordering::Relaxed);
            let (rel, dur) = self.timing();
            // After a seek, frames before the target only lead up to it.
            if let Some(target) = self.seek_target {
                // Drop frames whose display time ends at or before the target (the one shown at
                // `target` is the one whose interval contains it).
                if rel + dur <= target + 1e-6 {
                    continue;
                }
                self.seek_target = None;
            }
            if !self.live && mode.range.is_some() {
                if rel >= hi - 1e-6 {
                    if !self.at_end(sh, tx, mode, lo, hi, false)? {
                        return Ok(());
                    }
                    continue;
                }
                if rel + dur <= lo + 1e-6 {
                    // Before the in point (it moved, or a seek landed there).
                    self.jump(lo)?;
                    continue;
                }
            }
            let pts = self.next_pts(rel, dur);
            if self.late(sh, pts, dur) {
                continue;
            }
            if self.live {
                let (rel0, t0) = *self.live_start.get_or_insert((rel, std::time::Instant::now()));
                let ahead = (rel - rel0) - t0.elapsed().as_secs_f64();
                if ahead > 0.002 {
                    std::thread::sleep(std::time::Duration::from_secs_f64(ahead.min(0.25)));
                } else if ahead < -1.0 {
                    // Fell far behind (stall, clock jump): re-anchor instead of racing to catch up.
                    self.live_start = Some((rel, std::time::Instant::now()));
                }
            }
            let out = self.picture(sh)?;
            // Keep the frames just shown (only ones that are cheap to keep): about a second, so a switch to reverse
            // starts at once; ping-pong keeps the end of the forward pass for its turn.
            let pingpong_end = mode.direction == PlayDirection::PingPong && hi.is_finite() && rel >= hi - (self.rev_cap(sh) as f64 + 1.0) * dur.max(1e-3);
            let keep = if pingpong_end { self.rev_cap(sh) } else { ((1.0 / dur.max(1e-3)).ceil() as usize + 2).min(self.rev_cap(sh)) };
            match out.cheap_clone() {
                Some(c) => {
                    self.tail.push((rel, dur, c));
                    while self.tail.len() > keep {
                        let (_, _, old) = self.tail.remove(0);
                        sh.recycle(old);
                    }
                }
                None => {
                    for (_, _, f) in self.tail.drain(..) {
                        sh.recycle(f);
                    }
                }
            }
            self.last_sent = rel;
            sh.shown.fetch_add(1, Ordering::Relaxed);
            if tx.send(Msg::Frame(self.generation, pts, rel, out)).is_err() {
                return Ok(());
            }
        }
    }

    /// Loop region in seconds (the whole movie when none is set).
    fn bounds(&self, mode: PlayMode) -> (f64, f64) {
        let len = match self.duration {
            Some(d) if d > 0.0 => d,
            _ if self.loop_len > 0.0 => self.loop_len,
            _ => f64::INFINITY,
        };
        match mode.range {
            // A section that's empty once clamped to this clip (in point past the end) plays the whole clip.
            Some((a, b)) if a.clamp(0.0, len) + self.default_dur < b.min(len) => (a.clamp(0.0, len), b.min(len)),
            _ => (0.0, len),
        }
    }

    fn next_pts(&mut self, rel: f64, dur: f64) -> f64 {
        let pts = *self.out_clock.get_or_insert(rel);
        self.out_clock = Some(pts + dur);
        pts
    }

    /// True if a frame at `pts` is already too late to show (skipped, but never too many in a row).
    fn late(&mut self, sh: &Shared, pts: f64, dur: f64) -> bool {
        let clock = f64::from_bits(sh.clock.load(Ordering::Relaxed));
        if !self.live && pts + dur < clock - LATE && self.skips < MAX_SKIP {
            self.skips += 1;
            sh.skipped.fetch_add(1, Ordering::Relaxed);
            return true;
        }
        self.skips = 0;
        false
    }

    /// Reached the end of the loop region (or of the movie) going either way: loop, turn around,
    /// or stop until looping is switched back on. Returns false when the player is gone.
    fn at_end(&mut self, sh: &Shared, tx: &SyncSender<Msg>, mode: PlayMode, lo: f64, hi: f64, still: bool) -> Result<bool, String> {
        if still || !sh.ctl.lock().is_ok_and(|c| c.looping) {
            if tx.send(Msg::End(self.out_clock.unwrap_or(0.0))).is_err() {
                return Ok(false);
            }
            let Ok(mut ctl) = sh.ctl.lock() else {
                return Ok(false);
            };
            while !ctl.quit && ctl.seek.is_none() && (still || !ctl.looping) {
                ctl = match sh.cv.wait(ctl) {
                    Ok(c) => c,
                    Err(_) => return Ok(false),
                };
            }
            if ctl.quit {
                return Ok(false);
            }
            if ctl.seek.is_some() {
                // Handled at the top of the loop.
                return Ok(true);
            }
        }
        self.passes += 1;
        match (self.reversing, mode.direction) {
            (false, PlayDirection::PingPong) => {
                self.reversing = true;
                self.rev_end = self.last_sent;
                // Play backwards from the frames just shown (minus the turning frame), no decoding; the group
                // before them is decoded while they play.
                let mut tail = std::mem::take(&mut self.tail);
                if let Some((_, _, turning)) = tail.pop() {
                    sh.recycle(turning);
                }
                if let Some(first) = tail.first() {
                    self.rev_end = first.0;
                    for (_, _, f) in self.rev_buf.drain(..) {
                        sh.recycle(f);
                    }
                    self.rev_buf = tail;
                    self.start_rev_job(self.rev_end, lo)?;
                }
            }
            (false, _) => self.jump(lo)?,
            (true, PlayDirection::PingPong) => {
                self.reversing = false;
                self.jump(lo)?;
                // Don't show the turning frame twice.
                self.seek_target = Some(self.last_sent);
            }
            (true, _) => self.rev_end = hi,
        }
        Ok(true)
    }

    /// Continues forward from `t` (loop jump: the presentation clock keeps running).
    fn jump(&mut self, t: f64) -> Result<(), String> {
        if t <= 1e-6 { self.rewind() } else { self.seek_to(t) }
    }

    fn drop_reverse(&mut self, sh: &Shared) {
        for (_, _, f) in self.rev_buf.drain(..).chain(self.rev_next.drain(..)).chain(self.tail.drain(..)) {
            sh.recycle(f);
        }
        self.rev_job = None;
    }

    /// How many frames one reverse group may hold: about 512 MB of hardware surfaces (NV12, 1.5 bytes a
    /// pixel; enough for a 5 s keyframe group at 1080p), or 256 MB of converted RGBA pictures.
    fn rev_cap(&self, _sh: &Shared) -> usize {
        self.rev_cap_frames()
    }

    /// Frames of one reverse group that fit the memory budget.
    fn rev_cap_frames(&self) -> usize {
        let px = (self.dec.width() as usize * self.dec.height() as usize).max(1);
        let surfaces = self.hw && cfg!(target_os = "macos") && super::zero_copy();
        let budget = if surfaces { (512 << 20) / (px * 3 / 2) } else { (256 << 20) / (px * 4) };
        budget.clamp(12, 240)
    }

    /// Starts decoding the frames just before `end` (from the keyframe before it) into `rev_next`.
    fn start_rev_job(&mut self, end: f64, lo: f64) -> Result<(), String> {
        if end <= lo + 1e-6 {
            self.rev_job = None;
            return Ok(());
        }
        // Keyframes only: go back a whole group's worth in one jump (one jump per frame would refill the decoder each
        // time). Otherwise the keyframe before `end` is where it lands anyway.
        // Short sections (everything back to the start fits the budget): decode it all from the start, which every
        // file can seek to reliably (MPEG program streams can't seek to an exact time).
        let room = self.rev_cap_frames() as f64 * self.default_dur;
        let back = if end - lo <= room * 0.95 {
            end - lo
        } else if self.intra {
            room
        } else {
            self.default_dur
        };
        self.seek_to((end - back).max(lo))?;
        self.seek_target = None;
        self.rev_job = Some(RevJob { end, keep_from: None, done: false, at: f64::NAN, wrap: false, back });
        Ok(())
    }

    /// Decodes up to `budget` frames of the reverse job (frames before the kept window aren't converted).
    fn advance_rev_job(&mut self, sh: &Shared, lo: f64, budget: usize) -> Result<(), String> {
        self.advance_rev_job_for(sh, lo, budget, None)
    }

    /// `advance_rev_job`, stopping early once `time` has passed (slow decoders mustn't hold up the next frame).
    fn advance_rev_job_for(&mut self, sh: &Shared, lo: f64, budget: usize, time: Option<std::time::Duration>) -> Result<(), String> {
        let cap = self.rev_cap(sh);
        let start = std::time::Instant::now();
        for _ in 0..budget {
            if time.is_some_and(|t| start.elapsed() > t) {
                return Ok(());
            }
            let Some(job) = self.rev_job else { return Ok(()) };
            if job.done {
                return Ok(());
            }
            if !self.next_frame()? {
                self.rev_job = Some(RevJob { done: true, ..job });
                return Ok(());
            }
            sh.decoded.fetch_add(1, Ordering::Relaxed);
            let (rel, dur) = self.timing();
            // Files without a seek index (MPEG program streams) can land after the point asked for: go further back.
            if job.keep_from.is_none() && rel > job.end - dur.max(1e-3) * 0.5 && job.end - job.back > lo + 1e-6 {
                let back = job.back * 4.0;
                self.seek_to((job.end - back).max(lo))?;
                self.seek_target = None;
                self.rev_job = Some(RevJob { back, ..job });
                continue;
            }
            // The first frame after the seek is the keyframe: keep from there, or the last `cap` frames.
            let keep_from = job.keep_from.unwrap_or_else(|| rel.max(job.end - cap as f64 * dur.max(1e-3)).max(lo));
            let done = rel >= job.end - 1e-6;
            self.rev_job = Some(RevJob { keep_from: Some(keep_from), done, at: rel, ..job });
            if done {
                return Ok(());
            }
            if rel + 1e-6 < keep_from {
                continue;
            }
            let out = self.picture(sh)?;
            self.rev_next.push((rel, dur, out));
        }
        Ok(())
    }

    /// One step of reverse play: send the next buffered frame (and decode a little of the group before
    /// it), or, when this group is used up, switch to the next one. Returns false when the player is gone.
    fn reverse_step(&mut self, sh: &Shared, tx: &SyncSender<Msg>, mode: PlayMode, lo: f64, hi: f64) -> Result<bool, String> {
        if let Some((rel, dur, frame)) = self.rev_buf.pop() {
            let pts = self.next_pts(rel, dur);
            if self.late(sh, pts, dur) {
                sh.recycle(frame);
            } else {
                self.last_sent = rel;
                sh.shown.fetch_add(1, Ordering::Relaxed);
                if tx.send(Msg::Frame(self.generation, pts, rel, frame)).is_err() {
                    return Ok(false);
                }
            }
            // Meanwhile, some of the group before this one (sending waited for the player anyway): enough that it's
            // ready before these frames run out, at least a few.
            let budget = match self.rev_job {
                Some(j) if !j.done && j.at.is_finite() => {
                    let left = ((j.end - j.at) / self.default_dur.max(1e-3)).ceil().max(0.0) as usize;
                    (left / self.rev_buf.len().max(1) + 1).clamp(REVERSE_DECODE_AHEAD, 32)
                }
                _ => REVERSE_DECODE_AHEAD,
            };
            // At most about half a frame's time, so a slow decoder (big ProRes 4444) never stalls the picture.
            let time = std::time::Duration::from_secs_f64((self.default_dur * 0.5).clamp(0.004, 0.05));
            self.advance_rev_job_for(sh, lo, budget, Some(time))?;
            return Ok(true);
        }
        if !hi.is_finite() {
            // Unknown length: can't start from the end.
            self.reversing = false;
            return Ok(true);
        }
        self.rev_end = self.rev_end.min(hi);
        if self.rev_end <= lo + 1e-6 {
            // At the start of the section. Looping round: the group at the end may already be decoding (`wrap`).
            if !self.rev_job.is_some_and(|j| j.wrap) {
                return self.at_end(sh, tx, mode, lo, hi, false);
            }
            if !self.at_end(sh, tx, mode, lo, hi, false)? {
                return Ok(false);
            }
            if !self.reversing || self.rev_end <= lo + 1e-6 {
                return Ok(true);
            }
        }
        if self.rev_job.is_none() {
            self.start_rev_job(self.rev_end, lo)?;
        }
        // This group is used up: finish the next one (usually done already) and play it.
        let waited = std::time::Instant::now();
        while self.rev_job.is_some_and(|j| !j.done) {
            self.advance_rev_job(sh, lo, 64)?;
        }
        if waited.elapsed().as_secs_f64() > 0.25 {
            sh.reverse_slow.store(true, Ordering::Relaxed);
        }
        let keep_from = self.rev_job.and_then(|j| j.keep_from);
        self.rev_job = None;
        std::mem::swap(&mut self.rev_buf, &mut self.rev_next);
        self.rev_end = match self.rev_buf.first() {
            Some(f) => f.0,
            // Nothing decodable before `rev_end`: treat as the start.
            None => keep_from.unwrap_or(lo).min(self.rev_end - self.default_dur).max(lo),
        };
        if self.rev_buf.is_empty() && self.rev_end <= lo + 1e-6 {
            return self.at_end(sh, tx, mode, lo, hi, false);
        }
        // Start on the group before this one right away; it fills in while this one plays. At the start of a looping
        // reverse section, that's the group at its end.
        if self.rev_end <= lo + 1e-6 && mode.direction == PlayDirection::Reverse && hi.is_finite() && sh.ctl.lock().is_ok_and(|c| c.looping) {
            self.start_rev_job(hi, lo)?;
            if let Some(j) = self.rev_job.as_mut() {
                j.wrap = true;
            }
        } else {
            self.start_rev_job(self.rev_end, lo)?;
        }
        Ok(true)
    }

    /// Decodes the next frame into `self.frame`. Returns false at end of stream.
    fn next_frame(&mut self) -> Result<bool, String> {
        if self.hap.is_some() {
            return self.next_hap_frame();
        }
        loop {
            match self.dec.receive_frame(&mut self.frame) {
                Ok(()) => {
                    self.errors = 0;
                    return Ok(true);
                }
                Err(ff::Error::Eof) => return Ok(false),
                Err(ff::Error::Other { errno }) if errno == libc::EAGAIN => {}
                // Tolerate isolated corrupt frames; give up on a stream that never recovers.
                Err(e) if (self.hw && self.origin.is_none()) || self.errors > 100 => {
                    return Err(format!("decode failed ({e})"));
                }
                Err(e) => {
                    self.errors += 1;
                    log::debug!("{}: decode error ({e})", self.path);
                }
            }
            if self.eof_sent {
                return Ok(false);
            }
            loop {
                let mut pkt = ff::Packet::empty();
                match pkt.read(&mut self.input) {
                    Ok(()) if pkt.stream() == self.stream => {
                        if let Err(e) = self.dec.send_packet(&pkt) {
                            if self.hw && self.origin.is_none() {
                                return Err(format!("decode failed ({e})"));
                            }
                            log::debug!("{}: skipping bad packet ({e})", self.path);
                            continue;
                        }
                        break;
                    }
                    Ok(()) => {}
                    Err(ff::Error::Other { errno }) if errno == libc::EAGAIN => {}
                    Err(_) => {
                        let _ = self.dec.send_eof();
                        self.eof_sent = true;
                        break;
                    }
                }
            }
        }
    }

    /// Hap on the GPU: the next packet's blocks into `self.hap`, its timing into `self.frame` (so `timing`,
    /// seeks, loops and reverse work unchanged). A Hap flavour the GPU path can't take falls back to FFmpeg.
    fn next_hap_frame(&mut self) -> Result<bool, String> {
        loop {
            let mut pkt = ff::Packet::empty();
            match pkt.read(&mut self.input) {
                Ok(()) if pkt.stream() == self.stream => {
                    let Some(buf) = self.hap.as_mut() else { return Ok(false) };
                    let Some(data) = pkt.data() else { continue };
                    match super::hap::decode(data, buf, &mut self.hap_alpha) {
                        Ok((format, _)) => self.hap_format = format,
                        Err(e) => {
                            log::info!("{}: {e}; decoding on the CPU instead", self.path);
                            self.hap = None;
                            self.rewind()?;
                            return self.next_frame();
                        }
                    }
                    // SAFETY: plain fields of the frame we own; `timing` reads them like a decoded frame's.
                    unsafe {
                        let f = self.frame.as_mut_ptr();
                        (*f).pts = pkt.pts().unwrap_or(NOPTS);
                        (*f).best_effort_timestamp = (*f).pts;
                        (*f).duration = pkt.duration();
                    }
                    return Ok(true);
                }
                Ok(()) => {}
                Err(ff::Error::Other { errno }) if errno == libc::EAGAIN => {}
                Err(_) => return Ok(false),
            }
        }
    }

    /// The picture for the frame just decoded: GPU blocks (Hap) or converted pixels (everything else).
    fn picture(&mut self, sh: &Shared) -> Result<VideoFrame, String> {
        if let Some(buf) = &self.hap {
            let (width, height) = (self.dec.width(), self.dec.height());
            let alpha = (!self.hap_alpha.is_empty()).then(|| self.hap_alpha.clone());
            return Ok(VideoFrame::Compressed { width, height, format: self.hap_format, data: buf.clone(), alpha });
        }
        #[cfg(target_os = "macos")]
        if zero_copy() {
            // SAFETY: a VideoToolbox frame's data[3] is its CVPixelBufferRef.
            let f = unsafe { &*self.frame.as_ptr() };
            if f.format == Px::AV_PIX_FMT_VIDEOTOOLBOX as i32
                && let Some(pb) = unsafe { super::surface::PixelBuffer::retain_nv12(f.data[3] as *mut std::ffi::c_void) }
            {
                sh.hardware.store(true, Ordering::Relaxed);
                return Ok(VideoFrame::Surface(pb));
            }
        }
        let src = self.download(sh)?;
        self.scaler.convert(src, sh)
    }

    /// Frame time relative to the loop start, and frame duration, in seconds.
    fn timing(&mut self) -> (f64, f64) {
        let f = unsafe { &*self.frame.as_ptr() };
        let ts = if f.best_effort_timestamp != NOPTS {
            f.best_effort_timestamp
        } else {
            f.pts
        };
        let dur = if f.duration > 0 {
            f.duration as f64 * self.tb
        } else {
            self.default_dur
        };
        let mut rel = if ts == NOPTS {
            self.last_rel + dur
        } else {
            let origin = *self.origin.get_or_insert(ts);
            (ts - origin) as f64 * self.tb
        };
        // Keep time strictly increasing within a loop even with broken timestamps.
        if rel <= self.last_rel {
            rel = self.last_rel + 1e-4;
        }
        rel = rel.max(0.0);
        self.last_rel = rel;
        self.loop_len = self.loop_len.max(rel + dur);
        self.frames_in_loop += 1;
        (rel, dur)
    }

    fn fallback_to_software(&mut self, why: &str, sh: &Shared) -> Result<(), String> {
        log::info!(
            "{}: hardware decode failed ({why}), using software",
            self.path
        );
        (self.dec, self.codec, self.hw) = open_decoder(&self.input, self.stream, false)?;
        if let Ok(mut c) = sh.codec.lock() {
            c.clone_from(&self.codec);
        }
        self.rewind()
    }

    /// Jumps to `t` seconds (the keyframe before it; `seek_target` drops frames up to `t`).
    fn seek_to(&mut self, t: f64) -> Result<(), String> {
        let ts = self.start_ts + (t / self.tb) as i64;
        // SAFETY: seeking the demuxer we own, on this thread.
        let ok = unsafe { ffi::av_seek_frame(self.input.as_mut_ptr(), self.stream as i32, ts, ffi::AVSEEK_FLAG_BACKWARD) >= 0 };
        if !ok {
            self.input = super::open_input(&self.path).map_err(|e| format!("can't reopen ({e})"))?;
        }
        self.dec.flush();
        self.eof_sent = false;
        self.last_rel = -1.0;
        self.frames_in_loop = 0;
        self.seek_target = Some(t);
        Ok(())
    }

    fn rewind(&mut self) -> Result<(), String> {
        let ok = unsafe {
            ffi::av_seek_frame(
                self.input.as_mut_ptr(),
                self.stream as i32,
                self.start_ts,
                ffi::AVSEEK_FLAG_BACKWARD,
            ) >= 0
        };
        if !ok {
            self.input =
                super::open_input(&self.path).map_err(|e| format!("can't reopen ({e})"))?;
        }
        self.dec.flush();
        self.eof_sent = false;
        self.last_rel = -1.0;
        self.loop_len = self.loop_len.max(1e-3);
        self.frames_in_loop = 0;
        Ok(())
    }

    /// Returns a CPU-accessible frame, mapping VideoToolbox surfaces when needed.
    fn download(&mut self, sh: &Shared) -> Result<*const ffi::AVFrame, String> {
        unsafe {
            let f = self.frame.as_ptr();
            if (*f).format != Px::AV_PIX_FMT_VIDEOTOOLBOX as i32 {
                return Ok(f);
            }
            sh.hardware.store(true, Ordering::Relaxed);
            let m = self.mapped.as_mut_ptr();
            ffi::av_frame_unref(m);
            if ffi::av_hwframe_map(m, f, ffi::AV_HWFRAME_MAP_READ as i32) < 0 {
                ffi::av_frame_unref(m);
                let r = ffi::av_hwframe_transfer_data(m, f, 0);
                if r < 0 {
                    return Err(format!(
                        "hardware frame download failed ({})",
                        ff::Error::from(r)
                    ));
                }
            }
            (*m).color_range = (*f).color_range;
            (*m).colorspace = (*f).colorspace;
            Ok(m)
        }
    }
}

/// Opens a decoder for `stream`, attaching VideoToolbox when `try_hw` and the codec supports it.
fn open_decoder(
    input: &ff::format::context::Input,
    stream: usize,
    try_hw: bool,
) -> Result<(ff::decoder::Video, String, bool), String> {
    use ff::codec::Id;
    let st = input.stream(stream).ok_or("bad stream")?;
    let alpha_mode = st.metadata().get("alpha_mode") == Some("1");
    let params = st.parameters();
    let id = params.id();
    let src_fmt = unsafe { (*params.as_ptr()).format };
    let has_alpha = alpha_mode || fmt_flags(src_fmt) & ffi::AV_PIX_FMT_FLAG_ALPHA as u64 != 0;
    // Codecs VideoToolbox handles on Apple Silicon; others (MPEG-4 Part 2...) only log init failures.
    // Hardware decoding is VideoToolbox, so only on the Mac (elsewhere the native AV1 decoder, which only works with
    // a hwaccel, would give no frames: libdav1d decodes AV1 there).
    let try_hw = try_hw
        && cfg!(target_os = "macos")
        // Alpha survives VideoToolbox for ProRes 4444 (only); other codecs with alpha decode in software.
        && (!has_alpha || (id == Id::PRORES && std::env::var_os("PHOSPHOR_SW_PRORES_ALPHA").is_none()))
        && matches!(id, Id::H264 | Id::HEVC | Id::PRORES | Id::VP9 | Id::AV1);

    let codec = match id {
        // libavcodec's native VP8/VP9 decoders drop the alpha side channel; libvpx keeps it.
        Id::VP9 if alpha_mode => ff::decoder::find_by_name("libvpx-vp9"),
        Id::VP8 if alpha_mode => ff::decoder::find_by_name("libvpx"),
        // The native AV1 decoder is hwaccel-only; libdav1d (the default) is the software path.
        Id::AV1 if try_hw => ff::decoder::find_by_name("av1"),
        _ => None,
    }
    .or_else(|| ff::decoder::find(id))
    .ok_or_else(|| format!("unsupported codec ({id:?})"))?;

    let mut ctx = ff::codec::Context::from_parameters(params).map_err(err)?;
    // FFmpeg's Hap decoder refuses Hap R's tag; its frames never reach it (they go to the GPU as they are), so open it
    // as plain Hap.
    if id == Id::HAP && unsafe { (*ctx.as_ptr()).codec_tag } == u32::from_le_bytes(super::HAP_R) {
        unsafe { (*ctx.as_mut_ptr()).codec_tag = u32::from_le_bytes(*b"Hap1") };
    }
    let hw = try_hw && unsafe { attach_videotoolbox(ctx.as_mut_ptr(), codec.as_ptr()) };
    unsafe { (*ctx.as_mut_ptr()).pkt_timebase = (*st.as_ptr()).time_base };
    if !hw {
        let threads = std::thread::available_parallelism()
            .map_or(4, |n| n.get())
            .min(8);
        ctx.set_threading(ff::codec::threading::Config {
            kind: ff::codec::threading::Type::Frame,
            count: threads,
        });
    }
    let dec = ctx
        .decoder()
        .open_as(codec)
        .and_then(|o| o.video())
        .map_err(|e| format!("can't open {} decoder ({e})", codec.name()))?;
    if dec.width() == 0 || dec.height() == 0 {
        return Err("video has no size".into());
    }
    Ok((dec, codec.name().to_string(), hw))
}

/// Enables VideoToolbox on `ctx` if the decoder supports it. FFmpeg falls back to software
/// automatically if the hwaccel can't handle a particular stream.
unsafe fn attach_videotoolbox(ctx: *mut ffi::AVCodecContext, codec: *const ffi::AVCodec) -> bool {
    unsafe {
        let mut supported = false;
        for i in 0.. {
            let cfg = ffi::avcodec_get_hw_config(codec, i);
            if cfg.is_null() {
                break;
            }
            if (*cfg).device_type == ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX
                && (*cfg).methods & ffi::AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as i32 != 0
            {
                supported = true;
                break;
            }
        }
        if !supported {
            return false;
        }
        let mut dev: *mut ffi::AVBufferRef = ptr::null_mut();
        let r = ffi::av_hwdevice_ctx_create(
            &mut dev,
            ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
            ptr::null(),
            ptr::null_mut(),
            0,
        );
        if r < 0 || dev.is_null() {
            return false;
        }
        (*ctx).hw_device_ctx = dev;
        (*ctx).get_format = Some(pick_videotoolbox);
        true
    }
}

unsafe extern "C" fn pick_videotoolbox(ctx: *mut ffi::AVCodecContext, fmts: *const Px) -> Px {
    unsafe {
        let mut p = fmts;
        while *p != Px::AV_PIX_FMT_NONE {
            if *p == Px::AV_PIX_FMT_VIDEOTOOLBOX {
                return *p;
            }
            p = p.add(1);
        }
        ffi::avcodec_default_get_format(ctx, fmts)
    }
}

fn st_is_hap(input: &ff::format::context::Input, stream: usize) -> bool {
    input.stream(stream).is_some_and(|s| s.parameters().id() == ff::codec::Id::HAP)
}

/// A group of frames being decoded for reverse play: everything before `end`, kept from `keep_from` (known once
/// the first frame, the keyframe, is out).
#[derive(Clone, Copy, Debug)]
struct RevJob {
    end: f64,
    keep_from: Option<f64>,
    done: bool,
    /// Media time of the last frame decoded (NaN before the first): how much is left.
    at: f64,
    /// The group at the end of the section, decoded early because reverse is about to loop round to it.
    wrap: bool,
    /// How far before `end` the seek went (grows when a seek lands too late).
    back: f64,
}
