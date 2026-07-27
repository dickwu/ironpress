//! Geometry operations on resolved CSS affine transforms.

use crate::types::{Point, Rect};

use super::CssAffineMatrix;

impl CssAffineMatrix {
    /// Conjugate this affine transform around one point in the CSS top-down
    /// coordinate system.
    pub fn around(self, pivot: Point) -> Self {
        let [a, b, c, d, e, f] = self.components();
        let px = f64::from(pivot.x);
        let py = f64::from(pivot.y);
        Self::from_components(
            a,
            b,
            c,
            d,
            px + e - a * px - c * py,
            py + f - b * px - d * py,
        )
    }

    /// Transform one point in the CSS top-down coordinate system.
    pub fn transform_point(self, point: Point) -> Point {
        let [a, b, c, d, e, f] = self.components();
        let x = f64::from(point.x);
        let y = f64::from(point.y);
        Point::new((a * x + c * y + e) as f32, (b * x + d * y + f) as f32)
    }

    /// Smallest axis-aligned rectangle enclosing a transformed rectangle.
    pub fn enclosing_rect(self, rect: Rect) -> Rect {
        let corners = [
            rect.origin,
            Point::new(rect.right(), rect.origin.y),
            Point::new(rect.origin.x, rect.bottom()),
            Point::new(rect.right(), rect.bottom()),
        ]
        .map(|point| self.transform_point(point));
        let left = corners
            .iter()
            .map(|point| point.x)
            .fold(f32::INFINITY, f32::min);
        let top = corners
            .iter()
            .map(|point| point.y)
            .fold(f32::INFINITY, f32::min);
        let right = corners
            .iter()
            .map(|point| point.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let bottom = corners
            .iter()
            .map(|point| point.y)
            .fold(f32::NEG_INFINITY, f32::max);
        Rect::from_xywh(left, top, right - left, bottom - top)
    }
}
