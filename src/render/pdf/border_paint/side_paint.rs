use super::*;

/// Paint equal-colour solid sides as compound regions of one border ring.
///
/// Side ownership stays exclusive—the regions only share their frontier—but
/// one fill operation prevents a PDF rasterizer from antialiasing that shared
/// frontier twice and exposing a hairline between visually continuous sides.
pub(super) fn paint_solid_side_groups(
    content: &mut String,
    ring: BorderRingGeometry,
    sides: &[(PhysicalSide, &crate::layout::engine::LayoutBorderSide); 4],
    page_ext_gstates: &mut Vec<(String, f32)>,
    alpha_counter: &mut usize,
) {
    let mut painted_colors = Vec::with_capacity(sides.len());
    for (_, side) in sides {
        if !side.paints()
            || side.style != BorderStyle::Solid
            || painted_colors.contains(&side.color)
        {
            continue;
        }
        painted_colors.push(side.color);
        let alpha =
            begin_border_alpha(content, page_ext_gstates, alpha_counter, side.color.alpha());
        content.push_str(&PdfRgb::from(side.color).fill_operator());
        let clipped = ring.needs_curved_clip();
        if clipped {
            content.push_str("q\n");
            ring.push_clip(content);
        }
        for (edge, candidate) in sides {
            if candidate.paints()
                && candidate.style == BorderStyle::Solid
                && candidate.color == side.color
            {
                ring.side_region(*edge).push_path(content);
            }
        }
        content.push_str("f\n");
        if clipped {
            content.push_str("Q\n");
        }
        end_border_alpha(content, alpha);
    }
}

pub(super) fn paint_rounded_patterned_side(
    content: &mut String,
    ring: BorderRingGeometry,
    stroke: BorderStrokeGeometry,
    edge: PhysicalSide,
    side: &crate::layout::engine::LayoutBorderSide,
) {
    content.push_str("q\n");
    ring.side_region(edge).push_clip(content);
    paint_closed_rounded_pattern(content, ring, stroke, side);
    content.push_str("Q\n");
}

/// Paint one closed rounded dash/dot cadence through the canonical ring.
/// Uniform and mixed-side borders deliberately share this path so the same
/// authored geometry cannot acquire a different cadence through dispatch.
pub(super) fn paint_closed_rounded_pattern(
    content: &mut String,
    ring: BorderRingGeometry,
    stroke: BorderStrokeGeometry,
    side: &crate::layout::engine::LayoutBorderSide,
) {
    content.push_str("q\n");
    ring.push_clip(content);
    content.push_str(&PdfRgb::from(side.color).stroke_operator());
    content.push_str(&closed_border_pattern_for_style(
        side.style,
        side.width,
        stroke.path_length(),
    ));
    let stroke_width = if side.style == BorderStyle::Dashed {
        // Blink deliberately oversizes curved dashed paint, then lets the
        // rounded border ring own both visible edges. Coincident stroke and
        // clip edges otherwise antialias independently and leave a fringe.
        side.width * 2.2
    } else {
        side.width
    };
    content.push_str(&format!("{stroke_width} w\n"));
    content.push_str(&stroke.centerline.path_or_rect());
    content.push_str("S\n");
    content.push_str(reset_dash_pattern(side.style));
    content.push_str("Q\n");
}

pub(super) fn paint_square_patterned_side(
    content: &mut String,
    ring: BorderRingGeometry,
    border_box: PdfRect,
    edge: PhysicalSide,
    side: &crate::layout::engine::LayoutBorderSide,
) {
    let half = side.width / 2.0;
    let dotted = side.style == BorderStyle::Dotted;
    let endpoint_inset = if dotted { half } else { 0.0 };
    let (start, end, length) = match edge {
        PhysicalSide::Top => (
            PdfPoint::new(border_box.left + endpoint_inset, border_box.top() - half),
            PdfPoint::new(border_box.right() - endpoint_inset, border_box.top() - half),
            border_box.width - 2.0 * endpoint_inset,
        ),
        PhysicalSide::Right => (
            PdfPoint::new(border_box.right() - half, border_box.top() - endpoint_inset),
            PdfPoint::new(
                border_box.right() - half,
                border_box.bottom + endpoint_inset,
            ),
            border_box.height - 2.0 * endpoint_inset,
        ),
        PhysicalSide::Bottom => (
            PdfPoint::new(
                border_box.right() - endpoint_inset,
                border_box.bottom + half,
            ),
            PdfPoint::new(border_box.left + endpoint_inset, border_box.bottom + half),
            border_box.width - 2.0 * endpoint_inset,
        ),
        PhysicalSide::Left => (
            PdfPoint::new(border_box.left + half, border_box.bottom + endpoint_inset),
            PdfPoint::new(border_box.left + half, border_box.top() - endpoint_inset),
            border_box.height - 2.0 * endpoint_inset,
        ),
    };
    if length <= 0.0 {
        return;
    }
    content.push_str("q\n");
    ring.side_region(edge).push_clip(content);
    ring.push_clip(content);
    content.push_str(&PdfRgb::from(side.color).stroke_operator());
    let (array, phase) = corner_dash_array(length, side.width, dotted);
    if dotted {
        content.push_str("1 J\n");
    }
    content.push_str(&format!(
        "{} w\n[{array}] {phase} d\n{} {} m {} {} l S\n",
        side.width, start.x, start.y, end.x, end.y,
    ));
    content.push_str(reset_dash_pattern(side.style));
    content.push_str("Q\n");
}

pub(super) fn paint_border_band_side(
    content: &mut String,
    band: BorderRingGeometry,
    edge: PhysicalSide,
    color: PdfRgb,
    rounded: bool,
) {
    content.push_str(&color.fill_operator());
    if rounded {
        content.push_str("q\n");
        band.side_region(edge).push_clip(content);
        band.push_path(content);
        content.push_str("f*\nQ\n");
    } else {
        band.side_region(edge).push_path(content);
        content.push_str("f\n");
    }
}
