use super::{
    BlockFlowOwner, BlockFlowParticipant, BlockSize, BoxPaint, ChildContainer, Container,
    ContainingBlockConsumer, InlineFlowExtent, InlineOffset, IntoLayoutNode, LayoutElement,
    LayoutNode, LayoutVisitor, LayoutVisitorMut, PaintGroupOwner, PositioningOwner, PrincipalBox,
    TextBlock, impl_principal_layout_element,
};
use crate::layout::cells::TableCell;
use crate::layout::flow_metrics::{BlockMargins, MarginHolder};
use crate::style::computed::BorderCollapse;
use crate::types::{CornerRadii, EdgeSizes};

/// The principal box of one CSS table formatting context.
///
/// Rows, captions, and the table decoration remain independently fragmentable
/// children, while table-level positioning and graphical effects belong to
/// this single semantic box. Keeping that ownership intact is what makes a
/// transform, mask, opacity, or filter apply to the complete table rather than
/// to whichever flattened leaf happened to retain the property.
#[derive(Debug, Clone)]
pub(crate) struct Table {
    pub(crate) principal: Container,
}

impl Table {
    pub(crate) const fn new(principal: Container) -> Self {
        Self { principal }
    }
}

impl PrincipalBox for Table {
    fn principal(&self) -> &Container {
        &self.principal
    }

    fn principal_mut(&mut self) -> &mut Container {
        &mut self.principal
    }
}

impl_principal_layout_element!(Table, visit_table);

/// Capability exposed by the paint-only box that owns a table element's
/// background and border.
pub(crate) trait TableBoxDecorationOwner {
    fn decoration(&self) -> &TextBlock;
    fn open_fragment(&self, outer_extent: f32) -> LayoutNode;
    fn continuation_fragment(&self, outer_extent: f32) -> LayoutNode;
}

/// The table wrapper's own decoration, distinct from both textual content and
/// anonymous row/cell boxes.
///
/// Most layout operations intentionally see the wrapped block behavior. The
/// explicit capability lets flex fragmentation retain this principal box when
/// table rows continue on a later page instead of treating it as unrelated
/// zero-flow text.
#[derive(Debug, Clone)]
pub(crate) struct TableBoxDecoration {
    block: TextBlock,
}

impl TableBoxDecoration {
    pub(crate) const fn new(block: TextBlock) -> Self {
        Self { block }
    }

    fn with_outer_extent(mut self, outer_extent: f32) -> Self {
        let border_extent = self.block.box_model.border.vertical_width();
        self.block.box_model.size.height =
            BlockSize::fragment((outer_extent - border_extent).max(0.0));
        self.block.box_model.margins.end = -outer_extent;
        self
    }
}

impl TableBoxDecorationOwner for TableBoxDecoration {
    fn decoration(&self) -> &TextBlock {
        &self.block
    }

    fn open_fragment(&self, outer_extent: f32) -> LayoutNode {
        let mut fragment = self.clone();
        if fragment.block.fragmentation.box_fragmentation.decoration
            == crate::style::computed::BoxDecorationBreak::Slice
        {
            fragment.block.box_model.border.bottom.width = 0.0;
            fragment.block.paint.border_radii = fragment.block.paint.border_radii.clear_bottom();
        }
        fragment.with_outer_extent(outer_extent).boxed()
    }

    fn continuation_fragment(&self, outer_extent: f32) -> LayoutNode {
        let mut fragment = self.clone();
        fragment.block.box_model.margins.start = 0.0;
        if fragment.block.fragmentation.box_fragmentation.decoration
            == crate::style::computed::BoxDecorationBreak::Slice
        {
            fragment.block.box_model.border.top.width = 0.0;
            fragment.block.paint.border_radii = fragment.block.paint.border_radii.clear_top();
        }
        fragment.with_outer_extent(outer_extent).boxed()
    }
}

impl LayoutElement for TableBoxDecoration {
    fn clone_box(&self) -> LayoutNode {
        Box::new(self.clone())
    }

    fn accept(&self, visitor: &mut dyn LayoutVisitor) {
        visitor.visit_text_block(&self.block);
    }

    fn accept_mut(&mut self, visitor: &mut dyn LayoutVisitorMut) {
        visitor.visit_text_block(&mut self.block);
    }

    fn margin_holder(&self) -> Option<&dyn MarginHolder> {
        Some(&self.block)
    }

    fn margin_holder_mut(&mut self) -> Option<&mut dyn MarginHolder> {
        Some(&mut self.block)
    }

    fn inline_flow_extent(&self) -> Option<&dyn InlineFlowExtent> {
        Some(&self.block)
    }

    fn block_flow_participant(&self) -> Option<&dyn BlockFlowParticipant> {
        Some(&self.block)
    }

    fn block_flow_participant_mut(&mut self) -> Option<&mut dyn BlockFlowParticipant> {
        Some(&mut self.block)
    }

    fn containing_block_consumer_mut(&mut self) -> Option<&mut dyn ContainingBlockConsumer> {
        Some(&mut self.block)
    }

    fn positioning_owner(&self) -> Option<&dyn PositioningOwner> {
        Some(&self.block)
    }

    fn positioning_owner_mut(&mut self) -> Option<&mut dyn PositioningOwner> {
        Some(&mut self.block)
    }

    fn block_flow_owner(&self) -> Option<&dyn BlockFlowOwner> {
        Some(&self.block)
    }

    fn paint_group_owner(&self) -> Option<&dyn PaintGroupOwner> {
        Some(&self.block)
    }

    fn paint_group_owner_mut(&mut self) -> Option<&mut dyn PaintGroupOwner> {
        Some(&mut self.block)
    }

    fn box_paint_owner(&self) -> Option<&dyn super::BoxPaintOwner> {
        Some(&self.block)
    }

    fn table_box_decoration_owner(&self) -> Option<&dyn TableBoxDecorationOwner> {
        Some(self)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TableCells {
    pub(crate) cells: Vec<TableCell>,
    pub(crate) column_widths: Vec<f32>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TableFormatting {
    pub(crate) border_collapse: BorderCollapse,
    pub(crate) border_spacing: f32,
}

impl TableFormatting {
    pub(crate) const fn new(border_collapse: BorderCollapse, border_spacing: f32) -> Self {
        Self {
            border_collapse,
            border_spacing,
        }
    }

    pub(crate) const fn is_collapsed(self) -> bool {
        matches!(self.border_collapse, BorderCollapse::Collapse)
    }

    /// CSS Tables overrides authored table-root padding to zero in collapsed
    /// border mode. Keep the override on the formatting model so layout and
    /// paint cannot accidentally use different effective padding.
    pub(crate) const fn root_padding(self, authored: EdgeSizes) -> EdgeSizes {
        if self.is_collapsed() {
            EdgeSizes::ZERO
        } else {
            authored
        }
    }

    pub(crate) fn constrain_internal_decoration(self, paint: &mut BoxPaint) {
        if self.is_collapsed() {
            paint.border_image = None;
            paint.border_radii = CornerRadii::ZERO;
        }
    }

    pub(crate) const fn table_corner_radii(self, radii: CornerRadii) -> CornerRadii {
        if self.is_collapsed() {
            CornerRadii::ZERO
        } else {
            radii
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::computed::{BorderImage, BorderImagePaint, BorderImageSource};

    #[test]
    fn collapsed_internal_boxes_ignore_border_images_and_radii() {
        let mut paint = BoxPaint {
            border_image: Some(BorderImagePaint {
                source: BorderImageSource::Url("unused.svg".into()),
                geometry: BorderImage::default(),
            }),
            border_radii: CornerRadii::circular(12.0),
            ..Default::default()
        };

        TableFormatting::new(BorderCollapse::Collapse, 0.0)
            .constrain_internal_decoration(&mut paint);

        assert!(paint.border_image.is_none());
        assert!(paint.border_radii.is_zero());
    }

    #[test]
    fn collapsed_table_root_ignores_authored_padding() {
        let authored = EdgeSizes::new(1.0, 2.0, 3.0, 4.0);

        assert_eq!(
            TableFormatting::new(BorderCollapse::Collapse, 0.0).root_padding(authored),
            EdgeSizes::ZERO
        );
        assert_eq!(
            TableFormatting::new(BorderCollapse::Separate, 0.0).root_padding(authored),
            authored
        );
    }

    #[test]
    fn table_grid_identity_survives_cloning_and_separates_siblings() {
        let first = TableGridIdentity::from_source_path([3]);
        let first_clone = first.clone();
        let sibling = TableGridIdentity::from_source_path([4]);

        assert_eq!(first, first_clone);
        assert_ne!(first, sibling);
    }
}

/// Inline-axis geometry shared by every flattened row of one table.
///
/// The outer table box and the cell grid have different origins whenever the
/// table has padding, borders, or border spacing. Keeping both origins beside
/// the authoritative outer extent prevents sizing, painting, and print fitting
/// from reconstructing incompatible versions of the same table geometry.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TableInlineGeometry {
    box_offset: InlineOffset,
    grid_offset: InlineOffset,
    box_extent: f32,
}

impl TableInlineGeometry {
    pub(crate) const fn new(box_offset: InlineOffset, grid_offset: InlineOffset) -> Self {
        Self {
            box_offset,
            grid_offset,
            box_extent: 0.0,
        }
    }

    pub(crate) const fn with_box_extent(mut self, extent: f32) -> Self {
        self.box_extent = extent;
        self
    }

    pub(crate) const fn grid_offset(self) -> f32 {
        self.grid_offset.value()
    }

    pub(crate) const fn box_extent(self) -> f32 {
        self.box_extent
    }

    pub(crate) const fn box_end(self) -> f32 {
        self.box_offset.value() + self.box_extent
    }

    /// Rebase page-relative table geometry into its principal box.
    pub(crate) const fn relative_to(self, origin: InlineOffset) -> Self {
        Self {
            box_offset: InlineOffset::new(self.box_offset.value() - origin.value()),
            grid_offset: InlineOffset::new(self.grid_offset.value() - origin.value()),
            box_extent: self.box_extent,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TableFragmentation {
    pub(crate) repeats_as_header: bool,
    pub(crate) repeats_as_footer: bool,
    pub(crate) avoid_inside: bool,
    pub(crate) avoid_group: Option<TableFragmentGroup>,
}

/// Identity of one authored row group whose rows must move together.
///
/// A boolean cannot represent adjacency: two neighboring groups can both have
/// `break-inside: avoid` without becoming one unbreakable super-group.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TableFragmentGroup(usize);

impl TableFragmentGroup {
    pub(crate) const fn new(identity: usize) -> Self {
        Self(identity)
    }
}

/// Block-axis geometry of one row in the flattened table formatting context.
///
/// CSS margins belong to the table's outer box and may collapse with sibling
/// blocks. Grid spacing, table padding, and collapsed-border overhang are
/// internal table geometry and must never join that collapse. Keeping those
/// quantities separate prevents row rendering from treating a table inset as
/// an oversized CSS margin.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TableRowFlow {
    pub(crate) margins: BlockMargins,
    pub(crate) internal: BlockMargins,
    pub(crate) extra_end: f32,
}

impl TableRowFlow {
    pub(crate) const fn new(internal_start: f32) -> Self {
        Self {
            margins: BlockMargins::ZERO,
            internal: BlockMargins::new(internal_start, 0.0),
            extra_end: 0.0,
        }
    }

    pub(crate) const fn content_extent(self, row_height: f32) -> f32 {
        self.internal.total() + row_height + self.extra_end
    }

    pub(crate) const fn outer_extent(self, row_height: f32) -> f32 {
        self.margins.total() + self.content_extent(row_height)
    }
}

/// Stable source-tree identity shared by every row of one table grid.
///
/// Rows are flattened so pagination can fragment them independently, but table
/// backgrounds and borders still require one coordinated paint schedule. The
/// source path preserves that relationship across cloning and fragmentation
/// without reference counting or renderer-side guesses based on geometry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TableGridIdentity {
    source_path: Box<[usize]>,
}

impl TableGridIdentity {
    pub(crate) fn from_source_path(path: impl IntoIterator<Item = usize>) -> Self {
        let source_path = path.into_iter().collect::<Vec<_>>().into_boxed_slice();
        Self { source_path }
    }
}

/// Capability used by painters that coordinate all rows belonging to one
/// table grid without depending on the concrete row representation.
pub(crate) trait TableGridOwner {
    fn table_grid_identity(&self) -> &TableGridIdentity;
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TableRow {
    pub(crate) grid: TableGridIdentity,
    pub(crate) content: TableCells,
    pub(crate) flow: TableRowFlow,
    pub(crate) formatting: TableFormatting,
    pub(crate) fragmentation: TableFragmentation,
    pub(crate) inline: TableInlineGeometry,
}

impl TableGridOwner for TableRow {
    fn table_grid_identity(&self) -> &TableGridIdentity {
        &self.grid
    }
}

impl TableRow {
    /// Width of the table's outer box, including table padding, borders, and
    /// the two outer `border-spacing` edges. Every row in one table carries the
    /// same value so nested sizing and inline/flex probing never reconstruct a
    /// partial table width from cell tracks.
    pub(crate) const fn box_inline_extent(&self) -> f32 {
        self.inline.box_extent()
    }

    pub(crate) const fn grid_inline_offset(&self) -> f32 {
        self.inline.grid_offset()
    }

    pub(crate) const fn box_inline_end(&self) -> f32 {
        self.inline.box_end()
    }
}

impl MarginHolder for TableRow {
    fn margins(&self) -> &BlockMargins {
        &self.flow.margins
    }

    fn margins_mut(&mut self) -> &mut BlockMargins {
        &mut self.flow.margins
    }
}

impl InlineFlowExtent for TableRow {
    fn normal_flow_right_edge(&self) -> Option<f32> {
        let right = self.box_inline_end();
        right.is_finite().then_some(right.max(0.0))
    }
}

impl BlockFlowParticipant for TableRow {
    fn collapses_outer_margins(&self) -> bool {
        true
    }

    fn is_in_flow_block(&self) -> bool {
        true
    }
}

impl ChildContainer for TableRow {
    fn visit_layout_children(&self, visitor: &mut dyn FnMut(&dyn LayoutElement)) {
        for child in self
            .content
            .cells
            .iter()
            .flat_map(|cell| &cell.layout.content.children)
        {
            visitor(child.as_ref());
        }
    }

    fn visit_layout_children_mut(&mut self, visitor: &mut dyn FnMut(&mut dyn LayoutElement)) {
        for child in self
            .content
            .cells
            .iter_mut()
            .flat_map(|cell| &mut cell.layout.content.children)
        {
            visitor(child.as_mut());
        }
    }

    fn visit_layout_child_nodes_mut(&mut self, visitor: &mut dyn FnMut(&mut LayoutNode)) {
        for child in self
            .content
            .cells
            .iter_mut()
            .flat_map(|cell| &mut cell.layout.content.children)
        {
            visitor(child);
        }
    }
}

impl LayoutElement for TableRow {
    fn clone_box(&self) -> LayoutNode {
        Box::new(self.clone())
    }

    fn accept(&self, visitor: &mut dyn LayoutVisitor) {
        visitor.visit_table_row(self);
    }

    fn accept_mut(&mut self, visitor: &mut dyn LayoutVisitorMut) {
        visitor.visit_table_row(self);
    }

    fn margin_holder(&self) -> Option<&dyn MarginHolder> {
        Some(self)
    }

    fn margin_holder_mut(&mut self) -> Option<&mut dyn MarginHolder> {
        Some(self)
    }

    fn inline_flow_extent(&self) -> Option<&dyn InlineFlowExtent> {
        Some(self)
    }

    fn block_flow_participant(&self) -> Option<&dyn BlockFlowParticipant> {
        Some(self)
    }

    fn block_flow_participant_mut(&mut self) -> Option<&mut dyn BlockFlowParticipant> {
        Some(self)
    }

    fn table_grid_owner(&self) -> Option<&dyn TableGridOwner> {
        Some(self)
    }

    fn has_own_page_spanning_graphical_effect(&self) -> bool {
        self.content
            .cells
            .iter()
            .any(|cell| cell.layout.has_outset_graphical_effect())
    }
    fn visit_children(&self, visitor: &mut dyn FnMut(&dyn LayoutElement)) {
        self.visit_layout_children(visitor);
    }

    fn visit_children_mut(&mut self, visitor: &mut dyn FnMut(&mut dyn LayoutElement)) {
        self.visit_layout_children_mut(visitor);
    }

    fn visit_child_nodes_mut(&mut self, visitor: &mut dyn FnMut(&mut LayoutNode)) {
        self.visit_layout_child_nodes_mut(visitor);
    }
}
