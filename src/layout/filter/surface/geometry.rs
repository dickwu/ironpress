//! Geometry of a SourceGraphic and its physical raster backing.

use crate::layout::cells::TableRowCells;
use crate::layout::elements::{
    ColumnRule, Container, FlexRow, GridRow, Image, LayoutElement, LayoutVisitor, Positioning,
    TableRow, TextBlock,
};
use crate::layout::flow_metrics::BlockFlowSpacing;
use crate::types::{EdgeSizes, Point, Size};

/// Absolute top-down page position of a filter source's border box.
///
/// Skia rasterizes a filtered layer in device space. Retaining this anchor
/// makes the subpixel phase part of SourceGraphic construction, so the
/// resulting PDF image can be placed on integral device bounds without being
/// resampled a second time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SourceRasterAnchor {
    border_origin: Point,
}

impl SourceRasterAnchor {
    pub(crate) const fn at_border_origin(border_origin: Point) -> Self {
        Self { border_origin }
    }

    pub(crate) const fn border_origin(self) -> Point {
        self.border_origin
    }
}

/// Integral device bounds enclosing one authored point-space rectangle.
#[derive(Clone, Copy)]
struct DeviceRasterBounds {
    left: i64,
    top: i64,
    right: i64,
    bottom: i64,
}

impl DeviceRasterBounds {
    fn enclosing(
        anchor: SourceRasterAnchor,
        size: Size,
        authored_overflow: EdgeSizes,
        scale: crate::render::raster_scale::RasterScale,
    ) -> Option<Self> {
        let origin = anchor.border_origin();
        Some(Self {
            left: scale.floor(origin.x - authored_overflow.left)?,
            top: scale.floor(origin.y - authored_overflow.top)?,
            right: scale.ceil(origin.x + size.width + authored_overflow.right)?,
            bottom: scale.ceil(origin.y + size.height + authored_overflow.bottom)?,
        })
    }

    fn dimensions(self) -> Option<crate::util::RasterDimensions> {
        Some(crate::util::RasterDimensions {
            width: u32::try_from(self.right.checked_sub(self.left)?).ok()?,
            height: u32::try_from(self.bottom.checked_sub(self.top)?).ok()?,
        })
    }

    fn border_origin(
        self,
        anchor: SourceRasterAnchor,
        scale: crate::render::raster_scale::RasterScale,
    ) -> Point {
        let origin = anchor.border_origin();
        Point::new(
            origin.x - scale.pixels_to_points(self.left as f32),
            origin.y - scale.pixels_to_points(self.top as f32),
        )
    }
}

/// Border-box geometry retained when a painted filter source becomes an image.
#[derive(Debug, Clone)]
pub(crate) struct SourceGeometry {
    pub(crate) size: Size,
    pub(crate) flow: BlockFlowSpacing,
    pub(crate) positioning: Positioning,
}

/// Integer pixel bounds and local border-box position of one filter surface.
#[derive(Clone, Copy)]
struct RasterSurfaceFrame {
    dimensions: crate::util::RasterDimensions,
    border_origin: Point,
    paint_overflow: EdgeSizes,
}

impl RasterSurfaceFrame {
    fn resolve(
        size: Size,
        authored_overflow: EdgeSizes,
        dpi: f32,
        anchor: SourceRasterAnchor,
    ) -> Option<Self> {
        let scale = crate::render::raster_scale::RasterScale::at_dpi(dpi);
        let bounds = DeviceRasterBounds::enclosing(anchor, size, authored_overflow, scale)?;
        let dimensions = bounds.dimensions()?;
        let border_origin = bounds.border_origin(anchor, scale);
        let surface_size = Size::new(
            scale.pixels_to_points(dimensions.width as f32),
            scale.pixels_to_points(dimensions.height as f32),
        );
        Some(Self {
            dimensions,
            border_origin,
            paint_overflow: EdgeSizes::new(
                border_origin.y,
                surface_size.width - border_origin.x - size.width,
                surface_size.height - border_origin.y - size.height,
                border_origin.x,
            ),
        })
    }
}

/// One completely painted, unfiltered `SourceGraphic`.
pub(crate) struct SourceGraphic {
    pub(crate) pixels: crate::render::raster_pixels::PremultipliedRgba8,
    pub(crate) geometry: SourceRasterGeometry,
    pub(crate) paint_bounds: Option<crate::types::Rect>,
}

/// Relationship between the layout border box and its offscreen paint surface.
///
/// Layout retains the unexpanded border box. The raster frame owns the
/// device-quantized origin and extent, so reinserting a filtered image never
/// changes normal flow or re-derives sampling phase from point-space floats.
pub(crate) struct SourceRasterGeometry {
    pub(crate) layout: SourceGeometry,
    surface: RasterSurfaceFrame,
}

impl SourceRasterGeometry {
    pub(super) fn resolve(
        layout: SourceGeometry,
        authored_overflow: EdgeSizes,
        dpi: f32,
        anchor: SourceRasterAnchor,
    ) -> Option<Self> {
        let surface = RasterSurfaceFrame::resolve(layout.size, authored_overflow, dpi, anchor)?;
        Some(Self { layout, surface })
    }

    pub(super) fn dimensions(&self) -> crate::util::RasterDimensions {
        self.surface.dimensions
    }

    pub(crate) fn surface_size(&self) -> Size {
        Size::new(
            self.layout.size.width + self.surface.paint_overflow.horizontal(),
            self.layout.size.height + self.surface.paint_overflow.vertical(),
        )
    }

    pub(super) fn border_origin(&self) -> Point {
        self.surface.border_origin
    }

    pub(crate) fn paint_overflow(&self) -> EdgeSizes {
        self.surface.paint_overflow
    }

    pub(super) fn required_overflow_for(&self, paint_bounds: crate::types::Rect) -> EdgeSizes {
        let border_box = crate::types::Rect::new(self.surface.border_origin, self.layout.size);
        EdgeSizes::new(
            (border_box.origin.y - paint_bounds.origin.y).max(0.0),
            (paint_bounds.right() - border_box.right()).max(0.0),
            (paint_bounds.bottom() - border_box.bottom()).max(0.0),
            (border_box.origin.x - paint_bounds.origin.x).max(0.0),
        )
    }

    pub(crate) fn filter_geometry(&self) -> Option<crate::render::filter::FilterSourceGeometry> {
        crate::render::filter::FilterSourceGeometry::new(
            self.surface_size(),
            crate::types::Rect::new(self.surface.border_origin, self.layout.size),
        )
    }
}

pub(crate) fn source_geometry(element: &dyn LayoutElement) -> Option<SourceGeometry> {
    struct Geometry(Option<SourceGeometry>);

    impl LayoutVisitor for Geometry {
        fn visit_column_rule(&mut self, element: &ColumnRule) {
            self.0 = Some(SourceGeometry {
                size: Size::new(element.paint.width, element.height),
                flow: BlockFlowSpacing::default(),
                positioning: Positioning::default(),
            });
        }

        fn visit_text_block(&mut self, element: &TextBlock) {
            self.0 = element
                .box_model
                .size
                .width
                .fixed_value()
                .map(|width| SourceGeometry {
                    size: Size::new(width, element.border_box_block_extent()),
                    flow: BlockFlowSpacing::from_margins(element.box_model.margins),
                    positioning: element.positioning.clone(),
                });
        }

        fn visit_container(&mut self, element: &Container) {
            let height = container_source_height(element);
            self.0 = element
                .box_model
                .size
                .width
                .fixed_value()
                .map(|width| SourceGeometry {
                    size: Size::new(width, height),
                    flow: BlockFlowSpacing::from_margins(element.box_model.margins),
                    positioning: element.positioning.clone(),
                });
        }

        fn visit_flex_row(&mut self, element: &FlexRow) {
            let height = element.box_model.padding.vertical()
                + element
                    .box_model
                    .size
                    .height
                    .resolve(element.content.row_height)
                + element.box_model.border.vertical_width();
            self.0 = element.box_model.size.width.fixed_value().map(|width| {
                let mut positioning = element.positioning.clone();
                positioning.insets.left += element.inline_offset.value();
                SourceGeometry {
                    size: Size::new(width, height),
                    flow: BlockFlowSpacing::from_margins(element.box_model.margins),
                    positioning,
                }
            });
        }

        fn visit_grid_row(&mut self, element: &GridRow) {
            let width = element.content.column_widths.iter().sum::<f32>()
                + element.content.gap
                    * element.content.column_widths.len().saturating_sub(1) as f32
                + element.box_model.padding.horizontal()
                + element.box_model.border.horizontal_width();
            let height = element
                .content
                .cells
                .iter()
                .map(|cell| cell.layout.box_model.minimum_block_size)
                .fold(0.0_f32, f32::max)
                + element.box_model.padding.vertical()
                + element.box_model.border.vertical_width();
            self.0 = Some(SourceGeometry {
                size: Size::new(width, height),
                flow: BlockFlowSpacing::from_margins(element.box_model.margins),
                positioning: Default::default(),
            });
        }

        fn visit_table_row(&mut self, element: &TableRow) {
            self.0 = Some(SourceGeometry {
                size: Size::new(
                    element.box_inline_extent(),
                    element.content.cells.row_block_extent(),
                ),
                flow: element.flow,
                positioning: Positioning::default(),
            });
        }

        fn visit_image(&mut self, element: &Image) {
            self.0 = Some(SourceGeometry {
                size: element.geometry.size,
                flow: BlockFlowSpacing::from_margins(element.geometry.flow.margins),
                positioning: element.positioning.clone(),
            });
        }
    }

    let mut geometry = Geometry(None);
    element.accept(&mut geometry);
    geometry.0
}

/// Resolve an auto-width block descendant against its known content box.
fn source_geometry_in_content(
    element: &dyn LayoutElement,
    available_width: f32,
) -> Option<SourceGeometry> {
    if let Some(geometry) = source_geometry(element) {
        return Some(geometry);
    }

    struct AutoWidthGeometry {
        available_width: f32,
        geometry: Option<SourceGeometry>,
    }

    impl LayoutVisitor for AutoWidthGeometry {
        fn visit_text_block(&mut self, element: &TextBlock) {
            if !element.box_model.size.width.is_fill_available() {
                return;
            }
            self.geometry = Some(SourceGeometry {
                size: Size::new(self.available_width, element.border_box_block_extent()),
                flow: BlockFlowSpacing::from_margins(element.box_model.margins),
                positioning: element.positioning.clone(),
            });
        }

        fn visit_container(&mut self, element: &Container) {
            if element.box_model.size.width.is_fill_available() {
                self.geometry = Some(SourceGeometry {
                    size: Size::new(self.available_width, container_source_height(element)),
                    flow: BlockFlowSpacing::from_margins(element.box_model.margins),
                    positioning: element.positioning.clone(),
                });
            }
        }
    }

    let mut geometry = AutoWidthGeometry {
        available_width,
        geometry: None,
    };
    element.accept(&mut geometry);
    geometry.geometry
}

/// Resolved border box of one child in a block formatting context.
#[derive(Clone, Copy)]
pub(crate) struct BlockChildFrame {
    pub(crate) border_box: crate::types::Rect,
}

/// Coordinate spaces shared by every child of one block formatting context.
#[derive(Clone, Copy)]
pub(crate) struct BlockChildSpace {
    content_box: crate::types::Rect,
    padding_box: crate::types::Rect,
    absolute_containing_block: Option<crate::types::Rect>,
}

impl BlockChildSpace {
    pub(crate) const fn new(
        content_box: crate::types::Rect,
        padding_box: crate::types::Rect,
        absolute_containing_block: Option<crate::types::Rect>,
    ) -> Self {
        Self {
            content_box,
            padding_box,
            absolute_containing_block,
        }
    }
}

/// Resolve block children once for every SourceGraphic paint path. Sharing
/// this sequence prevents nested boxes from acquiring different device phases
/// based on which concrete parent type owns them.
pub(crate) fn block_child_frames(
    children: &[crate::layout::elements::LayoutNode],
    space: BlockChildSpace,
) -> Option<Vec<BlockChildFrame>> {
    use crate::style::computed::Position;

    let mut frames = Vec::new();
    frames.try_reserve_exact(children.len()).ok()?;
    let mut cursor_y = space.content_box.origin.y;
    let mut previous_margin_end = 0.0;
    for child in children {
        if let Some(placed) = child.fragment_placement_owner() {
            let placement = placed.fragment_placement();
            let geometry =
                source_geometry_in_content(placed.fragment_source(), placement.size.width)?;
            frames.push(BlockChildFrame {
                border_box: crate::types::Rect::new(
                    placement.resolve(space.content_box.origin, space.padding_box.origin),
                    geometry.size,
                ),
            });
            continue;
        }
        let geometry = source_geometry_in_content(child.as_ref(), space.content_box.size.width)?;
        let positioning = &geometry.positioning;
        let flow = geometry.flow;
        let (origin, advances_flow) = match positioning.scheme {
            Position::Absolute | Position::Fixed => {
                let containing_block = space.absolute_containing_block?;
                (
                    Point::new(
                        containing_block.origin.x + positioning.insets.left,
                        containing_block.origin.y + positioning.insets.top,
                    ),
                    false,
                )
            }
            Position::Static | Position::Relative | Position::Sticky => {
                cursor_y += collapsed_margin_start_extra(flow.margins.start, previous_margin_end);
                cursor_y += flow.internal.start;
                (
                    Point::new(
                        space.content_box.origin.x + positioning.insets.left,
                        cursor_y + positioning.insets.top,
                    ),
                    true,
                )
            }
        };
        frames.push(BlockChildFrame {
            border_box: crate::types::Rect::new(origin, geometry.size),
        });
        if advances_flow {
            cursor_y +=
                geometry.size.height + flow.internal.end + flow.extra_end + flow.margins.end;
            previous_margin_end = flow.margins.end;
        }
    }
    Some(frames)
}

fn collapsed_margin_start_extra(start: f32, previous_end: f32) -> f32 {
    let collapsed = if start >= 0.0 && previous_end >= 0.0 {
        start.max(previous_end)
    } else if start < 0.0 && previous_end < 0.0 {
        start.min(previous_end)
    } else {
        start + previous_end
    };
    collapsed - previous_end
}

fn container_source_height(element: &Container) -> f32 {
    let natural_height = element.box_model.padding.vertical()
        + element.box_model.border.vertical_width()
        + crate::layout::paginate::simulate_block_flow(&element.children).height;
    element.box_model.size.height.resolve(natural_height)
}
