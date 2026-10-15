//! Decoded frames to the two layouts the engine uploads: NV12 planes (most video) or RGBA (alpha, palette).

use super::*;

pub(super) fn fmt_flags(fmt: i32) -> u64 {
    if fmt < 0 {
        return 0;
    }
    // SAFETY: `fmt` came from FFmpeg, so it is a valid AVPixelFormat discriminant.
    let desc = unsafe { ffi::av_pix_fmt_desc_get(std::mem::transmute::<i32, Px>(fmt)) };
    if desc.is_null() {
        0
    } else {
        unsafe { (*desc).flags }
    }
}

/// Cached swscale context plus the direct NV12 copy path.
#[derive(Default)]
pub(super) struct Scaler {
    ctx: Option<ptr::NonNull<ffi::SwsContext>>,
    key: (i32, i32, i32, bool, i32, i32),
}

unsafe impl Send for Scaler {}

impl Drop for Scaler {
    fn drop(&mut self) {
        if let Some(c) = self.ctx.take() {
            unsafe { ffi::sws_freeContext(c.as_ptr()) };
        }
    }
}

impl Scaler {
    pub(super) fn convert(&mut self, f: *const ffi::AVFrame, sh: &Shared) -> Result<VideoFrame, String> {
        let f = unsafe { &*f };
        let (w, h) = (f.width.max(0) as u32, f.height.max(0) as u32);
        if w == 0 || h == 0 {
            return Err("decoded frame has no size".into());
        }
        let flags = fmt_flags(f.format);
        let rgba = f.format == Px::AV_PIX_FMT_PAL8 as i32
            || flags
                & (ffi::AV_PIX_FMT_FLAG_ALPHA | ffi::AV_PIX_FMT_FLAG_PAL | ffi::AV_PIX_FMT_FLAG_RGB)
                    as u64
                != 0;
        let full_range = f.color_range == ffi::AVColorRange::AVCOL_RANGE_JPEG;
        let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));

        let mut out = sh.take(rgba, w, h).unwrap_or_else(|| {
            if rgba {
                VideoFrame::Rgba {
                    width: w,
                    height: h,
                    data: vec![0; (w * h * 4) as usize],
                }
            } else {
                VideoFrame::Nv12 {
                    width: w,
                    height: h,
                    luma: vec![0; (w * h) as usize],
                    luma_stride: w,
                    chroma: vec![0; (cw * 2 * ch) as usize],
                    chroma_stride: cw * 2,
                }
            }
        });

        // NV12 straight from the decoder (VideoToolbox): plain row copy, no swscale.
        if !rgba
            && !full_range
            && f.format == Px::AV_PIX_FMT_NV12 as i32
            && let VideoFrame::Nv12 { luma, chroma, .. } = &mut out
            && copy_plane(luma, w as usize, f.data[0], f.linesize[0], h as usize)
            && copy_plane(
                chroma,
                (cw * 2) as usize,
                f.data[1],
                f.linesize[1],
                ch as usize,
            )
        {
            return Ok(out);
        }

        let dst_fmt = if rgba {
            Px::AV_PIX_FMT_RGBA
        } else {
            Px::AV_PIX_FMT_NV12
        };
        let space = f.colorspace as i32;
        let key = (
            f.format,
            f.width,
            f.height,
            full_range,
            space,
            dst_fmt as i32,
        );
        if self.ctx.is_none() || self.key != key {
            if let Some(c) = self.ctx.take() {
                unsafe { ffi::sws_freeContext(c.as_ptr()) };
            }
            let c = unsafe {
                ffi::sws_getContext(
                    f.width,
                    f.height,
                    std::mem::transmute::<i32, Px>(f.format),
                    f.width,
                    f.height,
                    dst_fmt,
                    ffi::SwsFlags::SWS_BILINEAR as i32,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null(),
                )
            };
            let c = ptr::NonNull::new(c).ok_or("unsupported pixel format")?;
            let cs = match f.colorspace {
                ffi::AVColorSpace::AVCOL_SPC_BT709 => ffi::SWS_CS_ITU709,
                ffi::AVColorSpace::AVCOL_SPC_BT2020_NCL
                | ffi::AVColorSpace::AVCOL_SPC_BT2020_CL => ffi::SWS_CS_BT2020,
                ffi::AVColorSpace::AVCOL_SPC_BT470BG | ffi::AVColorSpace::AVCOL_SPC_SMPTE170M => {
                    ffi::SWS_CS_ITU601
                }
                _ if f.height >= 720 => ffi::SWS_CS_ITU709,
                _ => ffi::SWS_CS_ITU601,
            };
            unsafe {
                let coeffs = ffi::sws_getCoefficients(cs);
                ffi::sws_setColorspaceDetails(
                    c.as_ptr(),
                    coeffs,
                    full_range as i32,
                    coeffs,
                    rgba as i32,
                    0,
                    1 << 16,
                    1 << 16,
                );
            }
            self.ctx = Some(c);
            self.key = key;
        }
        let Some(c) = self.ctx else {
            return Err("no scaler".into());
        };
        let (dst, stride): ([*mut u8; 4], [i32; 4]) = match &mut out {
            VideoFrame::Rgba { data, .. } => (
                [
                    data.as_mut_ptr(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                ],
                [(w * 4) as i32, 0, 0, 0],
            ),
            VideoFrame::Nv12 { luma, chroma, .. } => (
                [
                    luma.as_mut_ptr(),
                    chroma.as_mut_ptr(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                ],
                [w as i32, (cw * 2) as i32, 0, 0],
            ),
        };
        let r = unsafe {
            ffi::sws_scale(
                c.as_ptr(),
                f.data.as_ptr() as *const *const u8,
                f.linesize.as_ptr(),
                0,
                f.height,
                dst.as_ptr(),
                stride.as_ptr(),
            )
        };
        if r <= 0 {
            return Err("pixel format conversion failed".into());
        }
        Ok(out)
    }
}

/// Copies `rows` rows of `width` bytes from a strided FFmpeg plane into a tightly packed buffer.
fn copy_plane(dst: &mut [u8], width: usize, src: *const u8, linesize: i32, rows: usize) -> bool {
    if src.is_null() || linesize < width as i32 || dst.len() < width * rows {
        return false;
    }
    for y in 0..rows {
        // SAFETY: FFmpeg planes hold `rows` lines of `linesize` bytes, and linesize >= width.
        let line = unsafe { std::slice::from_raw_parts(src.add(y * linesize as usize), width) };
        dst[y * width..(y + 1) * width].copy_from_slice(line);
    }
    true
}
