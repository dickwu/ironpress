use super::*;
use crate::layout::text_emphasis::TextEmphasisMetrics;

pub(super) fn register_used_custom_fonts(
    pdf_writer: &mut PdfWriter,
    custom_fonts: &HashMap<String, TtfFont>,
    prepared_custom_fonts: &PreparedCustomFonts,
) {
    for (font_name, prepared_font) in prepared_custom_fonts {
        if let Some(ttf) = custom_fonts.get(prepared_font.source_font_name(font_name)) {
            pdf_writer.add_ttf_font(font_name, ttf, prepared_font);
        }
    }
}

pub(super) fn font_name_for_run(run: &TextRun) -> &str {
    match (&run.font_family, run.bold, run.font_style.is_slanted()) {
        // Helvetica (sans-serif)
        (FontFamily::Helvetica, true, true) => "Helvetica-BoldOblique",
        (FontFamily::Helvetica, true, false) => "Helvetica-Bold",
        (FontFamily::Helvetica, false, true) => "Helvetica-Oblique",
        (FontFamily::Helvetica, false, false) => "Helvetica",
        // Times Roman (serif)
        (FontFamily::TimesRoman, true, true) => "Times-BoldItalic",
        (FontFamily::TimesRoman, true, false) => "Times-Bold",
        (FontFamily::TimesRoman, false, true) => "Times-Italic",
        (FontFamily::TimesRoman, false, false) => "Times-Roman",
        // Courier (monospace)
        (FontFamily::Courier, true, true) => "Courier-BoldOblique",
        (FontFamily::Courier, true, false) => "Courier-Bold",
        (FontFamily::Courier, false, true) => "Courier-Oblique",
        (FontFamily::Courier, false, false) => "Courier",
        // Custom fonts — fall back to Helvetica variant for rendering name;
        // the actual font reference is handled separately by the renderer.
        (FontFamily::Custom(_), true, true) => "Helvetica-BoldOblique",
        (FontFamily::Custom(_), true, false) => "Helvetica-Bold",
        (FontFamily::Custom(_), false, true) => "Helvetica-Oblique",
        (FontFamily::Custom(_), false, false) => "Helvetica",
    }
}

pub(super) fn estimate_run_width(run: &TextRun) -> f32 {
    crate::fonts::str_width(&run.text, run.font_size, &run.font_family, run.bold)
}

pub(super) fn letter_spacing_extra(letter_spacing: f32, glyph_count: usize) -> f32 {
    letter_spacing * glyph_count.saturating_sub(1) as f32
}

pub(super) fn text_run_letter_spacing(run: &TextRun) -> f32 {
    run.metadata.letter_spacing
}

pub(super) fn effective_run_letter_spacing(block_letter_spacing: f32, run: &TextRun) -> f32 {
    let run_letter_spacing = text_run_letter_spacing(run);
    if run_letter_spacing != 0.0 {
        run_letter_spacing
    } else {
        block_letter_spacing
    }
}

/// Resolve the PDF font resource name for a text run.
///
/// Custom Type0 fonts are only safe when we also have shaped glyph output.
pub(super) fn resolve_font_name(
    run: &TextRun,
    custom_font: Option<(&str, &TtfFont)>,
    shaped: Option<&crate::text::ShapedRun>,
    custom_fonts: &HashMap<String, TtfFont>,
) -> String {
    if let (Some((resolved_name, _)), Some(_)) = (custom_font, shaped) {
        sanitize_pdf_name(&prepared_font_name_for_run(
            resolved_name,
            run,
            custom_fonts,
        ))
    } else {
        font_name_for_run(run).to_string()
    }
}

pub(super) use crate::render::text_decoration::whitespace_insets as decoration_ws_insets;

/// Paint data for one horizontal text run's propagated CSS decorations.
///
/// Every PDF text path uses this type so nested containers, flex cells, table
/// cells, and top-level text cannot silently acquire different decoration
/// capabilities. The two paint methods preserve CSS paint order: decoration
/// shadows sit behind glyphs, while the authored lines can be emitted after the
/// glyph paint without duplicating geometry calculations at each call site.
pub(super) struct HorizontalRunDecoration<'a> {
    run: &'a TextRun,
    custom_fonts: &'a HashMap<String, TtfFont>,
    origin: f32,
    start: f32,
    end: f32,
    baseline: f32,
}

impl<'a> HorizontalRunDecoration<'a> {
    pub(super) fn new(
        run: &'a TextRun,
        start: f32,
        width: f32,
        baseline: f32,
        custom_fonts: &'a HashMap<String, TtfFont>,
    ) -> Self {
        let (leading, trailing) = decoration_ws_insets(run, custom_fonts);
        Self {
            run,
            custom_fonts,
            origin: start,
            start: start + leading,
            end: start + width - trailing,
            baseline,
        }
    }

    /// Join a decoration across a styled-run boundary when either side paints
    /// the same line kind. Whitespace still trims the trailing edge of this run;
    /// only its leading inset is removed.
    pub(super) fn continuing_after(mut self, previous: Option<&TextRun>, run_start: f32) -> Self {
        if previous.is_some_and(|previous| {
            (previous.underline && self.run.underline)
                || (previous.line_through && self.run.line_through)
                || (previous.overline && self.run.overline)
        }) {
            self.start = run_start;
        }
        self
    }

    pub(super) fn paint_shadows(&self, content: &mut String) {
        if !self.has_lines() {
            return;
        }
        for shadow in self.run.text_shadow.iter().rev() {
            if shadow.blur > 0.0 {
                continue;
            }
            self.paint_layer(
                content,
                shadow.color.to_f32_rgb(),
                shadow.offset_x,
                -shadow.offset_y,
            );
        }
    }

    pub(super) fn paint_lines(&self, content: &mut String) {
        if !self.has_lines() {
            return;
        }
        self.paint_layer(
            content,
            self.run
                .decoration_color
                .unwrap_or(self.run.color)
                .to_f32_rgb(),
            0.0,
            0.0,
        );
    }

    /// Paint one horizontal run through the shared decoration and glyph path.
    /// Layout and annotation remain with the caller; this owns the invariant
    /// ordering shared by every horizontal text context.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_text(
        &self,
        content: &mut String,
        parent_font_size: f32,
        prepared_custom_fonts: &PreparedCustomFonts,
        word_spacing: f32,
        pdf_writer: &mut PdfWriter,
        page_images: &mut Vec<ImageRef>,
    ) -> f32 {
        self.paint_shadows(content);
        let advance = render_run_glyphs(
            content,
            self.run,
            self.origin,
            self.baseline,
            parent_font_size,
            self.custom_fonts,
            prepared_custom_fonts,
            word_spacing,
            pdf_writer,
            page_images,
        );
        self.paint_lines(content);
        advance
    }

    fn has_lines(&self) -> bool {
        self.run.underline || self.run.line_through || self.run.overline
    }

    fn paint_layer(
        &self,
        content: &mut String,
        color: (f32, f32, f32),
        inline_offset: f32,
        block_offset: f32,
    ) {
        let start = self.start + inline_offset;
        let end = self.end + inline_offset;
        if self.run.underline {
            push_decoration_stroke(
                content,
                color,
                self.run,
                DecorationLine::Underline,
                start,
                end,
                underline_center_y(self.run, self.baseline) + block_offset,
            );
        }
        if self.run.line_through {
            push_decoration_stroke(
                content,
                color,
                self.run,
                DecorationLine::LineThrough,
                start,
                end,
                self.baseline + self.run.font_size * 0.3 + block_offset,
            );
        }
        if self.run.overline {
            let (ascender_ratio, _) = crate::fonts::font_metrics_ratios(
                &self.run.font_family,
                self.run.bold,
                self.run.font_style.is_slanted(),
                self.custom_fonts,
            );
            push_decoration_stroke(
                content,
                color,
                self.run,
                DecorationLine::Overline,
                start,
                end,
                self.baseline
                    + ascender_ratio * self.run.font_size
                    + overline_lift(self.run)
                    + block_offset,
            );
        }
    }
}

pub(super) use crate::render::text_decoration::overline_lift;
pub(super) use crate::render::text_decoration::thickness as decoration_thickness;

pub(super) fn inline_background_y_and_height(
    run: &TextRun,
    text_y: f32,
    padding: EdgeSizes,
    custom_fonts: &HashMap<String, TtfFont>,
) -> (f32, f32) {
    let line_height = crate::fonts::font_line_metrics(
        &run.font_family,
        run.font_size,
        run.bold,
        run.font_style.is_slanted(),
        custom_fonts,
    )
    .normal_line_height();
    let strut = crate::layout::text::LineStrut::from_font(
        &run.font_family,
        run.font_size,
        run.bold,
        run.font_style.is_slanted(),
        line_height,
        custom_fonts,
    );
    (
        text_y - strut.below - padding.bottom,
        strut.above + strut.below + padding.vertical(),
    )
}

pub(super) fn underline_center_y(run: &TextRun, baseline_y: f32) -> f32 {
    // `text-underline-offset` is measured from the underline-position zero
    // point. For the supported horizontal `auto` position that point is the
    // alphabetic baseline; the line thickness extends outward from there.
    // Blink's automatic near-edge gap is half the resolved stroke width,
    // rounded outward to its CSS-pixel grid. An authored length (including
    // zero or a negative length) remains exact.
    baseline_y - crate::render::text_decoration::underline_distance_from_baseline(run)
}

pub(super) fn decoration_is_wavy(run: &TextRun) -> bool {
    run.metadata.decoration_style == crate::style::computed::TextDecorationStyle::Wavy
}

pub(super) fn decoration_is_emphasis(run: &TextRun) -> bool {
    run.metadata.emphasis.mark
}

pub(super) fn text_emphasis_baseline_shift(run: &TextRun) -> f32 {
    TextEmphasisMetrics::from_run(run).baseline_shift
}

pub(super) fn is_cjk_codepoint(ch: char) -> bool {
    matches!(
        u32::from(ch),
        0x3040..=0x30ff | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff | 0xac00..=0xd7af
    )
}

/// Identifies the decorated line so wavy geometry can extend away from text
/// without shifting a strike-through in either direction.
#[derive(Clone, Copy)]
pub(super) enum DecorationLine {
    Underline,
    LineThrough,
    Overline,
}

impl DecorationLine {
    fn wavy_axis_offset(self, thickness: f32) -> f32 {
        let offset = thickness + crate::fonts::PT_PER_CSS_PX;
        match self {
            // PDF coordinates increase upward, the inverse of CSS's block
            // direction. Wavy underlines need one decoration gap below their
            // solid-line axis, and overlines need the symmetric adjustment.
            Self::Underline => -offset,
            Self::LineThrough => 0.0,
            Self::Overline => offset,
        }
    }
}

/// Blink-compatible dimensions for a wavy text decoration. Its geometry is
/// based on the resolved decoration thickness, with a two-CSS-pixel minimum,
/// rather than the surrounding font size.
#[derive(Clone, Copy)]
struct WavyDecorationMetrics {
    step: f32,
    control_distance: f32,
}

impl WavyDecorationMetrics {
    fn from_thickness(thickness: f32) -> Self {
        let unit = thickness.max(2.0 * crate::fonts::PT_PER_CSS_PX);
        Self {
            step: unit * 2.5,
            control_distance: unit * 3.5,
        }
    }
}

pub(super) fn push_decoration_stroke(
    content: &mut String,
    color: (f32, f32, f32),
    run: &TextRun,
    line: DecorationLine,
    x1: f32,
    x2: f32,
    y: f32,
) {
    let thickness = decoration_thickness(run);
    if x2 <= x1 {
        return;
    }
    if !decoration_is_wavy(run) {
        let rect = PdfRect::new(x1, y - thickness / 2.0, x2 - x1, thickness);
        content.push_str(&PdfRgb::from(color).fill_operator());
        content.push_str(&rect.rect_path());
        content.push_str("f\n");
        return;
    }

    let stroke = thickness;
    let metrics = WavyDecorationMetrics::from_thickness(stroke);
    let axis_y = y + line.wavy_axis_offset(stroke);
    let clip_y = axis_y - metrics.control_distance - stroke * 2.0;
    let clip_h = (metrics.control_distance + stroke * 2.0) * 2.0;
    let mut x = x1 - 2.0 * metrics.step;
    let end_x = x2 + 4.0 * metrics.step;
    let mut path = format!("{x} {axis_y} m\n");
    while x + 2.0 * metrics.step <= end_x {
        let cx = x + metrics.step;
        x += 2.0 * metrics.step;
        path.push_str(&format!(
            "{cx} {} {cx} {} {x} {axis_y} c\n",
            axis_y - metrics.control_distance,
            axis_y + metrics.control_distance
        ));
    }
    content.push_str(&format!("q\n{x1} {clip_y} {} {clip_h} re\nW\nn\n", x2 - x1));
    content.push_str(&PdfRgb::from(color).stroke_operator());
    content.push_str(&format!("{stroke} w\n0 J\n1 j\n{path}S\nQ\n"));
}

/// Construct Chrome's filled-dot emphasis mark as a normal shaped glyph.
///
/// Keeping the mark as text, rather than approximating it with a circle,
/// preserves the selected font's outline and side bearings.
pub(crate) fn emphasis_mark_run(run: &TextRun) -> TextRun {
    TextRun {
        text: "•".to_owned(),
        font_size: run.font_size * TextEmphasisMetrics::MARK_FONT_SCALE,
        bold: run.bold,
        font_style: run.font_style,
        color: run.color,
        font_family: run.font_family.clone(),
        font_synthesis: run.font_synthesis,
        shaping: run.shaping,
        ..Default::default()
    }
}

pub(super) fn render_text_emphasis_marks(
    content: &mut String,
    run: &TextRun,
    x: f32,
    text_y: f32,
    color: crate::types::Color,
    custom_fonts: &HashMap<String, TtfFont>,
    prepared_custom_fonts: &PreparedCustomFonts,
    pdf_writer: &mut PdfWriter,
    page_images: &mut Vec<ImageRef>,
) {
    let metrics = TextEmphasisMetrics::from_run(run);
    let mut mark = emphasis_mark_run(run);
    mark.color = color;
    // Emphasis marks are ruby annotations (css-text-decor-4 §3.4). Chrome
    // resolves the annotation's inline extent upward to its CSS-pixel grid
    // before centering it over the base character.
    let mark_width =
        crate::fonts::ceil_to_css_pixel(estimate_run_width_with_fonts(&mark, custom_fonts));
    let mark_baseline = crate::fonts::round_to_css_pixel(text_y + metrics.mark_baseline_offset);
    let mut cx = x;
    for ch in run.text.chars() {
        let chs = ch.to_string();
        let adv = crate::layout::text::estimate_word_width(
            &chs,
            run.font_size,
            &run.font_family,
            run.bold,
            run.font_style.is_slanted(),
            custom_fonts,
        );
        if !ch.is_whitespace() {
            render_run_glyphs(
                content,
                &mark,
                cx + (adv - mark_width) / 2.0,
                mark_baseline,
                mark.font_size,
                custom_fonts,
                prepared_custom_fonts,
                0.0,
                pdf_writer,
                page_images,
            );
        }
        cx += adv;
    }
}

pub(super) fn estimate_run_width_with_fonts(
    run: &TextRun,
    custom_fonts: &HashMap<String, TtfFont>,
) -> f32 {
    if let Some(inline) = run.inline_box.as_deref() {
        return inline.outer_width();
    }
    if let Some(width) = crate::text::measure_text_width_with_shaping(
        &run.text,
        run.font_size,
        &run.font_family,
        run.bold,
        run.font_style.is_slanted(),
        run.shaping,
        custom_fonts,
    ) {
        return run.shaped_advance(width);
    }

    run.shaped_advance(estimate_run_width(run))
}

pub(crate) fn encode_pdf_hex_glyph(glyph_id: u16) -> String {
    format!("{glyph_id:04X}")
}
