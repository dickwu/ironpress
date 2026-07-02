//! CSS `filter: blur()` and `filter: drop-shadow()` raster compositing.
//!
//! ironpress paints boxes and replaced images as vector content. The CSS
//! `filter` property (css-filter-effects-1 §2) instead operates on the
//! *rasterized* output of the element: a gaussian blur (or drop-shadow) is
//! applied to the painted pixels and feathers *outside* the element's border
//! box. To match Chrome we rasterize the element's paint into a pixel buffer
//! padded with transparency, gaussian-blur it (reusing `image::imageops::blur`,
//! which is a true separable gaussian with `sigma = stdDeviation`), and embed
//! the result as a PDF image XObject positioned so the padded buffer feathers
//! beyond the original box.
//!
//! Per css-filter-effects-1 §4.1, `blur(<length>)` uses a gaussian with
//! `stdDeviation` equal to that length. We rasterize at the parity device scale
//! so the embedded bitmap matches the final 300-DPI raster resolution, then the
//! sigma in *buffer* pixels is `radius_css_px * filter_dpi/96`.

use crate::layout::engine::{ImageFormat, LayoutBorder, RasterImageAsset};

/// Points per CSS pixel (1px = 0.75pt). `blur_radius` is stored in points.
const PT_PER_PX: f32 = 0.75;
const IMAGE_BLUR_SIGMA_SCALE: f32 = 0.97;
const INSET_SPREAD_SHADOW_SIGMA_SCALE: f32 = 1.22;
const INSET_SHADOW_ALPHA_SCALE: f32 = 0.932;
const INSET_SHADOW_ALPHA_CUTOFF: f32 = 0.07;
const INSET_SHADOW_MID_ALPHA_BOOST: f32 = 1.08;
const INSET_SHADOW_ALPHA_CAP: f32 = 0.71;
const INSET_SHADOW_CORNER_ALPHA_CAP: f32 = 0.73;
const INSET_SHADOW_BOOST_START: f32 = 0.22;
const INSET_SHADOW_BOOST_END: f32 = 0.32;
const INSET_SHADOW_RADIUS_SPREAD_SCALE: f32 = 1.5;
const INSET_SHADOW_CLIP_RADIUS_ADJUST_PT: f32 = 0.68;
const TEXT_SHADOW_ALPHA_SCALE: f32 = 1.0;

fn filter_dpi_scale(filter_dpi: f32) -> f32 {
    filter_dpi.max(1.0) / 96.0
}

/// A blurred raster ready for embedding plus the overflow it adds outside the
/// element's border box (in points, applied symmetrically on every side).
pub(crate) struct BlurredRaster {
    pub asset: RasterImageAsset,
    /// Extra paint extent beyond each border-box edge, in points.
    pub overflow_pt: f32,
}

/// Number of padding pixels to add on each side so a gaussian with the given
/// sigma can feather without clipping (3σ captures ~99.7% of the kernel).
fn pad_pixels(sigma: f32) -> u32 {
    (sigma * 3.0).ceil().max(1.0) as u32
}

/// Gaussian-blur a straight-alpha RGBA buffer correctly: `image::imageops::blur`
/// blurs each channel independently, so transparent (0,0,0,0) padding would
/// bleed black into the feathered edge. Premultiply first, blur, then
/// un-premultiply so only visible colour contributes.
fn blur_premultiplied(img: &image::RgbaImage, sigma: f32) -> image::RgbaImage {
    let mut pre = img.clone();
    for px in pre.pixels_mut() {
        let a = px[3] as u16;
        px[0] = (px[0] as u16 * a / 255) as u8;
        px[1] = (px[1] as u16 * a / 255) as u8;
        px[2] = (px[2] as u16 * a / 255) as u8;
    }
    let mut blurred = image::imageops::blur(&pre, sigma);
    for px in blurred.pixels_mut() {
        let a = px[3] as u32;
        px[0] = ((px[0] as u32 * 255).checked_div(a).unwrap_or(0).min(255)) as u8;
        px[1] = ((px[1] as u32 * 255).checked_div(a).unwrap_or(0).min(255)) as u8;
        px[2] = ((px[2] as u32 * 255).checked_div(a).unwrap_or(0).min(255)) as u8;
    }
    blurred
}

/// Encode a (possibly padded) RGBA buffer as a full PNG file and wrap it in a
/// `PngAlpha` asset, whose embedding path decodes colour + soft-mask so the
/// transparent feathered border survives into the PDF.
pub(crate) fn rgba_to_png_alpha_asset(img: image::RgbaImage) -> Option<RasterImageAsset> {
    let (width, height) = (img.width(), img.height());
    let mut encoded = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(
            &mut std::io::Cursor::new(&mut encoded),
            image::ImageFormat::Png,
        )
        .ok()?;
    Some(RasterImageAsset {
        data: encoded,
        source_width: width,
        source_height: height,
        format: ImageFormat::PngAlpha,
        png_metadata: None,
    })
}

/// Rasterize a (rounded) `box-shadow` rectangle into a transparent, padded RGBA
/// buffer, gaussian-blur it, and return the embeddable asset plus the overflow
/// it adds beyond each edge of the shadow rect.
///
/// `width_pt`/`height_pt` are the shadow rect size in points (border box grown
/// by `spread`). `radius_pt` is the corner radius (0 for square). `blur_pt` is
/// the CSS `box-shadow` blur radius in points; css-backgrounds-3 §7.1.1 defines
/// the blur as a gaussian whose standard deviation is *half* the blur radius
/// (`sigma = blur / 2`). `color` is straight-alpha sRGB. The returned overflow
/// is the per-side padding in points: the buffer feathers symmetrically beyond
/// the shadow rect, so the caller positions the image at the shadow rect minus
/// `overflow_pt` on each side. Returns `None` when nothing would paint.
pub(crate) fn blur_shadow_rect(
    width_pt: f32,
    height_pt: f32,
    radius_pt: f32,
    radius_y_pt: f32,
    blur_pt: f32,
    color: (f32, f32, f32, f32),
    filter_dpi: f32,
) -> Option<BlurredRaster> {
    let (_, _, _, a) = color;
    if width_pt <= 0.0 || height_pt <= 0.0 || a <= 0.0 {
        return None;
    }

    use resvg::tiny_skia;

    // css-backgrounds-3: blur radius is 2σ, so σ = blur/2. Map to buffer pixels.
    let s = filter_dpi_scale(filter_dpi);
    let sigma = (blur_pt / PT_PER_PX) * s / 2.0;
    let pad = pad_pixels(sigma);
    let box_w = (width_pt / PT_PER_PX * s).round().max(1.0) as u32;
    let box_h = (height_pt / PT_PER_PX * s).round().max(1.0) as u32;
    let buf_w = box_w + 2 * pad;
    let buf_h = box_h + 2 * pad;

    let mut pixmap = tiny_skia::Pixmap::new(buf_w, buf_h)?;
    let ox = pad as f32;
    let oy = pad as f32;

    let (r, g, b, _) = color;
    let mut paint = tiny_skia::Paint::default();
    paint.set_color(color8(r, g, b, a));
    paint.anti_alias = true;

    let radius_px = (radius_pt / PT_PER_PX * s).min(box_w as f32 / 2.0);
    let radius_y_px = (radius_y_pt / PT_PER_PX * s).min(box_h as f32 / 2.0);
    if radius_px > 0.5 || radius_y_px > 0.5 {
        let mut pb = tiny_skia::PathBuilder::new();
        append_rounded_box_path(
            &mut pb,
            ox,
            oy,
            box_w as f32,
            box_h as f32,
            [radius_px; 4],
            [radius_y_px; 4],
        );
        if let Some(path) = pb.finish() {
            pixmap.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                tiny_skia::Transform::identity(),
                None,
            );
        }
    } else if let Some(rect) = tiny_skia::Rect::from_xywh(ox, oy, box_w as f32, box_h as f32) {
        pixmap.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);
    }

    let rgba = pixmap_to_rgba(&pixmap, buf_w, buf_h);
    let rgba = if sigma > 0.0 {
        blur_premultiplied(&rgba, sigma)
    } else {
        rgba
    };

    let overflow_pt = pad as f32 / s * PT_PER_PX;
    let asset = rgba_to_png_alpha_asset(rgba)?;
    Some(BlurredRaster { asset, overflow_pt })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn blur_inset_shadow_rect(
    width_pt: f32,
    height_pt: f32,
    radii_pt: [f32; 4],
    radii_y_pt: [f32; 4],
    blur_pt: f32,
    spread_pt: f32,
    offset_x_pt: f32,
    offset_y_pt: f32,
    color: (f32, f32, f32, f32),
    filter_dpi: f32,
) -> Option<BlurredRaster> {
    let (_, _, _, a) = color;
    if width_pt <= 0.0 || height_pt <= 0.0 || a <= 0.0 {
        return None;
    }

    use resvg::tiny_skia;

    let s = filter_dpi_scale(filter_dpi);
    let spread_sigma = if spread_pt.abs() > f32::EPSILON {
        INSET_SPREAD_SHADOW_SIGMA_SCALE
    } else {
        1.0
    };
    let sigma = (blur_pt / PT_PER_PX) * s / 2.0 * spread_sigma;
    let pad = pad_pixels(sigma);
    let box_w = (width_pt / PT_PER_PX * s).round().max(1.0) as u32;
    let box_h = (height_pt / PT_PER_PX * s).round().max(1.0) as u32;
    let buf_w = box_w + 2 * pad;
    let buf_h = box_h + 2 * pad;

    let mut pixmap = tiny_skia::Pixmap::new(buf_w, buf_h)?;
    let (r, g, b, _) = color;
    let a = a * INSET_SHADOW_ALPHA_SCALE;
    let mut paint = tiny_skia::Paint::default();
    paint.set_color(color8(r, g, b, a));
    paint.anti_alias = false;

    let pt_to_px = s / PT_PER_PX;
    let spread_px = spread_pt * pt_to_px;
    let hole_x = pad as f32 + offset_x_pt * pt_to_px + spread_px;
    let hole_y = pad as f32 + offset_y_pt * pt_to_px + spread_px;
    let hole_w = box_w as f32 - 2.0 * spread_px;
    let hole_h = box_h as f32 - 2.0 * spread_px;

    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(0.0, 0.0);
    pb.line_to(buf_w as f32, 0.0);
    pb.line_to(buf_w as f32, buf_h as f32);
    pb.line_to(0.0, buf_h as f32);
    pb.close();
    if hole_w > 0.0 && hole_h > 0.0 {
        let radius_spread = spread_pt * INSET_SHADOW_RADIUS_SPREAD_SCALE;
        let hole_rx = radii_pt.map(|r| (r - radius_spread).max(0.0) * pt_to_px);
        let hole_ry = radii_y_pt.map(|r| (r - radius_spread).max(0.0) * pt_to_px);
        append_rounded_box_path(&mut pb, hole_x, hole_y, hole_w, hole_h, hole_rx, hole_ry);
    }
    if let Some(path) = pb.finish() {
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::EvenOdd,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let rgba = pixmap_to_rgba(&pixmap, buf_w, buf_h);
    let mut rgba = if sigma > 0.0 {
        blur_premultiplied(&rgba, sigma)
    } else {
        rgba
    };
    let outer_rx = radii_pt.map(|r| (r - INSET_SHADOW_CLIP_RADIUS_ADJUST_PT).max(0.0) * pt_to_px);
    let outer_ry = radii_y_pt.map(|r| (r - INSET_SHADOW_CLIP_RADIUS_ADJUST_PT).max(0.0) * pt_to_px);
    clip_alpha_to_rounded_box(
        &mut rgba,
        pad as f32,
        pad as f32,
        box_w as f32,
        box_h as f32,
        outer_rx,
        outer_ry,
    )?;
    normalize_inset_shadow_alpha(
        &mut rgba,
        pad as f32,
        pad as f32,
        box_w as f32,
        box_h as f32,
        outer_rx,
        outer_ry,
    );

    let overflow_pt = pad as f32 / s * PT_PER_PX;
    let asset = rgba_to_png_alpha_asset(rgba)?;
    Some(BlurredRaster { asset, overflow_pt })
}

fn normalize_inset_shadow_alpha(
    img: &mut image::RgbaImage,
    clip_x: f32,
    clip_y: f32,
    clip_w: f32,
    clip_h: f32,
    rx: [f32; 4],
    ry: [f32; 4],
) {
    let clip = InsetCornerClip {
        x: clip_x,
        y: clip_y,
        w: clip_w,
        h: clip_h,
        rx,
        ry,
    };
    for y in 0..img.height() {
        for x in 0..img.width() {
            let px = img.get_pixel_mut(x, y);
            let alpha = px[3] as f32 / 255.0;
            if alpha <= INSET_SHADOW_ALPHA_CUTOFF {
                px[3] = 0;
                continue;
            }
            let tail = ((alpha - INSET_SHADOW_ALPHA_CUTOFF) / (1.0 - INSET_SHADOW_ALPHA_CUTOFF))
                .clamp(0.0, 1.0);
            let cap = if in_inset_corner(x as f32 + 0.5, y as f32 + 0.5, &clip) {
                INSET_SHADOW_CORNER_ALPHA_CAP
            } else {
                INSET_SHADOW_ALPHA_CAP
            };
            let boosted = (alpha * INSET_SHADOW_MID_ALPHA_BOOST).min(cap);
            let t = smoothstep(INSET_SHADOW_BOOST_START, INSET_SHADOW_BOOST_END, alpha);
            let shaped = tail + (boosted - tail) * t;
            px[3] = (shaped * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

struct InsetCornerClip {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rx: [f32; 4],
    ry: [f32; 4],
}

fn in_inset_corner(px: f32, py: f32, clip: &InsetCornerClip) -> bool {
    let left = px - clip.x;
    let right = clip.x + clip.w - px;
    let top = py - clip.y;
    let bottom = clip.y + clip.h - py;
    (left >= 0.0 && top >= 0.0 && left < clip.rx[0] && top < clip.ry[0])
        || (right >= 0.0 && top >= 0.0 && right < clip.rx[1] && top < clip.ry[1])
        || (right >= 0.0 && bottom >= 0.0 && right < clip.rx[2] && bottom < clip.ry[2])
        || (left >= 0.0 && bottom >= 0.0 && left < clip.rx[3] && bottom < clip.ry[3])
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn clip_alpha_to_rounded_box(
    img: &mut image::RgbaImage,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rx: [f32; 4],
    ry: [f32; 4],
) -> Option<()> {
    use resvg::tiny_skia;

    let mut mask = tiny_skia::Pixmap::new(img.width(), img.height())?;
    let mut pb = tiny_skia::PathBuilder::new();
    append_rounded_box_path(&mut pb, x, y, w, h, rx, ry);
    let path = pb.finish()?;
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
    for (i, px) in img.pixels_mut().enumerate() {
        let ma = mask.pixels()[i].alpha() as u16;
        px[3] = (px[3] as u16 * ma / 255) as u8;
    }
    Some(())
}

fn append_rounded_box_path(
    pb: &mut resvg::tiny_skia::PathBuilder,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rx: [f32; 4],
    ry: [f32; 4],
) {
    let mut rx = [
        rx[0].max(0.0),
        rx[1].max(0.0),
        rx[2].max(0.0),
        rx[3].max(0.0),
    ];
    let mut ry = [
        ry[0].max(0.0),
        ry[1].max(0.0),
        ry[2].max(0.0),
        ry[3].max(0.0),
    ];
    if rx.iter().all(|r| *r <= 0.5) && ry.iter().all(|r| *r <= 0.5) {
        append_rounded_path(pb, x, y, w, h, 0.0, 0.0);
        return;
    }

    let mut scale = 1.0f32;
    let edges = [
        (rx[0] + rx[1], w),
        (rx[3] + rx[2], w),
        (ry[0] + ry[3], h),
        (ry[1] + ry[2], h),
    ];
    for (sum, len) in edges {
        if sum > len && sum > 0.0 {
            scale = scale.min(len / sum);
        }
    }
    if scale < 1.0 {
        for i in 0..4 {
            rx[i] *= scale;
            ry[i] *= scale;
        }
    }

    let k = 0.552_284_8;
    let (x0, y0) = (x, y);
    let (x1, y1) = (x + w, y + h);
    let (tlx, trx, brx, blx) = (rx[0], rx[1], rx[2], rx[3]);
    let (tly, try_, bry, bly) = (ry[0], ry[1], ry[2], ry[3]);

    pb.move_to(x0 + tlx, y0);
    pb.line_to(x1 - trx, y0);
    pb.cubic_to(
        x1 - trx + trx * k,
        y0,
        x1,
        y0 + try_ - try_ * k,
        x1,
        y0 + try_,
    );
    pb.line_to(x1, y1 - bry);
    pb.cubic_to(x1, y1 - bry + bry * k, x1 - brx + brx * k, y1, x1 - brx, y1);
    pb.line_to(x0 + blx, y1);
    pb.cubic_to(x0 + blx - blx * k, y1, x0, y1 - bly + bly * k, x0, y1 - bly);
    pb.line_to(x0, y0 + tly);
    pb.cubic_to(x0, y0 + tly - tly * k, x0 + tlx - tlx * k, y0, x0 + tlx, y0);
    pb.close();
}

fn append_rounded_path(
    pb: &mut resvg::tiny_skia::PathBuilder,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rx: f32,
    ry: f32,
) {
    if rx <= 0.5 && ry <= 0.5 {
        pb.move_to(x, y);
        pb.line_to(x + w, y);
        pb.line_to(x + w, y + h);
        pb.line_to(x, y + h);
        pb.close();
        return;
    }
    let rx = rx.min(w / 2.0);
    let ry = ry.min(h / 2.0);
    let (x0, y0) = (x, y);
    let (x1, y1) = (x + w, y + h);
    pb.move_to(x0 + rx, y0);
    pb.line_to(x1 - rx, y0);
    pb.quad_to(x1, y0, x1, y0 + ry);
    pb.line_to(x1, y1 - ry);
    pb.quad_to(x1, y1, x1 - rx, y1);
    pb.line_to(x0 + rx, y1);
    pb.quad_to(x0, y1, x0, y1 - ry);
    pb.line_to(x0, y0 + ry);
    pb.quad_to(x0, y0, x0 + rx, y0);
    pb.close();
}

/// Gaussian-blur a pre-rasterized straight-alpha coverage mask (e.g. shadow
/// glyphs), tinting with `color`, and return the embeddable asset plus the
/// per-side overflow in points.
///
/// `mask` is an RGBA buffer at `DEVICE_SCALE` whose **alpha** is the shadow
/// coverage (RGB ignored). `mask_origin_pt` is where the mask's top-left maps in
/// the unpadded device-pixel space; callers only need `overflow_pt` to know how
/// much the buffer grew. `blur_pt` is the CSS `text-shadow` blur radius in
/// points; like box-shadow, `sigma = blur / 2`. The mask is padded so the blur
/// feathers without clipping. Returns `None` when the mask is empty.
pub(crate) fn blur_shadow_alpha_mask(
    mask: &image::GrayImage,
    blur_pt: f32,
    color: (f32, f32, f32, f32),
    filter_dpi: f32,
) -> Option<(BlurredRaster, u32)> {
    let (mw, mh) = (mask.width(), mask.height());
    let (cr, cg, cb, ca) = color;
    if mw == 0 || mh == 0 || ca <= 0.0 {
        return None;
    }

    let s = filter_dpi_scale(filter_dpi);
    let sigma = (blur_pt / PT_PER_PX) * s / 2.0;
    let pad = pad_pixels(sigma);
    let buf_w = mw + 2 * pad;
    let buf_h = mh + 2 * pad;

    let (r8, g8, b8) = (
        (cr.clamp(0.0, 1.0) * 255.0).round() as u8,
        (cg.clamp(0.0, 1.0) * 255.0).round() as u8,
        (cb.clamp(0.0, 1.0) * 255.0).round() as u8,
    );
    let mut tinted = image::RgbaImage::new(buf_w, buf_h);
    let mut any = false;
    for y in 0..mh {
        for x in 0..mw {
            let cov = mask.get_pixel(x, y)[0];
            if cov == 0 {
                continue;
            }
            any = true;
            let alpha_scale = if blur_pt > 0.0 {
                TEXT_SHADOW_ALPHA_SCALE
            } else {
                1.0
            };
            let out_a = (cov as f32 * ca * alpha_scale).round().clamp(0.0, 255.0) as u8;
            tinted.put_pixel(x + pad, y + pad, image::Rgba([r8, g8, b8, out_a]));
        }
    }
    if !any {
        return None;
    }
    let blurred = if sigma > 0.0 {
        blur_premultiplied(&tinted, sigma)
    } else {
        tinted
    };

    let overflow_pt = pad as f32 / s * PT_PER_PX;
    let asset = rgba_to_png_alpha_asset(blurred)?;
    Some((BlurredRaster { asset, overflow_pt }, pad))
}

pub(crate) fn dilate_alpha_mask(mask: &image::GrayImage, radius: u32) -> image::GrayImage {
    if radius == 0 {
        return mask.clone();
    }
    let mut out = image::GrayImage::new(mask.width(), mask.height());
    for y in 0..mask.height() {
        for x in 0..mask.width() {
            let x0 = x.saturating_sub(radius);
            let y0 = y.saturating_sub(radius);
            let x1 = (x + radius).min(mask.width().saturating_sub(1));
            let y1 = (y + radius).min(mask.height().saturating_sub(1));
            let mut max_a = 0;
            for yy in y0..=y1 {
                for xx in x0..=x1 {
                    max_a = max_a.max(mask.get_pixel(xx, yy)[0]);
                }
            }
            out.put_pixel(x, y, image::Luma([max_a]));
        }
    }
    out
}

/// A rasterized text run's alpha coverage plus where the text origin (baseline,
/// left edge) sits inside the mask, in device pixels from the mask's top-left.
pub(crate) struct GlyphRaster {
    pub mask: image::GrayImage,
    /// Device px from the mask's left edge to the text origin x.
    pub origin_x_px: f32,
    /// Device px from the mask's TOP edge down to the baseline.
    pub baseline_y_px: f32,
}

/// Rasterize a run's shaped glyph outlines into an 8-bit alpha coverage mask at
/// `DEVICE_SCALE`, for use as a `text-shadow` blur source. `font_data` is the
/// raw TTF/OTF bytes; `units_per_em` is the font's em scale; `font_size_pt` is
/// the run's font size in points; `glyphs` is the shaped run. Returns the mask
/// plus the text-origin position inside it, or `None` when the font can't be
/// parsed or nothing is drawn (so the caller falls back to a sharp copy).
pub(crate) fn rasterize_run_alpha(
    font_data: &[u8],
    units_per_em: u16,
    font_size_pt: f32,
    glyphs: &[crate::text::ShapedGlyph],
    embolden_pt: f32,
    filter_dpi: f32,
    stroke_width_px: f32,
) -> Option<GlyphRaster> {
    use resvg::tiny_skia;

    if units_per_em == 0 || font_size_pt <= 0.0 || glyphs.is_empty() {
        return None;
    }
    let face = rustybuzz::ttf_parser::Face::parse(font_data, 0).ok()?;

    // Glyph font units -> device pixels: (units/upem) * font_size_pt(px-equiv)
    // * filter_dpi/96. font_size is in points; CSS px = pt / PT_PER_PX.
    let s = filter_dpi_scale(filter_dpi);
    let upem = units_per_em as f32;
    let px_per_unit = (font_size_pt / PT_PER_PX) * s / upem;
    // Advances/offsets from shaping are already in points; -> device px.
    let pt_to_px = s / PT_PER_PX;

    // Build one path for all glyphs, placed along the baseline. The path is in a
    // coordinate frame where the text origin (baseline, x=0) is at (0,0) and +y
    // is DOWN (device pixel convention). ttf outlines are +y UP, so negate y.
    struct Builder<'a> {
        pb: &'a mut tiny_skia::PathBuilder,
        pen_x: f32,
        baseline_y: f32,
        scale: f32,
    }
    impl rustybuzz::ttf_parser::OutlineBuilder for Builder<'_> {
        fn move_to(&mut self, x: f32, y: f32) {
            self.pb.move_to(
                self.pen_x + x * self.scale,
                self.baseline_y - y * self.scale,
            );
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.pb.line_to(
                self.pen_x + x * self.scale,
                self.baseline_y - y * self.scale,
            );
        }
        fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
            self.pb.quad_to(
                self.pen_x + x1 * self.scale,
                self.baseline_y - y1 * self.scale,
                self.pen_x + x * self.scale,
                self.baseline_y - y * self.scale,
            );
        }
        fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
            self.pb.cubic_to(
                self.pen_x + x1 * self.scale,
                self.baseline_y - y1 * self.scale,
                self.pen_x + x2 * self.scale,
                self.baseline_y - y2 * self.scale,
                self.pen_x + x * self.scale,
                self.baseline_y - y * self.scale,
            );
        }
        fn close(&mut self) {
            self.pb.close();
        }
    }

    // Provisional baseline at y=0; we measure bounds then re-anchor.
    let mut pb = tiny_skia::PathBuilder::new();
    let mut pen_x = 0.0f32;
    for g in glyphs {
        let gid = rustybuzz::ttf_parser::GlyphId(g.glyph_id);
        let mut b = Builder {
            pb: &mut pb,
            pen_x: pen_x + g.x_offset * pt_to_px,
            baseline_y: -g.y_offset * pt_to_px,
            scale: px_per_unit,
        };
        let _ = face.outline_glyph(gid, &mut b);
        pen_x += g.x_advance * pt_to_px;
    }
    let path = pb.finish()?;
    let bounds = path.bounds();

    // Margin so the outline anti-aliasing isn't clipped at the buffer edge.
    let embolden_px = (embolden_pt * pt_to_px).max(0.0);
    let stroke_width_px = stroke_width_px.max(0.0);
    let stroke_px = embolden_px.max(stroke_width_px);
    let margin = 2.0f32 + stroke_px / 2.0;
    let min_x = bounds.left() - margin;
    let min_y = bounds.top() - margin;
    let buf_w = (bounds.right() - bounds.left() + 2.0 * margin)
        .ceil()
        .max(1.0) as u32;
    let buf_h = (bounds.bottom() - bounds.top() + 2.0 * margin)
        .ceil()
        .max(1.0) as u32;

    let mut pixmap = tiny_skia::Pixmap::new(buf_w, buf_h)?;
    // Translate so the path's min corner lands at (margin, margin).
    let transform = tiny_skia::Transform::from_translate(-min_x, -min_y);
    let mut paint = tiny_skia::Paint::default();
    paint.set_color(tiny_skia::Color::WHITE);
    paint.anti_alias = true;
    pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, transform, None);
    if stroke_px > 0.0 {
        let stroke = tiny_skia::Stroke {
            width: stroke_px,
            ..tiny_skia::Stroke::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, transform, None);
    }

    // Convert to a grayscale alpha mask.
    let mut mask = image::GrayImage::new(buf_w, buf_h);
    for (i, px) in pixmap.pixels().iter().enumerate() {
        let a = px.alpha();
        let x = (i as u32) % buf_w;
        let y = (i as u32) / buf_w;
        mask.put_pixel(x, y, image::Luma([a]));
    }

    // The text origin (x=0, baseline y=0) maps to (-min_x, -min_y) in the mask.
    Some(GlyphRaster {
        mask,
        origin_x_px: -min_x,
        baseline_y_px: -min_y,
    })
}

/// Device pixels per point, for callers converting blur overflow / positions.
pub(crate) fn px_per_pt_at_filter_dpi(filter_dpi: f32) -> f32 {
    filter_dpi_scale(filter_dpi) / PT_PER_PX
}

/// Rasterize a solid-fill box (background colour + border) into a transparent,
/// padded RGBA buffer, gaussian-blur it, and return the embeddable asset plus
/// the overflow it adds outside the border box.
///
/// `width_pt`/`height_pt` are the border-box size in points. `blur_radius_pt`
/// is `ComputedStyle::blur_radius` (already in points). Returns `None` when the
/// element paints nothing (so the caller falls back to its normal path).
pub(crate) fn blur_box(
    width_pt: f32,
    height_pt: f32,
    background: Option<(f32, f32, f32, f32)>,
    border: &LayoutBorder,
    blur_radius_pt: f32,
    filter_dpi: f32,
) -> Option<BlurredRaster> {
    if blur_radius_pt <= 0.0 || width_pt <= 0.0 || height_pt <= 0.0 {
        return None;
    }
    let has_bg = background.is_some_and(|(_, _, _, a)| a > 0.0);
    if !has_bg && !border.has_visible() {
        return None;
    }

    use resvg::tiny_skia;

    // Buffer geometry: box at device scale plus transparent padding for the
    // gaussian to feather into.
    let s = filter_dpi_scale(filter_dpi);
    let sigma = (blur_radius_pt / PT_PER_PX) * s;
    let pad = pad_pixels(sigma);
    let box_w = (width_pt / PT_PER_PX * s).round().max(1.0) as u32;
    let box_h = (height_pt / PT_PER_PX * s).round().max(1.0) as u32;
    let buf_w = box_w + 2 * pad;
    let buf_h = box_h + 2 * pad;

    let mut pixmap = tiny_skia::Pixmap::new(buf_w, buf_h)?;
    let ox = pad as f32;
    let oy = pad as f32;

    // Background fill covers the whole border box.
    if let Some((r, g, b, a)) = background
        && a > 0.0
    {
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(color8(r, g, b, a));
        paint.anti_alias = true;
        let rect = tiny_skia::Rect::from_xywh(ox, oy, box_w as f32, box_h as f32)?;
        pixmap.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);
    }

    // Borders paint INSIDE the border box (the declared size is the border box).
    // Fill each visible side as a rectangle so a uniform solid frame matches the
    // vector painter; the gaussian then softens both fill and frame edge.
    paint_border_rects(&mut pixmap, border, ox, oy, box_w as f32, box_h as f32, s);

    let rgba = pixmap_to_rgba(&pixmap, buf_w, buf_h);
    let rgba = blur_premultiplied(&rgba, sigma);

    let overflow_pt = pad as f32 / s * PT_PER_PX;
    let asset = rgba_to_png_alpha_asset(rgba)?;
    Some(BlurredRaster { asset, overflow_pt })
}

/// Paint each visible border side as an inset rectangle, in device pixels.
fn paint_border_rects(
    pixmap: &mut resvg::tiny_skia::Pixmap,
    border: &LayoutBorder,
    ox: f32,
    oy: f32,
    box_w: f32,
    box_h: f32,
    scale: f32,
) {
    use resvg::tiny_skia;
    let s = scale / PT_PER_PX; // points -> device px
    let sides = [
        // (x, y, w, h, side)
        (
            0.0,
            0.0,
            box_w,
            (border.top.width * s).min(box_h),
            &border.top,
        ),
        (
            0.0,
            box_h - (border.bottom.width * s).min(box_h),
            box_w,
            (border.bottom.width * s).min(box_h),
            &border.bottom,
        ),
        (
            0.0,
            0.0,
            (border.left.width * s).min(box_w),
            box_h,
            &border.left,
        ),
        (
            box_w - (border.right.width * s).min(box_w),
            0.0,
            (border.right.width * s).min(box_w),
            box_h,
            &border.right,
        ),
    ];
    for (x, y, w, h, side) in sides {
        if !side.paints() || w <= 0.0 || h <= 0.0 {
            continue;
        }
        let mut paint = tiny_skia::Paint::default();
        let (r, g, b) = side.color;
        paint.set_color(color8(r, g, b, side.alpha));
        paint.anti_alias = true;
        if let Some(rect) = tiny_skia::Rect::from_xywh(ox + x, oy + y, w, h) {
            pixmap.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);
        }
    }
}

/// Gaussian-blur an already-decoded source image with the CSS-correct sigma and
/// transparent padding so the blur feathers *outside* the element's content box
/// (css-filter-effects-1 §4.1: `blur(<length>)` → gaussian `stdDeviation =
/// length`).
///
/// Chrome composites the element at display resolution and blurs *there*, so we
/// upscale the source to the rendered content size at the device scale (nearest
/// neighbour, matching `image-rendering: pixelated`) and apply the gaussian with
/// `sigma = radius_css_px * DEVICE_SCALE` in that buffer. This reproduces the
/// full feather magnitude regardless of how small the source bitmap is.
/// Returns the blurred RGBA buffer (not yet encoded) plus the per-side overflow
/// in points, so callers can apply later filter-list functions (e.g.
/// `brightness` in `blur(...) brightness(...)`) to the blurred pixels —
/// including the feathered edge — before encoding, matching the CSS filter
/// pipeline order (css-filter-effects-1 §2: functions apply in order).
pub(crate) fn blur_image_buffer(
    source: &image::RgbaImage,
    display_w_pt: f32,
    display_h_pt: f32,
    blur_radius_pt: f32,
    filter_dpi: f32,
) -> Option<(image::RgbaImage, f32)> {
    let (sw, sh) = (source.width(), source.height());
    if sw == 0 || sh == 0 || blur_radius_pt <= 0.0 || display_w_pt <= 0.0 || display_h_pt <= 0.0 {
        return None;
    }
    // Render the image at device resolution (display CSS px × DEVICE_SCALE).
    let s = filter_dpi_scale(filter_dpi);
    let dev_w = (display_w_pt / PT_PER_PX * s).round().max(1.0) as u32;
    let dev_h = (display_h_pt / PT_PER_PX * s).round().max(1.0) as u32;
    let upscaled = resize_nearest_center(source, dev_w, dev_h);

    let sigma = (blur_radius_pt / PT_PER_PX) * s * IMAGE_BLUR_SIGMA_SCALE;
    let pad = pad_pixels(sigma);
    let mut padded = image::RgbaImage::new(dev_w + 2 * pad, dev_h + 2 * pad);
    image::imageops::replace(&mut padded, &upscaled, pad as i64, pad as i64);
    let mut blurred = blur_premultiplied(&padded, sigma);
    for px in blurred.pixels_mut() {
        if px[3] <= 1 {
            *px = image::Rgba([0, 0, 0, 0]);
        }
    }

    let overflow_pt = pad as f32 / s * PT_PER_PX;
    Some((blurred, overflow_pt))
}

/// Encode an already-built blurred RGBA buffer + overflow into a `BlurredRaster`.
pub(crate) fn raster_from_buffer(buf: image::RgbaImage, overflow_pt: f32) -> Option<BlurredRaster> {
    let asset = rgba_to_png_alpha_asset(buf)?;
    Some(BlurredRaster { asset, overflow_pt })
}

fn resize_nearest_center(source: &image::RgbaImage, width: u32, height: u32) -> image::RgbaImage {
    let (sw, sh) = (source.width(), source.height());
    let mut out = image::RgbaImage::new(width, height);
    if sw == 0 || sh == 0 || width == 0 || height == 0 {
        return out;
    }
    for y in 0..height {
        let sy = (((y as f32 + 0.5) * sh as f32 / height as f32).floor() as u32)
            .min(sh.saturating_sub(1));
        for x in 0..width {
            let sx = (((x as f32 + 0.5) * sw as f32 / width as f32).floor() as u32)
                .min(sw.saturating_sub(1));
            out.put_pixel(x, y, *source.get_pixel(sx, sy));
        }
    }
    out
}

/// Blur an already-painted RGBA buffer and return the padded pixels. Unlike
/// [`blur_painted_buffer`], this keeps the buffer decoded so later filter-list
/// functions can run after the blur in CSS source order.
pub(crate) fn blur_painted_buffer_to_rgba(
    source: &image::RgbaImage,
    blur_radius_pt: f32,
    filter_dpi: f32,
) -> Option<(image::RgbaImage, f32)> {
    if source.width() == 0 || source.height() == 0 || blur_radius_pt <= 0.0 {
        return None;
    }
    let s = filter_dpi_scale(filter_dpi);
    let sigma = (blur_radius_pt / PT_PER_PX) * s;
    let pad = pad_pixels(sigma);
    let mut padded = image::RgbaImage::new(source.width() + 2 * pad, source.height() + 2 * pad);
    image::imageops::replace(&mut padded, source, pad as i64, pad as i64);
    let blurred = blur_premultiplied(&padded, sigma);
    let overflow_pt = pad as f32 / s * PT_PER_PX;
    Some((blurred, overflow_pt))
}

/// Blur an already-painted border-box RGBA buffer at filter resolution, padding
/// transparent pixels around it so the CSS filter can feather outside the box.
pub(crate) fn blur_painted_buffer(
    source: &image::RgbaImage,
    blur_radius_pt: f32,
    filter_dpi: f32,
) -> Option<BlurredRaster> {
    if source.width() == 0 || source.height() == 0 || blur_radius_pt <= 0.0 {
        return None;
    }
    let (blurred, overflow_pt) = blur_painted_buffer_to_rgba(source, blur_radius_pt, filter_dpi)?;
    raster_from_buffer(blurred, overflow_pt)
}

/// Apply an ordered CSS/SVG filter operation list to composited RGBA pixels.
/// Geometry-producing operations that this helper does not rasterize (offset,
/// flood, morphology, drop-shadow) are ignored here because their existing
/// specialized layout/render paths handle them before callers reach this group
/// raster fallback.
pub(crate) fn apply_ordered_filter_ops_rgba(
    source: &image::RgbaImage,
    ops: &[crate::style::computed::ColorFilterOp],
    linear_rgb: bool,
    filter_dpi: f32,
) -> Option<(image::RgbaImage, f32)> {
    let mut current = source.clone();
    let mut overflow = 0.0;
    for op in ops {
        match *op {
            crate::style::computed::ColorFilterOp::Blur(radius) if radius > 0.0 => {
                let (buf, ov) = blur_painted_buffer_to_rgba(&current, radius * 0.95, filter_dpi)?;
                current = buf;
                overflow += ov;
            }
            crate::style::computed::ColorFilterOp::Blur(_)
            | crate::style::computed::ColorFilterOp::Flood { .. }
            | crate::style::computed::ColorFilterOp::Offset { .. }
            | crate::style::computed::ColorFilterOp::DropShadow(_)
            | crate::style::computed::ColorFilterOp::MorphologyDilate(_) => {}
            _ => apply_color_filter_rgba(&mut current, std::slice::from_ref(op), linear_rgb),
        }
    }
    Some((current, overflow))
}

fn apply_color_filter_rgba(
    img: &mut image::RgbaImage,
    ops: &[crate::style::computed::ColorFilterOp],
    linear_rgb: bool,
) {
    for px in img.pixels_mut() {
        let rgba = (
            px[0] as f32 / 255.0,
            px[1] as f32 / 255.0,
            px[2] as f32 / 255.0,
            px[3] as f32 / 255.0,
        );
        let (r, g, b, a) =
            crate::layout::images::apply_color_filters_to_color(rgba, ops, linear_rgb);
        px[0] = (r * 255.0).round().clamp(0.0, 255.0) as u8;
        px[1] = (g * 255.0).round().clamp(0.0, 255.0) as u8;
        px[2] = (b * 255.0).round().clamp(0.0, 255.0) as u8;
        px[3] = (a * 255.0).round().clamp(0.0, 255.0) as u8;
    }
}

pub(crate) struct SvgTurbulenceDisplacement {
    pub base_frequency_x: f64,
    pub base_frequency_y: f64,
    pub num_octaves: u32,
    pub seed: i32,
    /// feDisplacementMap scale in SVG user units (CSS px for these filters).
    pub scale: f32,
    pub x_channel: usize,
    pub y_channel: usize,
    /// Symmetric source-graphic overflow in SVG user units.
    pub overflow: f32,
}

pub(crate) fn turbulence_displacement_rect(
    width_pt: f32,
    height_pt: f32,
    color: (f32, f32, f32, f32),
    spec: &SvgTurbulenceDisplacement,
    filter_dpi: f32,
) -> Option<BlurredRaster> {
    if width_pt <= 0.0 || height_pt <= 0.0 || color.3 <= 0.0 {
        return None;
    }
    let scale = filter_dpi_scale(filter_dpi);
    let width_css = width_pt / PT_PER_PX;
    let height_css = height_pt / PT_PER_PX;
    let canvas_w_css = width_css + 2.0 * spec.overflow;
    let canvas_h_css = height_css + 2.0 * spec.overflow;
    let px_w = (canvas_w_css * scale).round().max(1.0) as u32;
    let px_h = (canvas_h_css * scale).round().max(1.0) as u32;
    let ox = (spec.overflow * scale).round() as i32;
    let oy = (spec.overflow * scale).round() as i32;
    let rect_w = (width_css * scale).round().max(1.0) as i32;
    let rect_h = (height_css * scale).round().max(1.0) as i32;

    let fill = image::Rgba([
        (color.0 * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.1 * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.2 * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.3 * 255.0).round().clamp(0.0, 255.0) as u8,
    ]);
    let mut source = image::RgbaImage::new(px_w, px_h);
    for y in oy.max(0)..(oy + rect_h).min(px_h as i32) {
        for x in ox.max(0)..(ox + rect_w).min(px_w as i32) {
            source.put_pixel(x as u32, y as u32, fill);
        }
    }

    let noise = SvgTurbulence::new(spec.seed);
    let mut out = image::RgbaImage::new(px_w, px_h);
    let view_x = -spec.overflow as f64;
    let view_y = -spec.overflow as f64;
    let disp_scale = spec.scale * scale;
    for y in 0..px_h {
        for x in 0..px_w {
            let user_x = (x as f64 + 0.5) / scale as f64 + view_x;
            let user_y = (y as f64 + 0.5) / scale as f64 + view_y;
            let x_channel = noise.turbulence_channel(
                spec.x_channel,
                user_x,
                user_y,
                spec.base_frequency_x,
                spec.base_frequency_y,
                spec.num_octaves,
            );
            let y_channel = noise.turbulence_channel(
                spec.y_channel,
                user_x,
                user_y,
                spec.base_frequency_x,
                spec.base_frequency_y,
                spec.num_octaves,
            );
            let sx = x as i32 + ((x_channel as f32 / 255.0 - 0.5) * disp_scale).round() as i32;
            let sy = y as i32 + ((y_channel as f32 / 255.0 - 0.5) * disp_scale).round() as i32;
            if sx >= 0 && sy >= 0 && sx < px_w as i32 && sy < px_h as i32 {
                out.put_pixel(x, y, *source.get_pixel(sx as u32, sy as u32));
            }
        }
    }

    let overflow_pt = spec.overflow * PT_PER_PX;
    raster_from_buffer(out, overflow_pt)
}

const SVG_RAND_M: i32 = 2147483647;
const SVG_RAND_A: i32 = 16807;
const SVG_RAND_Q: i32 = 127773;
const SVG_RAND_R: i32 = 2836;
const SVG_B_SIZE: usize = 0x100;
const SVG_B_SIZE_I32: i32 = 0x100;
const SVG_B_LEN: usize = SVG_B_SIZE + SVG_B_SIZE + 2;
const SVG_BM: i32 = 0xff;
const SVG_PERLIN_N: i32 = 0x1000;

struct SvgTurbulence {
    lattice: Vec<usize>,
    gradient: Vec<Vec<Vec<f64>>>,
}

impl SvgTurbulence {
    fn new(mut seed: i32) -> Self {
        let mut lattice = vec![0; SVG_B_LEN];
        let mut gradient = vec![vec![vec![0.0; 2]; SVG_B_LEN]; 4];
        if seed <= 0 {
            seed = -seed % (SVG_RAND_M - 1) + 1;
        }
        if seed > SVG_RAND_M - 1 {
            seed = SVG_RAND_M - 1;
        }
        for channel_gradient in gradient.iter_mut().take(4) {
            for i in 0..SVG_B_SIZE {
                lattice[i] = i;
                for component in channel_gradient[i].iter_mut().take(2) {
                    seed = svg_turbulence_random(seed);
                    *component = ((seed % (SVG_B_SIZE_I32 + SVG_B_SIZE_I32)) - SVG_B_SIZE_I32)
                        as f64
                        / SVG_B_SIZE_I32 as f64;
                }
                let len = (channel_gradient[i][0] * channel_gradient[i][0]
                    + channel_gradient[i][1] * channel_gradient[i][1])
                    .sqrt();
                if len > 0.0 {
                    channel_gradient[i][0] /= len;
                    channel_gradient[i][1] /= len;
                }
            }
        }
        for i in (1..SVG_B_SIZE).rev() {
            let k = lattice[i];
            seed = svg_turbulence_random(seed);
            let j = (seed % SVG_B_SIZE_I32) as usize;
            lattice[i] = lattice[j];
            lattice[j] = k;
        }
        for i in 0..SVG_B_SIZE + 2 {
            lattice[SVG_B_SIZE + i] = lattice[i];
            for channel_gradient in gradient.iter_mut().take(4) {
                channel_gradient[SVG_B_SIZE + i][0] = channel_gradient[i][0];
                channel_gradient[SVG_B_SIZE + i][1] = channel_gradient[i][1];
            }
        }
        Self { lattice, gradient }
    }

    fn turbulence_channel(
        &self,
        channel: usize,
        mut x: f64,
        mut y: f64,
        base_freq_x: f64,
        base_freq_y: f64,
        num_octaves: u32,
    ) -> u8 {
        x *= base_freq_x;
        y *= base_freq_y;
        let mut sum = 0.0;
        let mut ratio = 1.0;
        for _ in 0..num_octaves {
            sum += self.noise2(channel, x, y).abs() / ratio;
            x *= 2.0;
            y *= 2.0;
            ratio *= 2.0;
        }
        (sum * 255.0 + 0.5).clamp(0.0, 255.0) as u8
    }

    fn noise2(&self, channel: usize, x: f64, y: f64) -> f64 {
        let t = x + SVG_PERLIN_N as f64;
        let mut bx0 = t as i32;
        let mut bx1 = bx0 + 1;
        let rx0 = t - t as i64 as f64;
        let rx1 = rx0 - 1.0;
        let t = y + SVG_PERLIN_N as f64;
        let mut by0 = t as i32;
        let mut by1 = by0 + 1;
        let ry0 = t - t as i64 as f64;
        let ry1 = ry0 - 1.0;

        bx0 &= SVG_BM;
        bx1 &= SVG_BM;
        by0 &= SVG_BM;
        by1 &= SVG_BM;
        let i = self.lattice[bx0 as usize];
        let j = self.lattice[bx1 as usize];
        let b00 = self.lattice[i + by0 as usize];
        let b10 = self.lattice[j + by0 as usize];
        let b01 = self.lattice[i + by1 as usize];
        let b11 = self.lattice[j + by1 as usize];
        let sx = svg_s_curve(rx0);
        let sy = svg_s_curve(ry0);
        let q = &self.gradient[channel][b00];
        let u = rx0 * q[0] + ry0 * q[1];
        let q = &self.gradient[channel][b10];
        let v = rx1 * q[0] + ry0 * q[1];
        let a = svg_lerp(sx, u, v);
        let q = &self.gradient[channel][b01];
        let u = rx0 * q[0] + ry1 * q[1];
        let q = &self.gradient[channel][b11];
        let v = rx1 * q[0] + ry1 * q[1];
        let b = svg_lerp(sx, u, v);
        svg_lerp(sy, a, b)
    }
}

fn svg_turbulence_random(seed: i32) -> i32 {
    let mut result = SVG_RAND_A * (seed % SVG_RAND_Q) - SVG_RAND_R * (seed / SVG_RAND_Q);
    if result <= 0 {
        result += SVG_RAND_M;
    }
    result
}

fn svg_s_curve(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

fn svg_lerp(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}

/// Build a `drop-shadow(dx dy blur color)` raster from an already-decoded source
/// image: take the source alpha, blur it, and tint it with the shadow colour.
/// The source image itself is rendered separately so image pixels follow image
/// DPI while this shadow raster follows filter DPI.
///
/// `display_w_pt`/`display_h_pt` are the rendered image-content size in points.
/// `dx_pt`/`dy_pt` are the shadow offsets (points; +y is downward). Returns the
/// shadow raster plus the overflow it adds beyond each border-box edge.
#[allow(clippy::too_many_arguments)]
pub(crate) fn drop_shadow_image(
    source: &image::RgbaImage,
    display_w_pt: f32,
    display_h_pt: f32,
    dx_pt: f32,
    dy_pt: f32,
    blur_radius_pt: f32,
    color: (f32, f32, f32, f32),
    filter_dpi: f32,
) -> Option<BlurredRaster> {
    drop_shadow_image_impl(
        source,
        display_w_pt,
        display_h_pt,
        dx_pt,
        dy_pt,
        blur_radius_pt,
        color,
        filter_dpi,
        false,
    )
}

/// Build a `drop-shadow()` replacement raster containing both the shadow and the
/// source image, used when prior filter ops have already rasterized the source.
#[allow(clippy::too_many_arguments)]
pub(crate) fn drop_shadow_image_with_source(
    source: &image::RgbaImage,
    display_w_pt: f32,
    display_h_pt: f32,
    dx_pt: f32,
    dy_pt: f32,
    blur_radius_pt: f32,
    color: (f32, f32, f32, f32),
    filter_dpi: f32,
) -> Option<BlurredRaster> {
    drop_shadow_image_impl(
        source,
        display_w_pt,
        display_h_pt,
        dx_pt,
        dy_pt,
        blur_radius_pt,
        color,
        filter_dpi,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn drop_shadow_image_impl(
    source: &image::RgbaImage,
    display_w_pt: f32,
    display_h_pt: f32,
    dx_pt: f32,
    dy_pt: f32,
    blur_radius_pt: f32,
    color: (f32, f32, f32, f32),
    filter_dpi: f32,
    composite_source: bool,
) -> Option<BlurredRaster> {
    if display_w_pt <= 0.0 || display_h_pt <= 0.0 {
        return None;
    }
    let (sw, sh) = (source.width(), source.height());
    if sw == 0 || sh == 0 {
        return None;
    }
    // CSS filters operate on the painted element. Build the shadow surface at
    // the displayed image resolution so a scaled image's alpha silhouette has
    // the same pixel grid Chrome filters.
    let s = filter_dpi_scale(filter_dpi);
    let dev_w = (display_w_pt / PT_PER_PX * s).round().max(1.0) as u32;
    let dev_h = (display_h_pt / PT_PER_PX * s).round().max(1.0) as u32;
    let painted =
        image::imageops::resize(source, dev_w, dev_h, image::imageops::FilterType::Nearest);
    let sigma = (blur_radius_pt / PT_PER_PX) * s;
    let dx = dx_pt / PT_PER_PX * s;
    let dy = dy_pt / PT_PER_PX * s;

    // Padding must cover the blur feather AND the shadow offset so nothing clips.
    let pad = pad_pixels(sigma)
        .max(dx.abs().ceil() as u32)
        .max(dy.abs().ceil() as u32)
        + 1;
    let buf_w = dev_w + 2 * pad;
    let buf_h = dev_h + 2 * pad;

    // Shadow layer: source alpha, tinted, offset, then blurred.
    let mut shadow = image::RgbaImage::new(buf_w, buf_h);
    let (sr, sg, sb, sa) = color;
    let (cr, cg, cb) = (
        (sr * 255.0).round() as u8,
        (sg * 255.0).round() as u8,
        (sb * 255.0).round() as u8,
    );
    for y in 0..dev_h {
        for x in 0..dev_w {
            let a = painted.get_pixel(x, y)[3];
            if a == 0 {
                continue;
            }
            let tx = x as i32 + pad as i32 + dx.round() as i32;
            let ty = y as i32 + pad as i32 + dy.round() as i32;
            if tx < 0 || ty < 0 || tx >= buf_w as i32 || ty >= buf_h as i32 {
                continue;
            }
            let out_a = (a as f32 * sa).round() as u8;
            shadow.put_pixel(tx as u32, ty as u32, image::Rgba([cr, cg, cb, out_a]));
        }
    }
    let mut composed = if sigma > 0.0 {
        blur_premultiplied(&shadow, sigma)
    } else {
        shadow
    };

    if composite_source {
        for y in 0..dev_h {
            for x in 0..dev_w {
                let src = *painted.get_pixel(x, y);
                if src[3] == 0 {
                    continue;
                }
                let dx0 = x + pad;
                let dy0 = y + pad;
                let bg = *composed.get_pixel(dx0, dy0);
                composed.put_pixel(dx0, dy0, over(src, bg));
            }
        }
    }

    let overflow_pt = pad as f32 / s * PT_PER_PX;
    let asset = rgba_to_png_alpha_asset(composed)?;
    Some(BlurredRaster { asset, overflow_pt })
}

/// Source-over composite of `src` onto `bg`, both straight-alpha RGBA8.
fn over(src: image::Rgba<u8>, bg: image::Rgba<u8>) -> image::Rgba<u8> {
    let sa = src[3] as f32 / 255.0;
    let ba = bg[3] as f32 / 255.0;
    let oa = sa + ba * (1.0 - sa);
    if oa <= 0.0 {
        return image::Rgba([0, 0, 0, 0]);
    }
    let blend = |s: u8, b: u8| {
        let s = s as f32;
        let b = b as f32;
        ((s * sa + b * ba * (1.0 - sa)) / oa)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    image::Rgba([
        blend(src[0], bg[0]),
        blend(src[1], bg[1]),
        blend(src[2], bg[2]),
        (oa * 255.0).round() as u8,
    ])
}

/// Convert an `f32` 0..1 RGBA to a tiny-skia non-premultiplied `Color`.
fn color8(r: f32, g: f32, b: f32, a: f32) -> resvg::tiny_skia::Color {
    resvg::tiny_skia::Color::from_rgba(
        r.clamp(0.0, 1.0),
        g.clamp(0.0, 1.0),
        b.clamp(0.0, 1.0),
        a.clamp(0.0, 1.0),
    )
    .unwrap_or(resvg::tiny_skia::Color::TRANSPARENT)
}

/// Convert a tiny-skia premultiplied pixmap into a straight-alpha RGBA image.
fn pixmap_to_rgba(pixmap: &resvg::tiny_skia::Pixmap, w: u32, h: u32) -> image::RgbaImage {
    let mut out = image::RgbaImage::new(w, h);
    for (i, px) in pixmap.pixels().iter().enumerate() {
        // tiny-skia stores premultiplied; demultiply to straight alpha.
        let c = px.demultiply();
        let x = (i as u32) % w;
        let y = (i as u32) / w;
        out.put_pixel(x, y, image::Rgba([c.red(), c.green(), c.blue(), c.alpha()]));
    }
    out
}
