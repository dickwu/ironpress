//! Device-space canvas for CSS filter SourceGraphic painting.

use crate::layout::engine::{LayoutBorder, RasterImageAsset};
use crate::layout::filter::FilterRasterOutput;
use crate::render::borders::CssRoundedRect;
use crate::style::computed::BoxShadow;
use crate::types::{Color, CornerRadii, EdgeSizes, Point, Rect, Size};

use super::source_borders::{RasterBorder, RasterColumnRule};

const SAMPLE_OFFSETS: [f32; 4] = [0.125, 0.375, 0.625, 0.875];
const SAMPLE_COUNT: f32 = (SAMPLE_OFFSETS.len() * SAMPLE_OFFSETS.len()) as f32;

/// Filter painting uses the same semantic rectangle as layout and border
/// geometry. Keep the local name only as a migration-friendly visibility
/// alias for the sibling surface modules.
pub(super) type SurfaceRect = Rect;

pub(super) struct RasterCanvas<'a> {
    pub(super) pixels: &'a mut image::RgbaImage,
    pub(super) pixels_per_point: f32,
}

impl RasterCanvas<'_> {
    /// Composite an isolated, same-size group back onto this canvas with one
    /// group opacity. Child pixels have already been composited internally;
    /// scaling only their resulting alpha preserves CSS group-opacity overlap.
    pub(super) fn composite_group(&mut self, source: &image::RgbaImage, opacity: f32) {
        let opacity = opacity.clamp(0.0, 1.0);
        for (x, y, pixel) in source.enumerate_pixels() {
            if pixel[3] == 0 {
                continue;
            }
            let mut source_pixel = *pixel;
            source_pixel[3] = (f32::from(source_pixel[3]) * opacity).round() as u8;
            let destination = *self.pixels.get_pixel(x, y);
            self.pixels
                .put_pixel(x, y, alpha_over(source_pixel, destination));
        }
    }

    pub(super) fn fill(&mut self, rect: SurfaceRect, color: Color) {
        self.fill_rgba(rect, color.to_f32_rgba());
    }

    /// Paint a multicolumn rule in the same top-down geometry used by the
    /// SourceGraphic surface.
    pub(super) fn paint_column_rule(
        &mut self,
        origin: Point,
        height: f32,
        paint: crate::layout::engine::LayoutBorderSide,
    ) -> Option<()> {
        if paint.width <= 0.0 || height <= 0.0 || !paint.style.paints() {
            return Some(());
        }
        let rect = SurfaceRect::new(origin, Size::new(paint.width, height));
        let rule = RasterColumnRule::new(rect, paint);
        self.paint_sampled(rect, |point| rule.sample(point));
        Some(())
    }

    pub(super) fn fill_rounded(&mut self, shape: CssRoundedRect, color: Color) {
        self.paint_rounded(shape, |_| Some(color));
    }

    fn fill_rgba(&mut self, rect: SurfaceRect, color: (f32, f32, f32, f32)) {
        if rect.size.width <= 0.0 || rect.size.height <= 0.0 || color.3 <= 0.0 {
            return;
        }
        let device_left = rect.origin.x * self.pixels_per_point;
        let device_top = rect.origin.y * self.pixels_per_point;
        let device_right = (rect.origin.x + rect.size.width) * self.pixels_per_point;
        let device_bottom = (rect.origin.y + rect.size.height) * self.pixels_per_point;
        let x0 = device_floor(rect.origin.x, self.pixels_per_point, self.pixels.width());
        let y0 = device_floor(rect.origin.y, self.pixels_per_point, self.pixels.height());
        let x1 = device_ceil(
            rect.origin.x + rect.size.width,
            self.pixels_per_point,
            self.pixels.width(),
        );
        let y1 = device_ceil(
            rect.origin.y + rect.size.height,
            self.pixels_per_point,
            self.pixels.height(),
        );
        for y in y0..y1 {
            let coverage_y = interval_coverage(y, device_top, device_bottom);
            for x in x0..x1 {
                let coverage = coverage_y * interval_coverage(x, device_left, device_right);
                if coverage <= 0.0 {
                    continue;
                }
                let source = rgba8((color.0, color.1, color.2, color.3 * coverage));
                let destination = *self.pixels.get_pixel(x, y);
                self.pixels.put_pixel(x, y, alpha_over(source, destination));
            }
        }
    }

    pub(super) fn paint_rounded(
        &mut self,
        shape: CssRoundedRect,
        mut sample: impl FnMut(Point) -> Option<Color>,
    ) {
        let rect = shape.rect;
        if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
            return;
        }
        let x0 = device_floor(rect.origin.x, self.pixels_per_point, self.pixels.width());
        let y0 = device_floor(rect.origin.y, self.pixels_per_point, self.pixels.height());
        let x1 = device_ceil(
            rect.origin.x + rect.size.width,
            self.pixels_per_point,
            self.pixels.width(),
        );
        let y1 = device_ceil(
            rect.origin.y + rect.size.height,
            self.pixels_per_point,
            self.pixels.height(),
        );
        for y in y0..y1 {
            for x in x0..x1 {
                let coverage =
                    sample_coverage(x, y, self.pixels_per_point, |point| shape.contains(point));
                if coverage <= 0.0 {
                    continue;
                }
                let point = Point::new(
                    (x as f32 + 0.5) / self.pixels_per_point,
                    (y as f32 + 0.5) / self.pixels_per_point,
                );
                let Some(color) = sample(point) else {
                    continue;
                };
                self.composite_color(x, y, color, coverage);
            }
        }
    }

    fn composite_color(&mut self, x: u32, y: u32, color: Color, coverage: f32) {
        let (red, green, blue, alpha) = color.to_f32_rgba();
        let source = rgba8((red, green, blue, alpha * coverage));
        let destination = *self.pixels.get_pixel(x, y);
        self.pixels.put_pixel(x, y, alpha_over(source, destination));
    }

    /// Paint `outer - inner` once, retaining fractional device-pixel coverage
    /// at both edges. This is the square-corner form of an inset shadow.
    fn fill_ring(&mut self, outer: SurfaceRect, inner: SurfaceRect, color: Color) {
        let Some(inner) = outer.intersection(inner) else {
            self.fill(outer, color);
            return;
        };
        let scale = self.pixels_per_point;
        let outer_device = DeviceRect::from_surface(outer, scale);
        let inner_device = DeviceRect::from_surface(inner, scale);
        let x0 = device_floor(outer.origin.x, scale, self.pixels.width());
        let y0 = device_floor(outer.origin.y, scale, self.pixels.height());
        let x1 = device_ceil(
            outer.origin.x + outer.size.width,
            scale,
            self.pixels.width(),
        );
        let y1 = device_ceil(
            outer.origin.y + outer.size.height,
            scale,
            self.pixels.height(),
        );
        let (red, green, blue, alpha) = color.to_f32_rgba();
        for y in y0..y1 {
            for x in x0..x1 {
                let coverage = (outer_device.pixel_coverage(x, y)
                    - inner_device.pixel_coverage(x, y))
                .clamp(0.0, 1.0);
                if coverage <= 0.0 {
                    continue;
                }
                let source = rgba8((red, green, blue, alpha * coverage));
                let destination = *self.pixels.get_pixel(x, y);
                self.pixels.put_pixel(x, y, alpha_over(source, destination));
            }
        }
    }

    fn paint_asset_at(&mut self, asset: &RasterImageAsset, origin: Point) -> Option<()> {
        let decoded = crate::layout::images::decode_asset_to_rgba(asset)?;
        let destination = DevicePoint::new(
            (origin.x * self.pixels_per_point).round() as i32,
            (origin.y * self.pixels_per_point).round() as i32,
        );
        self.composite_image(
            &decoded,
            destination,
            DeviceClip::full(self.pixels.dimensions()),
        );
        Some(())
    }

    pub(super) fn paint_outset_shadows(
        &mut self,
        rect: SurfaceRect,
        shadows: &[BoxShadow],
        filter_dpi: f32,
    ) -> Option<()> {
        for shadow in shadows.iter().rev().filter(|shadow| !shadow.inset) {
            let shadow_rect = outset_shadow_rect(rect, *shadow, 0.0)?;
            if shadow.blur <= 0.0 {
                self.fill(shadow_rect, shadow.color);
                continue;
            }
            let blurred = crate::render::blur::blur_shadow_rect(
                shadow_rect.size.width,
                shadow_rect.size.height,
                crate::types::CornerRadii::ZERO,
                shadow,
                filter_dpi,
            )?;
            self.paint_asset_at(
                &blurred.asset,
                Point::new(
                    shadow_rect.origin.x - blurred.overflow_pt,
                    shadow_rect.origin.y - blurred.overflow_pt,
                ),
            )?;
        }
        Some(())
    }

    pub(super) fn paint_inset_shadows(
        &mut self,
        rect: SurfaceRect,
        shadows: &[BoxShadow],
        filter_dpi: f32,
    ) -> Option<()> {
        for shadow in shadows.iter().rev().filter(|shadow| shadow.inset) {
            if shadow.blur <= 0.0 {
                let hole = SurfaceRect::new(
                    Point::new(
                        rect.origin.x + shadow.offset_x,
                        rect.origin.y + shadow.offset_y,
                    ),
                    rect.size,
                )
                .inset(EdgeSizes::uniform(shadow.spread));
                self.fill_ring(rect, hole, shadow.color);
                continue;
            }
            let blurred = crate::render::blur::blur_inset_shadow_rect(
                rect.size.width,
                rect.size.height,
                crate::types::CornerRadii::ZERO,
                shadow,
                filter_dpi,
            )?;
            self.paint_asset_at(
                &blurred.asset,
                Point::new(
                    rect.origin.x - blurred.overflow_pt,
                    rect.origin.y - blurred.overflow_pt,
                ),
            )?;
        }
        Some(())
    }

    pub(super) fn paint_border(
        &mut self,
        rect: SurfaceRect,
        border: &LayoutBorder,
        radii: CornerRadii,
    ) -> Option<()> {
        if !border.has_visible() {
            return Some(());
        }
        let paint = RasterBorder::new(rect, border, radii);
        self.paint_sampled(rect, |point| paint.sample(point));
        Some(())
    }

    /// Paint one analytically sampled shape without compositing neighboring
    /// subpixel regions over each other. Premultiplied averaging preserves
    /// translucent mixed-color border transitions.
    fn paint_sampled(&mut self, bounds: Rect, mut sample: impl FnMut(Point) -> Option<Color>) {
        let x0 = device_floor(bounds.origin.x, self.pixels_per_point, self.pixels.width());
        let y0 = device_floor(bounds.origin.y, self.pixels_per_point, self.pixels.height());
        let x1 = device_ceil(bounds.right(), self.pixels_per_point, self.pixels.width());
        let y1 = device_ceil(bounds.bottom(), self.pixels_per_point, self.pixels.height());
        for y in y0..y1 {
            for x in x0..x1 {
                let mut premultiplied = [0.0_f32; 4];
                for offset_y in SAMPLE_OFFSETS {
                    for offset_x in SAMPLE_OFFSETS {
                        let point = Point::new(
                            (x as f32 + offset_x) / self.pixels_per_point,
                            (y as f32 + offset_y) / self.pixels_per_point,
                        );
                        let Some(color) = sample(point) else {
                            continue;
                        };
                        let (red, green, blue, alpha) = color.to_f32_rgba();
                        premultiplied[0] += red * alpha;
                        premultiplied[1] += green * alpha;
                        premultiplied[2] += blue * alpha;
                        premultiplied[3] += alpha;
                    }
                }
                let alpha = premultiplied[3] / SAMPLE_COUNT;
                if alpha <= 0.0 {
                    continue;
                }
                let color = Color::from_srgb(
                    premultiplied[0] / premultiplied[3],
                    premultiplied[1] / premultiplied[3],
                    premultiplied[2] / premultiplied[3],
                    alpha,
                );
                self.composite_color(x, y, color, 1.0);
            }
        }
    }

    pub(super) fn composite_mask(
        &mut self,
        mask: &image::GrayImage,
        destination: DevicePoint,
        color: Color,
    ) {
        let [red, green, blue, color_alpha] = color.to_rgba8();
        for y in 0..mask.height() {
            for x in 0..mask.width() {
                let alpha = u16::from(mask.get_pixel(x, y)[0]) * u16::from(color_alpha) / 255;
                if alpha == 0 {
                    continue;
                }
                let target_x = destination.x + x as i32;
                let target_y = destination.y + y as i32;
                if target_x < 0
                    || target_y < 0
                    || target_x >= self.pixels.width() as i32
                    || target_y >= self.pixels.height() as i32
                {
                    continue;
                }
                let target_x = target_x as u32;
                let target_y = target_y as u32;
                let background = *self.pixels.get_pixel(target_x, target_y);
                self.pixels.put_pixel(
                    target_x,
                    target_y,
                    alpha_over(
                        image::Rgba([red, green, blue, alpha.min(255) as u8]),
                        background,
                    ),
                );
            }
        }
    }

    pub(super) fn paint_image(
        &mut self,
        asset: &RasterImageAsset,
        content_box: SurfaceRect,
        sampling: crate::layout::elements::ImageSampling,
    ) -> Option<()> {
        let decoded = crate::layout::images::decode_asset_to_rgba(asset)?;
        let placement = crate::layout::images::compute_image_placement(
            content_box.size.width,
            content_box.size.height,
            decoded.width(),
            decoded.height(),
            sampling.object_fit,
            sampling.object_position,
        );
        let target_width = positive_device_length(placement.width, self.pixels_per_point)?;
        let target_height = positive_device_length(placement.height, self.pixels_per_point)?;
        let resized = crate::render::blur::resize_image_for_display(
            &decoded,
            target_width,
            target_height,
            sampling.rendering,
        );
        let destination = DevicePoint::new(
            ((content_box.origin.x + placement.offset_x) * self.pixels_per_point).round() as i32,
            ((content_box.origin.y + placement.offset_y) * self.pixels_per_point).round() as i32,
        );
        let clip =
            DeviceClip::from_rect(content_box, self.pixels_per_point, self.pixels.dimensions());
        self.composite_image(&resized, destination, clip);
        Some(())
    }

    pub(super) fn paint_filter_output(
        &mut self,
        output: &FilterRasterOutput,
        source_box: SurfaceRect,
    ) -> Option<()> {
        self.paint_expanded_raster(&output.asset, source_box, output.raster_overflow)
    }

    pub(super) fn paint_expanded_raster(
        &mut self,
        asset: &RasterImageAsset,
        source_box: SurfaceRect,
        overflow: EdgeSizes,
    ) -> Option<()> {
        self.paint_image(
            asset,
            SurfaceRect::new(
                Point::new(
                    source_box.origin.x - overflow.left,
                    source_box.origin.y - overflow.top,
                ),
                Size::new(
                    source_box.size.width + overflow.horizontal(),
                    source_box.size.height + overflow.vertical(),
                ),
            ),
            crate::layout::elements::ImageSampling {
                object_fit: crate::style::computed::ObjectFit::Fill,
                ..Default::default()
            },
        )
    }

    fn composite_image(
        &mut self,
        source: &image::RgbaImage,
        destination: DevicePoint,
        clip: DeviceClip,
    ) {
        for y in 0..source.height() {
            for x in 0..source.width() {
                let target_x = destination.x + x as i32;
                let target_y = destination.y + y as i32;
                if !clip.contains(target_x, target_y) {
                    continue;
                }
                let source_pixel = *source.get_pixel(x, y);
                if source_pixel[3] == 0 {
                    continue;
                }
                let target_x = target_x as u32;
                let target_y = target_y as u32;
                let background = *self.pixels.get_pixel(target_x, target_y);
                self.pixels
                    .put_pixel(target_x, target_y, alpha_over(source_pixel, background));
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct DevicePoint {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy)]
struct DeviceRect {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl DeviceRect {
    fn from_surface(rect: SurfaceRect, scale: f32) -> Self {
        Self {
            left: rect.origin.x * scale,
            top: rect.origin.y * scale,
            right: (rect.origin.x + rect.size.width) * scale,
            bottom: (rect.origin.y + rect.size.height) * scale,
        }
    }

    fn pixel_coverage(self, x: u32, y: u32) -> f32 {
        interval_coverage(x, self.left, self.right) * interval_coverage(y, self.top, self.bottom)
    }
}

impl DevicePoint {
    pub(super) const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy)]
struct DeviceClip {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl DeviceClip {
    const fn full(canvas: (u32, u32)) -> Self {
        Self {
            left: 0,
            top: 0,
            right: canvas.0 as i32,
            bottom: canvas.1 as i32,
        }
    }

    fn from_rect(rect: SurfaceRect, scale: f32, canvas: (u32, u32)) -> Self {
        Self {
            left: (rect.origin.x * scale).floor().max(0.0) as i32,
            top: (rect.origin.y * scale).floor().max(0.0) as i32,
            right: ((rect.origin.x + rect.size.width) * scale)
                .ceil()
                .min(canvas.0 as f32) as i32,
            bottom: ((rect.origin.y + rect.size.height) * scale)
                .ceil()
                .min(canvas.1 as f32) as i32,
        }
    }

    fn contains(self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

fn interval_coverage(pixel: u32, start: f32, end: f32) -> f32 {
    ((pixel as f32 + 1.0).min(end) - (pixel as f32).max(start)).clamp(0.0, 1.0)
}

fn sample_coverage(
    x: u32,
    y: u32,
    pixels_per_point: f32,
    mut contains: impl FnMut(Point) -> bool,
) -> f32 {
    let mut inside = 0_u8;
    for offset_y in SAMPLE_OFFSETS {
        for offset_x in SAMPLE_OFFSETS {
            inside += u8::from(contains(Point::new(
                (x as f32 + offset_x) / pixels_per_point,
                (y as f32 + offset_y) / pixels_per_point,
            )));
        }
    }
    f32::from(inside) / SAMPLE_COUNT
}

fn outset_shadow_rect(
    border_box: SurfaceRect,
    shadow: BoxShadow,
    blur_overflow: f32,
) -> Option<SurfaceRect> {
    let outset = shadow.spread + blur_overflow;
    let size = Size::new(
        border_box.size.width + 2.0 * outset,
        border_box.size.height + 2.0 * outset,
    );
    if !size.width.is_finite()
        || !size.height.is_finite()
        || size.width <= 0.0
        || size.height <= 0.0
    {
        return None;
    }
    Some(SurfaceRect::new(
        Point::new(
            border_box.origin.x + shadow.offset_x - outset,
            border_box.origin.y + shadow.offset_y - outset,
        ),
        size,
    ))
}

pub(super) fn box_shadow_overflow(
    size: Size,
    shadows: &[BoxShadow],
    filter_dpi: f32,
) -> Option<EdgeSizes> {
    let border_box = SurfaceRect::new(Point::ORIGIN, size);
    let mut overflow = EdgeSizes::ZERO;
    for shadow in shadows.iter().filter(|shadow| !shadow.inset) {
        let blur = crate::render::blur::box_shadow_blur_overflow(shadow.blur, filter_dpi)?;
        let Some(rect) = outset_shadow_rect(border_box, *shadow, blur) else {
            continue;
        };
        overflow.top = overflow.top.max((-rect.origin.y).max(0.0));
        overflow.left = overflow.left.max((-rect.origin.x).max(0.0));
        overflow.right = overflow
            .right
            .max((rect.origin.x + rect.size.width - size.width).max(0.0));
        overflow.bottom = overflow
            .bottom
            .max((rect.origin.y + rect.size.height - size.height).max(0.0));
    }
    Some(overflow)
}

fn positive_device_length(points: f32, scale: f32) -> Option<u32> {
    (points.is_finite() && points > 0.0 && scale.is_finite() && scale > 0.0)
        .then(|| (points * scale).round().max(1.0) as u32)
}

fn device_floor(points: f32, scale: f32, maximum: u32) -> u32 {
    (points * scale).floor().clamp(0.0, maximum as f32) as u32
}

fn device_ceil(points: f32, scale: f32, maximum: u32) -> u32 {
    (points * scale).ceil().clamp(0.0, maximum as f32) as u32
}

fn rgba8(color: (f32, f32, f32, f32)) -> image::Rgba<u8> {
    image::Rgba([
        (color.0 * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.1 * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.2 * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.3 * 255.0).round().clamp(0.0, 255.0) as u8,
    ])
}

fn alpha_over(source: image::Rgba<u8>, destination: image::Rgba<u8>) -> image::Rgba<u8> {
    let source_alpha = f32::from(source[3]) / 255.0;
    let destination_alpha = f32::from(destination[3]) / 255.0;
    let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
    if output_alpha <= 0.0 {
        return image::Rgba([0, 0, 0, 0]);
    }
    let blend = |source: u8, destination: u8| {
        ((f32::from(source) * source_alpha
            + f32::from(destination) * destination_alpha * (1.0 - source_alpha))
            / output_alpha)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    image::Rgba([
        blend(source[0], destination[0]),
        blend(source[1], destination[1]),
        blend(source[2], destination[2]),
        (output_alpha * 255.0).round().clamp(0.0, 255.0) as u8,
    ])
}
