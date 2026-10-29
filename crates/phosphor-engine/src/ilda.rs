//! ILDA files (.ild, the laser world's interchange format): every frame's points. Formats 0, 1 (indexed colour,
//! 3D / 2D), 2 (a palette), 4 and 5 (true colour, 3D / 2D). Used by laser looks (`laser`) and by the app, which
//! also plays these files as video clips.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    /// -1..1, y up (as in the file).
    pub x: f32,
    pub y: f32,
    pub rgb: [u8; 3],
    /// The beam is off on the way to this point.
    pub blank: bool,
}

/// The ILDA standard 64-colour palette (used until a file brings its own): red → yellow → green → cyan → blue →
/// magenta → white → back toward red.
pub fn default_palette() -> Vec<[u8; 3]> {
    let mut p = Vec::with_capacity(64);
    for i in 0..16u32 {
        p.push([255, (i * 16) as u8, 0]);
    }
    for i in 0..8u32 {
        p.push([(256 - i * 32).min(255) as u8, 255, 0]);
    }
    for i in 0..7u32 {
        p.push([0, 255, ((i * 255 + 3) / 7) as u8]);
    }
    for i in 0..9u32 {
        p.push([0, ((9 - i) * 255 / 9) as u8, 255]);
    }
    for i in 0..8u32 {
        p.push([(i * 32) as u8, 0, 255]);
    }
    for i in 0..8u32 {
        p.push([255, (i * 32) as u8, 255]);
    }
    for i in 0..8u32 {
        let v = (256 - i * 32).min(255) as u8;
        p.push([255, v, v]);
    }
    p
}

/// Every frame of an ILDA file (palette sections change the colours of the frames after them).
pub fn parse(data: &[u8]) -> Result<Vec<Vec<Point>>, String> {
    let mut frames = Vec::new();
    let mut palette = default_palette();
    let mut at = 0usize;
    let be16 = |b: &[u8]| i16::from_be_bytes([b[0], b[1]]);
    while at + 32 <= data.len() {
        let h = &data[at..at + 32];
        if &h[..4] != b"ILDA" {
            return Err(if frames.is_empty() { "not an ILDA laser file".into() } else { "the file is cut short".into() });
        }
        let format = h[7];
        let records = u16::from_be_bytes([h[24], h[25]]) as usize;
        at += 32;
        if records == 0 {
            break;
        }
        let size = match format {
            0 => 8,
            1 => 6,
            2 => 3,
            4 => 10,
            5 => 8,
            f => return Err(format!("ILDA format {f} isn't supported")),
        };
        let body = data.get(at..at + records * size).ok_or("the file is cut short")?;
        at += records * size;
        if format == 2 {
            palette = body.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
            continue;
        }
        let colour = |i: u8| palette.get(i as usize).copied().unwrap_or([255, 255, 255]);
        let frame = body
            .chunks_exact(size)
            .map(|r| {
                // The status byte comes after the coordinates (x, y and, in 3D formats, z).
                let s = if format == 0 || format == 4 { 6 } else { 4 };
                let rgb = match format {
                    0 | 1 => colour(r[s + 1]),
                    _ => [r[s + 3], r[s + 2], r[s + 1]],
                };
                Point { x: be16(&r[0..2]) as f32 / 32767.0, y: be16(&r[2..4]) as f32 / 32767.0, rgb, blank: r[s] & 0x40 != 0 }
            })
            .collect();
        frames.push(frame);
    }
    if frames.is_empty() {
        return Err("the laser file has no frames".into());
    }
    Ok(frames)
}
