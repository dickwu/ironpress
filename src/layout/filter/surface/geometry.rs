//! Geometry of a SourceGraphic and its physical raster backing.

use crate::layout::elements::{
    ColumnRule, Container, FlexRow, GridRow, Image, LayoutElement, LayoutVisitor, Positioning,
    TextBlock,
};
use crate::layout::flow_metrics::BlockMargins;
use crate::types::{EdgeSizes, Point, Size};

/// Border-box geometry retained when a painted filter source becomes an image.
#[derive(Debug, Clone)]
pub(crate) struct SourceGeometry {
    pub(crate) size: Size,
    pub(crate) margins: BlockMargins,
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
    fn resolve(size: Size, authored_overflow: EdgeSizes, dpi: f32) -> Option<Self> {
        let surface_size = Size::new(
            size.width + authored_overflow.horizontal(),
            size.height + authored_overflow.vertical(),
        );
        Some(Self {
            dimensions: crate::util::RasterDimensions {
                width: crate::render::blur::filter_raster_pixels_at_dpi(surface_size.width, dpi)?,
                height: crate::render::blur::filter_raster_pixels_at_dpi(surface_size.height, dpi)?,
            },
            border_origin: Point::new(authored_overflow.left, authored_overflow.top),
            paint_overflow: authored_overflow,
        })
    }
}

/// One completely painted, unfiltered `SourceGraphic`.
pub(crate) struct SourceGraphic {
    pub(crate) pixels: crate::render::raster_pixels::PremultipliedRgba8,
    pub(crate) geometry: SourceRasterGeometry,
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
    ) -> Option<Self> {
        let surface = RasterSurfaceFrame::resolve(layout.size, authored_overflow, dpi)?;
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
}

pub(crate) fn source_geometry(element: &dyn LayoutElement) -> Option<SourceGeometry> {
    struct Geometry(Option<SourceGeometry>);

    impl LayoutVisitor for Geometry {
        fn visit_column_rule(&mut self, element: &ColumnRule) {
            self.0 = Some(SourceGeometry {
                size: Size::new(element.paint.width, element.height),
                margins: BlockMargins::ZERO,
                positioning: element.positioning.clone(),
            });
        }

        fn visit_text_block(&mut self, element: &TextBlock) {
            let text_height = element.lines.iter().map(|line| line.height).sum::<f32>();
            let height = element.box_model.size.height.resolve(
                element.box_model.padding.vertical()
                    + text_height
                    + element.box_model.border.vertical_width(),
            );
            self.0 = element
                .box_model
                .size
                .width
                .fixed_value()
                .map(|width| SourceGeometry {
                    size: Size::new(width, height),
                    margins: element.box_model.margins,
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
                    margins: element.box_model.margins,
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
                    margins: element.box_model.margins,
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
                margins: element.box_model.margins,
                positioning: Default::default(),
            });
        }

        fn visit_image(&mut self, element: &Image) {
            self.0 = Some(SourceGeometry {
                size: element.geometry.size,
                margins: element.geometry.flow.margins,
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
            let text_height = element.lines.iter().map(|line| line.height).sum::<f32>();
            let height = element.box_model.size.height.resolve(
                element.box_model.padding.vertical()
                    + text_height
                    + element.box_model.border.vertical_width(),
            );
            self.geometry = Some(SourceGeometry {
                size: Size::new(self.available_width, height),
                margins: element.box_model.margins,
                positioning: element.positioning.clone(),
            });
        }

        fn visit_container(&mut self, element: &Container) {
            if element.box_model.size.width.is_fill_available() {
                self.geometry = Some(SourceGeometry {
                    size: Size::new(self.available_width, container_source_height(element)),
                    margins: element.box_model.margins,
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
pub(super) struct BlockChildFrame {
    pub(super) border_box: crate::types::Rect,
}

/// Resolve block children once for every SourceGraphic paint path. Sharing
/// this sequence prevents nested boxes from acquiring different device phases
/// based on which concrete parent type owns them.
pub(super) fn block_child_frames(
    children: &[crate::layout::elements::LayoutNode],
    content_box: crate::types::Rect,
    absolute_containing_block: Option<crate::types::Rect>,
) -> Option<Vec<BlockChildFrame>> {
    use crate::style::computed::Position;

    let mut frames = Vec::new();
    frames.try_reserve_exact(children.len()).ok()?;
    let mut cursor_y = content_box.origin.y;
    let mut previous_margin_end = 0.0;
    for child in children {
        let geometry = source_geometry_in_content(child.as_ref(), content_box.size.width)?;
        let positioning = &geometry.positioning;
        let (origin, advances_flow) = match positioning.scheme {
            Position::Absolute | Position::Fixed => {
                let containing_block = absolute_containing_block?;
                (
                    Point::new(
                        containing_block.origin.x + positioning.insets.left,
                        containing_block.origin.y + positioning.insets.top,
                    ),
                    false,
                )
            }
            Position::Static | Position::Relative | Position::Sticky => {
                cursor_y +=
                    collapsed_margin_start_extra(geometry.margins.start, previous_margin_end);
                (
                    Point::new(
                        content_box.origin.x + positioning.insets.left,
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
            cursor_y += geometry.size.height + geometry.margins.end;
            previous_margin_end = geometry.margins.end;
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
