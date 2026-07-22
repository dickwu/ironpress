use super::*;

/// Paint a truly uniform frame as one visual region.
///
/// The geometry still comes from the canonical CSS border ring. Merging equal
/// sides avoids antialias seams at otherwise artificial side frontiers. Returns
/// `false` only for 3D styles whose light and dark physical edges differ.
pub(super) fn paint_uniform_border(
    content: &mut String,
    border_box: RoundedRect,
    side: crate::layout::engine::LayoutBorderSide,
    page_ext_gstates: &mut Vec<(String, f32)>,
    alpha_counter: &mut usize,
) -> bool {
    if !side.paints() || is_bevel_style(side.style) {
        return false;
    }
    let border_box = RoundedRect::new(
        border_box.rect,
        border_box
            .radii
            .fit_to(border_box.rect.width, border_box.rect.height),
    );
    let alpha = begin_border_alpha(content, page_ext_gstates, alpha_counter, side.color.alpha());
    let color = PdfRgb::from(side.color);
    match side.style {
        BorderStyle::Solid => {
            content.push_str(&color.fill_operator());
            if exact_square_stroke(border_box, side) {
                paint_square_stroke(content, border_box.rect, side.width, color);
            } else {
                paint_ring(
                    content,
                    BorderRingGeometry::new(
                        border_box.rect,
                        border_box.radii,
                        EdgeSizes::uniform(side.width),
                    ),
                );
            }
        }
        BorderStyle::Double => {
            paint_double_strokes(content, border_box, side.width, color);
        }
        BorderStyle::Dashed | BorderStyle::Dotted => {
            if border_box.radii.is_zero() {
                content.push_str(&color.fill_operator());
                paint_square_pattern(content, border_box.rect, side);
            } else {
                paint_rounded_pattern(content, border_box, side, color);
            }
        }
        BorderStyle::None | BorderStyle::Hidden => {}
        BorderStyle::Groove | BorderStyle::Ridge | BorderStyle::Inset | BorderStyle::Outset => {
            end_border_alpha(content, alpha);
            return false;
        }
    }
    end_border_alpha(content, alpha);
    true
}

fn paint_ring(content: &mut String, ring: BorderRingGeometry) {
    ring.push_path(content);
    content.push_str("f*\n");
}

fn exact_square_stroke(
    border_box: RoundedRect,
    side: crate::layout::engine::LayoutBorderSide,
) -> bool {
    side.color.alpha() == 1.0
        && border_box.radii.is_zero()
        && side.width.is_finite()
        && border_box.rect.width > 2.0 * side.width
        && border_box.rect.height > 2.0 * side.width
}

fn paint_square_stroke(content: &mut String, border_box: PdfRect, width: f32, color: PdfRgb) {
    content.push_str(&color.stroke_operator());
    content.push_str("0 J\n0 j\n");
    content.push_str(&format!("{width} w\n"));
    content.push_str(
        &border_box
            .inset(EdgeSizes::uniform(width * 0.5))
            .rect_path(),
    );
    content.push_str("S\n");
}

fn paint_double_strokes(content: &mut String, border_box: RoundedRect, width: f32, color: PdfRgb) {
    let rule = width / 3.0;
    content.push_str(&color.stroke_operator());
    content.push_str("0 J\n0 j\n");
    content.push_str(&format!("{rule} w\n"));
    for inset in [rule * 0.5, width - rule * 0.5] {
        content.push_str(&border_box.inset(EdgeSizes::uniform(inset)).path_or_rect());
        content.push_str("S\n");
    }
}

fn paint_square_pattern(
    content: &mut String,
    rect: PdfRect,
    side: crate::layout::engine::LayoutBorderSide,
) {
    if side.style == BorderStyle::Dashed {
        paint_square_dashes(content, rect, side.width);
    } else {
        paint_square_dots(content, rect, side.width);
    }
}

fn paint_square_dashes(content: &mut String, rect: PdfRect, width: f32) {
    let dash = (width * 2.0).max(1.0);
    let nominal_gap = width.max(1.0);
    let add_rect = |content: &mut String, rect: PdfRect| {
        if !rect.is_empty() {
            content.push_str(&rect.rect_path());
        }
    };
    let horizontal = |content: &mut String, y: f32| {
        let count = (((rect.width + nominal_gap) / (dash + nominal_gap)).round()).max(1.0) as usize;
        let gap = if count > 1 {
            ((rect.width - count as f32 * dash) / (count - 1) as f32).max(0.0)
        } else {
            0.0
        };
        for index in 0..count {
            let offset = index as f32 * (dash + gap);
            add_rect(
                content,
                PdfRect::new(rect.left + offset, y, dash.min(rect.width - offset), width),
            );
        }
    };
    let vertical = |content: &mut String, x: f32| {
        let count =
            (((rect.height + nominal_gap) / (dash + nominal_gap)).round()).max(1.0) as usize;
        let gap = if count > 1 {
            ((rect.height - count as f32 * dash) / (count - 1) as f32).max(0.0)
        } else {
            0.0
        };
        for index in 0..count {
            let offset = index as f32 * (dash + gap);
            add_rect(
                content,
                PdfRect::new(
                    x,
                    rect.top() - offset - dash,
                    width,
                    dash.min(rect.height - offset),
                ),
            );
        }
    };
    horizontal(content, rect.bottom);
    horizontal(content, rect.top() - width);
    vertical(content, rect.right() - width);
    vertical(content, rect.left);
    content.push_str("f\n");
}

fn paint_square_dots(content: &mut String, rect: PdfRect, width: f32) {
    let radius = width * 0.5;
    if radius <= 0.0 {
        return;
    }
    let horizontal_span = (rect.width - width).max(0.0);
    let vertical_span = (rect.height - width).max(0.0);
    let horizontal_intervals = (horizontal_span / (width * 2.0)).round().max(1.0) as usize;
    let vertical_intervals = (vertical_span / (width * 2.0)).round().max(1.0) as usize;
    for index in 0..=horizontal_intervals {
        let x = rect.left + radius + index as f32 * horizontal_span / horizontal_intervals as f32;
        for y in [rect.bottom + radius, rect.top() - radius] {
            PdfEllipse::circle(PdfPoint::new(x, y), radius).push_path(content);
            content.push_str("h\n");
        }
    }
    for index in 0..=vertical_intervals {
        let y = rect.top() - radius - index as f32 * vertical_span / vertical_intervals as f32;
        for x in [rect.left + radius, rect.right() - radius] {
            PdfEllipse::circle(PdfPoint::new(x, y), radius).push_path(content);
            content.push_str("h\n");
        }
    }
    content.push_str("f\n");
}

fn paint_rounded_pattern(
    content: &mut String,
    border_box: RoundedRect,
    side: crate::layout::engine::LayoutBorderSide,
    color: PdfRgb,
) {
    let ring = BorderRingGeometry::new(
        border_box.rect,
        border_box.radii,
        EdgeSizes::uniform(side.width),
    );
    let centerline = border_box.inset(EdgeSizes::uniform(side.width * 0.5));
    content.push_str("q\n");
    ring.push_clip(content);
    content.push_str(&color.stroke_operator());
    content.push_str(&closed_pattern(
        side.style,
        side.width,
        centerline.perimeter(),
    ));
    content.push_str(&format!("{} w\n", side.width));
    content.push_str(&centerline.path_or_rect());
    content.push_str("S\n");
    content.push_str(reset_dash_pattern(side.style));
    content.push_str("Q\n");
}

fn closed_pattern(style: BorderStyle, width: f32, perimeter: f32) -> String {
    let width = width.max(0.1);
    match style {
        BorderStyle::Dotted => {
            let count = (perimeter / (2.0 * width)).round().max(1.0);
            format!("1 J\n[0 {}] 0 d\n", perimeter / count)
        }
        BorderStyle::Dashed => {
            let dash = (2.0 * width).min(perimeter);
            let nominal_gap = (width * (2.0 / 3.0)).max(1.0);
            let count = (perimeter / (dash + nominal_gap)).round().max(1.0);
            let gap = ((perimeter - count * dash) / count).max(0.1);
            format!("[{dash} {gap}] 0 d\n")
        }
        _ => String::new(),
    }
}
