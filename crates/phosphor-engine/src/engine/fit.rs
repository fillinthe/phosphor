//! Placing a frame on the canvas (fit / fill / stretch).

use super::*;

/// Draws `frame` into `target` the way it would sit on the canvas (fit/fill/stretch).
pub(super) fn push_fit_copy(target: wgpu::TextureView, frame: &Frame, canvas: (u32, u32), fit: FitMode, size: (u32, u32), passes: &mut Vec<Pass>) {
    let mut u = Uniforms::with_params(&[]);
    u.resolution = [size.0 as f32, size.1 as f32];
    u.opacity = 1.0;
    u.uv_xform = fit_transform(frame.size(), canvas, fit);
    let (tex0, tex2) = match frame.clone() {
        Frame::Rgb { view, .. } => (view, None),
        Frame::Yuv { luma, chroma, .. } => {
            u.yuv = 1;
            (luma, Some(chroma))
        }
    };
    passes.push(Pass { entry: "fs_composite", target, tex0, tex1: None, tex2, uniforms: u, mesh: None, bufs: Vec::new() });
}

/// Maps target uv → source uv so the source is fitted, filled, or stretched.
pub(super) fn fit_transform(source: (u32, u32), target: (u32, u32), mode: FitMode) -> [f32; 4] {
    let (sw, sh) = (source.0.max(1) as f32, source.1.max(1) as f32);
    let (tw, th) = (target.0.max(1) as f32, target.1.max(1) as f32);
    let k = match mode {
        FitMode::Stretch => return [1.0, 1.0, 0.0, 0.0],
        FitMode::Fit => (tw / sw).min(th / sh),
        FitMode::Fill => (tw / sw).max(th / sh),
    };
    let sx = tw / (sw * k);
    let sy = th / (sh * k);
    [sx, sy, 0.5 - 0.5 * sx, 0.5 - 0.5 * sy]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub(super) fn fit_letterboxes_and_fill_crops() {
        // 4:3 source into 16:9 target.
        let fit = fit_transform((640, 480), (1920, 1080), FitMode::Fit);
        assert!((fit[1] - 1.0).abs() < 1e-6 && fit[0] > 1.0, "fit pads horizontally: {fit:?}");
        let fill = fit_transform((640, 480), (1920, 1080), FitMode::Fill);
        assert!((fill[0] - 1.0).abs() < 1e-6 && fill[1] < 1.0, "fill crops vertically: {fill:?}");
        assert_eq!(fit_transform((1, 1), (9, 9), FitMode::Stretch), [1.0, 1.0, 0.0, 0.0]);
    }
}
