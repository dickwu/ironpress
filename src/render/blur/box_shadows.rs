//! Raster source construction for CSS box and text shadows.

use super::*;

/// Rasterize a (rounded) `box-shadow` rectangle into a transparent, padded RGBA
/// buffer and return the embeddable result.
pub(crate) fn blur_shadow_rect(
    width_pt: f32,
    height_pt: f32,
    radii: CornerRadii,
    shadow: &BoxShadow,
    filter_dpi: f32,
) -> Option<BlurredRaster> {
    let (r, g, b, a) = shadow.color.to_f32_rgba();
    if width_pt <= 0.0 || height_pt <= 0.0 || a <= 0.0 {
        return None;
    }

    use resvg::tiny_skia;

    let s = filter_dpi_scale(filter_dpi);
    let sigma = (shadow.blur / PT_PER_PX) * s / 2.0;
    let pad = pad_pixels(sigma)?;
    let box_x = filter_raster_axis(width_pt, s)?;
    let box_y = filter_raster_axis(height_pt, s)?;
    let buf_w = padded_pixels(box_x.pixels, pad)?;
    let buf_h = padded_pixels(box_y.pixels, pad)?;

    let mut pixmap = tiny_skia::Pixmap::new(buf_w, buf_h)?;
    let ox = pad as f32;
    let oy = pad as f32;

    let mut paint = tiny_skia::Paint::default();
    paint.set_color(color8(r, g, b, a));
    paint.anti_alias = true;

    let radii_px = radii.fit_to(width_pt, height_pt) * (s / PT_PER_PX);
    if !radii_px.is_zero() {
        let mut path = tiny_skia::PathBuilder::new();
        append_rounded_box_path(&mut path, ox, oy, box_x.paint_px, box_y.paint_px, radii_px);
        if let Some(path) = path.finish() {
            pixmap.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                tiny_skia::Transform::identity(),
                None,
            );
        }
    } else if let Some(rect) = tiny_skia::Rect::from_xywh(ox, oy, box_x.paint_px, box_y.paint_px) {
        pixmap.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);
    }

    let rgba = crate::render::raster_pixels::pixmap_to_rgba(&pixmap);
    let rgba = if sigma > 0.0 {
        gaussian_blur_premultiplied(&rgba, sigma)?
    } else {
        rgba
    };

    let overflow_pt = box_shadow_blur_overflow(shadow.blur, filter_dpi)?;
    let asset = rgba_to_png_alpha_asset(rgba, filter_dpi)?;
    Some(BlurredRaster { asset, overflow_pt })
}

pub(crate) fn blur_inset_shadow_rect(
    width_pt: f32,
    height_pt: f32,
    radii: CornerRadii,
    shadow: &BoxShadow,
    filter_dpi: f32,
) -> Option<BlurredRaster> {
    let (r, g, b, a) = shadow.color.to_f32_rgba();
    if width_pt <= 0.0 || height_pt <= 0.0 || a <= 0.0 {
        return None;
    }

    use resvg::tiny_skia;

    let s = filter_dpi_scale(filter_dpi);
    let sigma = (shadow.blur / PT_PER_PX) * s / 2.0;
    let pad = pad_pixels(sigma)?;
    let box_x = filter_raster_axis(width_pt, s)?;
    let box_y = filter_raster_axis(height_pt, s)?;
    let buf_w = padded_pixels(box_x.pixels, pad)?;
    let buf_h = padded_pixels(box_y.pixels, pad)?;

    let mut pixmap = tiny_skia::Pixmap::new(buf_w, buf_h)?;
    let mut paint = tiny_skia::Paint::default();
    paint.set_color(color8(r, g, b, a));
    paint.anti_alias = true;

    let pt_to_px = s / PT_PER_PX;
    let spread_px = shadow.spread * pt_to_px;
    let hole_x = pad as f32 + shadow.offset_x * pt_to_px + spread_px;
    let hole_y = pad as f32 + shadow.offset_y * pt_to_px + spread_px;
    let hole_w = box_x.paint_px - 2.0 * spread_px;
    let hole_h = box_y.paint_px - 2.0 * spread_px;

    let mut path = tiny_skia::PathBuilder::new();
    path.move_to(0.0, 0.0);
    path.line_to(buf_w as f32, 0.0);
    path.line_to(buf_w as f32, buf_h as f32);
    path.line_to(0.0, buf_h as f32);
    path.close();
    if hole_w > 0.0 && hole_h > 0.0 {
        let hole_radii = radii.grow(-shadow.spread) * pt_to_px;
        append_rounded_box_path(&mut path, hole_x, hole_y, hole_w, hole_h, hole_radii);
    }
    if let Some(path) = path.finish() {
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::EvenOdd,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let rgba = crate::render::raster_pixels::pixmap_to_rgba(&pixmap);
    let mut rgba = if sigma > 0.0 {
        gaussian_blur_premultiplied(&rgba, sigma)?
    } else {
        rgba
    };
    clip_alpha_to_rounded_box(
        &mut rgba,
        pad as f32,
        pad as f32,
        box_x.paint_px,
        box_y.paint_px,
        radii * pt_to_px,
    )?;
    let overflow_pt = pad as f32 / s * PT_PER_PX;
    let asset = rgba_to_png_alpha_asset(rgba, filter_dpi)?;
    Some(BlurredRaster { asset, overflow_pt })
}

fn clip_alpha_to_rounded_box(
    image: &mut image::RgbaImage,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radii: CornerRadii,
) -> Option<()> {
    use resvg::tiny_skia;

    let mut mask = tiny_skia::Pixmap::new(image.width(), image.height())?;
    let mut path = tiny_skia::PathBuilder::new();
    append_rounded_box_path(&mut path, x, y, width, height, radii);
    let path = path.finish()?;
    let mut paint = tiny_skia::Paint::default();
    paint.set_color(tiny_skia::Color::WHITE);
    paint.anti_alias = true;
    mask.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::identity(),
        None,
    );
    for (index, pixel) in image.pixels_mut().enumerate() {
        let mask_alpha = u16::from(mask.pixels()[index].alpha());
        pixel[3] = (u16::from(pixel[3]) * mask_alpha / 255) as u8;
    }
    Some(())
}

fn append_rounded_box_path(
    path: &mut resvg::tiny_skia::PathBuilder,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radii: CornerRadii,
) {
    let radii = radii.fit_to(width, height);
    if radii.is_zero() {
        append_rounded_path(path, x, y, width, height, 0.0, 0.0);
        return;
    }

    let k = 0.552_284_8;
    let (x0, y0) = (x, y);
    let (x1, y1) = (x + width, y + height);
    let (tlx, tly) = (radii.top_left.x, radii.top_left.y);
    let (trx, try_) = (radii.top_right.x, radii.top_right.y);
    let (brx, bry) = (radii.bottom_right.x, radii.bottom_right.y);
    let (blx, bly) = (radii.bottom_left.x, radii.bottom_left.y);

    path.move_to(x0 + tlx, y0);
    path.line_to(x1 - trx, y0);
    path.cubic_to(
        x1 - trx + trx * k,
        y0,
        x1,
        y0 + try_ - try_ * k,
        x1,
        y0 + try_,
    );
    path.line_to(x1, y1 - bry);
    path.cubic_to(x1, y1 - bry + bry * k, x1 - brx + brx * k, y1, x1 - brx, y1);
    path.line_to(x0 + blx, y1);
    path.cubic_to(x0 + blx - blx * k, y1, x0, y1 - bly + bly * k, x0, y1 - bly);
    path.line_to(x0, y0 + tly);
    path.cubic_to(x0, y0 + tly - tly * k, x0 + tlx - tlx * k, y0, x0 + tlx, y0);
    path.close();
}

fn append_rounded_path(
    path: &mut resvg::tiny_skia::PathBuilder,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius_x: f32,
    radius_y: f32,
) {
    if radius_x <= 0.0 && radius_y <= 0.0 {
        path.move_to(x, y);
        path.line_to(x + width, y);
        path.line_to(x + width, y + height);
        path.line_to(x, y + height);
        path.close();
        return;
    }
    let radius_x = radius_x.min(width / 2.0);
    let radius_y = radius_y.min(height / 2.0);
    let (x0, y0) = (x, y);
    let (x1, y1) = (x + width, y + height);
    path.move_to(x0 + radius_x, y0);
    path.line_to(x1 - radius_x, y0);
    path.quad_to(x1, y0, x1, y0 + radius_y);
    path.line_to(x1, y1 - radius_y);
    path.quad_to(x1, y1, x1 - radius_x, y1);
    path.line_to(x0 + radius_x, y1);
    path.quad_to(x0, y1, x0, y1 - radius_y);
    path.line_to(x0, y0 + radius_y);
    path.quad_to(x0, y0, x0 + radius_x, y0);
    path.close();
}

/// Blur a pre-rasterized straight-alpha glyph coverage mask and tint it.
pub(crate) fn blur_shadow_alpha_mask(
    mask: &image::GrayImage,
    blur_pt: f32,
    color: (f32, f32, f32, f32),
    filter_dpi: f32,
) -> Option<(BlurredRaster, u32)> {
    let (width, height) = mask.dimensions();
    let (red, green, blue, alpha) = color;
    if width == 0 || height == 0 || alpha <= 0.0 {
        return None;
    }

    let scale = filter_dpi_scale(filter_dpi);
    let sigma = (blur_pt / PT_PER_PX) * scale / 2.0;
    let padding = pad_pixels(sigma)?;
    let buffer_width = padded_pixels(width, padding)?;
    let buffer_height = padded_pixels(height, padding)?;
    let color = [
        (red.clamp(0.0, 1.0) * 255.0).round() as u8,
        (green.clamp(0.0, 1.0) * 255.0).round() as u8,
        (blue.clamp(0.0, 1.0) * 255.0).round() as u8,
    ];
    let mut tinted = image::RgbaImage::new(buffer_width, buffer_height);
    let mut painted = false;
    for y in 0..height {
        for x in 0..width {
            let coverage = mask.get_pixel(x, y)[0];
            if coverage == 0 {
                continue;
            }
            painted = true;
            let output_alpha = (f32::from(coverage) * alpha).round().clamp(0.0, 255.0) as u8;
            tinted.put_pixel(
                x + padding,
                y + padding,
                image::Rgba([color[0], color[1], color[2], output_alpha]),
            );
        }
    }
    if !painted {
        return None;
    }
    let blurred = if sigma > 0.0 {
        gaussian_blur_premultiplied(&tinted, sigma)?
    } else {
        tinted
    };

    let overflow_pt = padding as f32 / scale * PT_PER_PX;
    let asset = rgba_to_png_alpha_asset(blurred, filter_dpi)?;
    Some((BlurredRaster { asset, overflow_pt }, padding))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CornerRadii, CornerRadius};

    fn rounded_box_pixels(radii: CornerRadii) -> Vec<u8> {
        let mut pixmap = resvg::tiny_skia::Pixmap::new(8, 8).unwrap();
        let mut path = resvg::tiny_skia::PathBuilder::new();
        append_rounded_box_path(&mut path, 1.0, 1.0, 6.0, 6.0, radii);
        let path = path.finish().unwrap();
        let mut paint = resvg::tiny_skia::Paint::default();
        paint.set_color_rgba8(0, 0, 0, 255);
        paint.anti_alias = true;
        pixmap.fill_path(
            &path,
            &paint,
            resvg::tiny_skia::FillRule::Winding,
            resvg::tiny_skia::Transform::identity(),
            None,
        );
        pixmap.data().to_vec()
    }

    #[test]
    fn positive_subpixel_corner_radius_is_not_squared_off() {
        assert_ne!(
            rounded_box_pixels(CornerRadii::circular(0.49)),
            rounded_box_pixels(CornerRadii::ZERO)
        );
    }

    #[test]
    fn zero_radius_axis_makes_the_corner_square() {
        assert_eq!(
            rounded_box_pixels(CornerRadii::uniform(CornerRadius::new(2.0, 0.0))),
            rounded_box_pixels(CornerRadii::ZERO)
        );
    }

    #[test]
    fn rounded_box_path_preserves_per_corner_ellipses() {
        let radii = CornerRadii::new(
            CornerRadius::new(1.0, 2.0),
            CornerRadius::new(2.0, 1.0),
            CornerRadius::new(3.0, 1.0),
            CornerRadius::new(1.0, 3.0),
        );
        assert_ne!(
            rounded_box_pixels(radii),
            rounded_box_pixels(CornerRadii::circular(1.0))
        );
    }
}
