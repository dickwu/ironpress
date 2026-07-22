use crate::style::computed::{FilterOperation, ImageRendering};
use crate::types::{EdgeSizes, Size};

/// Filtered pixels and their directional paint overflow around the source box.
pub(crate) struct FilteredSurface {
    pub(crate) pixels: image::RgbaImage,
    pub(crate) overflow: EdgeSizes,
}

impl FilteredSurface {
    /// Remove fully transparent device-pixel borders without changing the
    /// pixel-to-page transform represented by `overflow`.
    fn compact(mut self, source_size: Size, filter_dpi: f32) -> Self {
        let (width, height) = self.pixels.dimensions();
        let mut bounds = AlphaBounds::empty(width, height);
        for (x, y, pixel) in self.pixels.enumerate_pixels() {
            if pixel[3] != 0 {
                bounds.include(x, y);
            }
        }
        if bounds.is_empty() || bounds.covers(width, height) {
            return self;
        }

        let point_per_pixel = 1.0 / crate::render::blur::px_per_pt_at_dpi(filter_dpi);
        let left = self.overflow.left - bounds.left as f32 * point_per_pixel;
        let top = self.overflow.top - bounds.top as f32 * point_per_pixel;
        let compact_width = bounds.width() as f32 * point_per_pixel;
        let compact_height = bounds.height() as f32 * point_per_pixel;
        self.overflow = EdgeSizes::new(
            top,
            compact_width - source_size.width - left,
            compact_height - source_size.height - top,
            left,
        );
        self.pixels = image::imageops::crop_imm(
            &self.pixels,
            bounds.left,
            bounds.top,
            bounds.width(),
            bounds.height(),
        )
        .to_image();
        self
    }
}

#[derive(Clone, Copy)]
struct AlphaBounds {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

impl AlphaBounds {
    const fn empty(width: u32, height: u32) -> Self {
        Self {
            left: width,
            top: height,
            right: 0,
            bottom: 0,
        }
    }

    fn include(&mut self, x: u32, y: u32) {
        self.left = self.left.min(x);
        self.top = self.top.min(y);
        self.right = self.right.max(x + 1);
        self.bottom = self.bottom.max(y + 1);
    }

    const fn is_empty(self) -> bool {
        self.left >= self.right || self.top >= self.bottom
    }

    const fn covers(self, width: u32, height: u32) -> bool {
        self.left == 0 && self.top == 0 && self.right == width && self.bottom == height
    }

    const fn width(self) -> u32 {
        self.right - self.left
    }

    const fn height(self) -> u32 {
        self.bottom - self.top
    }
}

/// Evaluate one ordered filter list over an already-composited source graphic.
///
/// Returning `None` for an unsupported SVG graph operation is deliberate: the
/// caller can retain the vector source and use its explicit fallback, while a
/// silent no-op would publish an incorrect filtered surface.
pub(crate) fn apply_operations_to_surface(
    source: &image::RgbaImage,
    source_size: Size,
    operations: &[FilterOperation],
    linear_rgb: bool,
    filter_dpi: f32,
) -> Option<FilteredSurface> {
    let mut pixels = source.clone();
    let mut overflow = EdgeSizes::ZERO;
    let mut color_run_start = None;
    for (operation_index, operation) in operations.iter().enumerate() {
        if is_color_operation(operation) {
            color_run_start.get_or_insert(operation_index);
            continue;
        }
        if let Some(start) = color_run_start.take() {
            apply_color_operations(&mut pixels, &operations[start..operation_index], linear_rgb);
        }
        match *operation {
            FilterOperation::Blur(radius) if radius > 0.0 => {
                let (filtered, amount) =
                    crate::render::blur::blur_painted_buffer_to_rgba(&pixels, radius, filter_dpi)?;
                pixels = filtered;
                overflow += EdgeSizes::uniform(amount);
            }
            FilterOperation::DropShadow(shadow) => {
                let painted_size = Size::new(
                    source_size.width + overflow.horizontal(),
                    source_size.height + overflow.vertical(),
                );
                let filtered = crate::render::blur::drop_shadow_image(
                    &pixels,
                    painted_size.width,
                    painted_size.height,
                    shadow,
                    ImageRendering::Auto,
                    filter_dpi,
                )?;
                pixels = image::load_from_memory(&filtered.asset.data)
                    .ok()?
                    .to_rgba8();
                overflow += EdgeSizes::uniform(filtered.overflow_pt);
            }
            FilterOperation::Blur(_) => {}
            FilterOperation::Flood { .. }
            | FilterOperation::Offset { .. }
            | FilterOperation::MorphologyDilate(_) => return None,
            _ => {}
        }
    }
    if let Some(start) = color_run_start {
        apply_color_operations(&mut pixels, &operations[start..], linear_rgb);
    }
    Some(FilteredSurface { pixels, overflow }.compact(source_size, filter_dpi))
}

fn is_color_operation(operation: &FilterOperation) -> bool {
    matches!(
        operation,
        FilterOperation::Grayscale(_)
            | FilterOperation::Sepia(_)
            | FilterOperation::Invert(_)
            | FilterOperation::Brightness(_)
            | FilterOperation::Contrast(_)
            | FilterOperation::Saturate(_)
            | FilterOperation::HueRotate(_)
            | FilterOperation::Opacity(_)
            | FilterOperation::Matrix(_)
    )
}

/// Evaluate one uninterrupted colour-function run in floating point and
/// quantize only when the run returns to the raster surface. Quantizing between
/// functions compounds channel error and does not represent the conceptual
/// image pipeline defined by Filter Effects.
fn apply_color_operations(
    pixels: &mut image::RgbaImage,
    operations: &[FilterOperation],
    linear_rgb: bool,
) {
    for pixel in pixels.pixels_mut() {
        let color = (
            f32::from(pixel[0]) / 255.0,
            f32::from(pixel[1]) / 255.0,
            f32::from(pixel[2]) / 255.0,
            f32::from(pixel[3]) / 255.0,
        );
        let (red, green, blue, alpha) =
            super::apply_operations_to_color(color, operations, linear_rgb);
        *pixel = image::Rgba([channel(red), channel(green), channel(blue), channel(alpha)]);
    }
}

fn channel(value: f32) -> u8 {
    (value * 255.0).round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::computed::DropShadow;
    use crate::types::Color;

    #[test]
    fn ordered_drop_shadow_consumes_the_composited_source() {
        let source = image::RgbaImage::from_pixel(450, 269, image::Rgba([20, 80, 160, 255]));
        let filtered = apply_operations_to_surface(
            &source,
            Size::new(108.0, 64.5),
            &[FilterOperation::DropShadow(DropShadow {
                dx: 1.5,
                dy: 0.75,
                blur: 0.0,
                color: Color::from_srgb(0.56, 0.64, 0.68, 1.0),
            })],
            false,
            300.0,
        )
        .expect("a finite painted source and shadow produce one surface");

        assert!(filtered.pixels.width() > source.width());
        assert!(filtered.pixels.height() > source.height());
        assert!(!filtered.overflow.is_zero());
    }

    #[test]
    fn opacity_remains_in_filter_list_order() {
        let source = image::RgbaImage::from_pixel(1, 1, image::Rgba([20, 80, 160, 255]));
        let filtered = apply_operations_to_surface(
            &source,
            Size::new(0.75, 0.75),
            &[FilterOperation::Opacity(0.25)],
            false,
            96.0,
        )
        .expect("opacity is a surface color operation");

        assert_eq!(filtered.pixels.get_pixel(0, 0)[3], 64);
    }

    #[test]
    fn consecutive_color_functions_quantize_only_at_the_surface_boundary() {
        let source = image::RgbaImage::from_pixel(1, 1, image::Rgba([231, 245, 255, 255]));
        let filtered = apply_operations_to_surface(
            &source,
            Size::new(0.75, 0.75),
            &[
                FilterOperation::Grayscale(0.18),
                FilterOperation::Contrast(1.08),
            ],
            false,
            96.0,
        )
        .expect("finite colour functions produce a surface");

        assert_eq!(filtered.pixels.get_pixel(0, 0).0, [242, 254, 255, 255]);
    }

    #[test]
    fn compact_surface_retains_the_configured_device_pixel_scale() {
        let mut pixels = image::RgbaImage::new(11, 7);
        for y in 1..6 {
            for x in 1..9 {
                pixels.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
            }
        }
        let compact = FilteredSurface {
            pixels,
            overflow: EdgeSizes::new(0.24, 0.96, 0.24, 0.48),
        }
        .compact(Size::new(1.2, 1.2), 300.0);

        assert_eq!(compact.pixels.dimensions(), (8, 5));
        assert!((compact.overflow.left - 0.24).abs() < f32::EPSILON);
        assert!((compact.overflow.right - 0.48).abs() < f32::EPSILON);
        assert!(compact.overflow.top.abs() < f32::EPSILON);
        assert!(compact.overflow.bottom.abs() < f32::EPSILON);
        assert!(
            ((1.2 + compact.overflow.horizontal()) * crate::render::blur::px_per_pt_at_dpi(300.0)
                - compact.pixels.width() as f32)
                .abs()
                < f32::EPSILON
        );
    }
}
