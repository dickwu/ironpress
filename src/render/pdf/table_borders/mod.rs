use super::*;

mod background;
mod geometry;
mod strokes;

pub(super) use background::CollapsedCellBackgroundBoundary;
pub(super) use geometry::{
    CollapsedCellTrackGeometry, CollapsedColumnTracks, CollapsedRowTracks,
    collapsed_table_horizontal_border_span, collapsed_table_vertical_border_span,
    paint_resolved_collapsed_cell_borders,
};
pub(super) use strokes::{
    paint_3d_border_line, paint_collapsed_outer_right_border, paint_table_cell_border_line,
};
