use super::*;

pub(in crate::render::pdf) use crate::render::text_decoration::DecorationLine;
pub(in crate::render::pdf) use crate::render::text_decoration::thickness as decoration_thickness;

pub(in crate::render::pdf) fn underline_center_y(
    run: &TextRun,
    decoration: &crate::style::computed::TextDecoration,
    baseline_y: f32,
) -> f32 {
    // `text-underline-offset` is measured from the underline-position zero
    // point. For the supported horizontal `auto` position that point is the
    // alphabetic baseline; the line thickness extends outward from there.
    // Blink's automatic near-edge gap is half the resolved stroke width,
    // rounded outward to its CSS-pixel grid. An authored length (including
    // zero or a negative length) remains exact.
    baseline_y - crate::render::text_decoration::underline_distance_from_baseline(run, decoration)
}

pub(in crate::render::pdf) fn decoration_is_wavy(
    decoration: &crate::style::computed::TextDecoration,
) -> bool {
    decoration.style == crate::style::computed::TextDecorationStyle::Wavy
}

impl DecorationLine {
    fn wavy_axis_offset(self, thickness: f32) -> f32 {
        let offset = thickness + crate::fonts::PT_PER_CSS_PX;
        match self {
            // PDF coordinates increase upward, the inverse of CSS's block
            // direction. Wavy underlines need one decoration gap below their
            // solid-line axis, and overlines need the symmetric adjustment.
            Self::Underline => -offset,
            Self::LineThrough => 0.0,
            Self::Overline => offset,
        }
    }
}

/// Blink-compatible dimensions for a wavy text decoration. Its geometry is
/// based on the resolved decoration thickness, with a two-CSS-pixel minimum,
/// rather than the surrounding font size.
#[derive(Clone, Copy)]
struct WavyDecorationMetrics {
    step: f32,
    control_distance: f32,
}

impl WavyDecorationMetrics {
    fn from_thickness(thickness: f32) -> Self {
        let unit = thickness.max(2.0 * crate::fonts::PT_PER_CSS_PX);
        Self {
            step: unit * 2.5,
            control_distance: unit * 3.5,
        }
    }
}

pub(in crate::render::pdf) fn push_decoration_stroke(
    content: &mut String,
    color: (f32, f32, f32),
    run: &TextRun,
    decoration: &crate::style::computed::TextDecoration,
    line: DecorationLine,
    x1: f32,
    x2: f32,
    y: f32,
) {
    let thickness = decoration_thickness(run, decoration);
    if x2 <= x1 {
        return;
    }
    if !decoration_is_wavy(decoration) {
        let rect = PdfRect::new(x1, y - thickness / 2.0, x2 - x1, thickness);
        content.push_str(&PdfRgb::from(color).fill_operator());
        content.push_str(&rect.rect_path());
        content.push_str("f\n");
        return;
    }

    let stroke = thickness;
    let metrics = WavyDecorationMetrics::from_thickness(stroke);
    let axis_y = y + line.wavy_axis_offset(stroke);
    let clip_y = axis_y - metrics.control_distance - stroke * 2.0;
    let clip_h = (metrics.control_distance + stroke * 2.0) * 2.0;
    let mut x = x1 - 2.0 * metrics.step;
    let end_x = x2 + 4.0 * metrics.step;
    let mut path = format!("{x} {axis_y} m\n");
    while x + 2.0 * metrics.step <= end_x {
        let cx = x + metrics.step;
        x += 2.0 * metrics.step;
        path.push_str(&format!(
            "{cx} {} {cx} {} {x} {axis_y} c\n",
            axis_y - metrics.control_distance,
            axis_y + metrics.control_distance
        ));
    }
    content.push_str(&format!("q\n{x1} {clip_y} {} {clip_h} re\nW\nn\n", x2 - x1));
    content.push_str(&PdfRgb::from(color).stroke_operator());
    content.push_str(&format!("{stroke} w\n0 J\n1 j\n{path}S\nQ\n"));
}
