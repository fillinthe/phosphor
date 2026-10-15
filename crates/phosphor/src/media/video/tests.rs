use super::{VideoFrame, VideoPlayer};

fn luma(f: &VideoFrame) -> Vec<u8> {
    match f {
        VideoFrame::Nv12 { luma, .. } => luma.clone(),
        VideoFrame::Rgba { data, .. } | VideoFrame::Compressed { data, .. } => data.clone(),
    }
}

/// Plays `p` on a fake clock from `from` to `to` seconds and returns the last frame shown.
fn play(p: &mut VideoPlayer, from: f64, to: f64) -> Option<Vec<u8>> {
    let mut last = None;
    let mut t = from;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while t <= to + 1e-9 && std::time::Instant::now() < deadline {
        // Give the decoder a moment, then advance the clock a frame.
        for _ in 0..200 {
            if let Some(f) = p.poll_frame(t) {
                last = Some(luma(&f));
                p.recycle(f);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        t += 1.0 / 30.0;
    }
    last
}

/// Jumping to 1.5 s shows the same picture as playing up to 1.5 s (the clip is 2.5 s).
#[test]
fn seek_lands_on_the_right_frame() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../media/test/h264.mp4");
    if !std::path::Path::new(path).exists() {
        return;
    }
    let mut played = VideoPlayer::open(path).unwrap();
    let reference = play(&mut played, 0.0, 1.5).expect("frames");
    let mut jumped = VideoPlayer::open(path).unwrap();
    jumped.poll_frame(0.0);
    jumped.seek(1.5);
    assert!((jumped.position() - 1.5).abs() < 1e-6);
    let landed = play(&mut jumped, 0.0, 0.0).expect("a frame after seeking");
    let diff: u64 = reference.iter().zip(&landed).map(|(a, b)| a.abs_diff(*b) as u64).sum::<u64>() / reference.len().max(1) as u64;
    assert!(diff <= 1, "picture after seek differs from playing to 1.5 s (mean diff {diff})");
}

/// Media times of the frames shown while playing `secs` on a fake clock.
fn positions(p: &mut VideoPlayer, secs: f64) -> Vec<f64> {
    let mut out = Vec::new();
    let mut t = 0.0;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while t <= secs && std::time::Instant::now() < deadline {
        for _ in 0..300 {
            if let Some(f) = p.poll_frame(t) {
                out.push(p.position());
                p.recycle(f);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        t += 1.0 / 30.0;
    }
    out
}

fn clip() -> Option<&'static str> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../media/test/h264.mp4");
    std::path::Path::new(path).exists().then_some(path)
}

/// Reverse play shows earlier and earlier frames, wrapping from the start to the end.
#[test]
fn reverse_plays_backwards_and_loops() {
    let Some(path) = clip() else { return };
    let mut p = VideoPlayer::open(path).unwrap();
    p.poll_frame(0.0);
    p.set_mode(super::PlayDirection::Reverse, None);
    let pos = positions(&mut p, 4.0);
    assert!(pos.len() > 60, "too few frames ({})", pos.len());
    let down = pos.windows(2).filter(|w| w[1] < w[0]).count();
    let wraps = pos.windows(2).filter(|w| w[1] > w[0] + 1.0).count();
    assert!(down >= pos.len() * 9 / 10 - 2, "not playing backwards: {pos:?}");
    assert!(wraps >= 1, "never wrapped to the end: {pos:?}");
}

/// A loop region keeps playback inside it; ping-pong turns around at both ends.
#[test]
fn loop_region_and_ping_pong() {
    let Some(path) = clip() else { return };
    let mut p = VideoPlayer::open(path).unwrap();
    p.set_mode(super::PlayDirection::Forward, Some((0.5, 1.0)));
    let pos = positions(&mut p, 2.0);
    let late = &pos[pos.len() / 4..];
    assert!(late.iter().all(|t| (0.45..1.0).contains(t)), "left the loop region: {pos:?}");
    let mut q = VideoPlayer::open(path).unwrap();
    q.set_mode(super::PlayDirection::PingPong, Some((0.5, 1.0)));
    let pos = positions(&mut q, 3.0);
    let late = &pos[pos.len() / 6..];
    assert!(late.iter().all(|t| (0.45..1.0).contains(t)), "left the loop region: {pos:?}");
    let turns = late.windows(3).filter(|w| (w[1] - w[0]).signum() != (w[2] - w[1]).signum()).count();
    assert!(turns >= 2, "never turned around: {pos:?}");
    let jumps = late.windows(2).filter(|w| (w[1] - w[0]).abs() > 0.2).count();
    assert_eq!(jumps, 0, "ping-pong jumped instead of turning: {pos:?}");
}
