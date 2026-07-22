//! Raster `SourceGraphic` painter for CSS filters.

use std::collections::HashMap;

use crate::layout::cells::GridCell;
use crate::layout::elements::{
    BoxModel, BoxPaint, ColumnRule, Container, FlexRow, GridRow, Image, LayoutElement,
    LayoutVisitor, MulticolContainer, Positioning, TextBlock, visit_layout_tree,
};
use crate::layout::engine::{FlexCell, TextLine, TextRun};
use crate::layout::flow_metrics::BlockMargins;
use crate::parser::ttf::TtfFont;
use crate::render::borders::CssRoundedRect;
use crate::style::computed::{AlignItems, Position, TextAlign};
use crate::types::{Color, EdgeSizes, Point, Size};

mod canvas;
mod gradient;
mod source_borders;

use canvas::{DevicePoint, RasterCanvas, SurfaceRect, box_shadow_overflow};
use gradient::FilterBackground;

/// Border-box geometry retained when a painted filter source becomes an image.
#[derive(Debug, Clone)]
pub(crate) struct SourceGeometry {
    pub(crate) size: Size,
    pub(crate) margins: BlockMargins,
    pub(crate) positioning: Positioning,
}

/// One completely painted, unfiltered `SourceGraphic`.
pub(crate) struct SourceGraphic {
    pub(crate) pixels: image::RgbaImage,
    pub(crate) geometry: SourceRasterGeometry,
}

/// Relationship between the layout border box and its offscreen paint surface.
///
/// Layout retains the unexpanded border box. Paint overflow only changes the
/// raster's origin and extent, so reinserting a filtered image never changes
/// normal flow.
pub(crate) struct SourceRasterGeometry {
    pub(crate) layout: SourceGeometry,
    pub(crate) paint_overflow: EdgeSizes,
}

impl SourceRasterGeometry {
    pub(super) fn surface_size(&self) -> Size {
        Size::new(
            self.layout.size.width + self.paint_overflow.horizontal(),
            self.layout.size.height + self.paint_overflow.vertical(),
        )
    }

    fn border_origin(&self) -> Point {
        Point::new(self.paint_overflow.left, self.paint_overflow.top)
    }
}

/// Common box state used by the source painter without flattening concrete
/// layout elements into another tagged representation.
trait FilterBox {
    fn box_model(&self) -> &BoxModel;
    fn paint(&self) -> &BoxPaint;
    fn positioning(&self) -> &Positioning;
}

/// Coordinates and inherited positioning state for one semantic source box.
/// Keeping this together prevents recursive paint paths from silently
/// re-anchoring absolute descendants to an intervening static box.
#[derive(Clone, Copy)]
struct ElementPaintSpace {
    border_box: SurfaceRect,
    inherited_containing_block: Option<SurfaceRect>,
    establishes_containing_block: bool,
    root_effects: RootEffectHandling,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RootEffectHandling {
    Paint,
    DeferToOwner,
}

/// The two CSS box edges needed while painting descendants. Normal-flow
/// children start in the content box; absolutely positioned descendants use
/// the containing padding box.
#[derive(Clone, Copy)]
struct DescendantPaintArea {
    content_box: SurfaceRect,
    absolute_containing_block: Option<SurfaceRect>,
    direct_child_effects: RootEffectHandling,
}

impl DescendantPaintArea {
    fn after_normal_flow(self, consumed: f32) -> Self {
        Self {
            content_box: SurfaceRect::new(
                Point::new(
                    self.content_box.origin.x,
                    self.content_box.origin.y + consumed,
                ),
                Size::new(
                    self.content_box.size.width,
                    (self.content_box.size.height - consumed).max(0.0),
                ),
            ),
            ..self
        }
    }
}

impl FilterBox for TextBlock {
    fn box_model(&self) -> &BoxModel {
        &self.box_model
    }

    fn paint(&self) -> &BoxPaint {
        &self.paint
    }

    fn positioning(&self) -> &Positioning {
        &self.positioning
    }
}

impl FilterBox for Container {
    fn box_model(&self) -> &BoxModel {
        &self.box_model
    }

    fn paint(&self) -> &BoxPaint {
        &self.paint
    }

    fn positioning(&self) -> &Positioning {
        &self.positioning
    }
}

impl FilterBox for FlexRow {
    fn box_model(&self) -> &BoxModel {
        &self.box_model
    }

    fn paint(&self) -> &BoxPaint {
        &self.paint
    }

    fn positioning(&self) -> &Positioning {
        &self.positioning
    }
}

struct SourcePainter<'a> {
    canvas: RasterCanvas<'a>,
    space: ElementPaintSpace,
    fonts: &'a HashMap<String, TtfFont>,
    filter_dpi: f32,
    result: Option<()>,
}

impl SourcePainter<'_> {
    fn paint_clipped_descendants(
        &mut self,
        clip: CssRoundedRect,
        paint: impl FnOnce(&mut SourcePainter<'_>) -> Option<()>,
    ) -> Option<()> {
        let mut group =
            image::RgbaImage::new(self.canvas.pixels.width(), self.canvas.pixels.height());
        let mut descendant_painter = SourcePainter {
            canvas: RasterCanvas {
                pixels: &mut group,
                pixels_per_point: self.canvas.pixels_per_point,
            },
            space: self.space,
            fonts: self.fonts,
            filter_dpi: self.filter_dpi,
            result: None,
        };
        paint(&mut descendant_painter)?;
        self.canvas.composite_clipped_group(&group, clip);
        Some(())
    }

    fn paint_box(&mut self, element: &impl FilterBox) -> Option<DescendantPaintArea> {
        let model = element.box_model();
        let paint = element.paint();
        if !paint.visible
            || (paint.group.effects.opacity < 1.0
                && self.space.root_effects == RootEffectHandling::Paint)
            || paint.group.effects.mix_blend_mode != crate::style::computed::BlendMode::Normal
            || paint.outline.width > 0.0
        {
            return None;
        }
        let rect = self.space.border_box;
        let background =
            FilterBackground::resolve(&paint.background, model, rect, paint.border_radii)?;
        self.canvas
            .paint_outset_shadows(rect, &paint.shadows, self.filter_dpi)?;
        background.paint(&mut self.canvas);
        let padding_box = rect.inset(model.border.widths());
        self.canvas
            .paint_inset_shadows(padding_box, &paint.shadows, self.filter_dpi)?;
        self.canvas
            .paint_border(rect, &model.border, paint.border_radii)?;
        let absolute_containing_block = if self.space.establishes_containing_block
            || element.positioning().scheme != Position::Static
        {
            Some(padding_box)
        } else {
            self.space.inherited_containing_block
        };
        Some(DescendantPaintArea {
            content_box: rect.inset(model.border.widths() + model.padding),
            absolute_containing_block,
            direct_child_effects: RootEffectHandling::Paint,
        })
    }

    fn paint_text_lines(
        &mut self,
        lines: &[TextLine],
        content: SurfaceRect,
        alignment: TextAlign,
        indent: f32,
    ) -> Option<()> {
        let mut line_top = content.origin.y;
        for (line_index, line) in lines.iter().enumerate() {
            let baseline_ascent = line_baseline_ascent(line, self.fonts);
            let baseline = line_top + baseline_ascent;
            let runs = merged_runs(&line.runs);
            let parent_font_size = crate::layout::text::line_primary_font_size(&runs);
            let line_width = runs
                .iter()
                .map(|run| run_width(run, self.fonts))
                .sum::<Option<f32>>()?;
            let first_indent = if line_index == 0 { indent } else { 0.0 };
            let line_x = match alignment {
                TextAlign::Right => {
                    content.origin.x
                        + first_indent
                        + (content.size.width - first_indent - line_width).max(0.0)
                }
                TextAlign::Center => {
                    content.origin.x
                        + first_indent
                        + (content.size.width - first_indent - line_width).max(0.0) / 2.0
                }
                _ => content.origin.x + first_indent,
            } + line.x_offset;
            let mut run_x = line_x;
            for run in &runs {
                if run.inline_box.is_some() || run.background_color.is_some() || run.text.is_empty()
                {
                    return None;
                }
                let (_, font) = crate::text::resolve_custom_font(
                    &run.font_family,
                    run.bold,
                    run.font_style.is_slanted(),
                    self.fonts,
                )?;
                let mut shaped = crate::text::shape_text_run(run, self.fonts)?;
                if run.metadata.letter_spacing != 0.0 {
                    let spaced_glyphs = shaped.glyphs.len().saturating_sub(1);
                    for glyph in shaped.glyphs.iter_mut().take(spaced_glyphs) {
                        glyph.x_advance += run.metadata.letter_spacing;
                    }
                }
                if run.metadata.word_spacing != 0.0 {
                    for glyph in &mut shaped.glyphs {
                        if glyph.unicode.as_slice() == [0x0020] {
                            glyph.x_advance += run.metadata.word_spacing;
                        }
                    }
                }
                let raster = crate::render::blur::rasterize_run_alpha(
                    crate::render::blur::GlyphRasterRequest {
                        font,
                        font_size: font.adjusted_font_size(run.font_size),
                        glyphs: &shaped.glyphs,
                        style: crate::render::blur::GlyphRasterStyle {
                            outline: run
                                .synthetic_bold_stroke_width(self.fonts)
                                .unwrap_or_default(),
                            shear: run.synthetic_italic_shear(self.fonts).unwrap_or_default(),
                            ..Default::default()
                        },
                        dpi: self.filter_dpi,
                    },
                )?;
                let run_baseline = baseline - run.glyph_baseline_shift(parent_font_size);
                let run_advance = run_width(run, self.fonts)?;
                for shadow in run.text_shadow.iter().rev() {
                    if shadow.blur > 0.0 {
                        return None;
                    }
                    let shadow_offset = Point::new(shadow.offset_x, shadow.offset_y);
                    self.paint_run_decorations(
                        run,
                        run_x,
                        run_advance,
                        run_baseline,
                        shadow_offset,
                        Some(shadow.color),
                        SurfaceDecorationPhase::All,
                    )?;
                    self.canvas.composite_mask(
                        &raster.mask,
                        DevicePoint::new(
                            ((run_x + shadow.offset_x) * self.canvas.pixels_per_point
                                - raster.origin_x_px)
                                .round() as i32,
                            ((run_baseline + shadow.offset_y) * self.canvas.pixels_per_point
                                - raster.baseline_y_px)
                                .round() as i32,
                        ),
                        shadow.color,
                    );
                }
                self.paint_run_decorations(
                    run,
                    run_x,
                    run_advance,
                    run_baseline,
                    Point::default(),
                    None,
                    SurfaceDecorationPhase::BelowText,
                )?;
                self.canvas.composite_mask(
                    &raster.mask,
                    DevicePoint::new(
                        (run_x * self.canvas.pixels_per_point - raster.origin_x_px).round() as i32,
                        (run_baseline * self.canvas.pixels_per_point - raster.baseline_y_px).round()
                            as i32,
                    ),
                    run.color,
                );
                self.paint_run_decorations(
                    run,
                    run_x,
                    run_advance,
                    run_baseline,
                    Point::default(),
                    None,
                    SurfaceDecorationPhase::AboveText,
                )?;
                run_x += run_advance;
            }
            line_top += line.height;
        }
        Some(())
    }

    fn paint_run_decorations(
        &mut self,
        run: &TextRun,
        run_start: f32,
        run_width: f32,
        baseline: f32,
        offset: Point,
        color_override: Option<Color>,
        phase: SurfaceDecorationPhase,
    ) -> Option<()> {
        if run.decorations.is_empty() {
            return Some(());
        }
        if run.decorations.iter().any(|decoration| {
            decoration.style != crate::style::computed::TextDecorationStyle::Solid
        }) {
            return None;
        }
        let (leading, trailing) =
            crate::render::text_decoration::whitespace_insets(run, self.fonts);
        let start = run_start + leading + offset.x;
        let width = (run_width - leading - trailing).max(0.0);
        for decoration in &run.decorations {
            let color = color_override.unwrap_or_else(|| decoration.resolved_color(run.color));
            let thickness = crate::render::text_decoration::thickness(run, decoration);
            let mut paint_line = |line, center_y: f32| {
                let axis_from_baseline = baseline + offset.y - center_y;
                let exclusions = crate::render::text_decoration::ink_skip_intervals(
                    run,
                    decoration,
                    line,
                    axis_from_baseline,
                    self.fonts,
                )
                .into_iter()
                .map(|interval| interval.translated(run_start + offset.x));
                for segment in crate::render::text_decoration::visible_segments(
                    crate::render::text_decoration::InlineInterval::new(start, start + width),
                    exclusions,
                ) {
                    self.canvas.fill(
                        SurfaceRect::new(
                            Point::new(segment.start, center_y - thickness / 2.0),
                            Size::new(segment.end - segment.start, thickness),
                        ),
                        color,
                    );
                }
            };
            if decoration.lines.underline && phase.paints_below_text() {
                paint_line(
                    crate::render::text_decoration::DecorationLine::Underline,
                    baseline
                        + crate::render::text_decoration::underline_distance_from_baseline(
                            run, decoration,
                        )
                        + offset.y,
                );
            }
            if decoration.lines.line_through && phase.paints_above_text() {
                paint_line(
                    crate::render::text_decoration::DecorationLine::LineThrough,
                    baseline - run.font_size * 0.3 + offset.y,
                );
            }
            if decoration.lines.overline && phase.paints_below_text() {
                let (ascender_ratio, _) = crate::fonts::font_metrics_ratios(
                    &run.font_family,
                    run.bold,
                    run.font_style.is_slanted(),
                    self.fonts,
                );
                paint_line(
                    crate::render::text_decoration::DecorationLine::Overline,
                    baseline
                        - ascender_ratio * run.font_size
                        - crate::render::text_decoration::overline_lift(run)
                        + offset.y,
                );
            }
        }
        Some(())
    }

    fn paint_container_children(
        &mut self,
        children: &[crate::layout::elements::LayoutNode],
        area: DescendantPaintArea,
    ) -> Option<()> {
        self.paint_children(children, area)
    }

    fn paint_children(
        &mut self,
        children: &[crate::layout::elements::LayoutNode],
        area: DescendantPaintArea,
    ) -> Option<()> {
        let mut cursor_y = area.content_box.origin.y;
        let mut previous_margin_end = 0.0;
        for child in children {
            let geometry = source_geometry_in_content(child.as_ref(), area.content_box.size.width)?;
            let positioning = &geometry.positioning;
            if child
                .paint_group_owner()
                .is_some_and(|owner| owner.paint_group().transform.value.is_some())
                && area.direct_child_effects != RootEffectHandling::DeferToOwner
            {
                return None;
            }
            let (origin, advances_flow) = match positioning.scheme {
                Position::Absolute | Position::Fixed => {
                    let containing_block = area.absolute_containing_block?;
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
                            area.content_box.origin.x + positioning.insets.left,
                            cursor_y + positioning.insets.top,
                        ),
                        true,
                    )
                }
            };
            paint_element(
                &mut self.canvas,
                child.as_ref(),
                ElementPaintSpace {
                    border_box: SurfaceRect::new(
                        origin,
                        Size::new(
                            geometry
                                .size
                                .width
                                .max(area.content_box.size.width.min(geometry.size.width)),
                            geometry.size.height,
                        ),
                    ),
                    inherited_containing_block: area.absolute_containing_block,
                    establishes_containing_block: false,
                    root_effects: area.direct_child_effects,
                },
                self.fonts,
                self.filter_dpi,
            )?;
            if advances_flow {
                cursor_y += geometry.size.height + geometry.margins.end;
                previous_margin_end = geometry.margins.end;
            }
        }
        Some(())
    }

    fn paint_flex_cell(
        &mut self,
        cell: &FlexCell,
        flex: &FlexRow,
        content: SurfaceRect,
        max_baseline: Option<f32>,
    ) -> Option<()> {
        let alignment = cell.effective_cross_alignment(flex.content.alignment);
        let baseline_shift = if alignment == AlignItems::Baseline {
            match (flex_cell_baseline(cell, self.fonts), max_baseline) {
                (Some(own), Some(maximum)) => (maximum - own).max(0.0),
                _ => 0.0,
            }
        } else {
            0.0
        };
        let cross = cell.cross_geometry(
            flex.content.row_height,
            flex.content.alignment,
            baseline_shift,
        );
        let rect = SurfaceRect::new(
            Point::new(
                content.origin.x + cell.x_offset,
                content.origin.y + cross.offset,
            ),
            Size::new(cell.width, cross.size),
        );
        self.paint_flex_cell_box(cell, rect)
    }

    fn paint_flex_cell_box(&mut self, cell: &FlexCell, rect: SurfaceRect) -> Option<()> {
        if let Some(output) = &cell.paint.filter_output {
            return self.canvas.paint_filter_output(output, rect);
        }
        let model = BoxModel {
            size: crate::layout::elements::LayoutSize::fixed(
                rect.size.width,
                Some(rect.size.height),
            ),
            padding: cell.padding,
            border: cell.border,
            ..Default::default()
        };
        let background = FilterBackground::resolve(
            &cell.paint.background,
            &model,
            rect,
            cell.paint.border_radii,
        )?;
        self.canvas
            .paint_outset_shadows(rect, &cell.paint.shadows, self.filter_dpi)?;
        background.paint(&mut self.canvas);
        let padding_box = rect.inset(cell.border.widths());
        self.canvas
            .paint_inset_shadows(padding_box, &cell.paint.shadows, self.filter_dpi)?;
        self.canvas
            .paint_border(rect, &cell.border, cell.paint.border_radii)?;
        let area = DescendantPaintArea {
            content_box: rect.inset(cell.border.widths() + cell.padding),
            absolute_containing_block: Some(rect.inset(cell.border.widths())),
            direct_child_effects: RootEffectHandling::DeferToOwner,
        };
        self.paint_text_lines(&cell.lines, area.content_box, cell.text_align, 0.0)?;
        let text_height = cell.lines.iter().map(|line| line.height).sum::<f32>();
        self.paint_children(&cell.nested_elements, area.after_normal_flow(text_height))
    }

    fn paint_grid_cell(&mut self, cell: &GridCell, rect: SurfaceRect) -> Option<()> {
        if let Some(output) = &cell.layout.paint.filter_output {
            return self.canvas.paint_filter_output(output, rect);
        }
        if cell.placement.clips {
            return None;
        }
        let border = cell.layout.box_model.border;
        let model = BoxModel {
            size: crate::layout::elements::LayoutSize::fixed(
                rect.size.width,
                Some(rect.size.height),
            ),
            padding: cell.layout.box_model.content_insets,
            border,
            ..Default::default()
        };
        let background = FilterBackground::resolve(
            &cell.layout.paint.background,
            &model,
            rect,
            cell.layout.paint.border_radii,
        )?;
        self.canvas
            .paint_outset_shadows(rect, &cell.layout.paint.shadows, self.filter_dpi)?;
        background.paint(&mut self.canvas);
        let padding_box = rect.inset(border.widths());
        self.canvas.paint_inset_shadows(
            padding_box,
            &cell.layout.paint.shadows,
            self.filter_dpi,
        )?;
        self.canvas
            .paint_border(rect, &border, cell.layout.paint.border_radii)?;
        let area = DescendantPaintArea {
            content_box: rect.inset(border.widths() + cell.layout.box_model.content_insets),
            absolute_containing_block: Some(rect.inset(border.widths())),
            direct_child_effects: RootEffectHandling::DeferToOwner,
        };
        self.paint_text_lines(
            &cell.layout.content.lines,
            area.content_box,
            cell.layout.alignment.inline,
            0.0,
        )?;
        let text_height = cell
            .layout
            .content
            .lines
            .iter()
            .map(|line| line.height)
            .sum::<f32>();
        self.paint_children(
            &cell.layout.content.children,
            area.after_normal_flow(text_height),
        )
    }
}

#[derive(Clone, Copy)]
enum SurfaceDecorationPhase {
    All,
    BelowText,
    AboveText,
}

impl SurfaceDecorationPhase {
    const fn paints_below_text(self) -> bool {
        matches!(self, Self::All | Self::BelowText)
    }

    const fn paints_above_text(self) -> bool {
        matches!(self, Self::All | Self::AboveText)
    }
}

impl LayoutVisitor for SourcePainter<'_> {
    fn visit_column_rule(&mut self, element: &ColumnRule) {
        self.result = self.canvas.paint_column_rule(
            Point::new(
                self.space.border_box.origin.x + element.origin.x,
                self.space.border_box.origin.y + element.origin.y,
            ),
            element.height,
            element.paint,
        );
    }

    fn visit_text_block(&mut self, element: &TextBlock) {
        self.result = (|| {
            if element.paint.group.transform.value.is_some()
                || element.text.writing_mode != crate::style::computed::WritingMode::HorizontalTb
            {
                return None;
            }
            let area = self.paint_box(element)?;
            let paint_lines = |painter: &mut SourcePainter<'_>| {
                painter.paint_text_lines(
                    &element.lines,
                    area.content_box,
                    element.text.alignment,
                    element.text.indent,
                )
            };
            if element.clipping.rect.is_some() {
                let clip = CssRoundedRect::new(self.space.border_box, element.paint.border_radii)
                    .inset(element.box_model.border.widths());
                self.paint_clipped_descendants(clip, paint_lines)
            } else {
                paint_lines(self)
            }
        })();
    }

    fn visit_container(&mut self, element: &Container) {
        self.result = (|| {
            let effects_owned_by_caller =
                self.space.root_effects == RootEffectHandling::DeferToOwner;
            if (element.paint.group.transform.value.is_some() && !effects_owned_by_caller)
                || (element.paint.group.effects.masking.clip_path.is_some()
                    && !effects_owned_by_caller)
                || (element.paint.group.effects.masking.image.is_some() && !effects_owned_by_caller)
            {
                return None;
            }
            let area = self.paint_box(element)?;
            if element.overflow.combined.clips() {
                let clip = CssRoundedRect::new(self.space.border_box, element.paint.border_radii)
                    .inset(element.box_model.border.widths());
                self.paint_clipped_descendants(clip, |painter| {
                    painter.paint_container_children(&element.children, area)
                })
            } else {
                self.paint_container_children(&element.children, area)
            }
        })();
    }

    fn visit_multicol_container(&mut self, element: &MulticolContainer) {
        let inherited = self.space.establishes_containing_block;
        self.space.establishes_containing_block = true;
        self.visit_container(&element.principal);
        self.space.establishes_containing_block = inherited;
    }

    fn visit_flex_row(&mut self, element: &FlexRow) {
        self.result = (|| {
            if element.paint.group.transform.value.is_some() {
                return None;
            }
            let content = self.paint_box(element)?.content_box;
            let max_baseline = flex_line_max_baseline(
                &element.content.cells,
                element.content.alignment,
                self.fonts,
            );
            for cell in &element.content.cells {
                self.paint_flex_cell(cell, element, content, max_baseline)?;
            }
            Some(())
        })();
    }

    fn visit_grid_row(&mut self, element: &GridRow) {
        self.result = (|| {
            let border_box = self.space.border_box;
            self.canvas.paint_border(
                border_box,
                &element.box_model.border,
                crate::types::CornerRadii::ZERO,
            )?;
            let content =
                border_box.inset(element.box_model.border.widths() + element.box_model.padding);
            let row_height = element
                .content
                .cells
                .iter()
                .map(|cell| cell.layout.box_model.minimum_block_size)
                .fold(0.0_f32, f32::max);
            for cell in &element.content.cells {
                let column = cell.placement.column_start;
                let span = cell.placement.column_span.max(1);
                let track_x = content.origin.x
                    + element
                        .content
                        .column_widths
                        .iter()
                        .take(column)
                        .sum::<f32>()
                    + element.content.gap * column as f32;
                let track_width = element
                    .content
                    .column_widths
                    .iter()
                    .skip(column)
                    .take(span)
                    .sum::<f32>()
                    + element.content.gap * span.saturating_sub(1) as f32;
                let rect = match cell.placement.inset {
                    Some(inset) => SurfaceRect::new(
                        Point::new(track_x + inset.offset.x, content.origin.y + inset.offset.y),
                        inset.size,
                    ),
                    None => SurfaceRect::new(
                        Point::new(track_x, content.origin.y),
                        Size::new(track_width, row_height),
                    ),
                };
                self.paint_grid_cell(cell, rect)?;
            }
            Some(())
        })();
    }

    fn visit_image(&mut self, element: &Image) {
        self.result = (|| {
            if element.paint.group.transform.value.is_some()
                || element.paint.filter_effect.is_some()
            {
                return None;
            }
            let rect = self.space.border_box;
            if !element.paint.raster_overflow.is_zero() {
                return self.canvas.paint_expanded_raster(
                    &element.source,
                    rect,
                    element.paint.raster_overflow,
                );
            }
            if let Some(background) = element.paint.background_color {
                self.canvas.fill(rect, background);
            }
            let content = rect.inset(element.geometry.border.widths());
            self.canvas
                .paint_image(&element.source, content, element.sampling)?;
            self.canvas.paint_border(
                rect,
                &element.geometry.border,
                crate::types::CornerRadii::ZERO,
            )
        })();
    }
}

/// Paint a layout subtree into one filter-resolution `SourceGraphic`.
pub(crate) fn paint_source_graphic(
    element: &dyn LayoutElement,
    fonts: &HashMap<String, TtfFont>,
    filter_dpi: f32,
) -> Option<SourceGraphic> {
    let layout = source_geometry(element)?;
    let paint_overflow = source_paint_overflow(element, layout.size, filter_dpi)?;
    let geometry = SourceRasterGeometry {
        layout,
        paint_overflow,
    };
    let surface_size = geometry.surface_size();
    let mut pixels = image::RgbaImage::new(
        crate::render::blur::filter_raster_pixels_at_dpi(surface_size.width, filter_dpi)?,
        crate::render::blur::filter_raster_pixels_at_dpi(surface_size.height, filter_dpi)?,
    );
    let mut canvas = RasterCanvas {
        pixels: &mut pixels,
        pixels_per_point: crate::render::blur::px_per_pt_at_dpi(filter_dpi),
    };
    paint_element(
        &mut canvas,
        element,
        ElementPaintSpace {
            border_box: SurfaceRect::new(geometry.border_origin(), geometry.layout.size),
            inherited_containing_block: None,
            establishes_containing_block: true,
            root_effects: RootEffectHandling::DeferToOwner,
        },
        fonts,
        filter_dpi,
    )?;
    Some(SourceGraphic { pixels, geometry })
}

/// Paint one grid item's complete border-box source before applying its filter.
pub(crate) fn paint_grid_cell_source(
    cell: &GridCell,
    size: Size,
    fonts: &HashMap<String, TtfFont>,
    filter_dpi: f32,
) -> Option<SourceGraphic> {
    let layout = SourceGeometry {
        size,
        margins: BlockMargins::ZERO,
        positioning: Positioning::default(),
    };
    let geometry = SourceRasterGeometry {
        layout,
        paint_overflow: EdgeSizes::ZERO,
    };
    let mut pixels = image::RgbaImage::new(
        crate::render::blur::filter_raster_pixels_at_dpi(size.width, filter_dpi)?,
        crate::render::blur::filter_raster_pixels_at_dpi(size.height, filter_dpi)?,
    );
    let mut painter = SourcePainter {
        canvas: RasterCanvas {
            pixels: &mut pixels,
            pixels_per_point: crate::render::blur::px_per_pt_at_dpi(filter_dpi),
        },
        space: ElementPaintSpace {
            border_box: SurfaceRect::new(Point::default(), size),
            inherited_containing_block: None,
            establishes_containing_block: true,
            root_effects: RootEffectHandling::DeferToOwner,
        },
        fonts,
        filter_dpi,
        result: None,
    };
    painter.result = painter.paint_grid_cell(cell, SurfaceRect::new(Point::default(), size));
    painter.result?;
    Some(SourceGraphic { pixels, geometry })
}

/// Resolve every flex item's concrete border-box size after line alignment.
///
/// The returned order is the cell order. Keeping this geometry calculation in
/// the SourceGraphic painter prevents the pagination materializer and normal
/// flex paint path from developing competing alignment rules.
pub(crate) fn flex_cell_source_sizes(
    flex: &FlexRow,
    fonts: &HashMap<String, TtfFont>,
) -> Vec<Size> {
    let max_baseline = flex_line_max_baseline(&flex.content.cells, flex.content.alignment, fonts);
    flex.content
        .cells
        .iter()
        .map(|cell| {
            let alignment = cell.effective_cross_alignment(flex.content.alignment);
            let baseline_shift = if alignment == AlignItems::Baseline {
                match (flex_cell_baseline(cell, fonts), max_baseline) {
                    (Some(own), Some(maximum)) => (maximum - own).max(0.0),
                    _ => 0.0,
                }
            } else {
                0.0
            };
            let cross = cell.cross_geometry(
                flex.content.row_height,
                flex.content.alignment,
                baseline_shift,
            );
            Size::new(cell.width, cross.size)
        })
        .collect()
}

/// Paint one flex item's complete border-box source before applying its
/// retained filter.
pub(crate) fn paint_flex_cell_source(
    cell: &FlexCell,
    size: Size,
    fonts: &HashMap<String, TtfFont>,
    filter_dpi: f32,
) -> Option<SourceGraphic> {
    let layout = SourceGeometry {
        size,
        margins: BlockMargins::ZERO,
        positioning: Positioning::default(),
    };
    let geometry = SourceRasterGeometry {
        layout,
        paint_overflow: flex_cell_paint_overflow(cell, size, filter_dpi)?,
    };
    let surface_size = geometry.surface_size();
    let mut pixels = image::RgbaImage::new(
        crate::render::blur::filter_raster_pixels_at_dpi(surface_size.width, filter_dpi)?,
        crate::render::blur::filter_raster_pixels_at_dpi(surface_size.height, filter_dpi)?,
    );
    let border_box = SurfaceRect::new(geometry.border_origin(), size);
    let mut painter = SourcePainter {
        canvas: RasterCanvas {
            pixels: &mut pixels,
            pixels_per_point: crate::render::blur::px_per_pt_at_dpi(filter_dpi),
        },
        space: ElementPaintSpace {
            border_box,
            inherited_containing_block: None,
            establishes_containing_block: true,
            root_effects: RootEffectHandling::DeferToOwner,
        },
        fonts,
        filter_dpi,
        result: None,
    };
    painter.result = painter.paint_flex_cell_box(cell, border_box);
    painter.result?;
    Some(SourceGraphic { pixels, geometry })
}

/// Conservative paint overflow for a complete flex-item source.
///
/// A complex flex item stores its principal box in `nested_elements`, so only
/// inspecting `FlexCell::box_shadow` loses effects owned by that principal
/// box. Component-wise subtree overflow is safe to over-allocate: transparent
/// padding changes neither layout nor filtered pixels, while preventing any
/// descendant paint from being clipped before the filter runs.
fn flex_cell_paint_overflow(cell: &FlexCell, size: Size, filter_dpi: f32) -> Option<EdgeSizes> {
    if let Some(output) = &cell.paint.filter_output {
        return Some(output.raster_overflow);
    }
    let mut overflow = box_shadow_overflow(size, &cell.paint.shadows, filter_dpi)?;
    for child in &cell.nested_elements {
        overflow = overflow.max_each(source_paint_overflow(child.as_ref(), size, filter_dpi)?);
    }
    Some(overflow)
}

fn source_paint_overflow(
    element: &dyn LayoutElement,
    size: Size,
    filter_dpi: f32,
) -> Option<EdgeSizes> {
    struct PaintOverflow {
        size: Size,
        filter_dpi: f32,
        result: Option<EdgeSizes>,
    }

    impl PaintOverflow {
        fn merge(&mut self, overflow: Option<EdgeSizes>) {
            self.result = self
                .result
                .zip(overflow)
                .map(|(current, next)| current.max_each(next));
        }

        fn merge_box(&mut self, paint: &BoxPaint) {
            self.merge(box_shadow_overflow(
                self.size,
                &paint.shadows,
                self.filter_dpi,
            ));
        }
    }

    impl LayoutVisitor for PaintOverflow {
        fn visit_column_rule(&mut self, _element: &ColumnRule) {
            // Column rules do not paint outside their retained geometry.
        }

        fn visit_text_block(&mut self, element: &TextBlock) {
            self.merge_box(&element.paint);
            for run in element.lines.iter().flat_map(|line| &line.runs) {
                self.merge(box_shadow_overflow(
                    self.size,
                    &run.text_shadow,
                    self.filter_dpi,
                ));
            }
        }

        fn visit_container(&mut self, element: &Container) {
            self.merge_box(&element.paint);
        }

        fn visit_flex_row(&mut self, element: &FlexRow) {
            self.merge_box(&element.paint);
            for cell in &element.content.cells {
                self.merge(box_shadow_overflow(
                    self.size,
                    &cell.paint.shadows,
                    self.filter_dpi,
                ));
                if let Some(output) = &cell.paint.filter_output {
                    self.merge(Some(output.raster_overflow));
                }
            }
        }

        fn visit_grid_row(&mut self, _element: &GridRow) {
            // Grid cell paint is bounded by its retained cell geometry.
        }

        fn visit_image(&mut self, element: &Image) {
            self.merge(Some(element.paint.raster_overflow));
        }
    }

    let mut overflow = PaintOverflow {
        size,
        filter_dpi,
        result: Some(EdgeSizes::ZERO),
    };
    visit_layout_tree(element, &mut overflow);
    overflow.result
}

fn paint_element(
    canvas: &mut RasterCanvas<'_>,
    element: &dyn LayoutElement,
    space: ElementPaintSpace,
    fonts: &HashMap<String, TtfFont>,
    filter_dpi: f32,
) -> Option<()> {
    let opacity = element_group_opacity(element);
    if space.root_effects == RootEffectHandling::Paint && opacity < 1.0 {
        let mut group = image::RgbaImage::new(canvas.pixels.width(), canvas.pixels.height());
        let mut group_canvas = RasterCanvas {
            pixels: &mut group,
            pixels_per_point: canvas.pixels_per_point,
        };
        paint_element(
            &mut group_canvas,
            element,
            ElementPaintSpace {
                root_effects: RootEffectHandling::DeferToOwner,
                ..space
            },
            fonts,
            filter_dpi,
        )?;
        canvas.composite_group(&group, opacity);
        return Some(());
    }
    let mut painter = SourcePainter {
        canvas: RasterCanvas {
            pixels: canvas.pixels,
            pixels_per_point: canvas.pixels_per_point,
        },
        space,
        fonts,
        filter_dpi,
        result: None,
    };
    element.accept(&mut painter);
    painter.result
}

fn element_group_opacity(element: &dyn LayoutElement) -> f32 {
    struct GroupOpacity(f32);

    impl LayoutVisitor for GroupOpacity {
        fn visit_text_block(&mut self, element: &TextBlock) {
            self.0 = element.paint.group.effects.opacity;
        }

        fn visit_container(&mut self, element: &Container) {
            self.0 = element.paint.group.effects.opacity;
        }

        fn visit_flex_row(&mut self, element: &FlexRow) {
            self.0 = element.paint.group.effects.opacity;
        }
    }

    let mut opacity = GroupOpacity(1.0);
    element.accept(&mut opacity);
    opacity.0.clamp(0.0, 1.0)
}

pub(crate) fn source_geometry(element: &dyn LayoutElement) -> Option<SourceGeometry> {
    struct Geometry(Option<SourceGeometry>);

    impl LayoutVisitor for Geometry {
        fn visit_column_rule(&mut self, _element: &ColumnRule) {
            self.0 = Some(SourceGeometry {
                size: Size::default(),
                margins: BlockMargins::ZERO,
                positioning: Positioning::default(),
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

/// Resolve an auto-width block descendant against the known content box that
/// contains it. Root filter sources remain strict because their containing
/// width is not implicit at this boundary.
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

fn container_source_height(element: &Container) -> f32 {
    let natural_height = element.box_model.padding.vertical()
        + element.box_model.border.vertical_width()
        + crate::layout::paginate::simulate_block_flow(&element.children).height;
    element.box_model.size.height.resolve(natural_height)
}

fn flex_cell_baseline(cell: &FlexCell, fonts: &HashMap<String, TtfFont>) -> Option<f32> {
    let mut prior = 0.0;
    let last = cell
        .lines
        .iter()
        .filter(|line| line.runs.iter().any(|run| !run.text.is_empty()))
        .inspect(|line| prior += line.height)
        .last();
    let Some(last) = last else {
        return cell
            .nested_elements
            .iter()
            .find_map(|element| element.atomic_inline_baseline())
            .map(|baseline| cell.border.top.width + cell.padding.top + baseline.baseline_offset());
    };
    prior -= last.height;
    Some(cell.border.top.width + cell.padding.top + prior + line_baseline_ascent(last, fonts))
}

fn flex_line_max_baseline(
    cells: &[FlexCell],
    alignment: AlignItems,
    fonts: &HashMap<String, TtfFont>,
) -> Option<f32> {
    cells
        .iter()
        .filter(|cell| cell.effective_cross_alignment(alignment) == AlignItems::Baseline)
        .filter_map(|cell| flex_cell_baseline(cell, fonts))
        .reduce(f32::max)
}

fn line_baseline_ascent(line: &TextLine, fonts: &HashMap<String, TtfFont>) -> f32 {
    line.baseline_ascent.unwrap_or_else(|| {
        let (ascent, descent) = line
            .runs
            .iter()
            .filter(|run| run.inline_box.is_none())
            .fold((0.0_f32, 0.0_f32), |(ascent, descent), run| {
                let metrics = crate::fonts::font_metrics_ratios(
                    &run.font_family,
                    run.bold,
                    run.font_style.is_slanted(),
                    fonts,
                );
                (
                    ascent.max(metrics.0 * run.font_size),
                    descent.max(metrics.1 * run.font_size),
                )
            });
        ascent + ((line.height - ascent - descent) / 2.0).max(0.0)
    })
}

fn run_width(run: &TextRun, fonts: &HashMap<String, TtfFont>) -> Option<f32> {
    if run.inline_box.is_some() {
        return None;
    }
    crate::text::measure_text_width_with_shaping(
        &run.text,
        run.font_size,
        &run.font_family,
        run.bold,
        run.font_style.is_slanted(),
        run.shaping,
        fonts,
    )
    .map(|width| {
        let letter_spacing =
            run.metadata.letter_spacing * run.text.chars().count().saturating_sub(1) as f32;
        let word_spacing = run.metadata.word_spacing
            * run
                .text
                .chars()
                .filter(|character| *character == ' ')
                .count() as f32;
        run.shaped_advance(width + letter_spacing + word_spacing)
    })
    .or_else(|| {
        let letter_spacing =
            run.metadata.letter_spacing * run.text.chars().count().saturating_sub(1) as f32;
        let word_spacing = run.metadata.word_spacing
            * run
                .text
                .chars()
                .filter(|character| *character == ' ')
                .count() as f32;
        Some(run.shaped_advance(
            crate::fonts::str_width(&run.text, run.font_size, &run.font_family, run.bold)
                + letter_spacing
                + word_spacing,
        ))
    })
}

fn merged_runs(runs: &[TextRun]) -> Vec<TextRun> {
    let mut merged: Vec<TextRun> = Vec::new();
    for run in runs {
        if run.inline_box.is_some() {
            merged.push(run.clone());
            continue;
        }
        if run.text.is_empty() {
            continue;
        }
        let compatible = merged.last().is_some_and(|previous| {
            previous.inline_box.is_none()
                && previous.font_size == run.font_size
                && previous.bold == run.bold
                && previous.font_style == run.font_style
                && previous.color == run.color
                && previous.font_family == run.font_family
                && previous.font_synthesis == run.font_synthesis
                && previous.vertical_align == run.vertical_align
                && previous.font_variant_position == run.font_variant_position
                && previous.metadata.letter_spacing == run.metadata.letter_spacing
                && previous.metadata.word_spacing == run.metadata.word_spacing
                && previous.background_color == run.background_color
                && previous.text_shadow.is_empty()
                && run.text_shadow.is_empty()
        });
        if compatible {
            if let Some(previous) = merged.last_mut() {
                previous.text.push_str(&run.text);
            }
        } else {
            merged.push(run.clone());
        }
    }
    merged
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

#[cfg(test)]
mod tests;
