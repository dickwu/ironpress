use super::{
    AtomicInlineBaseline, BlockFlowParticipant, FlowSpacing, InlineFlowExtent, LayoutElement,
    LayoutNode, LayoutVisitor, LayoutVisitorMut, PaintGroup, PaintGroupOwner, Positioning,
    PositioningOwner, ReplacedElement,
};
use crate::layout::engine::{
    ImageEffectRaster, LayoutBorder, RasterImageAsset, SvgReplacedContent,
};
use crate::layout::flow_metrics::{BlockMargins, MarginHolder};
use crate::style::computed::{ImageRendering, ObjectFit, ObjectPosition};
use crate::types::{Color, CornerRadii, EdgeSizes, Size};

/// Geometry shared by replaced raster and vector content.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ReplacedGeometry {
    pub(crate) size: Size,
    pub(crate) flow: FlowSpacing,
    pub(crate) border: LayoutBorder,
}

impl ReplacedGeometry {
    pub(crate) const fn new(size: Size, margins: BlockMargins, border: LayoutBorder) -> Self {
        Self {
            size,
            flow: FlowSpacing {
                margins,
                extra_end: 0.0,
            },
            border,
        }
    }
}

/// Sampling, fitting, and source-region behavior of a raster image.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ImageSampling {
    pub(crate) object_fit: ObjectFit,
    pub(crate) object_position: ObjectPosition,
    pub(crate) rendering: ImageRendering,
    pub(crate) source_crop: Option<[f32; 4]>,
}

/// Paint produced around or instead of the source image.
#[derive(Debug, Clone, Default)]
pub(crate) struct ImagePaint {
    pub(crate) background_color: Option<Color>,
    pub(crate) border_image: Option<crate::style::computed::BorderImagePaint>,
    pub(crate) border_radii: CornerRadii,
    pub(crate) raster_overflow: EdgeSizes,
    pub(crate) filter_effect: Option<ImageEffectRaster>,
    pub(crate) group: PaintGroup,
}

#[derive(Debug, Clone)]
pub(crate) struct Image {
    pub(crate) source: RasterImageAsset,
    pub(crate) geometry: ReplacedGeometry,
    pub(crate) positioning: Positioning,
    pub(crate) sampling: ImageSampling,
    pub(crate) paint: ImagePaint,
}

impl MarginHolder for Image {
    fn margins(&self) -> &BlockMargins {
        &self.geometry.flow.margins
    }

    fn margins_mut(&mut self) -> &mut BlockMargins {
        &mut self.geometry.flow.margins
    }
}

impl ReplacedElement for Image {
    fn geometry_mut(&mut self) -> &mut ReplacedGeometry {
        &mut self.geometry
    }
}

impl InlineFlowExtent for Image {
    fn normal_flow_right_edge(&self) -> Option<f32> {
        self.positioning
            .is_in_normal_flow()
            .then_some((self.positioning.insets.left + self.geometry.size.width).max(0.0))
            .filter(|right| right.is_finite())
    }
}

impl AtomicInlineBaseline for Image {
    fn baseline_offset(&self) -> f32 {
        self.geometry.flow.margins.start
            + self.geometry.size.height
            + self.geometry.flow.margins.end
    }
}

impl BlockFlowParticipant for Image {
    fn collapses_outer_margins(&self) -> bool {
        true
    }

    fn is_in_flow_block(&self) -> bool {
        true
    }
}

impl PositioningOwner for Image {
    fn positioning(&self) -> &Positioning {
        &self.positioning
    }

    fn positioning_mut(&mut self) -> &mut Positioning {
        &mut self.positioning
    }
}

impl PaintGroupOwner for Image {
    fn paint_group(&self) -> &PaintGroup {
        &self.paint.group
    }

    fn paint_group_mut(&mut self) -> &mut PaintGroup {
        &mut self.paint.group
    }
}

impl LayoutElement for Image {
    fn clone_box(&self) -> LayoutNode {
        Box::new(self.clone())
    }

    fn accept(&self, visitor: &mut dyn LayoutVisitor) {
        visitor.visit_image(self);
    }

    fn accept_mut(&mut self, visitor: &mut dyn LayoutVisitorMut) {
        visitor.visit_image(self);
    }

    fn margin_holder(&self) -> Option<&dyn MarginHolder> {
        Some(self)
    }

    fn margin_holder_mut(&mut self) -> Option<&mut dyn MarginHolder> {
        Some(self)
    }

    fn replaced_element_mut(&mut self) -> Option<&mut dyn ReplacedElement> {
        Some(self)
    }

    fn inline_flow_extent(&self) -> Option<&dyn InlineFlowExtent> {
        Some(self)
    }

    fn atomic_inline_baseline(&self) -> Option<&dyn AtomicInlineBaseline> {
        Some(self)
    }

    fn block_flow_participant(&self) -> Option<&dyn BlockFlowParticipant> {
        Some(self)
    }

    fn block_flow_participant_mut(&mut self) -> Option<&mut dyn BlockFlowParticipant> {
        Some(self)
    }

    fn positioning_owner(&self) -> Option<&dyn PositioningOwner> {
        Some(self)
    }

    fn positioning_owner_mut(&mut self) -> Option<&mut dyn PositioningOwner> {
        Some(self)
    }

    fn paint_group_owner(&self) -> Option<&dyn PaintGroupOwner> {
        Some(self)
    }

    fn paint_group_owner_mut(&mut self) -> Option<&mut dyn PaintGroupOwner> {
        Some(self)
    }

    fn has_own_page_spanning_graphical_effect(&self) -> bool {
        self.paint.group.transform.establishes_stacking_context()
            || !self.paint.raster_overflow.is_zero()
            || self.paint.filter_effect.is_some()
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SvgPaint {
    pub(crate) background_color: Option<Color>,
    pub(crate) border_image: Option<crate::style::computed::BorderImagePaint>,
    pub(crate) border_radii: CornerRadii,
    pub(crate) group: PaintGroup,
}

#[derive(Debug, Clone)]
pub(crate) struct Svg {
    pub(crate) tree: crate::parser::svg::SvgTree,
    pub(crate) geometry: ReplacedGeometry,
    pub(crate) positioning: Positioning,
    pub(crate) paint: SvgPaint,
    pub(crate) replaced: SvgReplacedContent,
}

impl MarginHolder for Svg {
    fn margins(&self) -> &BlockMargins {
        &self.geometry.flow.margins
    }

    fn margins_mut(&mut self) -> &mut BlockMargins {
        &mut self.geometry.flow.margins
    }
}

impl ReplacedElement for Svg {
    fn geometry_mut(&mut self) -> &mut ReplacedGeometry {
        &mut self.geometry
    }
}

impl InlineFlowExtent for Svg {
    fn normal_flow_right_edge(&self) -> Option<f32> {
        self.positioning
            .is_in_normal_flow()
            .then_some((self.positioning.insets.left + self.geometry.size.width).max(0.0))
            .filter(|right| right.is_finite())
    }
}

impl AtomicInlineBaseline for Svg {
    fn baseline_offset(&self) -> f32 {
        self.geometry.flow.margins.start
            + self.geometry.size.height
            + self.geometry.flow.margins.end
    }
}

impl BlockFlowParticipant for Svg {
    fn collapses_outer_margins(&self) -> bool {
        true
    }

    fn is_in_flow_block(&self) -> bool {
        true
    }
}

impl PositioningOwner for Svg {
    fn positioning(&self) -> &Positioning {
        &self.positioning
    }

    fn positioning_mut(&mut self) -> &mut Positioning {
        &mut self.positioning
    }
}

impl PaintGroupOwner for Svg {
    fn paint_group(&self) -> &PaintGroup {
        &self.paint.group
    }

    fn paint_group_mut(&mut self) -> &mut PaintGroup {
        &mut self.paint.group
    }
}

impl LayoutElement for Svg {
    fn clone_box(&self) -> LayoutNode {
        Box::new(self.clone())
    }

    fn accept(&self, visitor: &mut dyn LayoutVisitor) {
        visitor.visit_svg(self);
    }

    fn accept_mut(&mut self, visitor: &mut dyn LayoutVisitorMut) {
        visitor.visit_svg(self);
    }

    fn margin_holder(&self) -> Option<&dyn MarginHolder> {
        Some(self)
    }

    fn margin_holder_mut(&mut self) -> Option<&mut dyn MarginHolder> {
        Some(self)
    }

    fn replaced_element_mut(&mut self) -> Option<&mut dyn ReplacedElement> {
        Some(self)
    }

    fn inline_flow_extent(&self) -> Option<&dyn InlineFlowExtent> {
        Some(self)
    }

    fn atomic_inline_baseline(&self) -> Option<&dyn AtomicInlineBaseline> {
        Some(self)
    }

    fn block_flow_participant(&self) -> Option<&dyn BlockFlowParticipant> {
        Some(self)
    }

    fn block_flow_participant_mut(&mut self) -> Option<&mut dyn BlockFlowParticipant> {
        Some(self)
    }

    fn positioning_owner(&self) -> Option<&dyn PositioningOwner> {
        Some(self)
    }

    fn positioning_owner_mut(&mut self) -> Option<&mut dyn PositioningOwner> {
        Some(self)
    }

    fn paint_group_owner(&self) -> Option<&dyn PaintGroupOwner> {
        Some(self)
    }

    fn paint_group_owner_mut(&mut self) -> Option<&mut dyn PaintGroupOwner> {
        Some(self)
    }
}
