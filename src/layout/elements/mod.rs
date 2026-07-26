//! Concrete nodes in the post-layout tree.
//!
//! A layout node is behavior, not a tagged bag of optional fields. Concrete
//! structs own their data, capability traits expose shared behavior, and the
//! object-safe visitor is the one intentional dispatch boundary.

mod container;
mod flex;
mod fragmentation;
mod grid;
mod media;
mod metadata;
mod misc;
mod multicol;
mod principal;
mod properties;
mod stacking;
mod table;
#[cfg(test)]
pub(crate) mod test_support;
mod text;

pub(crate) use container::Container;
pub(crate) use flex::{FlexContent, FlexRow};
pub(crate) use fragmentation::*;
pub(crate) use grid::{GridContent, GridRow, GridRowStartSpace};
pub(crate) use media::{
    Image, ImagePaint, ImageSampling, ReplacedContent, ReplacedFragment, ReplacedGeometry, Svg,
    SvgPaint,
};
pub(crate) use metadata::{AvoidPageBreak, NamedString, PageBreak, RunningElement};
pub(crate) use misc::{ColumnRule, HorizontalRule, MathBlock, ProgressBar, ProgressColors};
pub(crate) use multicol::{MulticolColumn, MulticolContainer};
pub(crate) use principal::{PrincipalBox, impl_principal_layout_element};
pub(crate) use properties::*;
pub(crate) use stacking::{Stacking, StackingLevel, StackingParticipant, StackingRole};
pub(crate) use table::{
    Table, TableBoxDecoration, TableBoxDecorationOwner, TableCells, TableFormatting,
    TableFragmentGroup, TableFragmentation, TableGridIdentity, TableGridOwner, TableInlineGeometry,
    TableRow, TableRowFlow,
};
#[cfg(test)]
pub(crate) use test_support::{LayoutElementTestExt, LayoutElementTestMutExt};
pub(crate) use text::{BackgroundBoxGeometry, TextBlock};

use std::fmt::Debug;

/// A concrete node that can live in the heterogeneous layout tree.
pub(crate) trait LayoutElement: Debug {
    fn clone_box(&self) -> LayoutNode;
    fn accept(&self, visitor: &mut dyn LayoutVisitor);
    fn accept_mut(&mut self, visitor: &mut dyn LayoutVisitorMut);

    fn visit_children(&self, _visitor: &mut dyn FnMut(&dyn LayoutElement)) {}
    fn visit_children_mut(&mut self, _visitor: &mut dyn FnMut(&mut dyn LayoutElement)) {}
    fn visit_child_nodes_mut(&mut self, _visitor: &mut dyn FnMut(&mut LayoutNode)) {}

    /// Return this node's block-flow margin capability. Structural metadata
    /// returns `None`; every actual margin-owning box returns itself.
    fn margin_holder(&self) -> Option<&dyn crate::layout::flow_metrics::MarginHolder> {
        None
    }

    fn margin_holder_mut(&mut self) -> Option<&mut dyn crate::layout::flow_metrics::MarginHolder> {
        None
    }

    fn replaced_element_mut(&mut self) -> Option<&mut dyn ReplacedElement> {
        None
    }

    fn inline_flow_extent(&self) -> Option<&dyn InlineFlowExtent> {
        None
    }

    fn atomic_inline_baseline(&self) -> Option<&dyn AtomicInlineBaseline> {
        None
    }

    fn block_flow_participant(&self) -> Option<&dyn BlockFlowParticipant> {
        None
    }

    fn block_flow_participant_mut(&mut self) -> Option<&mut dyn BlockFlowParticipant> {
        None
    }

    fn containing_block_consumer_mut(&mut self) -> Option<&mut dyn ContainingBlockConsumer> {
        None
    }

    fn positioning_owner(&self) -> Option<&dyn PositioningOwner> {
        None
    }

    fn positioning_owner_mut(&mut self) -> Option<&mut dyn PositioningOwner> {
        None
    }

    fn block_flow_owner(&self) -> Option<&dyn BlockFlowOwner> {
        None
    }

    fn paint_group_owner(&self) -> Option<&dyn PaintGroupOwner> {
        None
    }

    fn paint_group_owner_mut(&mut self) -> Option<&mut dyn PaintGroupOwner> {
        None
    }

    /// Canonical visual decoration owned by this principal box.
    ///
    /// Replaced content can own a paint group without owning ordinary CSS box
    /// decoration, so this capability remains distinct from
    /// [`PaintGroupOwner`].
    fn box_paint_owner(&self) -> Option<&dyn BoxPaintOwner> {
        None
    }

    /// A box whose recursive in-flow renderer can paint decoration and
    /// contents in separate CSS stacking phases.
    fn in_flow_paint_phase_owner(&self) -> Option<&dyn BoxPaintOwner> {
        None
    }

    fn block_fragmentation_source(&self) -> Option<&dyn BlockFragmentationSource> {
        None
    }

    fn fragment_start_spacing_mut(&mut self) -> Option<&mut dyn FragmentStartSpacing> {
        None
    }

    /// Suppress formatting-context spacing at the start of this fragment.
    ///
    /// Wrapper boxes forward the transition through their first child only:
    /// fragmentation preserves the nested formatting hierarchy, and a gutter
    /// later in the child list is not at the fragment boundary. Concrete boxes
    /// that own suppressible spacing expose [`FragmentStartSpacing`] above.
    fn suppress_first_fragment_spacing(&mut self) {
        if let Some(spacing) = self.fragment_start_spacing_mut() {
            spacing.suppress_at_fragment_start();
            return;
        }
        let mut is_first_child = true;
        self.visit_child_nodes_mut(&mut |child| {
            if is_first_child {
                is_first_child = false;
                child.suppress_first_fragment_spacing();
            }
        });
    }

    fn box_fragmentation_owner(&self) -> Option<&dyn BoxFragmentationOwner> {
        None
    }

    fn box_fragmentation_owner_mut(&mut self) -> Option<&mut dyn BoxFragmentationOwner> {
        None
    }

    /// Outer block extent exposed by this node's fragmentation capability.
    ///
    /// Keeping margin composition at the capability boundary prevents each
    /// fragmentainer from rediscovering a concrete box type's metric layout.
    fn fragmentable_outer_block_extent(&self) -> Option<f32> {
        let extent = self.block_fragmentation_source()?.block_extent();
        Some(
            extent
                + self
                    .margin_holder()
                    .map_or(0.0, |holder| holder.margins().total()),
        )
    }

    fn filter_holder_mut(&mut self) -> Option<&mut dyn FilterHolder> {
        self.paint_group_owner_mut()
            .map(|owner| owner.paint_group_mut() as &mut dyn FilterHolder)
    }

    fn table_box_decoration_owner(&self) -> Option<&dyn TableBoxDecorationOwner> {
        None
    }

    /// Identity of the table grid whose rows participate in one coordinated
    /// background/border/content paint schedule.
    fn table_grid_owner(&self) -> Option<&dyn TableGridOwner> {
        None
    }

    /// Return a source whose complete filtered output has an exact vector
    /// representation. Most nodes need offscreen SourceGraphic compositing;
    /// concrete leaf boxes opt in only for filters they can preserve exactly.
    fn exact_vector_filter_source(
        &self,
    ) -> Option<&dyn crate::layout::filter::ExactVectorFilterSource> {
        None
    }

    /// Whether this subtree has graphical output that can cross a page edge
    /// after fragmentation has chosen its box fragments.
    ///
    /// CSS Fragmentation applies transforms and other graphical effects per
    /// fragment, but separates page boxes only after painting. Keeping this
    /// query recursive at the layout-element boundary prevents pagination from
    /// enumerating concrete descendants or silently missing a deeper effect.
    fn has_own_page_spanning_graphical_effect(&self) -> bool {
        let own_group_transform = self
            .paint_group_owner()
            .is_some_and(|owner| owner.paint_group().transform.establishes_stacking_context());
        own_group_transform
            || self
                .box_paint_owner()
                .is_some_and(|owner| owner.box_paint().has_outset_graphical_effect())
    }

    fn has_page_spanning_graphical_effect(&self) -> bool {
        if self.has_own_page_spanning_graphical_effect() {
            return true;
        }

        let mut descendant_has_effect = false;
        self.visit_children(&mut |child| {
            descendant_has_effect |= child.has_page_spanning_graphical_effect();
        });
        descendant_has_effect
    }

    /// Whether this node contributes paint only, without creating duplicate
    /// document semantics or influencing page-local flow corrections.
    fn is_page_paint_continuation(&self) -> bool {
        false
    }

    /// Whether this node advances normal flow. Positioned boxes derive this
    /// from their canonical positioning state; paint-only structural nodes may
    /// override it without inventing synthetic positioning coordinates.
    fn contributes_to_normal_flow(&self) -> bool {
        self.positioning_owner()
            .is_none_or(|owner| owner.positioning().is_in_normal_flow())
    }

    /// How this node participates in fragmentainer page retention and forced
    /// break sequencing. Most nodes are ordinary main-flow content.
    fn page_content_role(&self) -> PageContentRole {
        self.positioning_owner()
            .map_or(PageContentRole::MainFlow, |owner| {
                PageContentRole::MainFlow.for_position(owner.positioning().scheme)
            })
    }
}

pub(crate) type LayoutNode = Box<dyn LayoutElement>;

impl Clone for LayoutNode {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

// Preserve transparent ownership: generic algorithms can accept either a
// borrowed concrete node or a borrowed boxed node without learning where the
// heterogeneous tree stores its allocation boundary.
impl LayoutElement for LayoutNode {
    fn clone_box(&self) -> LayoutNode {
        self.as_ref().clone_box()
    }

    fn accept(&self, visitor: &mut dyn LayoutVisitor) {
        self.as_ref().accept(visitor);
    }

    fn accept_mut(&mut self, visitor: &mut dyn LayoutVisitorMut) {
        self.as_mut().accept_mut(visitor);
    }

    fn visit_children(&self, visitor: &mut dyn FnMut(&dyn LayoutElement)) {
        self.as_ref().visit_children(visitor);
    }

    fn visit_children_mut(&mut self, visitor: &mut dyn FnMut(&mut dyn LayoutElement)) {
        self.as_mut().visit_children_mut(visitor);
    }

    fn visit_child_nodes_mut(&mut self, visitor: &mut dyn FnMut(&mut LayoutNode)) {
        self.as_mut().visit_child_nodes_mut(visitor);
    }

    fn margin_holder(&self) -> Option<&dyn crate::layout::flow_metrics::MarginHolder> {
        self.as_ref().margin_holder()
    }

    fn margin_holder_mut(&mut self) -> Option<&mut dyn crate::layout::flow_metrics::MarginHolder> {
        self.as_mut().margin_holder_mut()
    }

    fn replaced_element_mut(&mut self) -> Option<&mut dyn ReplacedElement> {
        self.as_mut().replaced_element_mut()
    }

    fn inline_flow_extent(&self) -> Option<&dyn InlineFlowExtent> {
        self.as_ref().inline_flow_extent()
    }

    fn atomic_inline_baseline(&self) -> Option<&dyn AtomicInlineBaseline> {
        self.as_ref().atomic_inline_baseline()
    }

    fn block_flow_participant(&self) -> Option<&dyn BlockFlowParticipant> {
        self.as_ref().block_flow_participant()
    }

    fn block_flow_participant_mut(&mut self) -> Option<&mut dyn BlockFlowParticipant> {
        self.as_mut().block_flow_participant_mut()
    }

    fn containing_block_consumer_mut(&mut self) -> Option<&mut dyn ContainingBlockConsumer> {
        self.as_mut().containing_block_consumer_mut()
    }

    fn positioning_owner(&self) -> Option<&dyn PositioningOwner> {
        self.as_ref().positioning_owner()
    }

    fn positioning_owner_mut(&mut self) -> Option<&mut dyn PositioningOwner> {
        self.as_mut().positioning_owner_mut()
    }

    fn block_flow_owner(&self) -> Option<&dyn BlockFlowOwner> {
        self.as_ref().block_flow_owner()
    }

    fn paint_group_owner(&self) -> Option<&dyn PaintGroupOwner> {
        self.as_ref().paint_group_owner()
    }

    fn paint_group_owner_mut(&mut self) -> Option<&mut dyn PaintGroupOwner> {
        self.as_mut().paint_group_owner_mut()
    }

    fn box_paint_owner(&self) -> Option<&dyn BoxPaintOwner> {
        self.as_ref().box_paint_owner()
    }

    fn in_flow_paint_phase_owner(&self) -> Option<&dyn BoxPaintOwner> {
        self.as_ref().in_flow_paint_phase_owner()
    }

    fn block_fragmentation_source(&self) -> Option<&dyn BlockFragmentationSource> {
        self.as_ref().block_fragmentation_source()
    }

    fn fragment_start_spacing_mut(&mut self) -> Option<&mut dyn FragmentStartSpacing> {
        self.as_mut().fragment_start_spacing_mut()
    }

    fn box_fragmentation_owner(&self) -> Option<&dyn BoxFragmentationOwner> {
        self.as_ref().box_fragmentation_owner()
    }

    fn box_fragmentation_owner_mut(&mut self) -> Option<&mut dyn BoxFragmentationOwner> {
        self.as_mut().box_fragmentation_owner_mut()
    }

    fn filter_holder_mut(&mut self) -> Option<&mut dyn FilterHolder> {
        self.as_mut().filter_holder_mut()
    }

    fn table_box_decoration_owner(&self) -> Option<&dyn TableBoxDecorationOwner> {
        self.as_ref().table_box_decoration_owner()
    }

    fn exact_vector_filter_source(
        &self,
    ) -> Option<&dyn crate::layout::filter::ExactVectorFilterSource> {
        self.as_ref().exact_vector_filter_source()
    }

    fn has_page_spanning_graphical_effect(&self) -> bool {
        self.as_ref().has_page_spanning_graphical_effect()
    }

    fn has_own_page_spanning_graphical_effect(&self) -> bool {
        self.as_ref().has_own_page_spanning_graphical_effect()
    }

    fn is_page_paint_continuation(&self) -> bool {
        self.as_ref().is_page_paint_continuation()
    }

    fn contributes_to_normal_flow(&self) -> bool {
        self.as_ref().contributes_to_normal_flow()
    }

    fn page_content_role(&self) -> PageContentRole {
        self.as_ref().page_content_role()
    }
}

pub(crate) trait IntoLayoutNode: LayoutElement + Sized + 'static {
    fn boxed(self) -> LayoutNode {
        Box::new(self)
    }
}

impl<T> IntoLayoutNode for T where T: LayoutElement + Sized + 'static {}

/// Shared ownership of recursive layout children.
pub(crate) trait ChildContainer {
    fn visit_layout_children(&self, visitor: &mut dyn FnMut(&dyn LayoutElement));
    fn visit_layout_children_mut(&mut self, visitor: &mut dyn FnMut(&mut dyn LayoutElement));
    fn visit_layout_child_nodes_mut(&mut self, visitor: &mut dyn FnMut(&mut LayoutNode));
}

/// Behavior shared by raster and vector replaced elements.
pub(crate) trait ReplacedElement {
    fn geometry_mut(&mut self) -> &mut ReplacedGeometry;

    fn add_baseline_gap(&mut self, gap: f32) {
        self.geometry_mut().flow.extra_end += gap;
    }
}

/// A node that contributes a right edge to normal-flow print fitting.
pub(crate) trait InlineFlowExtent {
    fn normal_flow_right_edge(&self) -> Option<f32>;

    /// Max-content outer extent used when a containing formatting context is
    /// probed before its final available inline size is known.
    ///
    /// Most atomic boxes use their normal-flow extent directly. Structural
    /// containers override this when their provisional used width must not
    /// hide a narrower intrinsic descendant contribution.
    fn max_content_outer_extent(&self) -> Option<f32> {
        self.normal_flow_right_edge()
    }
}

/// Baseline contributed by an atomic inline-level layout node.
///
/// The offset is measured from the node's outer block-start edge. Replaced
/// elements use their block-end margin edge, allowing a mixed line to align
/// neighboring text without knowing the concrete media node type.
pub(crate) trait AtomicInlineBaseline {
    fn baseline_offset(&self) -> f32;
}

/// Margin-collapse and in-flow behavior shared by block-level boxes.
pub(crate) trait BlockFlowParticipant: crate::layout::flow_metrics::MarginHolder {
    fn collapses_outer_margins(&self) -> bool;
    fn is_in_flow_block(&self) -> bool;
}

/// A positioned node that can consume a containing block after flattened
/// children are attached to their structural parent.
pub(crate) trait ContainingBlockConsumer {
    fn attach_missing_containing_block(
        &mut self,
        containing_block: crate::layout::engine::ContainingBlock,
    );
}

/// Access to physical positioning shared by independently shaped node types.
pub(crate) trait PositioningOwner {
    fn positioning(&self) -> &Positioning;
    fn positioning_mut(&mut self) -> &mut Positioning;
}

/// Complete post-layout paint-group ownership.
///
/// Every ordinary box and formatting-context component exposes transforms and
/// post-compositing effects through this one capability. Renderers may supply
/// geometry, but may not propagate individual transform/mask/opacity fields.
pub(crate) trait PaintGroupOwner {
    fn paint_group(&self) -> &PaintGroup;
    fn paint_group_mut(&mut self) -> &mut PaintGroup;
}

/// Ownership of the canonical paint state shared by ordinary CSS boxes.
///
/// This is the single capability for backgrounds, borders, shadows, outlines,
/// filters, and their enclosing paint group. Algorithms interested in one of
/// those families should ask for this structure instead of adding a concrete
/// node visitor.
pub(crate) trait BoxPaintOwner {
    fn box_paint(&self) -> &BoxPaint;
    fn box_paint_mut(&mut self) -> &mut BoxPaint;

    /// Whether this box can expose its decoration and descendant-content paint
    /// as separate stacking fragments without changing group compositing.
    ///
    /// CSS paints in-flow block decorations below inline-level content. A
    /// transform, opacity/mask group, flattened background, or retained filter
    /// must remain atomic, while an ordinary box can participate in those two
    /// phases at any depth in the layout tree.
    fn supports_phased_paint(&self) -> bool {
        let paint = self.box_paint();
        paint.group.is_identity()
            && paint.background.layers.blur_radius == 0.0
            && paint.background.layers.clip != crate::style::computed::BackgroundClip::Text
            && paint.group.filter.is_none()
    }
}

impl<T: BoxPaintOwner + ?Sized> PaintGroupOwner for T {
    fn paint_group(&self) -> &PaintGroup {
        &self.box_paint().group
    }

    fn paint_group_mut(&mut self) -> &mut PaintGroup {
        &mut self.box_paint_mut().group
    }
}

/// A principal box that exposes legal block-axis fragmentation boundaries.
///
/// Fragmentainers ask for a boundary in source coordinates; they never infer
/// line or descendant structure from a concrete node type. `Emergency` permits
/// the CSS Fragmentation fallback that relaxes widows/orphans when no compliant
/// break can make progress.
pub(crate) trait BlockFragmentationSource {
    /// Visible block-axis extent available to fragmentainers. This can exceed
    /// the principal box's used flow size when `overflow: visible` descendants
    /// extend beyond a definite height.
    fn block_extent(&self) -> f32;

    fn find_block_break(&self, query: FragmentBreakQuery) -> Option<f32>;
}

/// Ownership of the box model and reference-box state required to fragment a
/// decorated principal box without special-casing its concrete node type.
pub(crate) trait BoxFragmentationOwner {
    fn fragmentation_box_model(&self) -> &BoxModel;
    fn box_fragmentation(&self) -> &BoxFragmentation;
    fn box_fragmentation_mut(&mut self) -> &mut BoxFragmentation;
}

/// Formatting-context spacing that is present between siblings but disappears
/// when the later sibling begins a new fragmentainer.
pub(crate) trait FragmentStartSpacing {
    fn suppress_at_fragment_start(&mut self);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FragmentBreakRule {
    Normal,
    Emergency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FragmentBreakDirection {
    LatestBefore,
    EarliestAfter,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum FragmentBreakScope {
    /// Consider class-A boundaries between block-level descendants. Balanced
    /// multicol prefers these before introducing additional line splits.
    BlockBoundaries,
    /// Consider every legal boundary exposed by the formatting context.
    #[default]
    All,
}

/// A source-coordinate query for a legal fragmentation boundary.
///
/// Grouping the consumed offset, target, rule relaxation, and search direction
/// keeps every fragmentable node on the same boundary-selection contract.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FragmentBreakQuery {
    consumed: f32,
    target: f32,
    rule: FragmentBreakRule,
    direction: FragmentBreakDirection,
    scope: FragmentBreakScope,
}

impl FragmentBreakQuery {
    pub(crate) const fn latest_before(consumed: f32, limit: f32, rule: FragmentBreakRule) -> Self {
        Self {
            consumed,
            target: limit,
            rule,
            direction: FragmentBreakDirection::LatestBefore,
            scope: FragmentBreakScope::All,
        }
    }

    pub(crate) const fn earliest_after(
        consumed: f32,
        minimum: f32,
        rule: FragmentBreakRule,
    ) -> Self {
        Self {
            consumed,
            target: minimum,
            rule,
            direction: FragmentBreakDirection::EarliestAfter,
            scope: FragmentBreakScope::All,
        }
    }

    pub(crate) const fn block_boundaries_only(mut self) -> Self {
        self.scope = FragmentBreakScope::BlockBoundaries;
        self
    }

    fn translated(self, offset: f32) -> Self {
        Self {
            consumed: (self.consumed - offset).max(0.0),
            target: self.target - offset,
            ..self
        }
    }

    fn permits(self, honors_constraints: bool) -> bool {
        matches!(self.rule, FragmentBreakRule::Emergency) || honors_constraints
    }

    fn accepts(self, candidate: f32) -> bool {
        use crate::layout::roundoff::exceeds_with_roundoff;

        if !exceeds_with_roundoff(candidate, self.consumed) {
            return false;
        }
        match self.direction {
            FragmentBreakDirection::LatestBefore => !exceeds_with_roundoff(candidate, self.target),
            FragmentBreakDirection::EarliestAfter => !exceeds_with_roundoff(self.target, candidate),
        }
    }

    fn select(self, current: Option<f32>, candidate: f32) -> Option<f32> {
        if !self.accepts(candidate) {
            return current;
        }
        Some(match (self.direction, current) {
            (FragmentBreakDirection::LatestBefore, Some(current)) => current.max(candidate),
            (FragmentBreakDirection::EarliestAfter, Some(current)) => current.min(candidate),
            (_, None) => candidate,
        })
    }
}

/// Access to float and clearance behavior on ordinary block boxes.
pub(crate) trait BlockFlowOwner {
    fn block_flow(&self) -> &BlockFlow;
}

/// A node's two independent pagination responsibilities.
///
/// Overflow continuations paint real content and therefore retain a page, but
/// they do not interrupt a sequence of forced breaks in the surrounding main
/// flow. Repeated decorations do neither. Keeping these states distinct avoids
/// dropping a final overflow fragment merely to preserve break coalescing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum PageContentRole {
    #[default]
    MainFlow,
    OverflowContinuation,
    RepeatedDecoration,
}

impl PageContentRole {
    pub(crate) const fn for_position(self, position: crate::style::computed::Position) -> Self {
        if matches!(position, crate::style::computed::Position::Fixed) {
            Self::RepeatedDecoration
        } else {
            self
        }
    }

    pub(crate) const fn interrupts_forced_break_sequence(self) -> bool {
        matches!(self, Self::MainFlow)
    }

    pub(crate) const fn retains_page(self) -> bool {
        !matches!(self, Self::RepeatedDecoration)
    }
}

/// Exhaustive operation boundary for immutable node-specific behavior.
pub(crate) trait LayoutVisitor {
    fn visit_avoid_page_break(&mut self, _element: &AvoidPageBreak) {}
    fn visit_column_rule(&mut self, _element: &ColumnRule) {}
    fn visit_text_block(&mut self, _element: &TextBlock) {}
    fn visit_table(&mut self, element: &Table) {
        self.visit_container(&element.principal);
    }
    fn visit_table_row(&mut self, _element: &TableRow) {}
    fn visit_grid_row(&mut self, _element: &GridRow) {}
    fn visit_image(&mut self, _element: &Image) {}
    fn visit_horizontal_rule(&mut self, _element: &HorizontalRule) {}
    fn visit_svg(&mut self, _element: &Svg) {}
    fn visit_flex_row(&mut self, _element: &FlexRow) {}
    fn visit_progress_bar(&mut self, _element: &ProgressBar) {}
    fn visit_math_block(&mut self, _element: &MathBlock) {}
    fn visit_container(&mut self, _element: &Container) {}
    fn visit_multicol_container(&mut self, element: &MulticolContainer) {
        self.visit_container(&element.principal);
    }
    fn visit_multicol_column(&mut self, element: &MulticolColumn) {
        self.visit_container(&element.principal);
    }
    fn visit_running_element(&mut self, _element: &RunningElement) {}
    fn visit_named_string(&mut self, _element: &NamedString) {}
    fn visit_page_break(&mut self, _element: &PageBreak) {}
}

/// Exhaustive operation boundary for mutable node-specific behavior.
pub(crate) trait LayoutVisitorMut {
    fn visit_avoid_page_break(&mut self, _element: &mut AvoidPageBreak) {}
    fn visit_column_rule(&mut self, _element: &mut ColumnRule) {}
    fn visit_text_block(&mut self, _element: &mut TextBlock) {}
    fn visit_table(&mut self, element: &mut Table) {
        self.visit_container(&mut element.principal);
    }
    fn visit_table_row(&mut self, _element: &mut TableRow) {}
    fn visit_grid_row(&mut self, _element: &mut GridRow) {}
    fn visit_image(&mut self, _element: &mut Image) {}
    fn visit_horizontal_rule(&mut self, _element: &mut HorizontalRule) {}
    fn visit_svg(&mut self, _element: &mut Svg) {}
    fn visit_flex_row(&mut self, _element: &mut FlexRow) {}
    fn visit_progress_bar(&mut self, _element: &mut ProgressBar) {}
    fn visit_math_block(&mut self, _element: &mut MathBlock) {}
    fn visit_container(&mut self, _element: &mut Container) {}
    fn visit_multicol_container(&mut self, element: &mut MulticolContainer) {
        self.visit_container(&mut element.principal);
    }
    fn visit_multicol_column(&mut self, element: &mut MulticolColumn) {
        self.visit_container(&mut element.principal);
    }
    fn visit_running_element(&mut self, _element: &mut RunningElement) {}
    fn visit_named_string(&mut self, _element: &mut NamedString) {}
    fn visit_page_break(&mut self, _element: &mut PageBreak) {}
}

/// Depth-first traversal shared by every tree-wide operation.
pub(crate) fn visit_layout_tree(element: &dyn LayoutElement, visitor: &mut dyn LayoutVisitor) {
    element.accept(visitor);
    element.visit_children(&mut |child| visit_layout_tree(child, visitor));
}

/// Mutable depth-first traversal shared by every tree-wide rewrite.
pub(crate) fn visit_layout_tree_mut(
    element: &mut dyn LayoutElement,
    visitor: &mut dyn LayoutVisitorMut,
) {
    element.accept_mut(visitor);
    element.visit_children_mut(&mut |child| visit_layout_tree_mut(child, visitor));
}
