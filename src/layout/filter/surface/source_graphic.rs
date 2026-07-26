//! Two-pass allocation for one recursively painted filter source.

use std::collections::HashMap;

use crate::layout::elements::LayoutElement;
use crate::parser::ttf::TtfFont;
use crate::render::raster_pixels::PremultipliedRgba8;
use crate::types::EdgeSizes;

use super::canvas::{PaintBounds, RasterCanvas, SurfaceRect};
use super::geometry::{SourceGraphic, SourceRasterAnchor, SourceRasterGeometry, source_geometry};
use super::{ElementPaintSpace, RootEffectHandling, paint_element, source_paint_overflow};

struct SourcePaintPass {
    pixels: PremultipliedRgba8,
    paint_bounds: Option<crate::types::Rect>,
}

impl SourcePaintPass {
    fn paint(
        element: &dyn LayoutElement,
        geometry: &SourceRasterGeometry,
        fonts: &HashMap<String, TtfFont>,
        filter_dpi: f32,
    ) -> Option<Self> {
        let dimensions = geometry.dimensions();
        let mut pixels = PremultipliedRgba8::transparent(dimensions.width, dimensions.height);
        let mut paint_bounds = PaintBounds::default();
        {
            let mut canvas = RasterCanvas {
                pixels: &mut pixels,
                pixels_per_point: crate::render::blur::px_per_pt_at_dpi(filter_dpi),
                paint_bounds: &mut paint_bounds,
            };
            paint_element(
                &mut canvas,
                element,
                ElementPaintSpace {
                    border_box: SurfaceRect::new(geometry.border_origin(), geometry.layout.size),
                    css_pixel_grid_origin: geometry.border_origin(),
                    inherited_containing_block: None,
                    establishes_containing_block: true,
                    root_effects: RootEffectHandling::DeferToOwner,
                },
                fonts,
                filter_dpi,
            )?;
        }
        Some(Self {
            pixels,
            paint_bounds: paint_bounds.resolve(),
        })
    }

    fn into_source(self, geometry: SourceRasterGeometry) -> SourceGraphic {
        SourceGraphic {
            pixels: self.pixels,
            geometry,
            paint_bounds: self.paint_bounds,
        }
    }
}

/// Paint the complete recursive source, expanding once when the first semantic
/// pass discovers positioned descendants outside the provisional allocation.
pub(crate) fn paint_source_graphic(
    element: &dyn LayoutElement,
    fonts: &HashMap<String, TtfFont>,
    filter_dpi: f32,
    anchor: SourceRasterAnchor,
) -> Option<SourceGraphic> {
    let layout = source_geometry(element)?;
    let authored_overflow = source_paint_overflow(element, layout.size, filter_dpi)?;
    let provisional =
        SourceRasterGeometry::resolve(layout.clone(), authored_overflow, filter_dpi, anchor)?;
    let first_pass = SourcePaintPass::paint(element, &provisional, fonts, filter_dpi)?;
    let required_overflow = first_pass.paint_bounds.map_or(EdgeSizes::ZERO, |bounds| {
        provisional.required_overflow_for(bounds)
    });
    if provisional
        .paint_overflow()
        .contains_each(required_overflow)
    {
        return Some(first_pass.into_source(provisional));
    }

    let geometry = SourceRasterGeometry::resolve(
        layout,
        authored_overflow.max_each(required_overflow),
        filter_dpi,
        anchor,
    )?;
    SourcePaintPass::paint(element, &geometry, fonts, filter_dpi)
        .map(|paint| paint.into_source(geometry))
}
