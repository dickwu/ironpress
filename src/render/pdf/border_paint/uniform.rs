use super::*;

/// A square solid border whose visible sides all share one paint.
///
/// Once constructed, the border can be emitted as non-overlapping,
/// axis-aligned bands. This is both the exact geometric union of its visible
/// sides and the PDF representation Chromium uses for square solid borders.
/// Mixed paints stay on the diagonal corner-partition path.
#[derive(Debug, Clone, Copy)]
struct SquareSolidBorder {
    border_box: PdfRect,
    widths: EdgeSizes,
    color: crate::types::Color,
}

impl SquareSolidBorder {
    fn from_layout(
        border_box: PdfRect,
        border: &crate::layout::engine::LayoutBorder,
        radii: CornerRadii,
    ) -> Option<Self> {
        if !radii.is_zero() {
            return None;
        }

        let mut color = None;
        let mut used_width = |side: &crate::layout::engine::LayoutBorderSide| {
            if !side.paints() {
                return Some(0.0);
            }
            if side.style != BorderStyle::Solid {
                return None;
            }
            match color {
                Some(existing) if existing != side.color => return None,
                Some(_) => {}
                None => color = Some(side.color),
            }
            Some(side.width)
        };
        let widths = EdgeSizes::new(
            used_width(&border.top)?,
            used_width(&border.right)?,
            used_width(&border.bottom)?,
            used_width(&border.left)?,
        );
        let color = color?;
        if widths.horizontal() > border_box.width || widths.vertical() > border_box.height {
            return None;
        }
        Some(Self {
            border_box,
            widths,
            color,
        })
    }

    fn bands(self) -> [PdfRect; 4] {
        let vertical_bottom = self.border_box.bottom + self.widths.bottom;
        let vertical_height = self.border_box.height - self.widths.top - self.widths.bottom;
        [
            PdfRect::new(
                self.border_box.left,
                self.border_box.top() - self.widths.top,
                self.border_box.width,
                self.widths.top,
            ),
            PdfRect::new(
                self.border_box.right() - self.widths.right,
                vertical_bottom,
                self.widths.right,
                vertical_height,
            ),
            PdfRect::new(
                self.border_box.left,
                self.border_box.bottom,
                self.border_box.width,
                self.widths.bottom,
            ),
            PdfRect::new(
                self.border_box.left,
                vertical_bottom,
                self.widths.left,
                vertical_height,
            ),
        ]
    }

    fn paint(
        self,
        content: &mut String,
        page_ext_gstates: &mut Vec<(String, f32)>,
        alpha_counter: &mut usize,
    ) {
        let alpha =
            begin_border_alpha(content, page_ext_gstates, alpha_counter, self.color.alpha());
        content.push_str(&PdfRgb::from(self.color).fill_operator());
        for band in self.bands().into_iter().filter(|band| !band.is_empty()) {
            content.push_str(&band.rect_path());
            content.push_str("f\n");
        }
        end_border_alpha(content, alpha);
    }
}

pub(super) fn paint_square_solid_border(
    content: &mut String,
    border_box: PdfRect,
    border: &crate::layout::engine::LayoutBorder,
    radii: CornerRadii,
    page_ext_gstates: &mut Vec<(String, f32)>,
    alpha_counter: &mut usize,
) -> bool {
    let Some(border) = SquareSolidBorder::from_layout(border_box, border, radii) else {
        return false;
    };
    border.paint(content, page_ext_gstates, alpha_counter);
    true
}

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
            paint_ring(
                content,
                BorderRingGeometry::new(
                    border_box.rect,
                    border_box.radii,
                    EdgeSizes::uniform(side.width),
                ),
            );
        }
        BorderStyle::Double => {
            paint_double_border(content, border_box, side.width, color);
        }
        BorderStyle::Dashed | BorderStyle::Dotted => {
            if border_box.radii.is_zero() {
                content.push_str(&color.fill_operator());
                paint_square_pattern(content, border_box.rect, side);
            } else {
                let widths = EdgeSizes::uniform(side.width);
                paint_closed_rounded_pattern(
                    content,
                    BorderRingGeometry::new(border_box.rect, border_box.radii, widths),
                    BorderStrokeGeometry::new(border_box.rect, border_box.radii, widths),
                    &side,
                );
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

fn paint_double_border(content: &mut String, border_box: RoundedRect, width: f32, color: PdfRgb) {
    let metrics = DoubleBorderMetrics::new(width);
    let rule = metrics.stripe_width();
    if border_box.radii.is_zero() {
        content.push_str(&color.stroke_operator());
        content.push_str("0 J\n0 j\n");
        content.push_str(&format!("{rule} w\n"));
        for inset in [
            metrics.outer_centerline_inset(),
            metrics.inner_centerline_inset(),
        ] {
            content.push_str(&border_box.rect.inset(EdgeSizes::uniform(inset)).rect_path());
            content.push_str("S\n");
        }
        return;
    }

    let rule_edges = EdgeSizes::uniform(rule);
    let width_edges = EdgeSizes::uniform(width);
    content.push_str(&color.fill_operator());
    for ring in [
        BorderRingGeometry::between(
            border_box.rect,
            border_box.radii,
            EdgeSizes::ZERO,
            rule_edges,
        ),
        BorderRingGeometry::between(
            border_box.rect,
            border_box.radii,
            EdgeSizes::uniform(metrics.inner_inset()),
            width_edges,
        ),
    ] {
        paint_ring(content, ring);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: f32, color: crate::types::Color) -> crate::layout::engine::LayoutBorderSide {
        crate::layout::engine::LayoutBorderSide {
            width,
            color,
            style: BorderStyle::Solid,
        }
    }

    #[test]
    fn square_single_color_fragment_uses_non_overlapping_rectangular_bands() {
        let color = crate::types::Color::from_srgb(0.2, 0.3, 0.4, 1.0);
        let border = crate::layout::engine::LayoutBorder {
            top: solid(1.0, color),
            right: solid(2.0, color),
            bottom: Default::default(),
            left: solid(3.0, color),
        };
        let square = SquareSolidBorder::from_layout(
            PdfRect::new(10.0, 20.0, 100.0, 50.0),
            &border,
            CornerRadii::ZERO,
        )
        .expect("single-color square border");

        assert_eq!(
            square.bands(),
            [
                PdfRect::new(10.0, 69.0, 100.0, 1.0),
                PdfRect::new(108.0, 20.0, 2.0, 49.0),
                PdfRect::new(10.0, 20.0, 100.0, 0.0),
                PdfRect::new(10.0, 20.0, 3.0, 49.0),
            ]
        );
    }

    #[test]
    fn mixed_solid_colors_require_diagonal_corner_ownership() {
        let border = crate::layout::engine::LayoutBorder {
            top: solid(1.0, crate::types::Color::BLACK),
            right: solid(1.0, crate::types::Color::WHITE),
            bottom: Default::default(),
            left: solid(1.0, crate::types::Color::BLACK),
        };

        assert!(
            SquareSolidBorder::from_layout(
                PdfRect::new(0.0, 0.0, 100.0, 50.0),
                &border,
                CornerRadii::ZERO,
            )
            .is_none()
        );
    }
}
