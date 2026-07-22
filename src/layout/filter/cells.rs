//! Filter ownership for layout cells.
//!
//! Grid, table, and flex formatting contexts store their items differently,
//! but a CSS filter always replaces one composited source box. This module is
//! the shared boundary that attaches that result to the concrete cell instead
//! of teaching every renderer a formatting-context-specific workaround.

use std::collections::HashMap;

use crate::layout::cells::{CellPaintHolder, GridCell};
use crate::layout::elements::{FilterHolder, FlexRow};
use crate::layout::engine::FlexCell;
use crate::parser::ttf::TtfFont;
use crate::types::Size;

use super::ResolvedFilter;

/// Materialize filters retained on flattened flex items after pagination has
/// established the concrete cell fragment sizes.
pub(crate) fn materialize_flex_row(
    flex: &mut FlexRow,
    fonts: &HashMap<String, TtfFont>,
    filter_dpi: f32,
) {
    let sizes = super::surface::flex_cell_source_sizes(flex, fonts);
    for (cell, size) in flex.content.cells.iter_mut().zip(sizes) {
        let Some(filter) = cell.cell_paint_mut().take_filter() else {
            continue;
        };
        if composite_flex_cell(cell, size, &filter, fonts, filter_dpi) {
            continue;
        }
        filter.apply_flex_cell_fallback(cell);
    }
}

fn composite_flex_cell(
    cell: &mut FlexCell,
    size: Size,
    filter: &ResolvedFilter,
    fonts: &HashMap<String, TtfFont>,
    filter_dpi: f32,
) -> bool {
    if !filter.has_composited_output() {
        return false;
    }
    let Some(source) = super::surface::paint_flex_cell_source(cell, size, fonts, filter_dpi) else {
        return false;
    };
    let Some((output, _)) =
        super::composite_source_graphic(source, filter, Default::default(), filter_dpi)
    else {
        return false;
    };
    cell.cell_paint_mut().filter_output = Some(output);
    true
}

pub(crate) fn composite_grid_cell(
    cell: &mut GridCell,
    size: Size,
    filter: &ResolvedFilter,
    fonts: &HashMap<String, TtfFont>,
    filter_dpi: f32,
) -> bool {
    if !filter.has_composited_output() {
        return false;
    }
    let Some(source) = super::surface::paint_grid_cell_source(cell, size, fonts, filter_dpi) else {
        return false;
    };
    let Some((output, _)) =
        super::composite_source_graphic(source, filter, Default::default(), filter_dpi)
    else {
        return false;
    };
    cell.cell_paint_mut().filter_output = Some(output);
    true
}
