use crate::style::computed::{FilterOperation, ImageRendering};
use crate::types::{EdgeSizes, Size};

use super::color_space::RasterFilterColorSpace;

/// Filtered pixels and their directional paint overflow around the source box.
pub(crate) struct FilteredSurface {
    pub(crate) pixels: image::RgbaImage,
    pub(crate) overflow: EdgeSizes,
}

struct PremultipliedFilteredSurface {
    pixels: crate::render::raster_pixels::PremultipliedRgba8,
    overflow: EdgeSizes,
}

/// Evaluate one ordered filter list over an already-composited source graphic.
///
/// Returning `None` for an unsupported SVG graph operation is deliberate: the
/// caller can retain the vector source and use its explicit fallback, while a
/// silent no-op would publish an incorrect filtered surface.
pub(crate) fn apply_operations_to_surface(
    source: &crate::render::raster_pixels::PremultipliedRgba8,
    source_size: Size,
    operations: &[FilterOperation],
    linear_rgb: bool,
    filter_dpi: f32,
) -> Option<FilteredSurface> {
    let mut pixels = source.clone();
    let working_space = RasterFilterColorSpace::resolve(linear_rgb);
    working_space.enter_surface(&mut pixels);
    let mut overflow = EdgeSizes::ZERO;
    let mut color_run_start = None;
    for (operation_index, operation) in operations.iter().enumerate() {
        if is_color_operation(operation) {
            color_run_start.get_or_insert(operation_index);
            continue;
        }
        if let Some(start) = color_run_start.take() {
            apply_color_operations(&mut pixels, &operations[start..operation_index]);
        }
        match *operation {
            FilterOperation::BlendWithFlood {
                color,
                mode,
                region,
            } => {
                let filtered = blend_with_flood(
                    &pixels,
                    source_size,
                    overflow,
                    working_space.enter_color(color),
                    mode,
                    region,
                    filter_dpi,
                )?;
                pixels = filtered.pixels;
                overflow = filtered.overflow;
            }
            FilterOperation::Blur(radius) if radius > 0.0 => {
                let (filtered, amount) =
                    crate::render::blur::blur_premultiplied_buffer(&pixels, radius, filter_dpi)?;
                pixels = filtered;
                overflow += EdgeSizes::uniform(amount);
            }
            FilterOperation::DropShadow(shadow) => {
                let shadow = crate::style::computed::DropShadow {
                    color: working_space.enter_color(shadow.color),
                    ..shadow
                };
                let painted_size = Size::new(
                    source_size.width + overflow.horizontal(),
                    source_size.height + overflow.vertical(),
                );
                let filtered = crate::render::blur::drop_shadow_image(
                    &pixels.clone().into_straight(),
                    painted_size.width,
                    painted_size.height,
                    shadow,
                    ImageRendering::Auto,
                    filter_dpi,
                )?;
                pixels = crate::render::raster_pixels::PremultipliedRgba8::from_straight(
                    &image::load_from_memory(&filtered.asset.data)
                        .ok()?
                        .to_rgba8(),
                );
                overflow += EdgeSizes::uniform(filtered.overflow_pt);
            }
            FilterOperation::Blur(_) => {}
            FilterOperation::Offset { .. } | FilterOperation::MorphologyDilate(_) => return None,
            _ => {}
        }
    }
    if let Some(start) = color_run_start {
        apply_color_operations(&mut pixels, &operations[start..]);
    }
    working_space.leave_surface(&mut pixels);
    Some(FilteredSurface {
        pixels: pixels.into_straight(),
        overflow,
    })
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

#[derive(Clone, Copy)]
struct RasterScale {
    horizontal: f32,
    vertical: f32,
}

impl RasterScale {
    fn at_dpi(dpi: f32) -> Option<Self> {
        let pixels_per_point = crate::render::blur::px_per_pt_at_dpi(dpi);
        (pixels_per_point.is_finite() && pixels_per_point > 0.0).then_some(Self {
            horizontal: pixels_per_point,
            vertical: pixels_per_point,
        })
    }
}

#[derive(Clone, Copy)]
struct RasterRegion {
    left: i64,
    top: i64,
    right: i64,
    bottom: i64,
}

impl RasterRegion {
    fn resolve(region: crate::types::Rect, source_size: Size, scale: RasterScale) -> Option<Self> {
        let width = f64::from(source_size.width * scale.horizontal);
        let height = f64::from(source_size.height * scale.vertical);
        let resolved = Self {
            left: floored_coordinate(f64::from(region.origin.x) * width)?,
            top: floored_coordinate(f64::from(region.origin.y) * height)?,
            right: ceiled_coordinate(f64::from(region.right()) * width)?,
            bottom: ceiled_coordinate(f64::from(region.bottom()) * height)?,
        };
        (resolved.right > resolved.left && resolved.bottom > resolved.top).then_some(resolved)
    }

    fn dimensions(self) -> Option<(u32, u32)> {
        Some((
            u32::try_from(self.right.checked_sub(self.left)?).ok()?,
            u32::try_from(self.bottom.checked_sub(self.top)?).ok()?,
        ))
    }

    fn paint_overflow(self, source_size: Size, scale: RasterScale) -> EdgeSizes {
        EdgeSizes::new(
            -(self.top as f32) / scale.vertical,
            self.right as f32 / scale.horizontal - source_size.width,
            self.bottom as f32 / scale.vertical - source_size.height,
            -(self.left as f32) / scale.horizontal,
        )
    }
}

fn rounded_coordinate(value: f64) -> Option<i64> {
    Some(stabilized_coordinate(value)?.round() as i64)
}

fn floored_coordinate(value: f64) -> Option<i64> {
    Some(stabilized_coordinate(value)?.floor() as i64)
}

fn ceiled_coordinate(value: f64) -> Option<i64> {
    Some(stabilized_coordinate(value)?.ceil() as i64)
}

fn stabilized_coordinate(value: f64) -> Option<f64> {
    const DEVICE_EDGE_EPSILON: f64 = 0.001;

    if !value.is_finite() || value < i64::MIN as f64 || value > i64::MAX as f64 {
        return None;
    }
    let integer = value.round();
    Some(if (value - integer).abs() <= DEVICE_EDGE_EPSILON {
        integer
    } else {
        value
    })
}

fn blend_with_flood(
    source: &crate::render::raster_pixels::PremultipliedRgba8,
    source_size: Size,
    source_overflow: EdgeSizes,
    color: crate::types::Color,
    mode: crate::style::computed::BlendMode,
    region: crate::types::Rect,
    filter_dpi: f32,
) -> Option<PremultipliedFilteredSurface> {
    let scale = RasterScale::at_dpi(filter_dpi)?;
    let raster_region = RasterRegion::resolve(region, source_size, scale)?;
    let (width, height) = raster_region.dimensions()?;
    let source_left = rounded_coordinate(-f64::from(source_overflow.left * scale.horizontal))?;
    let source_top = rounded_coordinate(-f64::from(source_overflow.top * scale.vertical))?;
    let flood = image::Rgba(color.to_rgba8());
    let transparent = image::Rgba([0, 0, 0, 0]);
    let source = source.clone().into_straight();
    let mut output = image::RgbaImage::new(width, height);

    for (x, y, output_pixel) in output.enumerate_pixels_mut() {
        let global_x = raster_region.left.checked_add(i64::from(x))?;
        let global_y = raster_region.top.checked_add(i64::from(y))?;
        let local_x = global_x.checked_sub(source_left)?;
        let local_y = global_y.checked_sub(source_top)?;
        let source_pixel = u32::try_from(local_x)
            .ok()
            .zip(u32::try_from(local_y).ok())
            .filter(|(x, y)| *x < source.width() && *y < source.height())
            .map_or(transparent, |(x, y)| *source.get_pixel(x, y));
        *output_pixel = crate::render::blend::composite_pixel(source_pixel, flood, mode, false)?;
    }

    Some(PremultipliedFilteredSurface {
        pixels: crate::render::raster_pixels::PremultipliedRgba8::from_straight(&output),
        overflow: raster_region.paint_overflow(source_size, scale),
    })
}

/// Evaluate one uninterrupted colour-function run in floating point and
/// quantize only when the run returns to the raster surface. Quantizing between
/// functions compounds channel error and does not represent the conceptual
/// image pipeline defined by Filter Effects.
fn apply_color_operations(
    pixels: &mut crate::render::raster_pixels::PremultipliedRgba8,
    operations: &[FilterOperation],
) {
    pixels.map_straight(|color| super::apply_operations_to_color(color, operations, false));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::computed::{BlendMode, DropShadow};
    use crate::types::{Color, Rect};

    #[test]
    fn flood_blend_preserves_the_flood_only_region() {
        let source = crate::render::raster_pixels::PremultipliedRgba8::from_straight(
            &image::RgbaImage::from_pixel(10, 6, image::Rgba([213, 0, 0, 255])),
        );
        let flood = Color::rgb(21, 101, 192);
        let filtered = apply_operations_to_surface(
            &source,
            Size::new(10.0, 6.0),
            &[FilterOperation::BlendWithFlood {
                color: flood,
                mode: BlendMode::Multiply,
                region: Rect::from_xywh(-0.2, -0.5, 1.4, 2.0),
            }],
            true,
            72.0,
        )
        .expect("a finite flood and source produce one blended surface");

        assert_eq!(filtered.pixels.dimensions(), (14, 12));
        for (actual, expected) in [
            (filtered.overflow.top, 3.0),
            (filtered.overflow.right, 2.0),
            (filtered.overflow.bottom, 3.0),
            (filtered.overflow.left, 2.0),
        ] {
            assert!((actual - expected).abs() < 0.000_01);
        }
        assert_eq!(filtered.pixels.get_pixel(0, 0).0, [22, 101, 192, 255]);
        assert_eq!(filtered.pixels.get_pixel(2, 3).0, [13, 0, 0, 255]);
    }

    #[test]
    fn fractional_filter_region_uses_outward_device_bounds() {
        let source_size = Size::new(10.0, 6.0);
        let scale = RasterScale {
            horizontal: 3.125,
            vertical: 3.125,
        };
        let region =
            RasterRegion::resolve(Rect::from_xywh(-0.2, -0.5, 1.4, 2.0), source_size, scale)
                .expect("a finite positive filter region has device bounds");

        assert_eq!(region.dimensions(), Some((45, 39)));
        let overflow = region.paint_overflow(source_size, scale);
        for (actual, expected) in [
            (overflow.top, 3.2),
            (overflow.right, 2.16),
            (overflow.bottom, 3.28),
            (overflow.left, 2.24),
        ] {
            assert!((actual - expected).abs() < 0.000_01);
        }
    }

    #[test]
    fn device_edge_stabilization_does_not_grow_integral_bounds() {
        assert_eq!(floored_coordinate(11.999_999), Some(12));
        assert_eq!(ceiled_coordinate(12.000_001), Some(12));
        assert_eq!(floored_coordinate(11.99), Some(11));
        assert_eq!(ceiled_coordinate(12.01), Some(13));
    }

    #[test]
    fn ordered_drop_shadow_consumes_the_composited_source() {
        let source = crate::render::raster_pixels::PremultipliedRgba8::from_straight(
            &image::RgbaImage::from_pixel(450, 269, image::Rgba([20, 80, 160, 255])),
        );
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
        let source = crate::render::raster_pixels::PremultipliedRgba8::from_straight(
            &image::RgbaImage::from_pixel(1, 1, image::Rgba([20, 80, 160, 255])),
        );
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
        let source = crate::render::raster_pixels::PremultipliedRgba8::from_straight(
            &image::RgbaImage::from_pixel(1, 1, image::Rgba([231, 245, 255, 255])),
        );
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
}
