//! Post-fragmentation materialization of retained CSS filters.
//!
//! Traversal is always performed through `LayoutElement::visit_child_nodes_mut`.
//! Formatting contexts may refine the absolute raster anchors supplied to that
//! traversal, but failure to resolve a refinement never suppresses a child.

use std::collections::HashMap;

use crate::layout::elements::{
    Container, FlexRow, GridRow, LayoutElement, LayoutNode, LayoutVisitor, LayoutVisitorMut,
};
use crate::parser::ttf::TtfFont;
use crate::types::{EdgeSizes, Point, Rect};

use super::surface::SourceRasterAnchor;

/// Absolute raster anchors for every direct node exposed by one layout
/// element, in the same order as `visit_child_nodes_mut`.
///
/// An absent plan means only that this formatting context cannot refine child
/// coordinates. It never changes whether those children are visited.
struct ChildRasterAnchors(Option<Vec<SourceRasterAnchor>>);

impl ChildRasterAnchors {
    fn resolve(
        element: &dyn LayoutElement,
        parent_anchor: SourceRasterAnchor,
        fonts: &HashMap<String, TtfFont>,
    ) -> Self {
        struct Resolver<'a> {
            parent_anchor: SourceRasterAnchor,
            fonts: &'a HashMap<String, TtfFont>,
            anchors: Option<Vec<SourceRasterAnchor>>,
        }

        impl Resolver<'_> {
            fn block_anchors(
                &self,
                children: &[LayoutNode],
                border_box: Rect,
                border: EdgeSizes,
                padding: EdgeSizes,
            ) -> Option<Vec<SourceRasterAnchor>> {
                let padding_box = border_box.inset(border);
                let content_box = padding_box.inset(padding);
                super::surface::block_child_frames(children, content_box, Some(padding_box)).map(
                    |frames| {
                        frames
                            .into_iter()
                            .map(|frame| {
                                SourceRasterAnchor::at_border_origin(frame.border_box.origin)
                            })
                            .collect()
                    },
                )
            }
        }

        impl LayoutVisitor for Resolver<'_> {
            fn visit_container(&mut self, element: &Container) {
                let Some(geometry) = super::surface::source_geometry(element) else {
                    return;
                };
                let border_box = Rect::new(self.parent_anchor.border_origin(), geometry.size);
                self.anchors = self.block_anchors(
                    &element.children,
                    border_box,
                    element.box_model.border.widths(),
                    element.box_model.padding,
                );
            }

            fn visit_flex_row(&mut self, element: &FlexRow) {
                let frames = super::surface::flex_cell_source_frames(element, self.fonts);
                let mut anchors = Vec::new();
                for (cell, frame) in element.content.cells.iter().zip(frames) {
                    let cell_anchor = frame.anchor_in(self.parent_anchor);
                    let cell_box = Rect::new(cell_anchor.border_origin(), frame.size);
                    let nested = self.block_anchors(
                        &cell.nested_elements,
                        cell_box,
                        cell.border.widths(),
                        cell.padding,
                    );
                    match nested {
                        Some(nested) => anchors.extend(nested),
                        None => anchors.extend(cell.nested_elements.iter().map(|_| cell_anchor)),
                    }
                }
                self.anchors = Some(anchors);
            }

            fn visit_grid_row(&mut self, element: &GridRow) {
                let frames = super::surface::grid_cell_source_frames(element);
                let mut anchors = Vec::new();
                for (cell, frame) in element.content.cells.iter().zip(frames) {
                    let cell_anchor = frame.anchor_in(self.parent_anchor);
                    let cell_box = Rect::new(cell_anchor.border_origin(), frame.size);
                    let nested = self.block_anchors(
                        &cell.layout.content.children,
                        cell_box,
                        cell.layout.box_model.border.widths(),
                        cell.layout.box_model.padding(),
                    );
                    match nested {
                        Some(nested) => anchors.extend(nested),
                        None => {
                            anchors.extend(cell.layout.content.children.iter().map(|_| cell_anchor))
                        }
                    }
                }
                self.anchors = Some(anchors);
            }
        }

        let mut resolver = Resolver {
            parent_anchor,
            fonts,
            anchors: None,
        };
        element.accept(&mut resolver);
        Self(resolver.anchors)
    }

    fn into_iter(self, fallback: SourceRasterAnchor) -> ChildRasterAnchorIter {
        ChildRasterAnchorIter {
            resolved: self.0.unwrap_or_default().into_iter(),
            fallback,
        }
    }
}

struct ChildRasterAnchorIter {
    resolved: std::vec::IntoIter<SourceRasterAnchor>,
    fallback: SourceRasterAnchor,
}

impl ChildRasterAnchorIter {
    fn next(&mut self) -> SourceRasterAnchor {
        self.resolved.next().unwrap_or(self.fallback)
    }
}

/// Materialize every retained filter after pagination, deepest descendants
/// first. A single generic traversal applies to every concrete layout node.
pub(crate) fn materialize_page_filters(
    pages: &mut [crate::layout::engine::Page],
    document_margin: crate::types::Margin,
    fonts: &HashMap<String, TtfFont>,
    filter_dpi: f32,
) {
    for page in pages {
        let margin = page.margin_override.unwrap_or(document_margin);
        for (y, element) in &mut page.elements {
            materialize_node_filter(
                element,
                source_raster_anchor(element.as_ref(), Point::new(margin.left, margin.top + *y)),
                fonts,
                filter_dpi,
            );
        }
        for element in page.running_elements.values_mut() {
            materialize_node_filter(
                element,
                source_raster_anchor(element.as_ref(), Point::new(margin.left, margin.top)),
                fonts,
                filter_dpi,
            );
        }
    }
}

fn materialize_node_filter(
    element: &mut LayoutNode,
    anchor: SourceRasterAnchor,
    fonts: &HashMap<String, TtfFont>,
    filter_dpi: f32,
) {
    let mut child_anchors =
        ChildRasterAnchors::resolve(element.as_ref(), anchor, fonts).into_iter(anchor);
    element.visit_child_nodes_mut(&mut |child| {
        materialize_node_filter(child, child_anchors.next(), fonts, filter_dpi);
    });

    struct CellFilterMaterializer<'a> {
        anchor: SourceRasterAnchor,
        fonts: &'a HashMap<String, TtfFont>,
        filter_dpi: f32,
    }

    impl LayoutVisitorMut for CellFilterMaterializer<'_> {
        fn visit_flex_row(&mut self, element: &mut FlexRow) {
            super::cells::materialize_flex_row(element, self.anchor, self.fonts, self.filter_dpi);
        }

        fn visit_grid_row(&mut self, element: &mut GridRow) {
            super::cells::materialize_grid_row(element, self.anchor, self.fonts, self.filter_dpi);
        }
    }

    element.accept_mut(&mut CellFilterMaterializer {
        anchor,
        fonts,
        filter_dpi,
    });

    let Some(filter) = element
        .filter_holder_mut()
        .and_then(crate::layout::elements::FilterHolder::take_filter)
    else {
        return;
    };
    if let Some(graphic) =
        super::composite_source(element.as_ref(), &filter, fonts, filter_dpi, anchor)
    {
        *element = graphic.into_layout_node();
    } else {
        filter.apply_primitive_fallback(std::slice::from_mut(element));
    }
}

fn source_raster_anchor(element: &dyn LayoutElement, flow_origin: Point) -> SourceRasterAnchor {
    let insets = super::surface::source_geometry(element)
        .map(|geometry| geometry.positioning.insets)
        .or_else(|| {
            element
                .positioning_owner()
                .map(|owner| owner.positioning().insets)
        })
        .unwrap_or(EdgeSizes::ZERO);
    SourceRasterAnchor::at_border_origin(Point::new(
        flow_origin.x + insets.left,
        flow_origin.y + insets.top,
    ))
}
