//! Reusable semantic properties carried by concrete layout nodes.

use crate::layout::engine::{ContainingBlock, LayoutBorder, StackingContext};
use crate::layout::flow_metrics::BlockMargins;
use crate::layout::helpers::BackgroundFields;
use crate::style::computed::{
    BlendMode, BoxDecorationBreak, BoxShadow, Clear, ClipPath, Float, Isolation, MaskMode,
    MaskSource, Overflow, Position, TextAlign, Transform, TransformOrigin, Visibility, WritingMode,
};
use crate::types::{Color, CornerRadii, EdgeSizes, Rect};

/// Inline-axis sizing carried by a laid-out box.
///
/// A box either fills the inline space offered by its containing layout frame,
/// or carries a fixed used width resolved by layout. Keeping that distinction
/// out of a raw `Option<f32>` prevents authored `width: auto` from being
/// confused with unresolved geometry.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct InlineSize(Option<f32>);

impl InlineSize {
    pub(crate) const FILL_AVAILABLE: Self = Self(None);

    pub(crate) const fn fixed(width: f32) -> Self {
        Self(Some(width))
    }

    pub(crate) fn from_fixed_value(width: Option<f32>) -> Self {
        width.map_or(Self::FILL_AVAILABLE, Self::fixed)
    }

    pub(crate) fn from_used(width: f32, available_width: f32, fixed_by_layout: bool) -> Self {
        if fixed_by_layout || width != available_width {
            Self::fixed(width)
        } else {
            Self::FILL_AVAILABLE
        }
    }

    pub(crate) const fn fixed_value(self) -> Option<f32> {
        self.0
    }

    pub(crate) const fn is_fill_available(self) -> bool {
        self.0.is_none()
    }

    pub(crate) fn resolve(self, available_width: f32) -> f32 {
        self.0.unwrap_or(available_width)
    }
}

/// Minimum and maximum constraints for one physical axis.
///
/// CSS sizing does not define these as a conventional ordered range: an
/// authored minimum may exceed the maximum, in which case the minimum wins.
/// Keeping the pair together makes the required `max(minimum, min(size,
/// maximum))` ordering impossible to reverse at individual layout call sites.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct SizeConstraints {
    minimum: Option<f32>,
    maximum: Option<f32>,
}

impl SizeConstraints {
    pub(crate) const fn new(minimum: Option<f32>, maximum: Option<f32>) -> Self {
        Self { minimum, maximum }
    }

    pub(crate) fn map(self, mut convert: impl FnMut(f32) -> f32) -> Self {
        Self::new(self.minimum.map(&mut convert), self.maximum.map(convert))
    }

    pub(crate) fn constrain(self, size: f32) -> f32 {
        let capped = self.maximum.map_or(size, |maximum| size.min(maximum));
        self.minimum.map_or(capped, |minimum| capped.max(minimum))
    }

    pub(crate) const fn minimum(self) -> Option<f32> {
        self.minimum
    }

    pub(crate) const fn maximum(self) -> Option<f32> {
        self.maximum
    }

    /// Constrain a definite preferred size, or expose an authored minimum as
    /// the initial used floor for an otherwise automatic size. A lone maximum
    /// cannot determine an automatic size before content has been measured.
    pub(crate) fn constrain_preferred(self, preferred: Option<f32>) -> Option<f32> {
        preferred.map(|size| self.constrain(size)).or(self.minimum)
    }
}

/// A laid-out block-axis size without conflating an already-resolved used
/// extent with an authored definite height.
///
/// Auto boxes can acquire a used extent from `min-height` or fragmentation and
/// still remain fragmentable. Definite boxes instead establish a hard extent;
/// overflowing descendants do not enlarge or split that principal box.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct BlockSize {
    used: Option<f32>,
    definite: bool,
}

impl BlockSize {
    pub(crate) const AUTO: Self = Self {
        used: None,
        definite: false,
    };

    pub(crate) const fn definite(used: f32) -> Self {
        Self {
            used: Some(used),
            definite: true,
        }
    }

    pub(crate) const fn from_definite(used: Option<f32>) -> Self {
        match used {
            Some(used) => Self::definite(used),
            None => Self::AUTO,
        }
    }

    /// Record a used extent whose final size may still grow with its contents.
    ///
    /// This is the shared representation for intrinsic auto sizing,
    /// `min-height`, and fragment geometry: all three supply a real used floor
    /// without turning the box into a fixed-height overflow container.
    pub(crate) const fn content_dependent(used: f32) -> Self {
        Self {
            used: Some(used),
            definite: false,
        }
    }

    pub(crate) const fn minimum(used: f32) -> Self {
        Self::content_dependent(used)
    }

    /// Record the used extent of one slice without turning the source box into
    /// a fixed-height overflow container.
    pub(crate) const fn fragment(used: f32) -> Self {
        Self::content_dependent(used)
    }

    pub(crate) const fn used(self) -> Option<f32> {
        self.used
    }

    pub(crate) const fn is_definite(self) -> bool {
        self.definite
    }

    pub(crate) fn resolve(self, content_height: f32) -> f32 {
        match (self.used, self.definite) {
            (Some(used), true) => used,
            (Some(minimum), false) => content_height.max(minimum),
            (None, _) => content_height,
        }
    }

    /// Resolve the authored block-axis constraints against a natural border
    /// box without losing whether the resulting extent is a floor or a hard
    /// cap. `min-height` remains fragmentable; an explicit `height`, or a
    /// `max-height` that actually clamps content, is definite.
    pub(crate) fn from_style(
        style: &crate::style::computed::ComputedStyle,
        natural_border_box_height: f32,
    ) -> Self {
        let edges = style.padding.vertical() + style.border.vertical_width();
        let border_box = |height: f32| match style.box_sizing {
            crate::style::computed::BoxSizing::BorderBox => height.max(0.0),
            crate::style::computed::BoxSizing::ContentBox => height.max(0.0) + edges,
        };
        let constraints = SizeConstraints::new(style.min_height, style.max_height).map(border_box);

        if let Some(height) = style.height {
            return Self::definite(constraints.constrain(border_box(height)));
        }

        let used = constraints.constrain(natural_border_box_height);
        if used < natural_border_box_height {
            return Self::definite(used);
        }

        Self::content_dependent(used)
    }
}

/// Physical dimensions of a laid-out border box. Inline and block sizing both
/// retain the constraint semantics needed by pagination.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct LayoutSize {
    pub(crate) width: InlineSize,
    pub(crate) height: BlockSize,
}

impl LayoutSize {
    /// A fixed inline extent paired with explicit block-axis sizing semantics.
    ///
    /// This is the preferred constructor for layout modes that compute an
    /// auto/minimum fragment height: their used extent must not accidentally
    /// become a definite CSS height merely because layout has measured it.
    pub(crate) const fn fixed_inline(width: f32, height: BlockSize) -> Self {
        Self {
            width: InlineSize::fixed(width),
            height,
        }
    }

    pub(crate) const fn fixed(width: f32, height: Option<f32>) -> Self {
        Self::fixed_inline(width, BlockSize::from_definite(height))
    }

    pub(crate) fn resolve_width(self, available_width: f32) -> f32 {
        self.width.resolve(available_width)
    }
}

/// Geometry shared by ordinary CSS boxes.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct BoxModel {
    pub(crate) size: LayoutSize,
    pub(crate) margins: BlockMargins,
    pub(crate) padding: EdgeSizes,
    pub(crate) border: LayoutBorder,
}

/// Original edge geometry of the reference box shared by every fragment.
///
/// Individual fragments remove edges adjoining a break, while percentage
/// shapes and image positioning for `box-decoration-break: slice` resolve
/// against the reassembled box with its authored border and padding restored.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct FragmentReferenceEdges {
    border: EdgeSizes,
    padding: EdgeSizes,
}

impl FragmentReferenceEdges {
    pub(crate) fn from_box_model(box_model: &BoxModel) -> Self {
        Self {
            border: box_model.border.widths(),
            padding: box_model.padding,
        }
    }

    pub(crate) const fn border(self) -> EdgeSizes {
        self.border
    }

    pub(crate) const fn padding(self) -> EdgeSizes {
        self.padding
    }
}

/// Position of one fragment inside the composite reference box required by
/// CSS Break 3 for `box-decoration-break: slice`.
///
/// This is shared by backgrounds, masks, and shape reference boxes so a
/// fragmented element cannot resolve those graphical effects against
/// competing geometries.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BoxFragmentSlice {
    block_offset: f32,
    composite_block_size: f32,
    edges: FragmentReferenceEdges,
}

impl BoxFragmentSlice {
    pub(crate) fn split(
        first_block_size: f32,
        continuation_block_size: f32,
        box_model: &BoxModel,
    ) -> (Self, Self) {
        let composite_block_size = first_block_size + continuation_block_size;
        let edges = FragmentReferenceEdges::from_box_model(box_model);
        (
            Self {
                block_offset: 0.0,
                composite_block_size,
                edges,
            },
            Self {
                block_offset: first_block_size,
                composite_block_size,
                edges,
            },
        )
    }

    pub(crate) const fn block_offset(self) -> f32 {
        self.block_offset
    }

    pub(crate) const fn composite_block_size(self) -> f32 {
        self.composite_block_size
    }

    pub(crate) const fn edges(self) -> FragmentReferenceEdges {
        self.edges
    }

    /// Split an already-sliced continuation without losing its position in the
    /// original reference box. The first result keeps this fragment's current
    /// offset; the continuation advances by the consumed border-box extent.
    const fn split_continuation(self, first_block_size: f32) -> (Self, Self) {
        (
            self,
            Self {
                block_offset: self.block_offset + first_block_size,
                ..self
            },
        )
    }
}

impl BoxModel {
    pub(crate) fn from_style(
        style: &crate::style::computed::ComputedStyle,
        margins: BlockMargins,
    ) -> Self {
        Self {
            size: LayoutSize {
                width: InlineSize::from_fixed_value(style.width),
                height: BlockSize::from_definite(style.height),
            },
            margins,
            padding: style.padding,
            border: LayoutBorder::from_computed(&style.border, style.color),
        }
    }
}

/// Resolved background layers and their local compositing rule.
#[derive(Debug, Clone)]
pub(crate) struct BackgroundPaint {
    pub(crate) color: Option<Color>,
    pub(crate) layers: BackgroundFields,
    pub(crate) blend_mode: BlendMode,
}

impl Default for BackgroundPaint {
    fn default() -> Self {
        Self {
            color: None,
            layers: BackgroundFields::default(),
            blend_mode: BlendMode::Normal,
        }
    }
}

impl BackgroundPaint {
    pub(crate) fn from_style(style: &crate::style::computed::ComputedStyle) -> Self {
        Self {
            color: style.background_color,
            layers: BackgroundFields::from_style(style),
            blend_mode: style.background_blend_mode,
        }
    }
}

/// Paint outside the border edge.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct OutlinePaint {
    pub(crate) width: f32,
    pub(crate) color: Option<Color>,
    pub(crate) offset: f32,
}

impl OutlinePaint {
    pub(crate) fn from_style(style: &crate::style::computed::ComputedStyle) -> Self {
        Self {
            width: style.outline_width,
            color: style.outline_color,
            offset: style.outline_offset,
        }
    }
}

/// Effects applied to a box after its contents have been composited.
///
/// Keeping the complete paint-group contract together is important when a
/// subtree is materialized as a filtered raster: the raster replaces only the
/// source graphic, while clipping, masking, opacity, and blending still apply
/// to the resulting group in this order.
#[derive(Debug, Clone)]
pub(crate) struct GroupEffects {
    pub(crate) opacity: f32,
    pub(crate) mix_blend_mode: BlendMode,
    pub(crate) isolation: Isolation,
    pub(crate) stacking_context: StackingContext,
    pub(crate) masking: Masking,
}

impl Default for GroupEffects {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            mix_blend_mode: BlendMode::Normal,
            isolation: Isolation::Auto,
            stacking_context: StackingContext::None,
            masking: Masking::default(),
        }
    }
}

impl GroupEffects {
    pub(crate) fn from_style(style: &crate::style::computed::ComputedStyle) -> Self {
        Self {
            opacity: style.opacity,
            mix_blend_mode: style.mix_blend_mode,
            isolation: style.isolation,
            stacking_context: (&style.filter).into(),
            masking: Masking::from_style(style),
        }
    }

    /// Whether painting this source needs no post-compositing wrapper.
    ///
    /// Optimized leaf painters use this as one eligibility check so adding a
    /// new group effect cannot silently leave an older fast path behind.
    pub(crate) fn is_identity(&self) -> bool {
        self.opacity >= 1.0
            && self.mix_blend_mode == BlendMode::Normal
            && !self.isolation.isolates()
            && self.stacking_context == StackingContext::None
            && self.masking.is_none()
    }
}

/// One CSS transform together with the pivot used to resolve it against a
/// concrete fragment border box.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct BoxTransform {
    pub(crate) value: Option<Transform>,
    pub(crate) origin: TransformOrigin,
    pub(crate) reference_box: crate::style::computed::TransformBox,
    /// A parent perspective establishes a spatial stacking group even though
    /// the projection itself is resolved onto transformed descendants.
    pub(crate) perspective: Option<f32>,
}

impl BoxTransform {
    pub(crate) const fn establishes_stacking_context(self) -> bool {
        self.value.is_some() || self.perspective.is_some()
    }
}

/// The complete graphical group applied to a box and all of its descendants.
///
/// Transforms are paint-time coordinate-system changes, not physical layout
/// positioning. Keeping them with the post-compositing effects makes the
/// recursive contract identical for ordinary boxes, replaced content, and
/// formatting-context cells.
#[derive(Debug, Clone, Default)]
pub(crate) struct PaintGroup {
    pub(crate) stacking: super::Stacking,
    pub(crate) transform: BoxTransform,
    pub(crate) effects: GroupEffects,
}

impl PaintGroup {
    pub(crate) fn from_style(style: &crate::style::computed::ComputedStyle) -> Self {
        Self {
            stacking: super::Stacking::from_style(style),
            transform: BoxTransform {
                value: style.transform,
                origin: style.transform_origin,
                reference_box: style.transform_box,
                perspective: style.perspective,
            },
            effects: GroupEffects::from_style(style),
        }
    }

    pub(crate) fn is_identity(&self) -> bool {
        !self.transform.establishes_stacking_context() && self.effects.is_identity()
    }

    pub(crate) fn establishes_stacking_context(&self) -> bool {
        self.transform.establishes_stacking_context()
            || self.effects.opacity < 1.0
            || self.effects.mix_blend_mode != BlendMode::Normal
            || self.effects.isolation.isolates()
            || self.effects.stacking_context != StackingContext::None
            || !self.effects.masking.is_none()
    }
}

/// Visual decoration and compositing shared by box nodes.
#[derive(Debug, Clone)]
pub(crate) struct BoxPaint {
    pub(crate) background: BackgroundPaint,
    pub(crate) border_image: Option<crate::style::computed::BorderImagePaint>,
    pub(crate) border_radii: CornerRadii,
    pub(crate) shadows: Vec<BoxShadow>,
    pub(crate) outline: OutlinePaint,
    pub(crate) filter: Option<crate::layout::filter::ResolvedFilter>,
    pub(crate) group: PaintGroup,
    pub(crate) visible: bool,
}

impl Default for BoxPaint {
    fn default() -> Self {
        Self {
            background: BackgroundPaint::default(),
            border_image: None,
            border_radii: CornerRadii::ZERO,
            shadows: Vec::new(),
            outline: OutlinePaint::default(),
            filter: None,
            group: PaintGroup::default(),
            visible: true,
        }
    }
}

impl BoxPaint {
    pub(crate) fn from_style(
        style: &crate::style::computed::ComputedStyle,
        size: LayoutSize,
    ) -> Self {
        let width = size.width.fixed_value().unwrap_or_default();
        let height = size.height.resolve(width);
        Self {
            background: BackgroundPaint::from_style(style),
            border_image: style.border_image.paint(),
            border_radii: style.resolve_corner_radii(width, height),
            shadows: style.box_shadow.clone(),
            outline: OutlinePaint::from_style(style),
            filter: None,
            group: PaintGroup::from_style(style),
            visible: style.visibility == Visibility::Visible,
        }
    }

    /// Whether paint owned by this box can escape its fragment border box.
    ///
    /// The exact overflow bounds are renderer geometry, but ownership of an
    /// outset effect is semantic layout state. Page separation uses this
    /// conservative predicate to retain complete graphical output without
    /// teaching fragmentation about individual effect implementations.
    pub(crate) fn has_outset_graphical_effect(&self) -> bool {
        self.group.transform.establishes_stacking_context()
            || self.shadows.iter().any(|shadow| !shadow.inset)
            || self.outline.width > 0.0
            || self.background.layers.blur_radius > 0.0
            || self
                .filter
                .as_ref()
                .is_some_and(crate::layout::filter::ResolvedFilter::has_composited_output)
    }
}

pub(crate) fn text_lines_have_outset_shadows(lines: &[crate::layout::engine::TextLine]) -> bool {
    lines
        .iter()
        .flat_map(|line| &line.runs)
        .any(|run| run.text_shadow.iter().any(|shadow| !shadow.inset))
}

/// Ownership of one resolved filter retained until fragmentation has produced
/// the concrete box fragments to which graphical effects apply.
pub(crate) trait FilterHolder {
    fn filter_slot_mut(&mut self) -> &mut Option<crate::layout::filter::ResolvedFilter>;

    fn take_filter(&mut self) -> Option<crate::layout::filter::ResolvedFilter> {
        self.filter_slot_mut().take()
    }
}

impl FilterHolder for BoxPaint {
    fn filter_slot_mut(&mut self) -> &mut Option<crate::layout::filter::ResolvedFilter> {
        &mut self.filter
    }
}

/// Normal-flow participation of a block-level box.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct BlockFlow {
    pub(crate) float: Float,
    pub(crate) clear: Clear,
}

/// Block-axis spacing and non-painting trailing flow extent.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct FlowSpacing {
    pub(crate) margins: BlockMargins,
    pub(crate) extra_end: f32,
}

impl FlowSpacing {
    pub(crate) const fn content_extent(self, block_size: f32) -> f32 {
        block_size + self.extra_end
    }

    pub(crate) const fn outer_extent(self, block_size: f32) -> f32 {
        self.margins.total() + self.content_extent(block_size)
    }
}

/// An inline-axis displacement from the containing content edge.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct InlineOffset(f32);

impl InlineOffset {
    pub(crate) const ZERO: Self = Self(0.0);

    pub(crate) const fn new(value: f32) -> Self {
        Self(value)
    }

    pub(crate) const fn value(self) -> f32 {
        self.0
    }

    /// Resolve the physical start edge of a normal-flow block-level box.
    ///
    /// Auto inline margins absorb positive free space after the used border-box
    /// width and the non-auto margin have been removed. This applies equally to
    /// ordinary blocks and to table wrappers after their fit-content width has
    /// been resolved.
    pub(crate) fn resolve_block_start(
        style: &crate::style::computed::ComputedStyle,
        containing_width: f32,
        border_box_width: f32,
    ) -> Self {
        let fixed_start = (!style.margin_left_auto)
            .then_some(style.margin.left)
            .unwrap_or_default();
        let fixed_end = (!style.margin_right_auto)
            .then_some(style.margin.right)
            .unwrap_or_default();
        let free_space = (containing_width - border_box_width - fixed_start - fixed_end).max(0.0);

        let start = match (style.margin_left_auto, style.margin_right_auto) {
            (true, true) => free_space / 2.0,
            (true, false) => free_space,
            (false, _) => fixed_start,
        };
        Self(start)
    }
}

impl std::ops::Add<f32> for InlineOffset {
    type Output = Self;

    fn add(self, rhs: f32) -> Self::Output {
        Self(self.0 + rhs)
    }
}

/// Physical positioned-layout state. Insets use the same top/right/bottom/left
/// edge representation as padding and borders.
#[derive(Debug, Clone, Default)]
pub(crate) struct Positioning {
    pub(crate) scheme: Position,
    pub(crate) insets: EdgeSizes,
    pub(crate) containing_block: Option<ContainingBlock>,
    pub(crate) containing_block_depth: usize,
}

impl Positioning {
    pub(crate) fn from_style(style: &crate::style::computed::ComputedStyle) -> Self {
        Self {
            scheme: style.position,
            insets: EdgeSizes::new(
                style.top.unwrap_or_default(),
                style.right.unwrap_or_default(),
                style.bottom.unwrap_or_default(),
                style.left.unwrap_or_default(),
            ),
            ..Default::default()
        }
    }

    pub(crate) const fn is_in_normal_flow(&self) -> bool {
        self.scheme.is_in_flow()
    }

    /// Preserve the subset of positioned state supported by replaced elements.
    pub(crate) fn for_replaced(style: &crate::style::computed::ComputedStyle) -> Self {
        let relative = style.position.is_relative();
        Self {
            scheme: relative
                .then_some(style.position)
                .unwrap_or(Position::Static),
            insets: EdgeSizes::new(
                relative
                    .then(|| style.top.unwrap_or_default())
                    .unwrap_or_default(),
                0.0,
                0.0,
                relative
                    .then(|| style.left.unwrap_or_default())
                    .unwrap_or_default(),
            ),
            ..Self::default()
        }
    }
}

/// Fragmentation behavior common to decorated box fragments.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct BoxFragmentation {
    pub(crate) decoration: BoxDecorationBreak,
    pub(crate) content_role: super::PageContentRole,
    pub(crate) reference_slice: Option<BoxFragmentSlice>,
}

impl BoxFragmentation {
    /// Reference-box slices for two fragments produced from this box.
    ///
    /// A continuation can fragment more than once. In that case both new
    /// fragments retain the original composite extent and authored edges;
    /// only the continuation offset advances.
    pub(crate) fn split_reference_box(
        self,
        first_block_size: f32,
        continuation_block_size: f32,
        box_model: &BoxModel,
    ) -> Option<(BoxFragmentSlice, BoxFragmentSlice)> {
        (self.decoration == BoxDecorationBreak::Slice).then(|| {
            self.reference_slice.map_or_else(
                || BoxFragmentSlice::split(first_block_size, continuation_block_size, box_model),
                |slice| slice.split_continuation(first_block_size),
            )
        })
    }
}

/// Line constraints used when a text box fragments.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TextFragmentation {
    pub(crate) box_fragmentation: BoxFragmentation,
    pub(crate) orphans: u8,
    pub(crate) widows: u8,
}

impl Default for TextFragmentation {
    fn default() -> Self {
        Self {
            box_fragmentation: BoxFragmentation::default(),
            orphans: 2,
            widows: 2,
        }
    }
}

/// Spacing that affects text advances rather than the surrounding box.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TextSpacing {
    pub(crate) letter: f32,
    pub(crate) word: f32,
}

/// Block-level text formatting kept separate from glyph-run styles.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TextBlockStyle {
    pub(crate) alignment: TextAlign,
    pub(crate) writing_mode: WritingMode,
    pub(crate) indent: f32,
    pub(crate) spacing: TextSpacing,
}

impl Default for TextBlockStyle {
    fn default() -> Self {
        Self {
            alignment: TextAlign::Left,
            writing_mode: WritingMode::HorizontalTb,
            indent: 0.0,
            spacing: TextSpacing::default(),
        }
    }
}

/// Overflow clip owned by a text box.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct DescendantClip {
    pub(crate) rect: Option<Rect>,
}

/// Document-level meaning carried by a laid-out text block.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TextSemantics {
    pub(crate) heading_level: Option<u8>,
}

/// Per-axis overflow behavior of a container.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct OverflowBehavior {
    pub(crate) combined: Overflow,
    pub(crate) x: Overflow,
    pub(crate) y: Overflow,
}

/// Clipping and masking sources that apply to one container paint group.
#[derive(Debug, Clone, Default)]
pub(crate) struct Masking {
    pub(crate) clip_path: Option<ClipPath>,
    pub(crate) image: Option<MaskSource>,
    pub(crate) mode: MaskMode,
}

impl Masking {
    pub(crate) fn from_style(style: &crate::style::computed::ComputedStyle) -> Self {
        Self {
            clip_path: style.clip_path.clone(),
            image: style.mask_image.clone(),
            mode: style.mask_mode,
        }
    }

    pub(crate) fn is_none(&self) -> bool {
        self.clip_path.is_none() && self.image.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::{BlockSize, BoxFragmentation, BoxModel, InlineSize, SizeConstraints};
    use crate::style::computed::{BoxDecorationBreak, ComputedStyle};

    #[test]
    fn inline_size_distinguishes_fill_available_from_fixed_geometry() {
        assert_eq!(InlineSize::FILL_AVAILABLE.resolve(320.0), 320.0);
        assert_eq!(InlineSize::fixed(180.0).resolve(320.0), 180.0);
        assert!(InlineSize::from_used(320.0, 320.0, false).is_fill_available());
        assert_eq!(
            InlineSize::from_used(320.0, 320.0, true).fixed_value(),
            Some(320.0)
        );
        assert!(InlineSize::FILL_AVAILABLE.is_fill_available());
        assert_eq!(InlineSize::fixed(180.0).fixed_value(), Some(180.0));
    }

    #[test]
    fn minimum_block_size_is_a_floor_not_a_content_cap() {
        let size = BlockSize::minimum(50.0);

        assert_eq!(size.resolve(20.0), 50.0);
        assert_eq!(size.resolve(80.0), 80.0);
        assert!(!size.is_definite());
    }

    #[test]
    fn minimum_size_wins_when_constraints_conflict() {
        let constraints = SizeConstraints::new(Some(68.0), Some(58.0));

        assert_eq!(constraints.constrain(40.0), 68.0);
        assert_eq!(constraints.constrain(80.0), 68.0);
        assert_eq!(constraints.constrain_preferred(None), Some(68.0));
    }

    #[test]
    fn maximum_alone_waits_for_automatic_content_size() {
        let constraints = SizeConstraints::new(None, Some(58.0));

        assert_eq!(constraints.constrain_preferred(None), None);
        assert_eq!(constraints.constrain(40.0), 40.0);
        assert_eq!(constraints.constrain(80.0), 58.0);
    }

    #[test]
    fn authored_minimum_remains_fragmentable_while_height_is_definite() {
        let minimum = BlockSize::from_style(
            &ComputedStyle {
                min_height: Some(50.0),
                ..ComputedStyle::default()
            },
            20.0,
        );
        let definite = BlockSize::from_style(
            &ComputedStyle {
                height: Some(50.0),
                ..ComputedStyle::default()
            },
            20.0,
        );

        assert_eq!(minimum.resolve(80.0), 80.0);
        assert!(!minimum.is_definite());
        assert_eq!(definite.resolve(80.0), 50.0);
        assert!(definite.is_definite());
    }

    #[test]
    fn auto_block_size_retains_its_intrinsic_used_extent() {
        let size = BlockSize::from_style(&ComputedStyle::default(), 72.0);

        assert_eq!(size.used(), Some(72.0));
        assert_eq!(size.resolve(0.0), 72.0);
        assert_eq!(size.resolve(96.0), 96.0);
        assert!(!size.is_definite());
    }

    #[test]
    fn repeated_slice_keeps_one_composite_reference_box() {
        let fragmentation = BoxFragmentation {
            decoration: BoxDecorationBreak::Slice,
            ..Default::default()
        };
        let (first, continuation) = fragmentation
            .split_reference_box(80.0, 120.0, &BoxModel::default())
            .expect("slice decoration has reference geometry");
        let continuation = BoxFragmentation {
            decoration: BoxDecorationBreak::Slice,
            reference_slice: Some(continuation),
            ..Default::default()
        };
        let (second, third) = continuation
            .split_reference_box(50.0, 70.0, &BoxModel::default())
            .expect("a continuation retains reference geometry");

        assert_eq!(first.block_offset(), 0.0);
        assert_eq!(second.block_offset(), 80.0);
        assert_eq!(third.block_offset(), 130.0);
        assert_eq!(first.composite_block_size(), 200.0);
        assert_eq!(second.composite_block_size(), 200.0);
        assert_eq!(third.composite_block_size(), 200.0);
    }
}
