//! Quantized working colour spaces for raster filter primitives.
//!
//! SVG filter primitives default to `linearRGB`, while CSS filter functions
//! use sRGB. Keeping the conversion at the complete surface boundary prevents
//! geometry primitives such as blur and drop-shadow from accidentally
//! operating on sRGB bytes just because they do not have colour parameters.

use crate::types::Color;

#[derive(Clone, Copy)]
pub(super) enum RasterFilterColorSpace {
    Srgb,
    LinearRgb,
}

impl RasterFilterColorSpace {
    pub(super) const fn resolve(linear_rgb: bool) -> Self {
        if linear_rgb {
            Self::LinearRgb
        } else {
            Self::Srgb
        }
    }

    pub(super) fn enter_surface(self, pixels: &mut image::RgbaImage) {
        if let Self::LinearRgb = self {
            for pixel in pixels.pixels_mut() {
                for channel in &mut pixel.0[..3] {
                    *channel = srgb_to_linear_byte(*channel);
                }
            }
        }
    }

    pub(super) fn leave_surface(self, pixels: &mut image::RgbaImage) {
        if let Self::LinearRgb = self {
            for pixel in pixels.pixels_mut() {
                for channel in &mut pixel.0[..3] {
                    *channel = linear_to_srgb_byte(*channel);
                }
            }
        }
    }

    pub(super) fn enter_color(self, color: Color) -> Color {
        let [red, green, blue, alpha] = color.to_rgba8();
        match self {
            Self::Srgb => Color::rgba8(red, green, blue, alpha),
            Self::LinearRgb => Color::rgba8(
                srgb_to_linear_byte(red),
                srgb_to_linear_byte(green),
                srgb_to_linear_byte(blue),
                alpha,
            ),
        }
    }
}

fn srgb_to_linear_byte(component: u8) -> u8 {
    let component = f32::from(component) / 255.0;
    let linear = if component <= 0.04045 {
        component / 12.92
    } else {
        ((component + 0.055) / 1.055).powf(2.4)
    };
    quantize(linear)
}

fn linear_to_srgb_byte(component: u8) -> u8 {
    let component = f32::from(component) / 255.0;
    let srgb = if component <= 0.0031308 {
        12.92 * component
    } else {
        1.055 * component.powf(1.0 / 2.4) - 0.055
    };
    quantize(srgb)
}

fn quantize(component: f32) -> u8 {
    (component * 255.0).round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_rgb_surface_boundary_matches_rgba8_quantization() {
        assert_eq!(srgb_to_linear_byte(17), 1);
        assert_eq!(linear_to_srgb_byte(1), 13);
        assert_eq!(linear_to_srgb_byte(srgb_to_linear_byte(213)), 213);
    }

    #[test]
    fn alpha_is_not_a_color_space_component() {
        let converted =
            RasterFilterColorSpace::LinearRgb.enter_color(Color::rgba8(17, 17, 17, 127));
        assert_eq!(converted.to_rgba8(), [1, 1, 1, 127]);
    }
}
