use super::*;

/// Returns the PDF dash-pattern operator string for a given border style.
/// Width-scaled dash/dot setup for a border side. Returns the PDF operators to
/// install before stroking: a dash array (`d`) and, for dotted, a round line cap
/// (`1 J`) so each dash collapses to a round dot of diameter = the stroke width.
///
/// CSS renders dotted as round dots roughly one border-width across spaced one
/// width apart, and dashed as segments a few widths long. Scaling by the stroke
/// width (rather than the previous fixed `[6 4]`/`[1 3]`) matches Chrome far more
/// closely and makes the pattern visible at any border thickness.
pub(super) fn dash_pattern_for_style(style: BorderStyle, width: f32) -> String {
    let w = width.max(0.1);
    match style {
        // Chrome paints dashed strokes with dashes ~2x the line width and gaps a
        // little under the line width (measured near 2:0.67), not the 3:3
        // (period 6x) of a naive equal pattern.
        BorderStyle::Dashed => {
            let dash = (w * 2.0).max(1.0);
            let gap = (w * (2.0 / 3.0)).max(1.0);
            format!("[{dash} {gap}] 0 d\n")
        }
        // Round dots: a zero-length dash under a round cap paints a filled dot of
        // diameter = line width; spacing = 2x width gives width-on / width-off.
        BorderStyle::Dotted => {
            let gap = (w * 2.0).max(1.0);
            format!("1 J\n[0 {gap}] 0 d\n")
        }
        _ => String::new(),
    }
}

/// Compute a corner-symmetric dash array and phase for one straight side.
pub(super) fn corner_dash_array(length: f32, border_width: f32, dotted: bool) -> (String, f32) {
    let length = length.max(0.0);
    if length <= 0.0 || border_width <= 0.0 {
        return (format!("{border_width}"), 0.0);
    }
    let (on, gap) = if dotted {
        (border_width, border_width)
    } else {
        ((border_width * 2.0).max(1.0), border_width.max(1.0))
    };
    let period = on + gap;
    if dotted {
        let count = (length / period).round().max(1.0);
        return (format!("0 {}", length / count), 0.0);
    }
    let count = (((length + gap) / period).round()).max(1.0);
    let adjusted_on = on.min(length);
    let adjusted_gap = if count > 1.0 {
        ((length - count * adjusted_on) / (count - 1.0)).max(0.1)
    } else {
        (length - adjusted_on).max(0.1)
    };
    (format!("{adjusted_on} {adjusted_gap}"), 0.0)
}

/// A dash pattern fitted to one side of a rounded border.
///
/// Dashed sides use the span offset to center a dash on each corner frontier.
/// Dotted sides retain one continuous cadence around the path. Each side is
/// clipped to its exclusive region, so neither construction creates corner
/// overdraw.
pub(super) fn side_dash_pattern_for_style(
    style: BorderStyle,
    width: f32,
    span: BorderPathSpan,
) -> String {
    let width = width.max(0.1);
    if !span.is_valid() {
        return dash_pattern_for_style(style, width);
    }

    match style {
        // Dots retain one continuous cadence around the centerline. Restarting
        // the pattern per side visibly changes the spacing at every corner;
        // the exclusive clip already assigns each circular mark to one side.
        BorderStyle::Dotted => dash_pattern_for_style(style, width),
        BorderStyle::Dashed => {
            let dash = (2.0 * width).min(span.length);
            let gap = width.max(1.0);
            // Center one dash on each corner frontier. The exclusive side clip
            // assigns one half to either adjoining side without overdraw.
            let partial = dash / 2.0;
            let period = dash + gap;
            let phase =
                (dash_phase_for_offset(span.offset, period) + dash - partial).rem_euclid(period);
            format!("[{dash} {gap}] {phase} d\n")
        }
        _ => String::new(),
    }
}

fn dash_phase_for_offset(offset: f32, period: f32) -> f32 {
    if !offset.is_finite() || !period.is_finite() || period <= 0.0 {
        return 0.0;
    }
    (period - offset.rem_euclid(period)).rem_euclid(period)
}

/// Reset the dash pattern (and line cap) back to solid/butt after a
/// dashed/dotted stroke so subsequent strokes are unaffected.
pub(super) fn reset_dash_pattern(style: BorderStyle) -> &'static str {
    match style {
        BorderStyle::Dashed => "[] 0 d\n",
        BorderStyle::Dotted => "[] 0 d\n0 J\n",
        _ => "",
    }
}

/// Apply a stroke-opacity ExtGState before painting a border side whose color is
/// translucent (`alpha < 1.0`). Mirrors the background-color alpha path: pushes a
/// `(name, alpha)` entry onto `page_ext_gstates` and emits `/{name} gs`. Returns
/// `true` when a non-default gstate was applied so the caller can reset it with
/// [`end_border_alpha`]. For opaque sides (`alpha >= 1.0`) nothing is emitted, so
/// existing output stays byte-identical.
pub(super) fn begin_border_alpha(
    content: &mut String,
    page_ext_gstates: &mut Vec<(String, f32)>,
    counter: &mut usize,
    alpha: f32,
) -> bool {
    if alpha < 1.0 {
        let gs_name = format!("GSbd{counter}");
        *counter += 1;
        page_ext_gstates.push((gs_name.clone(), alpha));
        content.push_str(&format!("/{gs_name} gs\n"));
        true
    } else {
        false
    }
}

/// Reset stroke opacity to the default gstate after a translucent border side.
/// No-op when `applied` is false.
pub(super) fn end_border_alpha(content: &mut String, applied: bool) {
    if applied {
        content.push_str("/GSDefault gs\n");
    }
}
