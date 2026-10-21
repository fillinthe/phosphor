//! The Session's low-latency picture: WebRTC (WHEP). A browser posts its offer to `/whep`; each viewer gets its own
//! WebRTC connection (str0m, its own UDP socket and thread). One H.264 encoder (the bundled `ffmpeg`, hardware on
//! the Mac, no B-frames, a keyframe every second) runs only while a WebRTC viewer is connected, fed the same frames
//! the MJPEG viewer gets. The player page tries WebRTC first and falls back to MJPEG.

use std::io::{Read, Write};
use std::net::UdpSocket;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender, TrySendError, channel, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use str0m::change::SdpOffer;
use str0m::format::Codec;
use str0m::media::{Frequency, MediaTime, Mid};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc, RtcConfig};

/// One encoded picture (an H.264 access unit, Annex B) and its time on the 90 kHz clock.
#[derive(Clone)]
pub struct Picture {
    pub data: Arc<[u8]>,
    pub time: u64,
    pub key: bool,
}

/// The running encoder: frames in (raw RGBA), access units out to every viewer.
struct Encoder {
    size: (u32, u32),
    frames: SyncSender<Vec<u8>>,
}

#[derive(Default)]
struct Inner {
    encoder: Option<Encoder>,
    viewers: Vec<Sender<Picture>>,
    fps: u32,
}

/// Shared by the session server (answers offers) and the app (pushes frames).
#[derive(Clone, Default)]
pub struct Hub {
    inner: Arc<Mutex<Inner>>,
    pub viewers: Arc<AtomicUsize>,
}

/// The access unit delimiter's start: every picture `ffmpeg` writes begins with one (`h264_metadata=aud=insert`).
const AUD: [u8; 5] = [0, 0, 0, 1, 9];

/// Splits an H.264 byte stream into access units at each access unit delimiter. Returns the complete ones and keeps
/// the rest in `buf`.
pub fn split_access_units(buf: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let starts: Vec<usize> = buf.windows(5).enumerate().filter(|(_, w)| *w == AUD).map(|(i, _)| i).take(2).collect();
        match starts[..] {
            [a, b] => {
                out.push(buf[a..b].to_vec());
                buf.drain(..b);
            }
            [a] if a > 0 => {
                buf.drain(..a);
            }
            _ => break,
        }
    }
    out
}

/// Whether an access unit holds an IDR picture (NAL type 5): where a new viewer can start.
pub fn is_keyframe(au: &[u8]) -> bool {
    let mut i = 0;
    while i + 4 < au.len() {
        if au[i..i + 3] == [0, 0, 1] {
            if au[i + 3] & 0x1f == 5 {
                return true;
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    false
}

impl Hub {
    /// Frames per second the encoder is told (the session's rate).
    pub fn set_fps(&self, fps: u32) {
        self.inner.lock().unwrap().fps = fps.clamp(5, 60);
    }

    /// A frame from the output (RGBA): encoded only while someone watches over WebRTC. Never blocks: a busy
    /// encoder drops the frame.
    pub fn push(&self, w: u32, h: u32, rgba: &[u8]) {
        if self.viewers.load(Ordering::Relaxed) == 0 {
            let mut inner = self.inner.lock().unwrap();
            inner.encoder = None;
            return;
        }
        let mut inner = self.inner.lock().unwrap();
        if inner.encoder.as_ref().is_none_or(|e| e.size != (w, h)) {
            let fps = if inner.fps == 0 { 20 } else { inner.fps };
            inner.encoder = start_encoder(self.inner.clone(), (w, h), fps);
        }
        if let Some(e) = &inner.encoder
            && let Err(TrySendError::Disconnected(_)) = e.frames.try_send(rgba.to_vec())
        {
            inner.encoder = None;
        }
    }

    /// Answers a browser's WHEP offer (SDP text): sets up its connection on its own thread and returns the answer
    /// (SDP text). `host` = this Mac's address on the network the browser reached it on.
    pub fn answer(&self, offer: &str, host: std::net::IpAddr) -> Result<String, String> {
        static CRYPTO: std::sync::Once = std::sync::Once::new();
        CRYPTO.call_once(|| str0m::crypto::from_feature_flags().install_process_default());
        let offer = SdpOffer::from_sdp_string(offer).map_err(|e| format!("bad offer: {e}"))?;
        let mut rtc = RtcConfig::new().clear_codecs().enable_h264(true).set_ice_lite(true).build(Instant::now());
        let socket = UdpSocket::bind((host, 0)).map_err(|e| e.to_string())?;
        let addr = socket.local_addr().map_err(|e| e.to_string())?;
        let candidate = Candidate::host(addr, "udp").map_err(|e| e.to_string())?;
        rtc.add_local_candidate(candidate);
        let answer = rtc.sdp_api().accept_offer(offer).map_err(|e| format!("can't answer: {e}"))?;
        let (tx, rx) = channel();
        self.inner.lock().unwrap().viewers.push(tx);
        self.viewers.fetch_add(1, Ordering::Relaxed);
        let count = self.viewers.clone();
        std::thread::Builder::new()
            .name("webrtc viewer".into())
            .spawn(move || {
                run_viewer(rtc, socket, rx);
                count.fetch_sub(1, Ordering::Relaxed);
            })
            .map_err(|e| e.to_string())?;
        Ok(answer.to_sdp_string())
    }
}

/// Starts `ffmpeg` encoding raw RGBA at `size` to H.264 (an access unit delimiter before each picture) and a thread
/// that hands each picture to every viewer.
fn start_encoder(inner: Arc<Mutex<Inner>>, size: (u32, u32), fps: u32) -> Option<Encoder> {
    let ffmpeg = crate::stream::ffmpeg_path()?;
    let codec: &[&str] = if cfg!(target_os = "macos") {
        &["-c:v", "h264_videotoolbox", "-realtime", "1", "-profile:v", "baseline", "-allow_sw", "1"]
    } else {
        &["-c:v", "libx264", "-preset", "ultrafast", "-tune", "zerolatency", "-profile:v", "baseline"]
    };
    let mut child = std::process::Command::new(ffmpeg)
        .args(["-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{}x{}", size.0, size.1), "-r", &fps.to_string(), "-i", "-"])
        .args(["-vf", "scale='min(1280,iw)':-2,format=yuv420p"])
        .args(codec)
        .args(["-b:v", "3000k", "-g", &fps.to_string(), "-bf", "0", "-bsf:v", "h264_metadata=aud=insert", "-f", "h264", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = sync_channel::<Vec<u8>>(1);
    std::thread::Builder::new()
        .name("webrtc encode in".into())
        .spawn(move || {
            for frame in rx {
                if stdin.write_all(&frame).is_err() {
                    break;
                }
            }
            let _ = child.kill();
            let _ = child.wait();
        })
        .ok()?;
    std::thread::Builder::new()
        .name("webrtc encode out".into())
        .spawn(move || {
            let mut buf = Vec::new();
            let mut chunk = vec![0u8; 64 * 1024];
            let mut n_frame: u64 = 0;
            while let Ok(n) = stdout.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                for au in split_access_units(&mut buf) {
                    let p = Picture { key: is_keyframe(&au), data: au.into(), time: n_frame * 90_000 / fps as u64 };
                    n_frame += 1;
                    inner.lock().unwrap().viewers.retain(|v| v.send(p.clone()).is_ok());
                }
            }
        })
        .ok()?;
    Some(Encoder { size, frames: tx })
}

/// One viewer's connection: UDP in and out, pictures from the encoder (from the first keyframe on), until the
/// browser goes away.
fn run_viewer(mut rtc: Rtc, socket: UdpSocket, pictures: Receiver<Picture>) {
    let local = match socket.local_addr() {
        Ok(a) => a,
        Err(_) => return,
    };
    let mut buf = vec![0u8; 2000];
    let mut mid: Option<Mid> = None;
    let mut started = false;
    let since = Instant::now();
    loop {
        // Everything str0m wants to send, until it says when it next needs time.
        let deadline = loop {
            match rtc.poll_output() {
                Ok(Output::Transmit(t)) => {
                    let _ = socket.send_to(&t.contents, t.destination);
                }
                Ok(Output::Timeout(t)) => break t,
                Ok(Output::Event(e)) => match e {
                    Event::IceConnectionStateChange(IceConnectionState::Disconnected) => return,
                    Event::MediaAdded(m) => mid = Some(m.mid),
                    _ => {}
                },
                Err(_) => return,
            }
        };
        // New pictures go out as soon as they come (a viewer that joined mid-stream waits for a keyframe).
        while let Ok(p) = pictures.try_recv() {
            let Some(m) = mid else { continue };
            if !started && !p.key {
                continue;
            }
            let Some(writer) = rtc.writer(m) else { continue };
            let pt = writer.payload_params().find(|pp| pp.spec().codec == Codec::H264 && pp.spec().format.packetization_mode == Some(1)).or_else(|| writer.payload_params().find(|pp| pp.spec().codec == Codec::H264)).map(|pp| pp.pt());
            let Some(pt) = pt else { continue };
            started = true;
            if writer.write(pt, Instant::now(), MediaTime::new(p.time, Frequency::NINETY_KHZ), p.data.clone()).is_err() {
                return;
            }
        }
        if !rtc.is_alive() || (mid.is_none() && since.elapsed() > Duration::from_secs(20)) {
            return;
        }
        // Wait for a packet, str0m's next timer, or a few ms (so pictures don't wait).
        let wait = deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(5));
        if wait.is_zero() {
            let _ = rtc.handle_input(Input::Timeout(Instant::now()));
            continue;
        }
        let _ = socket.set_read_timeout(Some(wait));
        let input = match socket.recv_from(&mut buf) {
            Ok((n, source)) => match buf[..n].try_into() {
                Ok(contents) => Input::Receive(Instant::now(), Receive { proto: Protocol::Udp, source, destination: local, contents }),
                Err(_) => continue,
            },
            Err(_) => Input::Timeout(Instant::now()),
        };
        if rtc.handle_input(input).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_byte_stream_splits_into_pictures_at_the_delimiters() {
        let idr = [0, 0, 0, 1, 0x65, 1, 2, 3];
        let p = [0, 0, 0, 1, 0x41, 9, 9];
        let mut buf: Vec<u8> = [&AUD[..], &[0xf0], &idr, &AUD[..], &[0xf0], &p, &AUD[..], &[0xf0]].concat();
        let aus = split_access_units(&mut buf);
        assert_eq!(aus.len(), 2);
        assert!(is_keyframe(&aus[0]) && !is_keyframe(&aus[1]));
        assert_eq!(buf, [&AUD[..], &[0xf0]].concat(), "the last, unfinished picture waits for more bytes");
    }

    #[test]
    fn an_offer_from_a_browser_gets_an_answer_with_h264() {
        let offer = "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\na=group:BUNDLE 0\r\na=msid-semantic: WMS\r\nm=video 9 UDP/TLS/RTP/SAVPF 102\r\nc=IN IP4 0.0.0.0\r\na=rtcp:9 IN IP4 0.0.0.0\r\na=ice-ufrag:abcd\r\na=ice-pwd:abcdefghijklmnopqrstuvwx\r\na=ice-options:trickle\r\na=fingerprint:sha-256 6B:8B:F0:65:5F:78:E2:51:3B:AC:6F:F3:3F:46:1B:35:DC:B8:5F:64:1A:24:C2:43:F0:A1:58:D0:A1:2C:19:08\r\na=setup:actpass\r\na=mid:0\r\na=recvonly\r\na=rtcp-mux\r\na=rtcp-rsize\r\na=rtpmap:102 H264/90000\r\na=rtcp-fb:102 nack\r\na=rtcp-fb:102 nack pli\r\na=fmtp:102 level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f\r\n";
        let hub = Hub::default();
        let answer = hub.answer(offer, "127.0.0.1".parse().unwrap()).expect("answer");
        assert!(answer.contains("H264/90000") && answer.contains("a=candidate"), "{answer}");
    }
}
