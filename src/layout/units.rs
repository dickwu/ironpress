//! Discrete precision used by browser inline layout.
//!
//! Layout remains point-based, but shaped glyph positions are resolved to CSS
//! app units before they influence intrinsic sizes, wrapping, or painting.

use crate::fonts::PT_PER_CSS_PX;

/// Chromium-compatible fixed-point precision in the point layout coordinate
/// system: one CSS reference pixel contains 64 app units.
pub(crate) struct CssAppUnit;

impl CssAppUnit {
    const PER_CSS_PIXEL: f32 = 64.0;
    pub(crate) const POINTS: f32 = PT_PER_CSS_PX / Self::PER_CSS_PIXEL;

    pub(crate) fn round_points(points: f32) -> f32 {
        (points / Self::POINTS).round() * Self::POINTS
    }

    pub(crate) fn floor_points(points: f32) -> f32 {
        (points / Self::POINTS).floor() * Self::POINTS
    }

    pub(crate) fn ceil_points(points: f32) -> f32 {
        (points / Self::POINTS).ceil() * Self::POINTS
    }
}

#[cfg(test)]
mod tests {
    use super::CssAppUnit;

    #[test]
    fn point_values_resolve_to_css_app_units_in_each_direction() {
        assert_eq!(CssAppUnit::round_points(8.208_984), 8.214_844);
        assert_eq!(CssAppUnit::floor_points(52.564_453), 52.558_594);
        assert_eq!(CssAppUnit::ceil_points(59.800_78), 59.800_78);
    }
}
