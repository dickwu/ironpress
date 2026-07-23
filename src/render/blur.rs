//! CSS `filter: blur()` and `filter: drop-shadow()` raster compositing.
//!
//! ironpress paints boxes and replaced images as vector content. The CSS
//! `filter` property (css-filter-effects-1 §2) instead operates on the
//! *rasterized* output of the element: a gaussian blur (or drop-shadow) is
//! applied to the painted pixels and feathers *outside* the element's border
//! box. We rasterize the element's paint into a pixel buffer
//! padded with transparency, blur it with the same discrete three-box
//! approximation used by Chromium's Skia path at practical filter sizes, and
//! embed the result as a PDF image XObject positioned so the padded buffer
//! feathers beyond the original box.
//!
//! Per css-filter-effects-1 §4.1, `blur(<length>)` uses a gaussian with
//! `stdDeviation` equal to that length. `filter_dpi` controls only the sampling
//! resolution of the embedded bitmap; PDF placement remains in authored points.

use crate::layout::engine::{ImageFormat, LayoutBorder, RasterImageAsset};
use crate::parser::ttf::TtfFont;
use crate::style::computed::{BoxShadow, DropShadow, ImageRendering};
use crate::types::{CornerRadii, EdgeSizes};

mod discrete;

use discrete::{DiscreteGaussianPlan, box_blur_axes};

/// Points per CSS pixel (1px = 0.75pt). `blur_radius` is stored in points.
const PT_PER_PX: f32 = 0.75;
/// Maximum device-pixel shortfall treated as arithmetic noise at a half-pixel
/// raster boundary.
const FILTER_RASTER_HALF_PIXEL_TOLERANCE: f64 = 0.01;

fn filter_dpi_scale(filter_dpi: f32) -> f32 {
    crate::style::raster_quality::raster_dpi_at_least(filter_dpi, 1.0) / 96.0
}

/// One authored filter extent represented both by the backing-pixel bound and
/// the unquantized paint extent within that bound.
#[derive(Clone, Copy)]
struct FilterRasterAxis {
    pixels: u32,
    paint_px: f32,
}

/// Quantize a positive authored point length for a filter backing image.
///
/// CSS dimensions commonly land exactly on half a device pixel (for example,
/// 140 CSS px at 300 DPI). Layout's `f32` arithmetic can represent that as a
/// less than one hundredth of a device pixel below the half and round it down. Resolve
/// only that floating-point tie noise so the raster's coverage has the same
/// authored dimensions as its vector placement.
fn filter_raster_axis(points: f32, scale: f32) -> Option<FilterRasterAxis> {
    let pixels = f64::from(points) / f64::from(PT_PER_PX) * f64::from(scale);
    Some(FilterRasterAxis {
        pixels: quantize_filter_raster_pixels(pixels)?,
        paint_px: pixels as f32,
    })
}

fn quantize_filter_raster_pixels(pixels: f64) -> Option<u32> {
    if !pixels.is_finite() || pixels <= 0.0 {
        return None;
    }
    let rounded = (pixels + 0.5 + FILTER_RASTER_HALF_PIXEL_TOLERANCE).floor();
    (rounded <= f64::from(u32::MAX)).then_some(rounded as u32)
}

fn filter_raster_pixels(points: f32, scale: f32) -> Option<u32> {
    Some(filter_raster_axis(points, scale)?.pixels)
}

/// Raster pixels for a point extent at the configured filter sampling density.
pub(crate) fn filter_raster_pixels_at_dpi(points: f32, filter_dpi: f32) -> Option<u32> {
    filter_raster_pixels(points, filter_dpi_scale(filter_dpi))
}

/// A blurred raster ready for embedding plus the overflow it adds outside the
/// element's border box (in points, applied symmetrically on every side).
pub(crate) struct BlurredRaster {
    pub asset: RasterImageAsset,
    /// Extra paint extent beyond each border-box edge, in points.
    pub overflow_pt: f32,
}

/// Device-quantized paint overflow of a blurred CSS box shadow.
///
/// Box-shadow uses half the authored blur radius as its gaussian sigma. Keep
/// this calculation beside the shadow rasterizer so source-graphic allocation
/// and actual shadow painting cannot disagree about the required padding.
pub(crate) fn box_shadow_blur_overflow(blur_radius_pt: f32, filter_dpi: f32) -> Option<f32> {
    if blur_radius_pt <= 0.0 {
        return Some(0.0);
    }
    let scale = filter_dpi_scale(filter_dpi);
    let sigma = (blur_radius_pt / PT_PER_PX) * scale / 2.0;
    let padding = pad_pixels(sigma)?;
    Some(padding as f32 / scale * PT_PER_PX)
}

/// Number of padding pixels to add on each side so a gaussian with the given
/// sigma can feather without clipping (3σ captures ~99.7% of the kernel).
fn pad_pixels(sigma: f32) -> Option<u32> {
    if !sigma.is_finite() || sigma < 0.0 {
        return None;
    }
    let pixels = (f64::from(sigma) * 3.0).ceil().max(1.0);
    (pixels <= f64::from(u32::MAX)).then_some(pixels as u32)
}

fn padded_pixels(content_pixels: u32, padding_pixels: u32) -> Option<u32> {
    content_pixels.checked_add(padding_pixels.checked_mul(2)?)
}

fn nonnegative_pixel_ceil(value: f32) -> Option<u32> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let pixels = f64::from(value).ceil();
    (pixels <= f64::from(u32::MAX)).then_some(pixels as u32)
}

/// The sampling method used for one CSS filter blur.
#[derive(Clone, Copy, Debug)]
enum FilterBlurSampling {
    SmallGaussian { sigma_px: f32 },
    ThreeBox(DiscreteGaussianPlan),
}

/// A CSS filter blur expressed in the pixels of its raster backing image.
#[derive(Clone, Copy)]
pub(crate) struct FilterBlurKernel {
    pub padding_px: u32,
    sampling: FilterBlurSampling,
}

impl FilterBlurKernel {
    pub(crate) fn new(blur_radius_pt: f32, filter_dpi: f32) -> Option<Self> {
        let nominal_sigma = blur_radius_pt / PT_PER_PX * filter_dpi_scale(filter_dpi);
        if !nominal_sigma.is_normal() || nominal_sigma <= 0.0 {
            return None;
        }
        let sampling = DiscreteGaussianPlan::from_sigma(nominal_sigma)
            .map(FilterBlurSampling::ThreeBox)
            .unwrap_or(FilterBlurSampling::SmallGaussian {
                sigma_px: nominal_sigma,
            });
        Some(Self {
            padding_px: pad_pixels(nominal_sigma)?,
            sampling,
        })
    }
}

/// Blur a CSS filter buffer with its discrete sampling plan.
pub(crate) fn blur_css_filter(
    img: &image::RgbaImage,
    kernel: FilterBlurKernel,
) -> Option<image::RgbaImage> {
    let premultiplied = crate::render::raster_pixels::premultiply_rgba8(img);
    let blurred = blur_css_filter_premultiplied(&premultiplied, kernel)?;
    Some(crate::render::raster_pixels::unpremultiply_rgba8(&blurred))
}

/// Blur an already-premultiplied CSS filter buffer and preserve that encoding.
///
/// Vector paint sources from tiny-skia are premultiplied already. Keeping them
/// that way avoids throwing away low-alpha colour precision before filtering.
fn blur_css_filter_premultiplied(
    premultiplied: &image::RgbaImage,
    kernel: FilterBlurKernel,
) -> Option<image::RgbaImage> {
    match kernel.sampling {
        FilterBlurSampling::SmallGaussian { sigma_px } => {
            Some(image::imageops::blur(premultiplied, sigma_px))
        }
        FilterBlurSampling::ThreeBox(plan) => box_blur_axes(premultiplied, plan),
    }
}

/// Gaussian-blur a straight-alpha RGBA buffer correctly: `image::imageops::blur`
/// blurs each channel independently, so transparent (0,0,0,0) padding would
/// bleed black into the feathered edge. Premultiply first, blur, then
/// un-premultiply so only visible colour contributes.
fn gaussian_blur_premultiplied(img: &image::RgbaImage, sigma: f32) -> Option<image::RgbaImage> {
    if !sigma.is_normal() || sigma <= 0.0 {
        return None;
    }
    let premultiplied = crate::render::raster_pixels::premultiply_rgba8(img);
    let blurred = blur_premultiplied_at_sigma(&premultiplied, sigma)?;
    Some(crate::render::raster_pixels::unpremultiply_rgba8(&blurred))
}

/// Apply the discrete Gaussian sampling Chromium uses for sufficiently broad
/// mask blurs, retaining a direct Gaussian for the small-radius case.
///
/// Shadow and filter sources arrive through different paint paths, but their
/// CSS blur radii describe the same visual kernel. Keeping the choice here
/// prevents those paths from drifting apart at high raster resolutions.
fn blur_premultiplied_at_sigma(
    premultiplied: &image::RgbaImage,
    sigma: f32,
) -> Option<image::RgbaImage> {
    if !sigma.is_normal() || sigma <= 0.0 {
        return None;
    }
    match DiscreteGaussianPlan::from_sigma(sigma) {
        Some(plan) => box_blur_axes(premultiplied, plan),
        None => Some(image::imageops::blur(premultiplied, sigma)),
    }
}

/// Encode a (possibly padded) RGBA buffer as a full PNG file and wrap it in a
/// `PngAlpha` asset, whose embedding path decodes colour + soft-mask so the
/// transparent feathered border survives into the PDF.
pub(crate) fn rgba_to_png_alpha_asset(
    img: image::RgbaImage,
    filter_dpi: f32,
) -> Option<RasterImageAsset> {
    let (width, height) = (img.width(), img.height());
    let mut encoded = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(
            &mut std::io::Cursor::new(&mut encoded),
            image::ImageFormat::Png,
        )
        .ok()?;
    Some(RasterImageAsset::rendered(
        encoded,
        width,
        height,
        ImageFormat::PngAlpha,
        None,
        filter_dpi,
    ))
}

/// Rasterize a (rounded) `box-shadow` rectangle into a transparent, padded RGBA
/// buffer, gaussian-blur it, and return the embeddable asset plus the overflow
/// it adds beyond each edge of the shadow rect.
///
/// `width_pt`/`height_pt` are the shadow rect size in points (border box grown
/// by `spread`). `radii` are its resolved corner radii. `shadow.blur` is the CSS
/// `box-shadow` blur radius in points; css-backgrounds-3 §7.1.1 defines the blur
/// as a gaussian whose standard deviation is *half* the blur radius
/// (`sigma = blur / 2`). The returned overflow
/// is the per-side padding in points: the buffer feathers symmetrically beyond
/// the shadow rect, so the caller positions the image at the shadow rect minus
/// `overflow_pt` on each side. Returns `None` when nothing would paint.
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

    // css-backgrounds-3: blur radius is 2σ, so σ = blur/2. Map to buffer pixels.
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
        let mut pb = tiny_skia::PathBuilder::new();
        append_rounded_box_path(&mut pb, ox, oy, box_x.paint_px, box_y.paint_px, radii_px);
        if let Some(path) = pb.finish() {
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
    // Spread changes the shadow shape, not the blur kernel.
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

    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(0.0, 0.0);
    pb.line_to(buf_w as f32, 0.0);
    pb.line_to(buf_w as f32, buf_h as f32);
    pb.line_to(0.0, buf_h as f32);
    pb.close();
    if hole_w > 0.0 && hole_h > 0.0 {
        let hole_radii = radii.grow(-shadow.spread) * pt_to_px;
        append_rounded_box_path(&mut pb, hole_x, hole_y, hole_w, hole_h, hole_radii);
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
    img: &mut image::RgbaImage,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radii: CornerRadii,
) -> Option<()> {
    use resvg::tiny_skia;

    let mut mask = tiny_skia::Pixmap::new(img.width(), img.height())?;
    let mut pb = tiny_skia::PathBuilder::new();
    append_rounded_box_path(&mut pb, x, y, w, h, radii);
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
    radii: CornerRadii,
) {
    let radii = radii.fit_to(w, h);
    if radii.is_zero() {
        append_rounded_path(pb, x, y, w, h, 0.0, 0.0);
        return;
    }

    let k = 0.552_284_8;
    let (x0, y0) = (x, y);
    let (x1, y1) = (x + w, y + h);
    let (tlx, tly) = (radii.top_left.x, radii.top_left.y);
    let (trx, try_) = (radii.top_right.x, radii.top_right.y);
    let (brx, bry) = (radii.bottom_right.x, radii.bottom_right.y);
    let (blx, bly) = (radii.bottom_left.x, radii.bottom_left.y);

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
    if rx <= 0.0 && ry <= 0.0 {
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
    let pad = pad_pixels(sigma)?;
    let buf_w = padded_pixels(mw, pad)?;
    let buf_h = padded_pixels(mh, pad)?;

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
            let out_a = (cov as f32 * ca).round().clamp(0.0, 255.0) as u8;
            tinted.put_pixel(x + pad, y + pad, image::Rgba([r8, g8, b8, out_a]));
        }
    }
    if !any {
        return None;
    }
    let blurred = if sigma > 0.0 {
        gaussian_blur_premultiplied(&tinted, sigma)?
    } else {
        tinted
    };

    let overflow_pt = pad as f32 / s * PT_PER_PX;
    let asset = rgba_to_png_alpha_asset(blurred, filter_dpi)?;
    Some((BlurredRaster { asset, overflow_pt }, pad))
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

/// Synthetic face effects applied while rasterizing one shaped glyph run.
///
/// Keeping these together prevents filter, shadow, and PDF raster paths from
/// independently dropping one part of the resolved font presentation.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct GlyphRasterStyle {
    pub(crate) embolden: f32,
    pub(crate) outline: f32,
    pub(crate) shear: f32,
}

/// Complete semantic input for one glyph-outline rasterization.
pub(crate) struct GlyphRasterRequest<'a> {
    pub(crate) font: &'a TtfFont,
    pub(crate) font_size: f32,
    pub(crate) glyphs: &'a [crate::text::ShapedGlyph],
    pub(crate) style: GlyphRasterStyle,
    pub(crate) dpi: f32,
}

/// Rasterize a run's shaped glyph outlines into an 8-bit alpha coverage mask at
/// `DEVICE_SCALE`, for use as a `text-shadow` blur source. `font_data` is the
/// raw TTF/OTF bytes; `units_per_em` is the font's em scale; `font_size_pt` is
/// the run's font size in points; `glyphs` is the shaped run. Returns the mask
/// plus the text-origin position inside it, or `None` when the font can't be
/// parsed or nothing is drawn (so the caller falls back to a sharp copy).
pub(crate) fn rasterize_run_alpha(request: GlyphRasterRequest<'_>) -> Option<GlyphRaster> {
    use resvg::tiny_skia;

    if request.font.units_per_em == 0 || request.font_size <= 0.0 || request.glyphs.is_empty() {
        return None;
    }

    let face =
        rustybuzz::ttf_parser::Face::parse(&request.font.data, request.font.face_index.get())
            .ok()?;

    // Glyph font units -> device pixels: (units/upem) * font_size_pt(px-equiv)
    // * filter_dpi/96. font_size is in points; CSS px = pt / PT_PER_PX.
    let s = filter_dpi_scale(request.dpi);
    let upem = request.font.units_per_em as f32;
    let px_per_unit = (request.font_size / PT_PER_PX) * s / upem;
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
        shear: f32,
    }
    impl rustybuzz::ttf_parser::OutlineBuilder for Builder<'_> {
        fn move_to(&mut self, x: f32, y: f32) {
            self.pb.move_to(
                self.pen_x + (x + self.shear * y) * self.scale,
                self.baseline_y - y * self.scale,
            );
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.pb.line_to(
                self.pen_x + (x + self.shear * y) * self.scale,
                self.baseline_y - y * self.scale,
            );
        }
        fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
            self.pb.quad_to(
                self.pen_x + (x1 + self.shear * y1) * self.scale,
                self.baseline_y - y1 * self.scale,
                self.pen_x + (x + self.shear * y) * self.scale,
                self.baseline_y - y * self.scale,
            );
        }
        fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
            self.pb.cubic_to(
                self.pen_x + (x1 + self.shear * y1) * self.scale,
                self.baseline_y - y1 * self.scale,
                self.pen_x + (x2 + self.shear * y2) * self.scale,
                self.baseline_y - y2 * self.scale,
                self.pen_x + (x + self.shear * y) * self.scale,
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
    for g in request.glyphs {
        let gid = rustybuzz::ttf_parser::GlyphId(g.glyph_id);
        let mut b = Builder {
            pb: &mut pb,
            pen_x: pen_x + g.x_offset * pt_to_px,
            baseline_y: -g.y_offset * pt_to_px,
            scale: px_per_unit,
            shear: request.style.shear,
        };
        let _ = face.outline_glyph(gid, &mut b);
        pen_x += g.x_advance * pt_to_px;
    }
    let path = pb.finish()?;
    let bounds = path.bounds();

    // Margin so the outline anti-aliasing isn't clipped at the buffer edge.
    let embolden_px = (request.style.embolden * pt_to_px).max(0.0);
    let outline_px = (request.style.outline * pt_to_px).max(0.0);
    let stroke_px = embolden_px.max(outline_px);
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

/// Device pixels per point at one physical raster resolution.
pub(crate) fn px_per_pt_at_dpi(dpi: f32) -> f32 {
    filter_dpi_scale(dpi) / PT_PER_PX
}

/// Rasterize a solid-fill box (background colour + border) into a transparent,
/// padded RGBA buffer, gaussian-blur it, and return the embeddable asset plus
/// the overflow it adds outside the border box.
///
/// `width_pt`/`height_pt` are the border-box size in points. `blur_radius_pt`
/// is `ComputedStyle::filter.blur_radius` (already in points). Returns `None` when the
/// element paints nothing (so the caller falls back to its normal path).
pub(crate) fn blur_box(
    width_pt: f32,
    height_pt: f32,
    background: Option<crate::types::Color>,
    border: &LayoutBorder,
    blur_radius_pt: f32,
    filter_dpi: f32,
) -> Option<BlurredRaster> {
    if blur_radius_pt <= 0.0 || width_pt <= 0.0 || height_pt <= 0.0 {
        return None;
    }
    let has_bg = background.is_some_and(|color| color.alpha() > 0.0);
    if !has_bg && !border.has_visible() {
        return None;
    }

    use resvg::tiny_skia;

    // Buffer geometry: box at device scale plus transparent padding for the
    // gaussian to feather into.
    let s = filter_dpi_scale(filter_dpi);
    let kernel = FilterBlurKernel::new(blur_radius_pt, filter_dpi)?;
    let pad = kernel.padding_px;
    let box_x = filter_raster_axis(width_pt, s)?;
    let box_y = filter_raster_axis(height_pt, s)?;
    let buf_w = padded_pixels(box_x.pixels, pad)?;
    let buf_h = padded_pixels(box_y.pixels, pad)?;

    let mut pixmap = tiny_skia::Pixmap::new(buf_w, buf_h)?;
    let ox = pad as f32;
    let oy = pad as f32;

    // Background fill covers the whole border box.
    if let Some(color) = background
        && color.alpha() > 0.0
    {
        let (r, g, b, a) = color.to_f32_rgba();
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(color8(r, g, b, a));
        paint.anti_alias = true;
        let rect = tiny_skia::Rect::from_xywh(ox, oy, box_x.paint_px, box_y.paint_px)?;
        pixmap.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);
    }

    // Borders paint INSIDE the border box (the declared size is the border box).
    // Fill each visible side as a rectangle so a uniform solid frame matches the
    // vector painter; the gaussian then softens both fill and frame edge.
    paint_border_rects(
        &mut pixmap,
        border,
        ox,
        oy,
        box_x.paint_px,
        box_y.paint_px,
        s,
    );

    let premultiplied = crate::render::raster_pixels::pixmap_to_premultiplied_rgba(&pixmap);
    let rgba = crate::render::raster_pixels::unpremultiply_rgba8(&blur_css_filter_premultiplied(
        &premultiplied,
        kernel,
    )?);

    let overflow_pt = pad as f32 / s * PT_PER_PX;
    let asset = rgba_to_png_alpha_asset(rgba, filter_dpi)?;
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
        let (r, g, b) = side.color.to_f32_rgb();
        paint.set_color(color8(r, g, b, side.color.alpha()));
        paint.anti_alias = true;
        if let Some(rect) = tiny_skia::Rect::from_xywh(ox + x, oy + y, w, h) {
            pixmap.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);
        }
    }
}

/// Rasterize a decoded image at its painted content-box size and the configured
/// filter resolution. CSS filter functions operate on this painted image, not
/// on the source bitmap's intrinsic pixel grid.
pub(crate) fn rasterize_image_buffer(
    source: &image::RgbaImage,
    display_w_pt: f32,
    display_h_pt: f32,
    image_rendering: ImageRendering,
    filter_dpi: f32,
) -> Option<image::RgbaImage> {
    if source.width() == 0 || source.height() == 0 || display_w_pt <= 0.0 || display_h_pt <= 0.0 {
        return None;
    }
    let scale = filter_dpi_scale(filter_dpi);
    let width = filter_raster_pixels(display_w_pt, scale)?;
    let height = filter_raster_pixels(display_h_pt, scale)?;
    let css_scaled = if image_rendering.is_pixelated() {
        pixelated_image_at_css_size(source, display_w_pt, display_h_pt)?
    } else {
        source.clone()
    };
    Some(resize_image_for_display(
        &css_scaled,
        width,
        height,
        image_rendering,
    ))
}

/// Apply CSS Images' `pixelated` operation in CSS image coordinates. The
/// physical PDF/filter backing scale is intentionally applied later: it is an
/// output-quality decision, not a second CSS resize.
pub(crate) fn pixelated_image_at_css_size(
    source: &image::RgbaImage,
    display_w_pt: f32,
    display_h_pt: f32,
) -> Option<image::RgbaImage> {
    if source.width() == 0 || source.height() == 0 || display_w_pt <= 0.0 || display_h_pt <= 0.0 {
        return None;
    }
    let target_width = css_image_pixels(display_w_pt)?;
    let target_height = css_image_pixels(display_h_pt)?;
    let integer_width = nearest_pixelated_multiple(source.width(), display_w_pt);
    let integer_height = nearest_pixelated_multiple(source.height(), display_h_pt);
    let integer_scaled = image::imageops::resize(
        source,
        integer_width,
        integer_height,
        image::imageops::FilterType::Nearest,
    );
    Some(resize_rgba_smooth(
        &integer_scaled,
        target_width,
        target_height,
    ))
}

/// Smoothly resize a transparent raster in CSS image coordinates.
///
/// The `image` crate's triangle kernel samples just beyond an exactly aligned
/// transparent edge when shrinking. CSS Images' `pixelated` algorithm requires
/// its second stage to be smooth, but it must not grow a source alpha silhouette
/// whose integral-multiple boundary maps exactly to the target grid. Sampling
/// pixel centres with premultiplied bilinear interpolation preserves that
/// boundary and prevents transparent RGB from creating a coloured fringe.
fn resize_rgba_smooth(
    source: &image::RgbaImage,
    target_width: u32,
    target_height: u32,
) -> image::RgbaImage {
    let (source_width, source_height) = source.dimensions();
    if (source_width, source_height) == (target_width, target_height) {
        return source.clone();
    }
    let mut output = image::RgbaImage::new(target_width, target_height);
    for y in 0..target_height {
        let (top, bottom, vertical) = bilinear_axis(source_height, target_height, y);
        for x in 0..target_width {
            let (left, right, horizontal) = bilinear_axis(source_width, target_width, x);
            let top_left = premultiplied_pixel(source.get_pixel(left, top));
            let top_right = premultiplied_pixel(source.get_pixel(right, top));
            let bottom_left = premultiplied_pixel(source.get_pixel(left, bottom));
            let bottom_right = premultiplied_pixel(source.get_pixel(right, bottom));
            let top = interpolate_premultiplied(top_left, top_right, horizontal);
            let bottom = interpolate_premultiplied(bottom_left, bottom_right, horizontal);
            output.put_pixel(
                x,
                y,
                unpremultiply_pixel(interpolate_premultiplied(top, bottom, vertical)),
            );
        }
    }
    output
}

/// Return the source pixels surrounding a destination pixel centre and the
/// interpolation weight toward the latter one.
fn bilinear_axis(source_len: u32, target_len: u32, target_index: u32) -> (u32, u32, f32) {
    if source_len <= 1 || target_len == 0 {
        return (0, 0, 0.0);
    }
    let position = (((target_index as f32 + 0.5) * source_len as f32 / target_len as f32) - 0.5)
        .clamp(0.0, source_len.saturating_sub(1) as f32);
    let first = position.floor() as u32;
    let second = (first + 1).min(source_len - 1);
    (first, second, position - first as f32)
}

fn premultiplied_pixel(pixel: &image::Rgba<u8>) -> [f32; 4] {
    let alpha = f32::from(pixel[3]) / 255.0;
    [
        f32::from(pixel[0]) * alpha,
        f32::from(pixel[1]) * alpha,
        f32::from(pixel[2]) * alpha,
        alpha,
    ]
}

fn interpolate_premultiplied(start: [f32; 4], end: [f32; 4], amount: f32) -> [f32; 4] {
    std::array::from_fn(|channel| start[channel] + (end[channel] - start[channel]) * amount)
}

fn unpremultiply_pixel(pixel: [f32; 4]) -> image::Rgba<u8> {
    let alpha = pixel[3].clamp(0.0, 1.0);
    if alpha == 0.0 {
        return image::Rgba([0, 0, 0, 0]);
    }
    image::Rgba([
        (pixel[0] / alpha).round().clamp(0.0, 255.0) as u8,
        (pixel[1] / alpha).round().clamp(0.0, 255.0) as u8,
        (pixel[2] / alpha).round().clamp(0.0, 255.0) as u8,
        (alpha * 255.0).round().clamp(0.0, 255.0) as u8,
    ])
}

fn css_image_pixels(display_axis_pt: f32) -> Option<u32> {
    let pixels = display_axis_pt / PT_PER_PX;
    (pixels.is_finite() && pixels > 0.0).then(|| pixels.round().clamp(1.0, u32::MAX as f32) as u32)
}

/// Rasterize a CSS-scaled image into a physical target surface.
pub(crate) fn resize_image_for_display(
    source: &image::RgbaImage,
    target_width: u32,
    target_height: u32,
    image_rendering: ImageRendering,
) -> image::RgbaImage {
    match image_rendering {
        ImageRendering::Pixelated | ImageRendering::CrispEdges => {
            resize_nearest_center(source, target_width, target_height)
        }
        ImageRendering::Smooth => image::imageops::resize(
            source,
            target_width,
            target_height,
            image::imageops::FilterType::Triangle,
        ),
        ImageRendering::HighQuality => image::imageops::resize(
            source,
            target_width,
            target_height,
            image::imageops::FilterType::Lanczos3,
        ),
        // CSS leaves the UA's `auto` algorithm open. Retain Ironpress's
        // established nearest-centre choice so this new property does not alter
        // existing documents that did not opt into a scaling preference.
        ImageRendering::Auto => resize_nearest_center(source, target_width, target_height),
    }
}

fn nearest_pixelated_multiple(source_axis: u32, display_axis_pt: f32) -> u32 {
    let target_css_px = display_axis_pt / PT_PER_PX;
    let multiple = (target_css_px / source_axis as f32).round().max(1.0);
    (source_axis as f32 * multiple)
        .round()
        .clamp(1.0, u32::MAX as f32) as u32
}

/// Encode an already-built blurred RGBA buffer + overflow into a `BlurredRaster`.
pub(crate) fn raster_from_buffer(
    buf: image::RgbaImage,
    overflow_pt: f32,
    filter_dpi: f32,
) -> Option<BlurredRaster> {
    let asset = rgba_to_png_alpha_asset(buf, filter_dpi)?;
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
    let kernel = FilterBlurKernel::new(blur_radius_pt, filter_dpi)?;
    let pad = kernel.padding_px;
    let padded_w = padded_pixels(source.width(), pad)?;
    let padded_h = padded_pixels(source.height(), pad)?;
    let mut padded = image::RgbaImage::new(padded_w, padded_h);
    image::imageops::replace(&mut padded, source, pad as i64, pad as i64);
    let blurred = blur_css_filter(&padded, kernel)?;
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
    raster_from_buffer(blurred, overflow_pt, filter_dpi)
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
    /// Filter-region extent beyond the source graphic, in SVG user units.
    pub filter_region_overflow: EdgeSizes,
}

/// A rasterized SVG filter result with its directional paint extent.
pub(crate) struct SvgFilterRaster {
    pub asset: RasterImageAsset,
    pub raster_overflow: EdgeSizes,
}

pub(crate) fn turbulence_displacement_rect(
    width_pt: f32,
    height_pt: f32,
    color: crate::types::Color,
    spec: &SvgTurbulenceDisplacement,
    filter_dpi: f32,
) -> Option<SvgFilterRaster> {
    if width_pt <= 0.0 || height_pt <= 0.0 || color.a <= 0.0 {
        return None;
    }
    let scale = filter_dpi_scale(filter_dpi);
    let width_css = width_pt / PT_PER_PX;
    let height_css = height_pt / PT_PER_PX;
    let overflow = spec.filter_region_overflow;
    let canvas_w_css = width_css + overflow.horizontal();
    let canvas_h_css = height_css + overflow.vertical();
    let px_w = (canvas_w_css * scale).round().max(1.0) as u32;
    let px_h = (canvas_h_css * scale).round().max(1.0) as u32;
    let ox = (overflow.left * scale).round() as i32;
    let oy = (overflow.top * scale).round() as i32;
    let rect_w = (width_css * scale).round().max(1.0) as i32;
    let rect_h = (height_css * scale).round().max(1.0) as i32;

    let fill = image::Rgba(color.to_rgba8());
    let mut source = image::RgbaImage::new(px_w, px_h);
    for y in oy.max(0)..(oy + rect_h).min(px_h as i32) {
        for x in ox.max(0)..(ox + rect_w).min(px_w as i32) {
            source.put_pixel(x as u32, y as u32, fill);
        }
    }

    let noise = SvgTurbulence::new(spec.seed);
    let mut out = image::RgbaImage::new(px_w, px_h);
    let view_x = -f64::from(overflow.left);
    let view_y = -f64::from(overflow.top);
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

    Some(SvgFilterRaster {
        asset: rgba_to_png_alpha_asset(out, filter_dpi)?,
        raster_overflow: overflow * PT_PER_PX,
    })
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
    lattice: [usize; SVG_B_LEN],
    gradient: [[[f64; 2]; SVG_B_LEN]; 4],
}

impl SvgTurbulence {
    fn new(mut seed: i32) -> Self {
        let mut lattice = [0; SVG_B_LEN];
        let mut gradient = [[[0.0; 2]; SVG_B_LEN]; 4];
        if seed <= 0 {
            seed = -seed % (SVG_RAND_M - 1) + 1;
        }
        if seed > SVG_RAND_M - 1 {
            seed = SVG_RAND_M - 1;
        }
        for channel_gradient in &mut gradient {
            for i in 0..SVG_B_SIZE {
                lattice[i] = i;
                loop {
                    seed = svg_turbulence_random(seed);
                    let x = ((seed % (SVG_B_SIZE_I32 + SVG_B_SIZE_I32)) - SVG_B_SIZE_I32) as f64
                        / SVG_B_SIZE_I32 as f64;
                    seed = svg_turbulence_random(seed);
                    let y = ((seed % (SVG_B_SIZE_I32 + SVG_B_SIZE_I32)) - SVG_B_SIZE_I32) as f64
                        / SVG_B_SIZE_I32 as f64;
                    let length = (x * x + y * y).sqrt();
                    if length == 0.0 || length > 1.0 {
                        continue;
                    }
                    channel_gradient[i] = [x / length, y / length];
                    break;
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
            for channel_gradient in &mut gradient {
                channel_gradient[SVG_B_SIZE + i] = channel_gradient[i];
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

struct DropShadowSurface {
    painted: image::RgbaImage,
    kernel: Option<FilterBlurKernel>,
    offset: DeviceOffset,
    padding: u32,
    width: u32,
    height: u32,
    scale: f32,
}

impl DropShadowSurface {
    fn new(
        source: &image::RgbaImage,
        display_w_pt: f32,
        display_h_pt: f32,
        shadow: DropShadow,
        image_rendering: ImageRendering,
        filter_dpi: f32,
    ) -> Option<Self> {
        if display_w_pt <= 0.0
            || display_h_pt <= 0.0
            || !shadow.blur.is_finite()
            || !shadow.dx.is_finite()
            || !shadow.dy.is_finite()
            || source.width() == 0
            || source.height() == 0
        {
            return None;
        }
        let scale = filter_dpi_scale(filter_dpi);
        let content_width = filter_raster_pixels(display_w_pt, scale)?;
        let content_height = filter_raster_pixels(display_h_pt, scale)?;
        let painted = rasterize_image_buffer(
            source,
            display_w_pt,
            display_h_pt,
            image_rendering,
            filter_dpi,
        )?;
        let kernel = (shadow.blur > 0.0)
            .then(|| FilterBlurKernel::new(shadow.blur, filter_dpi))
            .flatten();
        let offset = DeviceOffset::from_points(shadow.dx, shadow.dy, scale);
        let padding = kernel
            .map_or(1, |kernel| kernel.padding_px)
            .max(offset.padding()?)
            .checked_add(1)?;
        Some(Self {
            painted,
            kernel,
            offset,
            padding,
            width: padded_pixels(content_width, padding)?,
            height: padded_pixels(content_height, padding)?,
            scale,
        })
    }

    fn shadow_layer(self, shadow: DropShadow) -> Option<(image::RgbaImage, Self)> {
        let mut layer = image::RgbaImage::new(self.width, self.height);
        let [r, g, b, alpha] = shadow.color.to_rgba8();
        let color = [r, g, b];
        paint_translated_alpha(
            &mut layer,
            &self.painted,
            self.padding,
            self.offset,
            color,
            f32::from(alpha) / 255.0,
        );
        let layer = match self.kernel {
            Some(kernel) => blur_css_filter(&layer, kernel)?,
            None => layer,
        };
        Some((layer, self))
    }

    fn overflow_pt(&self) -> f32 {
        self.padding as f32 / self.scale * PT_PER_PX
    }
}

/// Build a `drop-shadow()` replacement raster containing both the shadow and the
/// source image on one configured-resolution filter surface.
pub(crate) fn drop_shadow_image(
    source: &image::RgbaImage,
    display_w_pt: f32,
    display_h_pt: f32,
    shadow: DropShadow,
    image_rendering: ImageRendering,
    filter_dpi: f32,
) -> Option<BlurredRaster> {
    let (mut composed, surface) = DropShadowSurface::new(
        source,
        display_w_pt,
        display_h_pt,
        shadow,
        image_rendering,
        filter_dpi,
    )?
    .shadow_layer(shadow)?;

    for y in 0..surface.painted.height() {
        for x in 0..surface.painted.width() {
            let src = *surface.painted.get_pixel(x, y);
            if src[3] == 0 {
                continue;
            }
            let dx0 = x + surface.padding;
            let dy0 = y + surface.padding;
            let bg = *composed.get_pixel(dx0, dy0);
            composed.put_pixel(dx0, dy0, over(src, bg));
        }
    }

    let overflow_pt = surface.overflow_pt();
    let asset = rgba_to_png_alpha_asset(composed, filter_dpi)?;
    Some(BlurredRaster { asset, overflow_pt })
}

/// A CSS filter offset converted to the device-pixel coordinates of its filter
/// surface. Keeping both axes together prevents accidental mixed rounding.
#[derive(Clone, Copy)]
struct DeviceOffset {
    x: f32,
    y: f32,
}

impl DeviceOffset {
    fn from_points(x: f32, y: f32, device_scale: f32) -> Self {
        Self {
            x: x / PT_PER_PX * device_scale,
            y: y / PT_PER_PX * device_scale,
        }
    }

    fn padding(self) -> Option<u32> {
        nonnegative_pixel_ceil(self.x.abs())
            .zip(nonnegative_pixel_ceil(self.y.abs()))
            .map(|(horizontal, vertical)| horizontal.max(vertical))
    }
}

/// Translate a source alpha field without discarding fractional device-pixel
/// offsets. Depositing each source sample into its four destination neighbours
/// is bilinear resampling of the alpha field; a single rounded placement makes
/// a 0.5-pixel CSS offset visibly wider on opposite sides of a hard shadow.
fn paint_translated_alpha(
    target: &mut image::RgbaImage,
    source: &image::RgbaImage,
    padding: u32,
    offset: DeviceOffset,
    color: [u8; 3],
    opacity: f32,
) {
    let width = target.width();
    let height = target.height();
    let mut alpha = vec![0.0; target.pixels().len()];
    for y in 0..source.height() {
        for x in 0..source.width() {
            let coverage = f32::from(source.get_pixel(x, y)[3]) / 255.0 * opacity;
            if coverage == 0.0 {
                continue;
            }
            let destination_x = x as f32 + padding as f32 + offset.x;
            let destination_y = y as f32 + padding as f32 + offset.y;
            let left = destination_x.floor() as i32;
            let top = destination_y.floor() as i32;
            let horizontal = destination_x - left as f32;
            let vertical = destination_y - top as f32;
            for (dy, vertical_weight) in [(0, 1.0 - vertical), (1, vertical)] {
                for (dx, horizontal_weight) in [(0, 1.0 - horizontal), (1, horizontal)] {
                    let target_x = left + dx;
                    let target_y = top + dy;
                    if target_x < 0
                        || target_y < 0
                        || target_x >= width as i32
                        || target_y >= height as i32
                    {
                        continue;
                    }
                    let index = target_y as usize * width as usize + target_x as usize;
                    alpha[index] += coverage * horizontal_weight * vertical_weight;
                }
            }
        }
    }
    for (pixel, coverage) in target.pixels_mut().zip(alpha) {
        let alpha = (coverage.clamp(0.0, 1.0) * 255.0).round() as u8;
        if alpha != 0 {
            *pixel = image::Rgba([color[0], color[1], color[2], alpha]);
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CornerRadius, EdgeSizes};

    #[test]
    fn pixelated_css_scaling_keeps_a_natural_size_image_unmodified() {
        let source = image::RgbaImage::from_raw(2, 1, vec![0, 0, 0, 255, 255, 255, 255, 255])
            .expect("test image dimensions are valid");

        let scaled = pixelated_image_at_css_size(&source, 2.0 * PT_PER_PX, PT_PER_PX)
            .expect("positive CSS image size should rasterize");

        assert_eq!(scaled, source);
    }

    #[test]
    fn pixelated_css_scaling_smooths_only_after_the_integer_stage() {
        let source = image::RgbaImage::from_raw(2, 1, vec![0, 0, 0, 255, 255, 255, 255, 255])
            .expect("test image dimensions are valid");

        let scaled = pixelated_image_at_css_size(&source, 5.0 * PT_PER_PX, PT_PER_PX)
            .expect("positive CSS image size should rasterize");

        assert_eq!(scaled.dimensions(), (5, 1));
        assert_eq!(scaled.get_pixel(0, 0)[0], 0);
        assert_eq!(scaled.get_pixel(4, 0)[0], 255);
        assert!(
            (0..255).contains(&scaled.get_pixel(2, 0)[0]),
            "the non-integer remainder should be smoothly resampled"
        );
    }

    #[test]
    fn pixelated_css_scaling_preserves_an_integral_alpha_boundary() {
        let source = image::RgbaImage::from_fn(64, 64, |x, y| {
            if (4..60).contains(&x) && (4..60).contains(&y) {
                image::Rgba([220, 40, 40, 255])
            } else {
                image::Rgba([220, 40, 40, 0])
            }
        });

        let scaled = pixelated_image_at_css_size(&source, 160.0 * PT_PER_PX, 160.0 * PT_PER_PX)
            .expect("positive CSS image size should rasterize");
        let alpha_bounds = scaled
            .enumerate_pixels()
            .filter_map(|(x, y, pixel)| (pixel[3] != 0).then_some((x, y)))
            .fold(
                None::<(u32, u32, u32, u32)>,
                |bounds, (x, y)| match bounds {
                    Some((left, top, right, bottom)) => {
                        Some((left.min(x), top.min(y), right.max(x), bottom.max(y)))
                    }
                    None => Some((x, y, x, y)),
                },
            );

        assert_eq!(alpha_bounds, Some((10, 10, 149, 149)));
    }

    #[test]
    fn svg_displacement_raster_preserves_directional_filter_region() {
        let raster = turbulence_displacement_rect(
            90.0 * PT_PER_PX,
            58.0 * PT_PER_PX,
            crate::types::Color::from_srgb(0.83, 0.0, 0.0, 1.0),
            &SvgTurbulenceDisplacement {
                base_frequency_x: 0.08,
                base_frequency_y: 0.08,
                num_octaves: 1,
                seed: 7,
                scale: 18.0,
                x_channel: 0,
                y_channel: 1,
                filter_region_overflow: EdgeSizes::new(11.6, 18.0, 11.6, 18.0),
            },
            300.0,
        )
        .expect("filter region produces a raster");

        assert_eq!(
            (raster.asset.source_width, raster.asset.source_height),
            (394, 254)
        );
        let expected_overflow = EdgeSizes::new(11.6, 18.0, 11.6, 18.0) * PT_PER_PX;
        assert_eq!(raster.raster_overflow, expected_overflow);
    }

    #[test]
    fn svg_turbulence_uses_filter_effects_rejection_sampling() {
        let turbulence = SvgTurbulence::new(7);
        // Filter Effects 1 §9.21 supplies this exact pseudo-random sequence.
        // Rejection sampling changes the subsequent lattice permutation, so
        // this catches the tempting but non-conforming "normalize everything"
        // implementation too.
        assert_eq!(&turbulence.lattice[..8], &[0, 78, 89, 7, 57, 173, 142, 40]);
        let [x, y] = turbulence.gradient[0][0];
        assert!((x - 0.809_942_121_543_021_1).abs() < 1e-15);
        assert!((y + 0.586_509_812_151_842_8).abs() < 1e-15);
    }

    #[test]
    fn filter_raster_pixel_rounding_ignores_half_pixel_float_noise() {
        assert_eq!(filter_raster_pixels(105.0, 3.125), Some(438));
        assert_eq!(filter_raster_pixels(104.999_99, 3.125), Some(438));
        assert_eq!(filter_raster_pixels(104.997, 3.125), Some(437));
    }

    #[test]
    fn filter_raster_axis_keeps_fractional_paint_extent_inside_rounded_backing() {
        let axis = filter_raster_axis(135.0, 3.125).expect("finite positive extent");
        assert_eq!(axis.pixels, 563);
        assert_eq!(axis.paint_px, 562.5);
    }

    #[test]
    fn blur_buffer_dimensions_reject_overflow() {
        assert_eq!(padded_pixels(u32::MAX, 1), None);
        assert_eq!(pad_pixels(f32::INFINITY), None);
        assert_eq!(nonnegative_pixel_ceil(f32::NAN), None);
    }

    #[test]
    fn css_filter_kernel_keeps_authored_overflow_and_selects_three_boxes() {
        let kernel = FilterBlurKernel::new(4.5, 300.0).unwrap();
        assert_eq!(kernel.padding_px, 57); // 3 × 6 CSS px × 300 / 96.
        let FilterBlurSampling::ThreeBox(plan) = kernel.sampling else {
            panic!("broad CSS blur should use the bounded integer plan");
        };
        assert_eq!(plan.pass_widths(), [35, 35, 35]);
    }

    #[test]
    fn css_filter_kernel_rejects_non_finite_input() {
        assert!(FilterBlurKernel::new(f32::INFINITY, 300.0).is_none());
        assert!(FilterBlurKernel::new(4.5, f32::NAN).is_some());
    }

    #[test]
    fn discrete_gaussian_matches_chromium_pdf_alpha_profile() {
        let plan = DiscreteGaussianPlan::from_sigma(15.625).expect("finite sigma has a plan");
        assert_eq!(plan.pass_widths(), [29, 29, 29]);

        let mut source = image::RgbaImage::new(594, 594);
        for y in 47..547 {
            for x in 47..547 {
                source.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
            }
        }
        let blurred =
            box_blur_axes(&source, plan).expect("valid image and plan produce a blurred image");
        let expected = [
            (10, 1),
            (15, 3),
            (20, 9),
            (25, 19),
            (30, 34),
            (35, 57),
            (40, 86),
            (45, 118),
            (46, 124),
            (47, 131),
            (48, 137),
            (50, 150),
        ];
        for (x, alpha) in expected {
            assert_eq!(blurred.get_pixel(x, 300)[3], alpha, "x={x}");
        }
    }

    fn rounded_box_pixels(radii: CornerRadii) -> Vec<u8> {
        let mut pixmap = resvg::tiny_skia::Pixmap::new(8, 8).unwrap();
        let mut builder = resvg::tiny_skia::PathBuilder::new();
        append_rounded_box_path(&mut builder, 1.0, 1.0, 6.0, 6.0, radii);
        let path = builder.finish().unwrap();
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
