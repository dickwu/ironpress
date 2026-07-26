use super::*;

/// Track geometry needed to map layout-owned collapsed-border segments into
/// page coordinates. Segment state remains in row/column units until paint so
/// pagination never has to rewrite physical endpoints.
#[derive(Debug, Clone, Copy)]
pub(super) struct CollapsedColumnTracks<'a> {
    widths: &'a [f32],
    start: usize,
}

impl<'a> CollapsedColumnTracks<'a> {
    pub(super) const fn new(widths: &'a [f32], start: usize) -> Self {
        Self { widths, start }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CollapsedRowTracks<'a> {
    heights: &'a [Option<f32>],
    element_index: usize,
    current_height: f32,
}

impl<'a> CollapsedRowTracks<'a> {
    pub(super) const fn new(
        heights: &'a [Option<f32>],
        element_index: usize,
        current_height: f32,
    ) -> Self {
        Self {
            heights,
            element_index,
            current_height,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CollapsedCellTrackGeometry<'a> {
    /// Collapsed borders resolve on the unsnapped table grid. Chromium snaps
    /// cell background destinations independently, but not these centerlines.
    cell: LayoutBoxGeometry,
    columns: CollapsedColumnTracks<'a>,
    rows: CollapsedRowTracks<'a>,
}

impl<'a> CollapsedCellTrackGeometry<'a> {
    pub(super) const fn new(
        cell: LayoutBoxGeometry,
        columns: CollapsedColumnTracks<'a>,
        rows: CollapsedRowTracks<'a>,
    ) -> Self {
        Self {
            cell,
            columns,
            rows,
        }
    }

    fn border_box(self) -> PdfRect {
        self.cell.border_box
    }

    fn column_offset(&self, offset: usize) -> f32 {
        self.columns
            .widths
            .iter()
            .skip(self.columns.start)
            .take(offset)
            .sum()
    }

    fn row_offset(&self, offset: usize) -> f32 {
        if offset == 0 {
            return 0.0;
        }
        self.rows.current_height
            + self
                .rows
                .heights
                .iter()
                .skip(self.rows.element_index.saturating_add(1))
                .filter_map(|height| *height)
                .take(offset.saturating_sub(1))
                .sum::<f32>()
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn paint_resolved_collapsed_cell_borders(
    content: &mut String,
    cell: &TableCell,
    geometry: CollapsedCellTrackGeometry<'_>,
    page_ext_gstates: &mut Vec<(String, f32)>,
    bg_alpha_counter: &mut usize,
) {
    let border_box = geometry.border_box();
    for edge in [
        PhysicalSide::Top,
        PhysicalSide::Right,
        PhysicalSide::Bottom,
        PhysicalSide::Left,
    ] {
        let physical_side = match edge {
            PhysicalSide::Top => PhysicalSide::Top,
            PhysicalSide::Right => PhysicalSide::Right,
            PhysicalSide::Bottom => PhysicalSide::Bottom,
            PhysicalSide::Left => PhysicalSide::Left,
        };
        for segment in cell.table.collapsed_segments.get(physical_side) {
            let (x1, y1, x2, y2) = match edge {
                PhysicalSide::Top | PhysicalSide::Bottom => {
                    let mut left = border_box.left + geometry.column_offset(segment.track_offset);
                    let mut right = border_box.left
                        + geometry
                            .column_offset(segment.track_offset.saturating_add(segment.track_span));
                    if segment.track_offset == 0 {
                        if cell.table.collapsed_outer_edges.left {
                            left -= cell.layout.box_model.border_insets.left;
                        } else {
                            left += cell.layout.box_model.border_insets.left;
                        }
                    }
                    if segment.track_offset.saturating_add(segment.track_span) >= cell.span.columns
                    {
                        right += cell.layout.box_model.border_insets.right;
                    }
                    let y = if edge == PhysicalSide::Top {
                        border_box.top()
                    } else {
                        border_box.bottom
                    };
                    (left, y, right, y)
                }
                PhysicalSide::Right | PhysicalSide::Left => {
                    let mut top = border_box.top() - geometry.row_offset(segment.track_offset);
                    let mut bottom = border_box.top()
                        - geometry
                            .row_offset(segment.track_offset.saturating_add(segment.track_span));
                    if segment.track_offset == 0 && cell.table.collapsed_outer_edges.top {
                        top -= cell.layout.box_model.border_insets.top;
                    }
                    if segment.track_offset.saturating_add(segment.track_span) >= cell.span.rows
                        && cell.table.collapsed_outer_edges.bottom
                    {
                        bottom += cell.layout.box_model.border_insets.bottom;
                    }
                    let x = if edge == PhysicalSide::Right {
                        border_box.right()
                    } else {
                        border_box.left
                    };
                    (x, top, x, bottom)
                }
            };
            paint_table_cell_border_line(
                content,
                &segment.side,
                edge,
                x1,
                y1,
                x2,
                y2,
                page_ext_gstates,
                bg_alpha_counter,
            );
        }
    }
}

/// The paint span of a vertical collapsed border after the winning outer
/// horizontal borders have claimed the corner intersections.
pub(super) fn collapsed_table_vertical_border_span(
    cell: &TableCell,
    border_collapse: BorderCollapse,
    top: f32,
    bottom: f32,
) -> (f32, f32) {
    if border_collapse != BorderCollapse::Collapse {
        return (top, bottom);
    }
    let border = &cell.layout.box_model.border;
    let top_inset = (cell.table.collapsed_outer_edges.top && border.top.paints())
        .then_some(border.top.width / 2.0)
        .unwrap_or(0.0);
    let bottom_inset = (cell.table.collapsed_outer_edges.bottom && border.bottom.paints())
        .then_some(border.bottom.width / 2.0)
        .unwrap_or(0.0);
    (
        (top - top_inset).max(bottom),
        (bottom + bottom_inset).min(top),
    )
}

/// Clip a horizontal collapsed border to the cell's used border-box.
///
/// Collapsed borders are centered on grid lines, while CSS Tables requires an
/// internal cell side to be clipped to the border-box drawing area represented
/// by its real used value. Consequently a horizontal side begins after an
/// internal leading half-border and ends after its trailing half-border. At the
/// table's leading outer edge that same half-border extends outward instead.
/// `border_insets` remains authoritative even when conflict resolution assigns
/// the one painted copy of a shared border to the neighboring cell.
pub(super) fn collapsed_table_horizontal_border_span(
    cell: &TableCell,
    border_collapse: BorderCollapse,
    touches_left_edge: bool,
    left: f32,
    right: f32,
) -> (f32, f32) {
    if border_collapse != BorderCollapse::Collapse {
        return (left, right);
    }
    let border_insets = cell.layout.box_model.border_insets;
    let clipped_left = if touches_left_edge {
        left - border_insets.left
    } else {
        left + border_insets.left
    };
    let clipped_right = right + border_insets.right;
    (clipped_left, clipped_right)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn paint_collapsed_outer_right_border(
    content: &mut String,
    side: &crate::layout::engine::LayoutBorderSide,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    page_ext_gstates: &mut Vec<(String, f32)>,
    bg_alpha_counter: &mut usize,
) {
    if !side.paints() || width <= 0.0 || height <= 0.0 {
        return;
    }
    let alpha = begin_border_alpha(
        content,
        page_ext_gstates,
        bg_alpha_counter,
        side.color.alpha(),
    );
    let (r, g, b) = side.color.to_f32_rgb();
    content.push_str(&format!("{r} {g} {b} rg\n{x} {y} {width} {height} re\nf\n"));
    end_border_alpha(content, alpha);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn paint_table_cell_border_line(
    content: &mut String,
    side: &crate::layout::engine::LayoutBorderSide,
    edge: PhysicalSide,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    page_ext_gstates: &mut Vec<(String, f32)>,
    bg_alpha_counter: &mut usize,
) {
    if !side.paints() {
        return;
    }
    if is_bevel_style(side.style) {
        paint_3d_border_line(
            content,
            side,
            edge,
            x1,
            y1,
            x2,
            y2,
            page_ext_gstates,
            bg_alpha_counter,
        );
        return;
    }
    let (r, g, b) = side.color.to_f32_rgb();
    let a = begin_border_alpha(
        content,
        page_ext_gstates,
        bg_alpha_counter,
        side.color.alpha(),
    );
    if side.style == BorderStyle::Solid {
        let (x, y, width, height) = match edge {
            PhysicalSide::Top | PhysicalSide::Bottom => (
                x1.min(x2),
                y1 - side.width / 2.0,
                (x2 - x1).abs(),
                side.width,
            ),
            PhysicalSide::Right | PhysicalSide::Left => (
                x1 - side.width / 2.0,
                y1.min(y2),
                side.width,
                (y2 - y1).abs(),
            ),
        };
        if width > 0.0 && height > 0.0 {
            content.push_str(&format!("{r} {g} {b} rg\n{x} {y} {width} {height} re\nf\n"));
        }
        end_border_alpha(content, a);
        return;
    }
    content.push_str(&format!("{r} {g} {b} rg\n"));
    match side.style {
        BorderStyle::Double => paint_double_border_areas(content, edge, x1, y1, x2, y2, side.width),
        BorderStyle::Dashed => paint_dashed_border_areas(content, edge, x1, y1, x2, y2, side.width),
        BorderStyle::Dotted => paint_dotted_border_areas(content, edge, x1, y1, x2, y2, side.width),
        _ => {
            content.push_str(&format!("{r} {g} {b} RG\n"));
            content.push_str(&format!("{} w\n{x1} {y1} m {x2} {y2} l S\n", side.width));
        }
    }
    end_border_alpha(content, a);
}

fn paint_double_border_areas(
    content: &mut String,
    edge: PhysicalSide,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    width: f32,
) {
    let metrics = DoubleBorderMetrics::new(width);
    let rule = metrics.stripe_width();
    let inner = metrics.inner_inset();
    match edge {
        PhysicalSide::Top | PhysicalSide::Bottom => {
            let left = x1.min(x2);
            let length = (x2 - x1).abs();
            let bottom = y1 - width / 2.0;
            content.push_str(&format!(
                "{left} {bottom} {length} {rule} re\n{left} {} {length} {rule} re\nf\n",
                bottom + inner,
            ));
        }
        PhysicalSide::Right | PhysicalSide::Left => {
            let bottom = y1.min(y2);
            let length = (y2 - y1).abs();
            let left = x1 - width / 2.0;
            content.push_str(&format!(
                "{left} {bottom} {rule} {length} re\n{} {bottom} {rule} {length} re\nf\n",
                left + inner,
            ));
        }
    }
}

fn paint_dashed_border_areas(
    content: &mut String,
    edge: PhysicalSide,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    width: f32,
) {
    let dash = (width * 2.0).max(1.0);
    let gap = (width * (2.0 / 3.0)).max(1.0);
    let horizontal = matches!(edge, PhysicalSide::Top | PhysicalSide::Bottom);
    let length = if horizontal {
        (x2 - x1).abs()
    } else {
        (y2 - y1).abs()
    };
    let mut offset = 0.0;
    while offset < length {
        let segment = dash.min(length - offset);
        if horizontal {
            let start = if x2 >= x1 {
                x1 + offset
            } else {
                x1 - offset - segment
            };
            content.push_str(&format!(
                "{start} {} {segment} {width} re\n",
                y1 - width / 2.0,
            ));
        } else {
            let start = if y2 >= y1 {
                y1 + offset
            } else {
                y1 - offset - segment
            };
            content.push_str(&format!(
                "{} {start} {width} {segment} re\n",
                x1 - width / 2.0,
            ));
        }
        offset += dash + gap;
    }
    content.push_str("f\n");
}

fn paint_dotted_border_areas(
    content: &mut String,
    edge: PhysicalSide,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    width: f32,
) {
    let horizontal = matches!(edge, PhysicalSide::Top | PhysicalSide::Bottom);
    let length = if horizontal {
        (x2 - x1).abs()
    } else {
        (y2 - y1).abs()
    };
    let direction = if horizontal {
        (x2 - x1).signum()
    } else {
        (y2 - y1).signum()
    };
    let step = width * 2.0;
    let mut offset = 0.0;
    while offset <= length + 0.001 {
        let center = if horizontal {
            PdfPoint::new(x1 + direction * offset, y1)
        } else {
            PdfPoint::new(x1, y1 + direction * offset)
        };
        PdfEllipse::circle(center, width / 2.0).push_path(content);
        offset += step;
    }
    content.push_str("f\n");
}

#[allow(clippy::too_many_arguments)]
pub(super) fn paint_3d_border_line(
    content: &mut String,
    side: &crate::layout::engine::LayoutBorderSide,
    edge: PhysicalSide,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    page_ext_gstates: &mut Vec<(String, f32)>,
    bg_alpha_counter: &mut usize,
) {
    let alpha = begin_border_alpha(
        content,
        page_ext_gstates,
        bg_alpha_counter,
        side.color.alpha(),
    );
    let mut stroke = |inner_band: bool, width: f32, offset: f32| {
        let (nx, ny) = match edge {
            PhysicalSide::Top => (0.0, 1.0),
            PhysicalSide::Right => (1.0, 0.0),
            PhysicalSide::Bottom => (0.0, -1.0),
            PhysicalSide::Left => (-1.0, 0.0),
        };
        let (r, g, b) = bevel_edge_color(side.style, edge, inner_band, side.color.to_f32_rgb());
        content.push_str(&format!(
            "{r} {g} {b} RG\n{width} w\n{} {} m {} {} l S\n",
            x1 + nx * offset,
            y1 + ny * offset,
            x2 + nx * offset,
            y2 + ny * offset,
        ));
    };
    if matches!(side.style, BorderStyle::Groove | BorderStyle::Ridge) {
        let half = side.width / 2.0;
        let quarter = side.width / 4.0;
        stroke(false, half, quarter);
        stroke(true, half, -quarter);
    } else {
        stroke(false, side.width, 0.0);
    }
    end_border_alpha(content, alpha);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::cells::{CellBox, CellBoxModel, TableCellState};
    use crate::layout::engine::{LayoutBorder, LayoutBorderSide};
    use crate::types::PhysicalEdgeFlags;

    fn outer_border(width: f32) -> LayoutBorderSide {
        LayoutBorderSide {
            width,
            style: BorderStyle::Solid,
            ..Default::default()
        }
    }

    #[test]
    fn collapsed_vertical_border_yields_outer_horizontal_intersections() {
        let cell = TableCell {
            layout: CellBox {
                box_model: CellBoxModel {
                    border: LayoutBorder {
                        top: outer_border(12.0),
                        bottom: outer_border(8.0),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
            table: TableCellState {
                collapsed_outer_edges: PhysicalEdgeFlags {
                    top: true,
                    bottom: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(
            collapsed_table_vertical_border_span(&cell, BorderCollapse::Collapse, 100.0, 0.0),
            (94.0, 4.0)
        );
    }

    #[test]
    fn collapsed_horizontal_border_clips_at_internal_perpendicular_edges() {
        let cell = TableCell {
            layout: CellBox {
                box_model: CellBoxModel {
                    border_insets: EdgeSizes::new(0.0, 4.0, 0.0, 3.0),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(
            collapsed_table_horizontal_border_span(
                &cell,
                BorderCollapse::Collapse,
                false,
                10.0,
                100.0,
            ),
            (13.0, 104.0)
        );
    }

    #[test]
    fn separate_borders_keep_their_full_vertical_span() {
        let cell = TableCell::default();

        assert_eq!(
            collapsed_table_vertical_border_span(&cell, BorderCollapse::Separate, 100.0, 0.0),
            (100.0, 0.0)
        );
    }

    #[test]
    fn collapsed_horizontal_border_covers_outer_corner_halves() {
        let cell = TableCell {
            layout: CellBox {
                box_model: CellBoxModel {
                    border: LayoutBorder {
                        left: outer_border(6.0),
                        right: outer_border(8.0),
                        ..Default::default()
                    },
                    border_insets: EdgeSizes::new(0.0, 4.0, 0.0, 3.0),
                    ..Default::default()
                },
                ..Default::default()
            },
            table: TableCellState {
                collapsed_outer_edges: PhysicalEdgeFlags {
                    left: true,
                    right: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(
            collapsed_table_horizontal_border_span(
                &cell,
                BorderCollapse::Collapse,
                true,
                10.0,
                100.0,
            ),
            (7.0, 104.0)
        );
    }

    #[test]
    fn solid_border_line_uses_a_filled_coverage_band() {
        let side = LayoutBorderSide {
            width: 2.0,
            style: BorderStyle::Solid,
            color: crate::types::Color::BLACK,
            ..Default::default()
        };
        let mut content = String::new();
        let mut states = Vec::new();
        let mut counter = 0;

        paint_table_cell_border_line(
            &mut content,
            &side,
            PhysicalSide::Left,
            10.0,
            30.0,
            10.0,
            5.0,
            &mut states,
            &mut counter,
        );

        assert!(content.contains("0 0 0 rg\n9 5 2 25 re\nf\n"));
        assert!(!content.contains(" S\n"));
    }

    #[test]
    fn double_border_divides_integral_css_widths_in_css_pixels() {
        assert_eq!(DoubleBorderMetrics::new(6.0).stripe_width(), 2.25);
        assert_eq!(DoubleBorderMetrics::new(7.5).inner_inset(), 5.25);
    }

    #[test]
    fn dashed_border_uses_explicit_filled_segments() {
        let mut content = String::new();

        paint_dashed_border_areas(&mut content, PhysicalSide::Left, 10.0, 0.0, 10.0, 60.0, 6.0);

        assert!(content.starts_with("7 0 6 12 re\n"));
        assert!(content.contains("7 16 6 12 re\n"));
        assert!(content.ends_with("f\n"));
        assert!(!content.contains(" S\n"));
    }

    #[test]
    fn dotted_border_uses_explicit_circle_paths() {
        let mut content = String::new();

        paint_dotted_border_areas(&mut content, PhysicalSide::Top, 0.0, 10.0, 24.0, 10.0, 6.0);

        assert_eq!(content.matches("h\n").count(), 3);
        assert!(content.ends_with("f\n"));
        assert!(!content.contains(" S\n"));
    }
}
