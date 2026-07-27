//! Hierarchical paint-group compositing inside one filter SourceGraphic.

use crate::layout::elements::{PaintGroup, TransformReferenceBox};
use crate::render::raster_pixels::{PremultipliedRgba8, RasterGroupCompositing};
use crate::style::computed::{BlendMode, CssAffineMatrix};
use crate::types::{EdgeSizes, Rect};

use super::canvas::{PaintBounds, RasterCanvas};
use super::painter::{ElementPaintSpace, RootEffectHandling, SourcePainter};

/// A paint group resolved against one concrete border box.
///
/// Unsupported post-compositing effects fail at this single boundary instead
/// of being rediscovered by every concrete source painter.
struct SourcePaintGroup {
    transform: CssAffineMatrix,
    opacity: f32,
    blend_mode: BlendMode,
    requires_isolation: bool,
}

impl SourcePaintGroup {
    fn resolve(
        group: &PaintGroup,
        border_box: Rect,
        reference_box: Option<&dyn TransformReferenceBox>,
    ) -> Option<Self> {
        if !group.effects.masking.is_none() {
            return None;
        }
        let transform = group
            .transform
            .resolve(
                border_box,
                reference_box.map_or(EdgeSizes::ZERO, TransformReferenceBox::content_insets),
            )
            .unwrap_or_default();
        Some(Self {
            transform,
            opacity: group.effects.opacity,
            blend_mode: group.effects.mix_blend_mode,
            requires_isolation: group.transform.value.is_some()
                || group.effects.needs_source_isolation(),
        })
    }
}

impl SourcePainter<'_> {
    /// Paint one complete source subtree in its own group when CSS effects
    /// require isolation. The caller supplies only semantic box geometry and
    /// the ordinary source-paint operation.
    pub(super) fn paint_group(
        &mut self,
        space: ElementPaintSpace,
        group: &PaintGroup,
        reference_box: Option<&dyn TransformReferenceBox>,
        paint_source: impl FnOnce(&mut SourcePainter<'_>) -> Option<()>,
    ) -> Option<()> {
        if space.root_effects == RootEffectHandling::DeferToOwner {
            return self.paint_directly(space, paint_source);
        }
        let compositing = SourcePaintGroup::resolve(group, space.border_box, reference_box)?;
        if !compositing.requires_isolation {
            return self.paint_directly(space, paint_source);
        }

        let mut group_pixels = PremultipliedRgba8::transparent(
            self.canvas.pixels.width(),
            self.canvas.pixels.height(),
        );
        let mut group_bounds = PaintBounds::default();
        {
            let canvas = RasterCanvas {
                pixels: &mut group_pixels,
                pixels_per_point: self.canvas.pixels_per_point,
                paint_bounds: &mut group_bounds,
            };
            let mut group_painter = SourcePainter::new(
                canvas,
                space.with_root_effects(RootEffectHandling::DeferToOwner),
                self.fonts,
                self.filter_dpi,
            );
            paint_source(&mut group_painter)?;
        }
        self.canvas.pixels.composite_group(
            &group_pixels,
            RasterGroupCompositing::from_css(
                compositing.transform,
                compositing.opacity,
                compositing.blend_mode,
                self.canvas.pixels_per_point,
            ),
        )?;
        if compositing.opacity > 0.0 {
            self.canvas
                .paint_bounds
                .include_transformed(group_bounds, compositing.transform);
        }
        Some(())
    }

    fn paint_directly(
        &mut self,
        space: ElementPaintSpace,
        paint_source: impl FnOnce(&mut SourcePainter<'_>) -> Option<()>,
    ) -> Option<()> {
        let canvas = RasterCanvas {
            pixels: &mut *self.canvas.pixels,
            pixels_per_point: self.canvas.pixels_per_point,
            paint_bounds: &mut *self.canvas.paint_bounds,
        };
        let mut painter = SourcePainter::new(canvas, space, self.fonts, self.filter_dpi);
        paint_source(&mut painter)
    }
}
