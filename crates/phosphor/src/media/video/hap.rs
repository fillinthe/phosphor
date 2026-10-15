//! Hap frames straight to GPU blocks: reads the Hap section headers, undoes Snappy (whole frame or chunked),
//! and hands back DXT1/DXT5 blocks the GPU samples directly (`Engine::upload_compressed`). No pixel decoding.
//! Format reference: the Hap specification (github.com/Vidvox/hap/blob/master/documentation/HapVideoDRAFT.md).

use phosphor_engine::CompressedFormat;

/// One section: (header length, payload length, type byte).
fn section(b: &[u8]) -> Result<(usize, usize, u8), String> {
    if b.len() < 4 {
        return Err("truncated Hap section".into());
    }
    let size = b[0] as usize | (b[1] as usize) << 8 | (b[2] as usize) << 16;
    if size != 0 {
        return Ok((4, size, b[3]));
    }
    if b.len() < 8 {
        return Err("truncated Hap section".into());
    }
    Ok((8, u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize, b[3]))
}

fn texture(ty: u8) -> Result<CompressedFormat, String> {
    match ty & 0x0F {
        0x0B => Ok(CompressedFormat::Bc1),
        0x0E => Ok(CompressedFormat::Bc3),
        0x0F => Ok(CompressedFormat::Bc3YCoCg),
        other => Err(format!("Hap texture type {other:#x} (Hap R / Hap Q Alpha) isn't GPU-decoded yet")),
    }
}

/// Decodes one Hap packet into DXT blocks in `out` (reused between frames). Returns the block format.
pub fn decode(packet: &[u8], out: &mut Vec<u8>) -> Result<CompressedFormat, String> {
    let (head, size, ty) = section(packet)?;
    let body = packet.get(head..head + size).ok_or("truncated Hap frame")?;
    let format = texture(ty)?;
    out.clear();
    match ty >> 4 {
        0x0A => out.extend_from_slice(body),
        0x0B => snappy(body, out)?,
        0x0C => chunked(body, out)?,
        other => return Err(format!("Hap compressor {other:#x} isn't supported")),
    }
    Ok(format)
}

fn snappy(src: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
    let len = snap::raw::decompress_len(src).map_err(|e| e.to_string())?;
    let start = out.len();
    out.resize(start + len, 0);
    snap::raw::Decoder::new().decompress(src, &mut out[start..]).map_err(|e| e.to_string())?;
    Ok(())
}

/// "Complex" frames: a decode-instructions section (per-chunk compressor, sizes, optional offsets), then the chunks.
fn chunked(body: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
    let (head, size, ty) = section(body)?;
    if ty != 0x01 {
        return Err("Hap frame without decode instructions".into());
    }
    let instructions = body.get(head..head + size).ok_or("truncated decode instructions")?;
    let data = &body[head + size..];
    let (mut compressors, mut sizes, mut offsets): (&[u8], Vec<usize>, Option<Vec<usize>>) = (&[], Vec::new(), None);
    let words = |b: &[u8]| b.chunks_exact(4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]) as usize).collect::<Vec<_>>();
    let mut at = 0;
    while at < instructions.len() {
        let (h, n, t) = section(&instructions[at..])?;
        let payload = instructions.get(at + h..at + h + n).ok_or("truncated decode instruction")?;
        match t {
            0x02 => compressors = payload,
            0x03 => sizes = words(payload),
            0x04 => offsets = Some(words(payload)),
            _ => {}
        }
        at += h + n;
    }
    if compressors.len() != sizes.len() {
        return Err("Hap chunk tables don't match".into());
    }
    let mut pos = 0;
    for (i, (&c, &n)) in compressors.iter().zip(&sizes).enumerate() {
        let start = offsets.as_ref().and_then(|o| o.get(i).copied()).unwrap_or(pos);
        let chunk = data.get(start..start + n).ok_or("truncated Hap chunk")?;
        match c {
            0x0A => out.extend_from_slice(chunk),
            0x0B => snappy(chunk, out)?,
            other => return Err(format!("Hap chunk compressor {other:#x}")),
        }
        pos = start + n;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(size: usize, ty: u8) -> Vec<u8> {
        if size < 1 << 24 { vec![size as u8, (size >> 8) as u8, (size >> 16) as u8, ty] } else { let mut h = vec![0, 0, 0, ty]; h.extend((size as u32).to_le_bytes()); h }
    }

    fn blocks() -> Vec<u8> {
        (0..4096u32).flat_map(|i| (i.wrapping_mul(2654435761) >> 7).to_le_bytes()).collect()
    }

    #[test]
    fn uncompressed_snappy_and_chunked_frames_give_the_same_blocks() {
        let raw = blocks();
        let mut out = Vec::new();
        // Uncompressed DXT1.
        let mut p = header(raw.len(), 0xAB);
        p.extend(&raw);
        assert_eq!(decode(&p, &mut out).unwrap(), CompressedFormat::Bc1);
        assert_eq!(out, raw);
        // Snappy, Hap Q.
        let z = snap::raw::Encoder::new().compress_vec(&raw).unwrap();
        let mut p = header(z.len(), 0xBF);
        p.extend(&z);
        assert_eq!(decode(&p, &mut out).unwrap(), CompressedFormat::Bc3YCoCg);
        assert_eq!(out, raw);
        // Complex: two chunks, one snappy, one stored, DXT5.
        let (a, b) = raw.split_at(raw.len() / 2);
        let za = snap::raw::Encoder::new().compress_vec(a).unwrap();
        let mut instr = header(2, 0x02);
        instr.extend([0x0B, 0x0A]);
        instr.extend(header(8, 0x03));
        instr.extend((za.len() as u32).to_le_bytes());
        instr.extend((b.len() as u32).to_le_bytes());
        let mut body = header(instr.len(), 0x01);
        body.extend(&instr);
        body.extend(&za);
        body.extend(b);
        let mut p = header(body.len(), 0xCE);
        p.extend(&body);
        assert_eq!(decode(&p, &mut out).unwrap(), CompressedFormat::Bc3);
        assert_eq!(out, raw);
        // Hap R isn't GPU-decoded (caller falls back to FFmpeg).
        let mut p = header(4, 0xA1);
        p.extend([0, 0, 0, 0]);
        assert!(decode(&p, &mut out).is_err());
    }
}
