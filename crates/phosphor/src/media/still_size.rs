//! How big a still picture is kept for the GPU. A 24-megapixel photo decoded at full size was a 96 MB upload on the UI
//! thread (a hitch at every clip change) and, past the graphics card's largest texture, a black layer. Pictures are
//! made smaller on the thread that decodes them, never larger.

/// The longest side a still is kept at: twice the canvas's longer side (room to zoom in), at least 4096, never more
/// than the graphics card's largest texture (`device_max`, wgpu's `max_texture_dimension_2d`).
pub fn longest_side(canvas: (u32, u32), device_max: u32) -> u32 {
    let wanted = canvas.0.max(canvas.1).saturating_mul(2).max(4096);
    wanted.min(device_max.max(1))
}

/// `img` with its longer side at most `max_side` (aspect kept); unchanged when it already fits.
pub fn fit(img: image::DynamicImage, max_side: u32) -> image::DynamicImage {
    if img.width().max(img.height()) <= max_side {
        return img;
    }
    // `thumbnail` averages areas (no ringing) and is several times faster than a filtered resize on a big photo.
    img.thumbnail(max_side, max_side)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stills_are_kept_at_twice_the_canvas_within_the_card() {
        assert_eq!(longest_side((1920, 1080), 16384), 4096, "at least 4096");
        assert_eq!(longest_side((3840, 2160), 16384), 7680, "twice a 4K canvas");
        assert_eq!(longest_side((3840, 2160), 8192), 7680);
        assert_eq!(longest_side((7680, 4320), 8192), 8192, "never past the card's largest texture");
        assert_eq!(longest_side((1920, 1080), 2048), 2048, "a small card's limit wins over the 4096 floor");
        assert_eq!(longest_side((1920, 1080), 0), 1, "a nonsense limit doesn't make a zero-size picture");
    }

    #[test]
    fn big_pictures_shrink_with_their_shape_and_small_ones_stay() {
        let photo = image::DynamicImage::new_rgba8(6000, 4000);
        let fitted = fit(photo, 4096);
        assert_eq!((fitted.width(), fitted.height()), (4096, 2731));
        let tall = fit(image::DynamicImage::new_rgba8(1000, 9000), 4096);
        assert_eq!(tall.height(), 4096);
        assert!(tall.width() <= 456 && tall.width() >= 454, "{}", tall.width());
        let small = fit(image::DynamicImage::new_rgba8(800, 600), 4096);
        assert_eq!((small.width(), small.height()), (800, 600));
    }
}
