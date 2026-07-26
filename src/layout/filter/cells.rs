//! Filter ownership for layout cells.
//!
//! Grid, table, and flex formatting contexts store their items differently,
//! but a CSS filter always replaces one composited source box. This module is
//! the shared boundary that attaches that result to the concrete cell instead
//! of teaching every renderer a formatting-context-specific workaround.

use std::collections::HashMap;

use crate::layout::cells::{CellPaintHolder, GridCell};
use crate::layout::elements::{FilterHolder, FlexRow, GridRow};
use crate::layout::engine::FlexCell;
use crate::parser::ttf::TtfFont;
use crate::types::Size;

use super::ResolvedFilter;

/// Retain a grid-item filter until pagination supplies the row's absolute
/// device-space anchor.
pub(crate) fn retain_grid_cell_filter(cell: &mut GridCell, filter: ResolvedFilter) {
    if filter.requires_source_surface() {
        *cell.cell_paint_mut().filter_slot_mut() = Some(filter);
    }
}

/// Materialize filters retained on flattened flex items after pagination has
/// established the concrete cell fragment sizes.
pub(crate) fn materialize_flex_row(
    flex: &mut FlexRow,
    anchor: super::surface::SourceRasterAnchor,
    fonts: &HashMap<String, TtfFont>,
    filter_dpi: f32,
) {
    let frames = super::surface::flex_cell_source_frames(flex, fonts);
    for (cell, frame) in flex.content.cells.iter_mut().zip(frames) {
        let Some(filter) = cell.cell_paint_mut().take_filter() else {
            continue;
        };
        if composite_flex_cell(
            cell,
            frame.size,
            frame.anchor_in(anchor),
            &filter,
            fonts,
            filter_dpi,
        ) {
            continue;
        }
        filter.apply_flex_cell_fallback(cell);
    }
}

fn composite_flex_cell(
    cell: &mut FlexCell,
    size: Size,
    anchor: super::surface::SourceRasterAnchor,
    filter: &ResolvedFilter,
    fonts: &HashMap<String, TtfFont>,
    filter_dpi: f32,
) -> bool {
    if !filter.requires_source_surface() {
        return false;
    }
    let compositing = super::FilterCompositing::from_group(&cell.cell_paint_mut().group);
    let Some(source) =
        super::surface::paint_flex_cell_source(cell, size, fonts, filter_dpi, anchor)
    else {
        return false;
    };
    let Some((output, _)) =
        super::composite_source_graphic(source, filter, compositing, filter_dpi)
    else {
        return false;
    };
    cell.cell_paint_mut().filter_output = Some(output);
    true
}

/// Materialize filters retained on grid items after pagination has established
/// the concrete cell fragment sizes and device-space phase.
pub(crate) fn materialize_grid_row(
    grid: &mut GridRow,
    anchor: super::surface::SourceRasterAnchor,
    fonts: &HashMap<String, TtfFont>,
    filter_dpi: f32,
) {
    let frames = super::surface::grid_cell_source_frames(grid);
    for (cell, frame) in grid.content.cells.iter_mut().zip(frames) {
        let Some(filter) = cell.cell_paint_mut().take_filter() else {
            continue;
        };
        if !composite_grid_cell(
            cell,
            frame.size,
            frame.anchor_in(anchor),
            &filter,
            fonts,
            filter_dpi,
        ) {
            *cell.cell_paint_mut().filter_slot_mut() = Some(filter);
        }
    }
}

fn composite_grid_cell(
    cell: &mut GridCell,
    size: Size,
    anchor: super::surface::SourceRasterAnchor,
    filter: &ResolvedFilter,
    fonts: &HashMap<String, TtfFont>,
    filter_dpi: f32,
) -> bool {
    if !filter.requires_source_surface() {
        return false;
    }
    let compositing = super::FilterCompositing::from_group(&cell.cell_paint_mut().group);
    let Some(source) =
        super::surface::paint_grid_cell_source(cell, size, fonts, filter_dpi, anchor)
    else {
        return false;
    };
    let Some((output, _)) =
        super::composite_source_graphic(source, filter, compositing, filter_dpi)
    else {
        return false;
    };
    cell.cell_paint_mut().filter_output = Some(output);
    true
}
