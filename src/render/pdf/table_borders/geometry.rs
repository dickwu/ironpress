use super::*;

/// Track geometry needed to map layout-owned collapsed-border segments into
/// page coordinates. Segment state remains in row/column units until paint so
/// pagination never has to rewrite physical endpoints.
#[derive(Debug, Clone, Copy)]
pub(in crate::render::pdf) struct CollapsedColumnTracks<'a> {
    widths: &'a [f32],
    start: usize,
}

impl<'a> CollapsedColumnTracks<'a> {
    pub(in crate::render::pdf) const fn new(widths: &'a [f32], start: usize) -> Self {
        Self { widths, start }
    }
}

#[derive(Debug, Clone, Copy)]
pub(in crate::render::pdf) struct CollapsedRowTracks<'a> {
    heights: &'a [Option<f32>],
    element_index: usize,
    current_height: f32,
}

impl<'a> CollapsedRowTracks<'a> {
    pub(in crate::render::pdf) const fn new(
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
pub(in crate::render::pdf) struct CollapsedCellTrackGeometry<'a> {
    /// Collapsed borders resolve on the unsnapped table grid. Chromium snaps
    /// cell background destinations independently, but not these centerlines.
    cell: LayoutBoxGeometry,
    columns: CollapsedColumnTracks<'a>,
    rows: CollapsedRowTracks<'a>,
}

impl<'a> CollapsedCellTrackGeometry<'a> {
    pub(in crate::render::pdf) const fn new(
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
pub(in crate::render::pdf) fn paint_resolved_collapsed_cell_borders(
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
        for segment in cell.table.collapsed_segments.get(edge) {
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
pub(in crate::render::pdf) fn collapsed_table_vertical_border_span(
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
pub(in crate::render::pdf) fn collapsed_table_horizontal_border_span(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::cells::{CellBox, CellBoxModel, TableCellState};
    use crate::layout::engine::{LayoutBorder, LayoutBorderSide};
    use crate::types::PhysicalEdgeFlags;

    fn solid_border(width: f32) -> LayoutBorderSide {
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
                        top: solid_border(12.0),
                        bottom: solid_border(8.0),
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
                        left: solid_border(6.0),
                        right: solid_border(8.0),
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
}
