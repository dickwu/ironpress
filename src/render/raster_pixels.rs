//! Lossless conversions between straight-alpha image buffers and tiny-skia's
//! premultiplied paint surfaces.

use resvg::tiny_skia;

pub(crate) fn rgba_to_pixmap(image: &image::RgbaImage) -> Option<tiny_skia::Pixmap> {
    let mut pixmap = tiny_skia::Pixmap::new(image.width(), image.height())?;
    for (destination, source) in pixmap.pixels_mut().iter_mut().zip(image.pixels()) {
        *destination =
            tiny_skia::ColorU8::from_rgba(source[0], source[1], source[2], source[3]).premultiply();
    }
    Some(pixmap)
}

/// Convert a tiny-skia premultiplied surface into straight-alpha RGBA.
pub(crate) fn pixmap_to_rgba(pixmap: &tiny_skia::Pixmap) -> image::RgbaImage {
    image::RgbaImage::from_fn(pixmap.width(), pixmap.height(), |x, y| {
        let index = y as usize * pixmap.width() as usize + x as usize;
        let color = pixmap.pixels()[index].demultiply();
        image::Rgba([color.red(), color.green(), color.blue(), color.alpha()])
    })
}

/// Retain tiny-skia's premultiplication for consumers which explicitly expect
/// a premultiplied filter input.
pub(crate) fn pixmap_to_premultiplied_rgba(pixmap: &tiny_skia::Pixmap) -> image::RgbaImage {
    image::RgbaImage::from_fn(pixmap.width(), pixmap.height(), |x, y| {
        let index = y as usize * pixmap.width() as usize + x as usize;
        let color = pixmap.pixels()[index];
        image::Rgba([color.red(), color.green(), color.blue(), color.alpha()])
    })
}

/// Convert straight-alpha RGBA bytes to the premultiplied representation used
/// by Chromium's Skia raster pipeline.
///
/// Skia rounds `component * alpha / 255` to the nearest byte. Keeping that
/// quantization here is important for filters: a later unpremultiplication can
/// magnify a one-byte truncation at low alpha into a visible channel error.
pub(crate) fn premultiply_rgba8(image: &image::RgbaImage) -> image::RgbaImage {
    let mut premultiplied = image.clone();
    for pixel in premultiplied.pixels_mut() {
        let alpha = pixel[3];
        for channel in &mut pixel.0[..3] {
            *channel = premultiply_channel(*channel, alpha);
        }
    }
    premultiplied
}

/// Convert premultiplied RGBA bytes back to straight alpha with the nearest-
/// even quantization used when Chromium exports an RGBA8 surface to PDF.
pub(crate) fn unpremultiply_rgba8(image: &image::RgbaImage) -> image::RgbaImage {
    let mut unpremultiplied = image.clone();
    for pixel in unpremultiplied.pixels_mut() {
        let alpha = pixel[3];
        for channel in &mut pixel.0[..3] {
            *channel = unpremultiply_channel(*channel, alpha);
        }
    }
    unpremultiplied
}

fn premultiply_channel(component: u8, alpha: u8) -> u8 {
    let product = u32::from(component) * u32::from(alpha) + 128;
    ((product + (product >> 8)) >> 8) as u8
}

fn unpremultiply_channel(component: u8, alpha: u8) -> u8 {
    if alpha == 0 {
        return 0;
    }
    let denominator = u32::from(alpha);
    let numerator = u32::from(component) * 255;
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let doubled_remainder = remainder * 2;
    let round_up =
        doubled_remainder > denominator || (doubled_remainder == denominator && quotient % 2 == 1);
    (quotient + u32::from(round_up)).min(255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiplication_rounds_every_byte_pair_to_nearest() {
        for alpha in 0..=u8::MAX {
            for component in 0..=u8::MAX {
                let expected = (f32::from(component) * f32::from(alpha) / 255.0).round() as u8;
                assert_eq!(premultiply_channel(component, alpha), expected);
            }
        }
    }

    #[test]
    fn unpremultiplication_uses_nearest_even_ties() {
        assert_eq!(unpremultiply_channel(0, 0), 0);
        assert_eq!(unpremultiply_channel(1, 3), 85);
        assert_eq!(unpremultiply_channel(2, 9), 57);
        assert_eq!(unpremultiply_channel(5, 9), 142);
        assert_eq!(unpremultiply_channel(4, 19), 54);
        assert_eq!(unpremultiply_channel(2, 204), 2);
        assert_eq!(unpremultiply_channel(11, 22), 128);
    }
}
