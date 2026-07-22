//! Font metrics available while resolving font-relative CSS lengths.
//!
//! The layout traversal already owns the loaded-font map.  Keeping this small
//! borrowed context explicit makes `ex` and `ch` resolution safe, local, and
//! independent of thread-local state.

use std::collections::HashMap;

use crate::parser::ttf::TtfFont;
use crate::style::computed::{ComputedStyle, FontFamily, FontWeight};

#[derive(Clone, Copy, Default)]
pub(crate) struct FontMetrics<'a> {
    fonts: Option<&'a HashMap<String, TtfFont>>,
}

impl<'a> FontMetrics<'a> {
    pub(crate) const fn new(fonts: &'a HashMap<String, TtfFont>) -> Self {
        Self { fonts: Some(fonts) }
    }

    /// x-height ratio for the font a style selects.
    pub(crate) fn style_x_height_ratio(self, style: &ComputedStyle) -> Option<f32> {
        self.resolve(style, TtfFont::x_height_ratio)
    }

    /// `ch` advance ratio for the font a style selects.
    pub(crate) fn style_ch_ratio(self, style: &ComputedStyle) -> Option<f32> {
        self.resolve(style, TtfFont::ch_ratio)
    }

    fn resolve(self, style: &ComputedStyle, extract: impl Fn(&TtfFont) -> f32) -> Option<f32> {
        let fonts = self.fonts?;
        let resolved = crate::system_fonts::resolve_font_family(
            &style.font_stack,
            fonts,
            style.font_weight == FontWeight::Bold,
            style.font_style.is_slanted(),
            style.font_stretch,
        );
        let FontFamily::Custom(name) = resolved else {
            return None;
        };
        crate::system_fonts::find_font(
            fonts,
            &name,
            style.font_weight == FontWeight::Bold,
            style.font_style.is_slanted(),
        )
        .map(|(_, font)| extract(font))
    }
}
