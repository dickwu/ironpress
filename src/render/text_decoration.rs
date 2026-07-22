//! Renderer-independent geometry for horizontal CSS text decorations.

use std::collections::HashMap;

use crate::layout::engine::TextRun;
use crate::parser::ttf::TtfFont;

pub(crate) fn thickness(run: &TextRun) -> f32 {
    let thickness = if let Some(thickness) = run.metadata.decoration_thickness {
        thickness
    } else if run.metadata.decoration_style == crate::style::computed::TextDecorationStyle::Wavy {
        run.font_size * 0.075
    } else {
        run.font_size * 0.085
    };
    thickness.max(crate::fonts::PT_PER_CSS_PX)
}

pub(crate) fn underline_distance_from_baseline(run: &TextRun) -> f32 {
    let top_offset = run
        .metadata
        .underline_offset
        .unwrap_or_else(|| crate::fonts::ceil_to_css_pixel(thickness(run) / 2.0));
    top_offset + thickness(run) / 2.0
}

pub(crate) fn overline_lift(run: &TextRun) -> f32 {
    run.font_size * 0.065
}

/// Width of leading and trailing whitespace excluded from a decoration line.
pub(crate) fn whitespace_insets(
    run: &TextRun,
    custom_fonts: &HashMap<String, TtfFont>,
) -> (f32, f32) {
    if run.inline_box.is_some() {
        return (0.0, 0.0);
    }
    let leading: String = run.text.chars().take_while(|c| c.is_whitespace()).collect();
    let trailing: String = run
        .text
        .chars()
        .rev()
        .take_while(|c| c.is_whitespace())
        .collect();
    let measure = |text: &str| {
        if text.is_empty() {
            0.0
        } else {
            crate::layout::text::estimate_word_width(
                text,
                run.font_size,
                &run.font_family,
                run.bold,
                run.font_style.is_slanted(),
                custom_fonts,
            )
        }
    };
    (measure(&leading), measure(&trailing))
}
