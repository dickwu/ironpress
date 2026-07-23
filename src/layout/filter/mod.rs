//! CSS filter compositing over laid-out element subtrees.
//!
//! Layout supplies one semantic source tree. Filter painting turns that tree
//! into `SourceGraphic` once, then applies the ordered filter list to the
//! resulting surface. Individual boxes, glyphs, and replaced descendants must
//! never be filtered independently when CSS requires a group.

pub(crate) mod cells;
mod fallback;
pub(crate) mod surface;
mod vector_source;

pub(crate) use vector_source::ExactVectorFilterSource;

use std::collections::HashMap;

use crate::layout::elements::{
    Image, ImagePaint, ImageSampling, IntoLayoutNode, LayoutNode, LayoutVisitorMut,
    ReplacedGeometry,
};
use crate::layout::engine::{LayoutBorder, RasterImageAsset};
use crate::parser::dom::ElementNode;
use crate::parser::ttf::TtfFont;
use crate::style::computed::{ComputedStyle, FilterOperation, ObjectFit};
use crate::types::EdgeSizes;

/// An owned filter surface kept with the semantic box that produced it.
///
/// The raster is already rendered at the configured filter DPI. Its overflow
/// records the portion added outside the source border box by operations such
/// as blur and drop-shadow.
#[derive(Debug, Clone)]
pub(crate) struct FilterRasterOutput {
    pub(crate) asset: RasterImageAsset,
    pub(crate) raster_overflow: EdgeSizes,
}

/// A resolved filter list together with its color-interpolation space.
#[derive(Debug, Clone)]
pub(crate) struct ResolvedFilter {
    pub(crate) operations: Vec<FilterOperation>,
    pub(crate) linear_rgb: bool,
}

/// Effects applied after an element's filter has produced its composited
/// output. CSS applies these to the filtered group rather than to each source
/// primitive independently.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FilterCompositing {
    pub(crate) opacity: f32,
}

impl Default for FilterCompositing {
    fn default() -> Self {
        Self { opacity: 1.0 }
    }
}

impl ResolvedFilter {
    pub(crate) fn from_style(
        style: &mut ComputedStyle,
        definitions: &HashMap<String, ElementNode>,
    ) -> Self {
        let mut linear_rgb = false;
        if let Some(id) = style.filter.url_id.clone() {
            let Some(filter) = definitions.get(&id) else {
                style.filter = Default::default();
                return Self {
                    operations: Vec::new(),
                    linear_rgb,
                };
            };
            let (operations, uses_linear_rgb) =
                crate::parser::svg::filter_element_color_ops(filter);
            if !operations.is_empty() {
                linear_rgb = uses_linear_rgb;
            }
            style.filter.operations.extend(operations);
        }
        Self {
            operations: style.filter.operations.clone(),
            linear_rgb,
        }
    }

    pub(crate) fn has_composited_output(&self) -> bool {
        self.operations
            .iter()
            .any(FilterOperation::requires_group_rasterization)
    }
}

/// One renderer-owned filtered SourceGraphic, before it is reinserted into
/// normal flow as an atomic image.
pub(crate) struct FilteredGraphic {
    asset: RasterImageAsset,
    geometry: surface::SourceGeometry,
    overflow: EdgeSizes,
    group: crate::layout::elements::PaintGroup,
}

impl FilteredGraphic {
    pub(crate) fn into_layout_node(self) -> LayoutNode {
        Image {
            source: self.asset,
            geometry: ReplacedGeometry::new(
                self.geometry.size,
                self.geometry.margins,
                LayoutBorder::default(),
            ),
            positioning: self.geometry.positioning,
            sampling: ImageSampling {
                object_fit: ObjectFit::Fill,
                ..Default::default()
            },
            paint: ImagePaint {
                raster_overflow: self.overflow,
                group: self.group,
                ..Default::default()
            },
        }
        .boxed()
    }
}

/// Retain a resolved filter on its semantic box until pagination has split the
/// box into the fragments to which graphical effects are applied.
pub(crate) fn retain_for_fragmentation(
    element: &mut dyn crate::layout::elements::LayoutElement,
    filter: &ResolvedFilter,
) -> bool {
    let Some(holder) = element.filter_holder_mut() else {
        return false;
    };
    *holder.filter_slot_mut() = Some(filter.clone());
    true
}

/// Materialize every retained filter after pagination, deepest descendants
/// first. Child replacement is intentionally exposed by the layout tree as a
/// generic node operation so nested filters compose at arbitrary depth.
pub(crate) fn materialize_page_filters(
    pages: &mut [crate::layout::engine::Page],
    fonts: &HashMap<String, TtfFont>,
    filter_dpi: f32,
) {
    for page in pages {
        for (_, element) in &mut page.elements {
            materialize_node_filter(element, fonts, filter_dpi);
        }
        for element in page.running_elements.values_mut() {
            materialize_node_filter(element, fonts, filter_dpi);
        }
    }
}

fn materialize_node_filter(
    element: &mut LayoutNode,
    fonts: &HashMap<String, TtfFont>,
    filter_dpi: f32,
) {
    element.visit_child_nodes_mut(&mut |child| {
        materialize_node_filter(child, fonts, filter_dpi);
    });

    struct CellFilterMaterializer<'a> {
        fonts: &'a HashMap<String, TtfFont>,
        filter_dpi: f32,
    }

    impl LayoutVisitorMut for CellFilterMaterializer<'_> {
        fn visit_flex_row(&mut self, element: &mut crate::layout::elements::FlexRow) {
            cells::materialize_flex_row(element, self.fonts, self.filter_dpi);
        }
    }

    element.accept_mut(&mut CellFilterMaterializer { fonts, filter_dpi });

    let Some(filter) = element
        .filter_holder_mut()
        .and_then(crate::layout::elements::FilterHolder::take_filter)
    else {
        return;
    };
    if let Some(graphic) = composite_source(element.as_ref(), &filter, fonts, filter_dpi) {
        *element = graphic.into_layout_node();
    } else {
        filter.apply_primitive_fallback(std::slice::from_mut(element));
    }
}

pub(crate) fn composite_source(
    element: &dyn crate::layout::elements::LayoutElement,
    filter: &ResolvedFilter,
    fonts: &HashMap<String, TtfFont>,
    filter_dpi: f32,
) -> Option<FilteredGraphic> {
    let exact_vector = element
        .exact_vector_filter_source()
        .is_some_and(|source| source.supports_exact_vector_filter(&filter.operations));
    if !filter.has_composited_output() || exact_vector {
        return None;
    }
    let source = surface::paint_source_graphic(element, fonts, filter_dpi)?;
    let (output, geometry) =
        composite_source_graphic(source, filter, Default::default(), filter_dpi)?;
    Some(FilteredGraphic {
        asset: output.asset,
        geometry,
        overflow: output.raster_overflow,
        group: element
            .paint_group_owner()
            .map(crate::layout::elements::PaintGroupOwner::paint_group)
            .cloned()
            .unwrap_or_default()
            .with_materialized_filter(),
    })
}

/// Apply a resolved filter to a source surface while retaining the source box
/// geometry separately from filter overflow.
pub(crate) fn composite_source_graphic(
    source: surface::SourceGraphic,
    filter: &ResolvedFilter,
    compositing: FilterCompositing,
    filter_dpi: f32,
) -> Option<(FilterRasterOutput, surface::SourceGeometry)> {
    if !filter.has_composited_output() {
        return None;
    }
    let surface_size = source.geometry.surface_size();
    let mut filtered = crate::render::filter::apply_operations_to_surface(
        &source.pixels,
        surface_size,
        &filter.operations,
        filter.linear_rgb,
        filter_dpi,
    )?;
    if compositing.opacity < 1.0 {
        let opacity = compositing.opacity.clamp(0.0, 1.0);
        for pixel in filtered.pixels.pixels_mut() {
            pixel[3] = (f32::from(pixel[3]) * opacity).round() as u8;
        }
    }
    let layout_geometry = source.geometry.layout;
    let raster_overflow = source.geometry.paint_overflow + filtered.overflow;
    Some((
        FilterRasterOutput {
            asset: crate::render::blur::rgba_to_png_alpha_asset(filtered.pixels, filter_dpi)?,
            raster_overflow,
        },
        layout_geometry,
    ))
}
