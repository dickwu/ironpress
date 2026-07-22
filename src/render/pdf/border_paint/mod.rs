use super::*;

mod column_rules;
mod side_paint;
mod uniform;

pub(super) use column_rules::*;
use side_paint::*;
use uniform::*;

/// Mutable PDF resources shared by ordinary border styles and border images.
/// Keeping them together lets every box use one decoration entry point without
/// duplicating the border-image replacement rule at each renderer call site.
pub(super) struct BorderPaintResources<'a> {
    pub(super) shadings: &'a mut Vec<ShadingEntry>,
    pub(super) shading_counter: &'a mut usize,
    pub(super) page_ext_gstates: &'a mut Vec<(String, f32)>,
    pub(super) alpha_counter: &'a mut usize,
    pub(super) pdf_writer: &'a mut PdfWriter,
    pub(super) page_images: &'a mut Vec<ImageRef>,
}

impl<'a> BorderPaintResources<'a> {
    pub(super) fn from_page(ctx: &'a mut PageRenderContext<'_>) -> Self {
        Self {
            shadings: ctx.shadings,
            shading_counter: ctx.shading_counter,
            page_ext_gstates: ctx.page_ext_gstates,
            alpha_counter: ctx.bg_alpha_counter,
            pdf_writer: ctx.text.pdf_writer,
            page_images: ctx.text.page_images,
        }
    }
}

/// Paint one box's border decoration.
///
/// A successfully resolved `border-image-source` replaces the ordinary border
/// styles. `border-radius` affects only the fallback styles, never the image.
pub(super) fn paint_box_decoration(
    content: &mut String,
    geometry: FragmentPaintGeometry,
    border: &crate::layout::engine::LayoutBorder,
    radii: CornerRadii,
    border_image: Option<&crate::style::computed::BorderImagePaint>,
    resources: BorderPaintResources<'_>,
) {
    if let Some(border_image) = border_image {
        let positioning = geometry.positioning();
        let clip =
            geometry.decoration_clip(border_image.geometry.outsets.resolve(positioning.border));
        if let Some(clip) = clip {
            content.push_str("q\n");
            content.push_str(&clip.rect_path());
            content.push_str("W n\n");
        }
        let painted = render_border_image(
            content,
            border_image,
            positioning,
            resources.shadings,
            resources.shading_counter,
            resources.page_ext_gstates,
            resources.pdf_writer,
            resources.page_images,
        );
        if clip.is_some() {
            content.push_str("Q\n");
        }
        if !painted {
            paint_css_border(
                content,
                geometry.painting().border_box,
                border,
                radii,
                resources.page_ext_gstates,
                resources.alpha_counter,
            );
        }
    } else {
        paint_css_border(
            content,
            geometry.painting().border_box,
            border,
            radii,
            resources.page_ext_gstates,
            resources.alpha_counter,
        );
    }
}

/// Paint a layout border around a complete border box.
///
/// A text box can be in normal flow or be positioned absolutely by a
/// fragmented layout.  Its paint position must not change which border path
/// is used: use one closed path for a uniform frame, filled areas for flat
/// non-uniform solid borders, and the per-side painter for the remaining
/// styles.
fn paint_css_border(
    content: &mut String,
    border_box: PdfRect,
    border: &crate::layout::engine::LayoutBorder,
    radii: CornerRadii,
    page_ext_gstates: &mut Vec<(String, f32)>,
    bg_alpha_counter: &mut usize,
) {
    if let Some(side) = border.uniform_paint_side()
        && paint_uniform_border(
            content,
            border_box.rounded(radii),
            side,
            page_ext_gstates,
            bg_alpha_counter,
        )
    {
        return;
    }
    paint_partitioned_border(
        content,
        border_box,
        border,
        radii,
        page_ext_gstates,
        bg_alpha_counter,
    );
}

/// Paint all non-uniform borders through one rounded ring and one corner
/// partition. Every side style is confined to its region, so style/color
/// transitions cannot overlap adjacent sides.
fn paint_partitioned_border(
    content: &mut String,
    border_box: PdfRect,
    border: &crate::layout::engine::LayoutBorder,
    radii: CornerRadii,
    page_ext_gstates: &mut Vec<(String, f32)>,
    bg_alpha_counter: &mut usize,
) {
    let widths = border.widths();
    let ring = BorderRingGeometry::new(border_box, radii, widths);
    let stroke = BorderStrokeGeometry::new(border_box, radii, widths);
    let half_widths = widths * 0.5;
    let double_rules = widths.map(double_rule_width);
    let outer_double =
        BorderRingGeometry::between(border_box, radii, EdgeSizes::ZERO, double_rules);
    let inner_double =
        BorderRingGeometry::between(border_box, radii, widths - double_rules, widths);
    let outer_half = BorderRingGeometry::between(border_box, radii, EdgeSizes::ZERO, half_widths);
    let inner_half = BorderRingGeometry::between(border_box, radii, half_widths, widths);
    if border.top.paints()
        && [border.right, border.bottom, border.left]
            .into_iter()
            .all(|side| {
                side.paints() && side.style == BorderStyle::Solid && side.color == border.top.color
            })
        && border.top.style == BorderStyle::Solid
    {
        let alpha = begin_border_alpha(
            content,
            page_ext_gstates,
            bg_alpha_counter,
            border.top.color.alpha(),
        );
        content.push_str(&PdfRgb::from(border.top.color).fill_operator());
        ring.push_path(content);
        content.push_str("f*\n");
        end_border_alpha(content, alpha);
        return;
    }
    if let Some(side) = border.uniform_paint_side()
        && is_bevel_style(side.style)
    {
        paint_uniform_partitioned_bevel(
            content,
            &side,
            ring,
            outer_half,
            inner_half,
            page_ext_gstates,
            bg_alpha_counter,
        );
        return;
    }
    let sides = [
        (PhysicalSide::Top, &border.top),
        (PhysicalSide::Right, &border.right),
        (PhysicalSide::Bottom, &border.bottom),
        (PhysicalSide::Left, &border.left),
    ];

    // Equal solid sides form one visual region. Emit each colour as one PDF
    // fill so the exclusive diagonal ownership remains geometric rather than
    // becoming an antialiased seam between separately rasterized subpaths.
    paint_solid_side_groups(content, ring, &sides, page_ext_gstates, bg_alpha_counter);

    for (edge, side) in sides {
        if !side.paints() || side.style == BorderStyle::Solid {
            continue;
        }
        let alpha = begin_border_alpha(
            content,
            page_ext_gstates,
            bg_alpha_counter,
            side.color.alpha(),
        );
        let rounded = !radii.is_zero();

        match side.style {
            BorderStyle::Solid => {}
            BorderStyle::Double => {
                let color = PdfRgb::from(side.color);
                paint_border_band_side(content, outer_double, edge, color, rounded);
                paint_border_band_side(content, inner_double, edge, color, rounded);
            }
            BorderStyle::Groove | BorderStyle::Ridge | BorderStyle::Inset | BorderStyle::Outset => {
                let base = side.color.to_f32_rgb();
                if matches!(side.style, BorderStyle::Groove | BorderStyle::Ridge) {
                    let outer_color = bevel_edge_color(side.style, edge, false, base);
                    let inner_color = bevel_edge_color(side.style, edge, true, base);
                    paint_border_band_side(
                        content,
                        outer_half,
                        edge,
                        PdfRgb::from(outer_color),
                        rounded,
                    );
                    paint_border_band_side(
                        content,
                        inner_half,
                        edge,
                        PdfRgb::from(inner_color),
                        rounded,
                    );
                } else {
                    let color = bevel_edge_color(side.style, edge, false, base);
                    paint_border_band_side(content, ring, edge, PdfRgb::from(color), rounded);
                }
            }
            BorderStyle::Dashed | BorderStyle::Dotted => {
                if rounded {
                    paint_rounded_patterned_side(content, ring, stroke, edge, side);
                } else {
                    paint_square_patterned_side(content, ring, border_box, edge, side);
                }
            }
            BorderStyle::None | BorderStyle::Hidden => {}
        }

        end_border_alpha(content, alpha);
    }
}

/// Paint a uniform 3D frame with the canonical ring partition while merging
/// the equal-colour top/left and bottom/right regions into single fill
/// operations. The merged paths have no internal antialias seam and still do
/// not repaint either side of the diagonal frontier.
fn paint_uniform_partitioned_bevel(
    content: &mut String,
    side: &crate::layout::engine::LayoutBorderSide,
    ring: BorderRingGeometry,
    outer_half: BorderRingGeometry,
    inner_half: BorderRingGeometry,
    page_ext_gstates: &mut Vec<(String, f32)>,
    bg_alpha_counter: &mut usize,
) {
    let alpha = begin_border_alpha(
        content,
        page_ext_gstates,
        bg_alpha_counter,
        side.color.alpha(),
    );
    let base = side.color.to_f32_rgb();
    if matches!(side.style, BorderStyle::Groove | BorderStyle::Ridge) {
        paint_uniform_bevel_band(content, outer_half, side.style, false, base);
        paint_uniform_bevel_band(content, inner_half, side.style, true, base);
    } else {
        paint_uniform_bevel_band(content, ring, side.style, false, base);
    }
    end_border_alpha(content, alpha);
}

fn paint_uniform_bevel_band(
    content: &mut String,
    band: BorderRingGeometry,
    style: BorderStyle,
    inner_band: bool,
    base: (f32, f32, f32),
) {
    content.push_str("q\n");
    band.push_clip(content);
    for (color, edges) in [
        (
            bevel_edge_color(style, PhysicalSide::Top, inner_band, base),
            [PhysicalSide::Top, PhysicalSide::Left],
        ),
        (
            bevel_edge_color(style, PhysicalSide::Bottom, inner_band, base),
            [PhysicalSide::Right, PhysicalSide::Bottom],
        ),
    ] {
        content.push_str(&PdfRgb::from(color).fill_operator());
        for edge in edges {
            band.side_region(edge).push_path(content);
        }
        content.push_str("f\n");
    }
    content.push_str("Q\n");
}
