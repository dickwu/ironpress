//! Device geometry for an SVG filter surface and its object bounding box.

use crate::render::raster_pixels::PremultipliedRgba8;
use crate::style::computed::NormalizedFilterRegion;
use crate::types::{EdgeSizes, Rect, Size};

/// Geometry needed to resolve normalized SVG filter coordinates.
///
/// The source surface may include authored paint overflow. `object_bounds`
/// therefore retains the border box independently instead of treating the
/// allocation itself as the object bounding box.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FilterSourceGeometry {
    surface_size: Size,
    object_bounds: Rect,
}

impl FilterSourceGeometry {
    pub(crate) fn new(surface_size: Size, object_bounds: Rect) -> Option<Self> {
        let values = [
            surface_size.width,
            surface_size.height,
            object_bounds.origin.x,
            object_bounds.origin.y,
            object_bounds.size.width,
            object_bounds.size.height,
        ];
        (values.into_iter().all(f32::is_finite)
            && surface_size.width > 0.0
            && surface_size.height > 0.0
            && object_bounds.size.width > 0.0
            && object_bounds.size.height > 0.0)
            .then_some(Self {
                surface_size,
                object_bounds,
            })
    }

    pub(super) const fn surface_size(self) -> Size {
        self.surface_size
    }

    fn resolve(self, region: NormalizedFilterRegion) -> Rect {
        let normalized = region.as_rect();
        Rect::from_xywh(
            self.object_bounds.origin.x + normalized.origin.x * self.object_bounds.size.width,
            self.object_bounds.origin.y + normalized.origin.y * self.object_bounds.size.height,
            normalized.size.width * self.object_bounds.size.width,
            normalized.size.height * self.object_bounds.size.height,
        )
    }
}

#[derive(Clone, Copy)]
pub(super) struct RasterScale {
    horizontal: f32,
    vertical: f32,
}

impl RasterScale {
    pub(super) fn at_dpi(dpi: f32) -> Option<Self> {
        let pixels_per_point = crate::render::blur::px_per_pt_at_dpi(dpi);
        (pixels_per_point.is_finite() && pixels_per_point > 0.0).then_some(Self {
            horizontal: pixels_per_point,
            vertical: pixels_per_point,
        })
    }

    #[cfg(test)]
    pub(super) const fn uniform(pixels_per_point: f32) -> Self {
        Self {
            horizontal: pixels_per_point,
            vertical: pixels_per_point,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RasterRegion {
    pub(super) left: i64,
    pub(super) top: i64,
    right: i64,
    bottom: i64,
}

impl RasterRegion {
    pub(super) fn resolve(
        region: NormalizedFilterRegion,
        geometry: FilterSourceGeometry,
        scale: RasterScale,
    ) -> Option<Self> {
        let resolved = geometry.resolve(region);
        let bounds = Self {
            left: floored_coordinate(f64::from(resolved.origin.x) * f64::from(scale.horizontal))?,
            top: floored_coordinate(f64::from(resolved.origin.y) * f64::from(scale.vertical))?,
            right: ceiled_coordinate(f64::from(resolved.right()) * f64::from(scale.horizontal))?,
            bottom: ceiled_coordinate(f64::from(resolved.bottom()) * f64::from(scale.vertical))?,
        };
        (bounds.right > bounds.left && bounds.bottom > bounds.top).then_some(bounds)
    }

    pub(super) fn dimensions(self) -> Option<(u32, u32)> {
        Some((
            u32::try_from(self.right.checked_sub(self.left)?).ok()?,
            u32::try_from(self.bottom.checked_sub(self.top)?).ok()?,
        ))
    }

    pub(super) fn paint_overflow(
        self,
        geometry: FilterSourceGeometry,
        scale: RasterScale,
    ) -> EdgeSizes {
        let surface_size = geometry.surface_size();
        EdgeSizes::new(
            -(self.top as f32) / scale.vertical,
            self.right as f32 / scale.horizontal - surface_size.width,
            self.bottom as f32 / scale.vertical - surface_size.height,
            -(self.left as f32) / scale.horizontal,
        )
    }

    pub(super) fn source_frame(
        pixels: &PremultipliedRgba8,
        overflow: EdgeSizes,
        scale: RasterScale,
    ) -> Option<Self> {
        let left = rounded_coordinate(-f64::from(overflow.left * scale.horizontal))?;
        let top = rounded_coordinate(-f64::from(overflow.top * scale.vertical))?;
        Some(Self {
            left,
            top,
            right: left.checked_add(i64::from(pixels.width()))?,
            bottom: top.checked_add(i64::from(pixels.height()))?,
        })
    }

    /// Copy the intersection of `source_region` into this region, leaving
    /// authored transparent space intact and discarding pixels outside the
    /// hard SVG filter clip.
    pub(super) fn extract(
        self,
        source: &PremultipliedRgba8,
        source_region: Self,
    ) -> Option<PremultipliedRgba8> {
        let (width, height) = self.dimensions()?;
        let mut output = PremultipliedRgba8::transparent(width, height);
        let Some(intersection) = self.intersection(source_region) else {
            return Some(output);
        };
        let (copy_width, copy_height) = intersection.dimensions()?;
        let source_x = u32::try_from(intersection.left.checked_sub(source_region.left)?).ok()?;
        let source_y = u32::try_from(intersection.top.checked_sub(source_region.top)?).ok()?;
        let target_x = u32::try_from(intersection.left.checked_sub(self.left)?).ok()?;
        let target_y = u32::try_from(intersection.top.checked_sub(self.top)?).ok()?;
        let source_right = source_x.checked_add(copy_width)?;
        let source_bottom = source_y.checked_add(copy_height)?;
        let target_right = target_x.checked_add(copy_width)?;
        let target_bottom = target_y.checked_add(copy_height)?;
        if source_right > source.width()
            || source_bottom > source.height()
            || target_right > output.width()
            || target_bottom > output.height()
        {
            return None;
        }
        for y in 0..copy_height {
            for x in 0..copy_width {
                let pixel = *source.get_pixel(source_x + x, source_y + y);
                output
                    .as_image_mut()
                    .put_pixel(target_x + x, target_y + y, pixel);
            }
        }
        Some(output)
    }

    fn intersection(self, other: Self) -> Option<Self> {
        let intersection = Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        };
        (intersection.right > intersection.left && intersection.bottom > intersection.top)
            .then_some(intersection)
    }
}

pub(super) fn rounded_coordinate(value: f64) -> Option<i64> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Point;

    #[test]
    fn normalized_region_resolves_against_the_object_not_its_surface() {
        let geometry = FilterSourceGeometry::new(
            Size::new(14.0, 10.0),
            Rect::new(Point::new(2.0, 2.0), Size::new(10.0, 6.0)),
        )
        .expect("the test geometry is finite and positive");
        let scale = RasterScale::uniform(3.125);
        let normalized = NormalizedFilterRegion::new(-0.2, -0.5, 1.4, 2.0)
            .expect("the normalized test region is valid");
        let region = RasterRegion::resolve(normalized, geometry, scale)
            .expect("the filter region has device bounds");

        assert_eq!(region.dimensions(), Some((44, 39)));
        let overflow = region.paint_overflow(geometry, scale);
        for (actual, expected) in [
            (overflow.top, 1.28),
            (overflow.right, 0.08),
            (overflow.bottom, 1.2),
            (overflow.left, 0.0),
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
}
