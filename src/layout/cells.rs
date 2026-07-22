use crate::layout::elements::{
    BoxPaint, BoxPaintOwner, LayoutNode, LayoutSize, Positioning, PositioningOwner,
};
use crate::layout::engine::{LayoutBorder, TextLine};
use crate::layout::filter::FilterRasterOutput;
use crate::style::computed::{TextAlign, VerticalAlign};
use crate::types::{EdgeSizes, PhysicalEdgeFlags, PhysicalEdges, Point, Size};

/// A resolved portion of one cell edge, expressed in table tracks relative to
/// the cell's first row or column. Track coordinates survive pagination and do
/// not duplicate physical point geometry in layout state.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CollapsedBorderSegment {
    pub(crate) track_offset: usize,
    pub(crate) track_span: usize,
    pub(crate) side: crate::layout::engine::LayoutBorderSide,
}

/// Conflict-resolved paint sections for all four edges of one table cell.
pub(crate) type CollapsedBorderSegments = PhysicalEdges<Vec<CollapsedBorderSegment>>;

/// Text and child layout owned by one table or grid cell.
#[derive(Debug, Clone, Default)]
pub struct CellContent {
    pub lines: Vec<TextLine>,
    pub children: Vec<LayoutNode>,
}

/// Canonical box paint plus an optional materialized filter replacement.
///
/// Cells reuse `BoxPaint`; maintaining a smaller parallel set of background,
/// shadow, masking, and opacity fields is how child-only paint paths drifted
/// away from ordinary boxes.
#[derive(Debug, Clone, Default)]
pub(crate) struct CellPaint {
    pub(crate) box_paint: BoxPaint,
    pub(crate) filter_output: Option<FilterRasterOutput>,
}

impl CellPaint {
    pub(crate) fn from_style(
        style: &crate::style::computed::ComputedStyle,
        size: LayoutSize,
    ) -> Self {
        Self {
            box_paint: BoxPaint::from_style(style, size),
            ..Default::default()
        }
    }

    pub(crate) fn has_outset_graphical_effect(&self) -> bool {
        self.box_paint.has_outset_graphical_effect()
            || self
                .filter_output
                .as_ref()
                .is_some_and(|output| !output.raster_overflow.is_zero())
    }
}

impl std::ops::Deref for CellPaint {
    type Target = BoxPaint;

    fn deref(&self) -> &Self::Target {
        &self.box_paint
    }
}

impl std::ops::DerefMut for CellPaint {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.box_paint
    }
}

impl crate::layout::elements::FilterHolder for CellPaint {
    fn filter_slot_mut(&mut self) -> &mut Option<crate::layout::filter::ResolvedFilter> {
        &mut self.box_paint.filter
    }
}

/// Resolved box-model geometry shared by table and grid cells.
#[derive(Debug, Clone, Default)]
pub struct CellBoxModel {
    /// Insets from the cell edge to its content. These include authored padding
    /// and the cell-owned share of its border.
    pub content_insets: EdgeSizes,
    /// Cell-owned border share, kept separately for padding-box containing
    /// blocks and collapsed-border geometry.
    pub border_insets: EdgeSizes,
    pub border: LayoutBorder,
    /// Minimum grid-line distance in the block axis.
    pub minimum_block_size: f32,
}

/// Text alignment within one table or grid cell.
#[derive(Debug, Clone, Copy, Default)]
pub struct CellAlignment {
    pub inline: TextAlign,
    pub block: VerticalAlign,
}

/// Layout and paint state common to table and grid cells.
#[derive(Debug, Clone, Default)]
pub struct CellBox {
    pub content: CellContent,
    pub box_model: CellBoxModel,
    pub(crate) paint: CellPaint,
    pub(crate) positioning: Positioning,
    pub alignment: CellAlignment,
}

impl CellBox {
    pub(crate) fn has_outset_graphical_effect(&self) -> bool {
        self.paint.has_outset_graphical_effect()
            || crate::layout::elements::text_lines_have_outset_shadows(&self.content.lines)
    }
}

/// Access to the common cell box without erasing the concrete cell role.
pub trait CellBoxHolder {
    fn cell_box(&self) -> &CellBox;
}

/// Access to paint effects shared by every concrete cell representation.
pub(crate) trait CellPaintHolder {
    fn cell_paint_mut(&mut self) -> &mut CellPaint;
}

impl CellPaintHolder for CellBox {
    fn cell_paint_mut(&mut self) -> &mut CellPaint {
        &mut self.paint
    }
}

impl PositioningOwner for CellBox {
    fn positioning(&self) -> &Positioning {
        &self.positioning
    }

    fn positioning_mut(&mut self) -> &mut Positioning {
        &mut self.positioning
    }
}

impl BoxPaintOwner for CellBox {
    fn box_paint(&self) -> &BoxPaint {
        &self.paint.box_paint
    }

    fn box_paint_mut(&mut self) -> &mut BoxPaint {
        &mut self.paint.box_paint
    }
}

/// Number of table tracks occupied by a table cell.
#[derive(Debug, Clone, Copy)]
pub struct TableCellSpan {
    pub columns: usize,
    pub rows: usize,
}

impl Default for TableCellSpan {
    fn default() -> Self {
        Self {
            columns: 1,
            rows: 1,
        }
    }
}

/// Definite table-cell height contribution to the row tracks it spans.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TableCellHeightConstraint {
    pub(crate) specified: f32,
    pub(crate) rowspan: usize,
}

impl TableCellHeightConstraint {
    /// CSS Tables defines a row's minimum from the cell's computed height.
    /// Collapsed borders can extend beyond the row grid, but do not reduce this
    /// track contribution.
    pub(crate) fn minimum_row_height(self) -> f32 {
        self.specified / self.rowspan.max(1) as f32
    }
}

/// Table-only border and visibility state.
#[derive(Debug, Clone, Default)]
pub struct TableCellState {
    /// Physical edges that form the outside of the collapsed table grid.
    pub collapsed_outer_edges: PhysicalEdgeFlags,
    pub(crate) collapsed_segments: CollapsedBorderSegments,
    pub(crate) collapsed_resolution_complete: bool,
    pub hide_if_empty: bool,
    pub clips: bool,
}

impl TableCellState {
    pub(crate) fn has_resolved_collapsed_borders(&self) -> bool {
        self.collapsed_resolution_complete
    }
}

/// A concrete table cell.
#[derive(Debug, Clone, Default)]
pub struct TableCell {
    pub layout: CellBox,
    pub span: TableCellSpan,
    pub table: TableCellState,
}

impl CellBoxHolder for TableCell {
    fn cell_box(&self) -> &CellBox {
        &self.layout
    }
}

impl CellPaintHolder for TableCell {
    fn cell_paint_mut(&mut self) -> &mut CellPaint {
        self.layout.cell_paint_mut()
    }
}

/// Placement of a grid item's painted box within its track cell.
#[derive(Debug, Clone, Copy)]
pub struct GridInset {
    pub offset: Point,
    pub size: Size,
}

/// Grid-only placement and fragmentation state.
#[derive(Debug, Clone)]
pub struct GridCellPlacement {
    pub inset: Option<GridInset>,
    pub clips: bool,
    /// Zero-based start track retained independently of storage and paint order.
    pub column_start: usize,
    pub column_span: usize,
    pub row_span: usize,
}

impl Default for GridCellPlacement {
    fn default() -> Self {
        Self {
            inset: None,
            clips: false,
            column_start: 0,
            column_span: 1,
            row_span: 1,
        }
    }
}

/// A concrete grid cell.
#[derive(Debug, Clone, Default)]
pub struct GridCell {
    pub layout: CellBox,
    pub placement: GridCellPlacement,
}

impl CellBoxHolder for GridCell {
    fn cell_box(&self) -> &CellBox {
        &self.layout
    }
}

impl CellPaintHolder for GridCell {
    fn cell_paint_mut(&mut self) -> &mut CellPaint {
        self.layout.cell_paint_mut()
    }
}
