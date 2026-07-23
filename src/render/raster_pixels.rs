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
