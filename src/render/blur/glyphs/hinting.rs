//! Fontations outline hinting for raster filter text.

use crate::parser::ttf::TtfFont;
use crate::text::ShapedGlyph;
use resvg::tiny_skia;
use skrifa::MetadataProvider as _;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{Engine, HintingInstance, HintingOptions, OutlinePen, SmoothMode, Target};

struct GlyphPathPen<'a> {
    builder: &'a mut tiny_skia::PathBuilder,
    pen_x: f32,
    baseline_y: f32,
    shear: f32,
}

impl GlyphPathPen<'_> {
    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        (self.pen_x + x + self.shear * y, self.baseline_y - y)
    }
}

impl OutlinePen for GlyphPathPen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.point(x, y);
        self.builder.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.point(x, y);
        self.builder.line_to(x, y);
    }

    fn quad_to(&mut self, control_x: f32, control_y: f32, x: f32, y: f32) {
        let (control_x, control_y) = self.point(control_x, control_y);
        let (x, y) = self.point(x, y);
        self.builder.quad_to(control_x, control_y, x, y);
    }

    fn curve_to(
        &mut self,
        first_x: f32,
        first_y: f32,
        second_x: f32,
        second_y: f32,
        x: f32,
        y: f32,
    ) {
        let (first_x, first_y) = self.point(first_x, first_y);
        let (second_x, second_y) = self.point(second_x, second_y);
        let (x, y) = self.point(x, y);
        self.builder
            .cubic_to(first_x, first_y, second_x, second_y, x, y);
    }

    fn close(&mut self) {
        self.builder.close();
    }
}

/// Build one hinted device-space path while preserving shaped run advances.
pub(super) fn hinted_run_path(
    font: &TtfFont,
    size_px: f32,
    glyphs: &[ShapedGlyph],
    points_to_pixels: f32,
    shear: f32,
) -> Option<tiny_skia::Path> {
    if !size_px.is_finite() || size_px <= 0.0 {
        return None;
    }
    let font_ref = skrifa::FontRef::from_index(&font.data, font.face_index.get()).ok()?;
    let outlines = font_ref.outline_glyphs();
    let hinting = HintingInstance::new(
        &outlines,
        Size::new(size_px),
        LocationRef::default(),
        HintingOptions {
            engine: Engine::AutoFallback,
            target: Target::from(SmoothMode::Normal),
        },
    )
    .ok()?;

    let mut builder = tiny_skia::PathBuilder::new();
    let mut pen_x = 0.0;
    for shaped in glyphs {
        let glyph = outlines.get(skrifa::GlyphId::new(u32::from(shaped.glyph_id)))?;
        let mut pen = GlyphPathPen {
            builder: &mut builder,
            pen_x: pen_x + shaped.x_offset * points_to_pixels,
            baseline_y: -shaped.y_offset * points_to_pixels,
            shear,
        };
        glyph.draw(&hinting, &mut pen).ok()?;
        pen_x += shaped.x_advance * points_to_pixels;
    }
    builder.finish()
}
