use super::*;
use crate::layout::elements::TextBlock;
use crate::render::pdf::geometry::BackgroundBorderPaint;

pub(in crate::render::pdf) fn render_text_block(
    content: &mut String,
    element: &TextBlock,
    frame: PageElementFrame<'_>,
    phase: ElementPaintPhase,
    bookmarks: &mut Vec<BookmarkEntry>,
    ctx: &mut PageRenderContext<'_>,
) {
    let page_size = frame.page_size;
    let margin = frame.margin;
    let available_width = frame.available_width;
    let y_pos = &frame.y_pos;
    let elem_idx = frame.element_index;
    let page_idx = frame.page_index;
    let lines = &element.lines;
    let text_align = &element.text.alignment;
    let background_color = &element.paint.background.color;
    let padding = &element.box_model.padding;
    let border = &element.box_model.border;
    let block_width = &element.box_model.size.width;
    let block_height = &element.box_model.size.height;
    let float = &element.flow.float;
    let position = &element.positioning.scheme;
    let offset_top = &element.positioning.insets.top;
    let offset_left = &element.positioning.insets.left;
    let containing_block = &element.positioning.containing_block;
    let box_shadow = &element.paint.shadows;
    let visible = &element.paint.visible;
    let clip_rect = &element.clipping.rect;
    let box_transform = &element.paint.group.transform;
    let transform = &box_transform.value;
    let background_gradient = &element.paint.background.layers.gradient;
    let background_radial_gradient = &element.paint.background.layers.radial_gradient;
    let background_conic_gradient = &element.paint.background.layers.conic_gradient;
    let background_svg = &element.paint.background.layers.svg;
    let background_blur_radius = &element.paint.background.layers.blur_radius;
    let background_size = &element.paint.background.layers.size;
    let background_position = &element.paint.background.layers.position;
    let background_repeat = &element.paint.background.layers.repeat;
    let background_origin = &element.paint.background.layers.origin;
    let background_clip = &element.paint.background.layers.clip;
    let background_blend_mode = &element.paint.background.blend_mode;
    let tb_radii = &element.paint.border_radii;
    let outline_width = &element.paint.outline.width;
    let outline_color = &element.paint.outline.color;
    let tb_outline_offset = &element.paint.outline.offset;
    let letter_spacing = &element.text.spacing.letter;
    let css_word_spacing = &element.text.spacing.word;
    let text_indent = &element.text.indent;
    let heading_level = &element.semantics.heading_level;
    let writing_mode = &element.text.writing_mode;
    // Skip rendering if visibility: hidden (but space is preserved)
    if !visible {
        return;
    }

    // Collect heading bookmark for PDF outlines
    if phase.paints_contents()
        && let Some(level) = heading_level
    {
        let title: String = lines
            .iter()
            .flat_map(|l| l.runs.iter().map(|r| r.text.as_str()))
            .collect::<Vec<_>>()
            .join("");
        if !title.trim().is_empty() {
            bookmarks.push(BookmarkEntry {
                title: title.trim().to_string(),
                level: *level,
                page_index: page_idx,
                y_pos: *y_pos,
            });
        }
    }

    // Compute block_x with float/position offsets
    let block_x = match position {
        Position::Absolute | Position::Fixed => {
            // Position relative to the containing block.
            // bottom/right offsets are pre-resolved into top/left
            // at layout time, so we only use offset_left here.
            containing_block.map_or(margin.left + offset_left, |cb| {
                margin.left + cb.x + offset_left
            })
        }
        Position::Relative | Position::Sticky => margin.left + offset_left,
        Position::Static => match float {
            Float::Right => {
                let render_w = block_width.resolve(available_width);
                margin.left + available_width - render_w
            }
            _ => margin.left + offset_left,
        },
    };
    // PDF y-axis is bottom-up.
    // y_pos already includes absolute/relative offsets from pagination.
    let block_y = if *position == Position::Static && *offset_top < 0.0 {
        page_size.height - margin.top - y_pos - offset_top
    } else {
        page_size.height - margin.top - y_pos
    };

    // Use explicit block_width if set, otherwise available_width
    let render_width = block_width.resolve(available_width);
    // `total_h` is the PADDING-box height (content + padding, no
    // border).  The block FLOW advance already accounts for the
    // vertical border (see layout::block / paginate), so `block_y`
    // is the BORDER-box top.  The rendered box geometry (fill,
    // border stroke, box-shadow, clip, text origin) must therefore
    // use the BORDER box so it matches the flow and Chrome.
    let total_h =
        text_block_total_height(lines, *padding, block_height.used(), clip_rect.is_some());
    // Border-box height = padding-box height + vertical border.
    // `render_width` is already the border-box width, so the box
    // is `render_width` × `border_box_h`, top at `block_y`.
    let border_vert = border.top.width + border.bottom.width;
    let border_box_h = total_h + border_vert;
    let block_bottom = block_y - border_box_h;
    let tb_geometry = BoxGeometry::from_layout(
        PdfRect::new(block_x, block_bottom, render_width, border_box_h),
        border,
        *padding,
    );
    let tb_border_box = tb_geometry.border_box.rounded(*tb_radii);

    // Apply transform if set (wrap in q/Q).
    // Rotate and scale are applied around the element's centre so
    // that the element stays in its layout position (matching
    // CSS `transform-origin: 50% 50%`).  The combined matrix is:
    //   T(cx,cy) · M · T(-cx,-cy)
    // which in PDF `cm` notation is a single 6-value matrix.
    let projected_transform = transform.filter(is_projected_transform);
    if phase == ElementPaintPhase::All
        && projected_transform.is_some()
        && lines.is_empty()
        && background_gradient.is_none()
        && background_radial_gradient.is_none()
        && background_conic_gradient.is_none()
        && background_svg.is_none()
        && *background_blur_radius == 0.0
        && tb_radii.is_zero()
        && *outline_width == 0.0
        && box_shadow.is_empty()
        && element.paint.group.effects.is_identity()
        && clip_rect.is_none()
        && *background_clip == BackgroundClip::Border
    {
        render_projected_solid_box(
            content,
            ctx.text.pdf_writer.page_content_transform,
            box_transform,
            tb_geometry,
            *background_color,
            border,
        );
        return;
    }

    if phase == ElementPaintPhase::All
        && transform
            .filter(|transform| !is_projected_transform(transform))
            .is_some()
        && lines.is_empty()
        && background_gradient.is_none()
        && background_radial_gradient.is_none()
        && background_conic_gradient.is_none()
        && background_svg.is_none()
        && *background_blur_radius == 0.0
        && tb_radii.is_zero()
        && *outline_width == 0.0
        && box_shadow.is_empty()
        && element.paint.group.effects.is_identity()
        && clip_rect.is_none()
        && *background_clip == BackgroundClip::Border
        && render_affine_solid_box(
            content,
            ctx.text.pdf_writer,
            ctx.text.page_images,
            box_transform,
            tb_geometry,
            *background_color,
            border,
        )
    {
        return;
    }

    let tb_group = PaintGroupScope::begin(
        content,
        element,
        tb_geometry.for_fragment(element.fragmentation.box_fragmentation),
        ctx,
    );
    let transformed_paint_space = ctx.text.pdf_writer.transformed_paint_space(ctx.paint_box);
    let needs_transform = transformed_paint_space.is_some();

    // CSS `overflow: hidden`/`clip`/`scroll`/`auto` clips at the
    // PADDING box (border box inset by the border widths) and the
    // rounded inner corners when border-radius is set. The clip
    // must NOT cover the box's OWN background, border, or outline —
    // a box's border and outline always paint fully visible. So the
    // clip is opened later (after the border/outline are stroked)
    // and scoped to the inline text content only; see `needs_clip`
    // below the outline-paint block.
    let needs_clip = phase.paints_contents() && clip_rect.is_some();

    // Draw box-shadow with blur (references the border box).
    if phase.paints_decoration() {
        render_box_shadows(
            content,
            box_shadow,
            tb_geometry.for_fragment(element.fragmentation.box_fragmentation),
            *tb_radii,
            ctx.page_ext_gstates,
            ctx.bg_alpha_counter,
            ctx.text.pdf_writer,
            ctx.text.page_images,
        );
    }

    // CSS `filter: blur()` on a solid box (css-filter-effects-1
    // §4.1): the box's painted output (background fill + border)
    // is gaussian-blurred and feathers outside the border box.
    // ironpress paints boxes as vector content, so for a plain
    // solid box (no gradient/SVG bg, no text, no transform/opacity
    // wrapper, square corners) rasterize bg+border, blur it, and
    // embed it in place of the sharp vector paint.
    if phase == ElementPaintPhase::All
        && *background_blur_radius > 0.0
        && !needs_transform
        && element.paint.group.effects.is_identity()
        && lines.is_empty()
        && background_gradient.is_none()
        && background_radial_gradient.is_none()
        && background_conic_gradient.is_none()
        && background_svg.is_none()
        && tb_radii.is_zero()
        && *outline_width == 0.0
        && box_shadow.is_empty()
        && let Some(blurred) = crate::render::blur::blur_box(
            render_width,
            border_box_h,
            *background_color,
            border,
            *background_blur_radius,
            ctx.text.pdf_writer.opts.raster_quality.filter_dpi,
        )
    {
        let img_obj_id = ctx.text.pdf_writer.add_image_object(
            &blurred.asset.data,
            blurred.asset.source_width,
            blurred.asset.source_height,
            blurred.asset.format,
            blurred.asset.png_metadata.as_ref(),
        );
        let img_name = format!("Im{img_obj_id}");
        let ov = blurred.overflow_pt;
        content.push_str(&format!(
            "q\n{w} 0 0 {h} {ix} {iy} cm\n/{name} Do\nQ\n",
            w = render_width + 2.0 * ov,
            h = border_box_h + 2.0 * ov,
            ix = block_x - ov,
            iy = block_bottom - ov,
            name = img_name,
        ));
        ctx.text.page_images.push(ImageRef {
            name: img_name,
            obj_id: img_obj_id,
        });
        tb_group.finish(content, ctx);
        return;
    }

    if phase == ElementPaintPhase::All
        && *background_blur_radius > 0.0
        && !needs_transform
        && element.paint.group.effects.is_identity()
        && !lines.is_empty()
        && clip_rect.is_none()
        && matches!(
            writing_mode,
            crate::style::computed::WritingMode::HorizontalTb
        )
        && background_gradient.is_none()
        && background_radial_gradient.is_none()
        && background_conic_gradient.is_none()
        && background_svg.is_none()
        && tb_radii.is_zero()
        && *outline_width == 0.0
        && box_shadow.is_empty()
        && *background_clip == BackgroundClip::Border
        && let Some(blurred) = blurred_simple_text_block(
            render_width,
            border_box_h,
            *background_color,
            lines,
            *padding,
            border,
            *text_align,
            *letter_spacing,
            *css_word_spacing,
            *text_indent,
            *background_blur_radius,
            ctx.text.pdf_writer.opts.raster_quality.filter_dpi,
            ctx.text.custom_fonts,
        )
    {
        let img_obj_id = ctx.text.pdf_writer.add_image_object(
            &blurred.asset.data,
            blurred.asset.source_width,
            blurred.asset.source_height,
            blurred.asset.format,
            blurred.asset.png_metadata.as_ref(),
        );
        let img_name = format!("Im{img_obj_id}");
        let ov = blurred.overflow_pt;
        content.push_str(&format!(
            "q\n{w} 0 0 {h} {ix} {iy} cm\n/{name} Do\nQ\n",
            w = render_width + 2.0 * ov,
            h = border_box_h + 2.0 * ov,
            ix = block_x - ov,
            iy = block_bottom - ov,
            name = img_name,
        ));
        ctx.text.page_images.push(ImageRef {
            name: img_name,
            obj_id: img_obj_id,
        });
        tb_group.finish(content, ctx);
        return;
    }

    let tb_reference = tb_geometry.background_origin_box(*background_origin);
    let tb_clip_box = tb_geometry.background_paint_box(
        *background_clip,
        *tb_radii,
        BackgroundBorderPaint::new(border, element.paint.border_image.as_ref()),
    );
    let tb_needs_clip = *background_clip != BackgroundClip::Border || tb_clip_box != tb_border_box;
    let tb_text_clip_background = *background_clip == BackgroundClip::Text;
    let tb_gradient_clip = !tb_text_clip_background;
    let tb_layer_box =
        background_layer_box(*background_size, *background_position, *background_repeat);
    let tb_bg_blend_mode = background_blend_mode.background_layer(0);
    let tb_bg_blended = tb_bg_blend_mode != crate::style::computed::BlendMode::Normal;
    // Draw background if specified
    if phase.paints_decoration()
        && let Some(background) = background_color
    {
        let (r, g, b, a) = background.to_f32_rgba();
        let needs_bg_alpha = a < 1.0;
        if needs_bg_alpha {
            let gs_name = format!("GSbg{elem_idx}");
            ctx.page_ext_gstates.push((gs_name.clone(), a));
            content.push_str(&format!("/{gs_name} gs\n"));
        }
        let color = PdfRgb::from((r, g, b));
        let uses_device_css_clip = !needs_transform
            && is_device_clippable_box_background(
                *background_clip,
                tb_clip_box.radii,
                ctx.text.pdf_writer.page_content_transform,
                tb_clip_box.rect,
            )
            && paint_device_clipped_css_solid(
                content,
                ctx.text.pdf_writer.page_content_transform,
                tb_border_box.rect,
                tb_clip_box.rect,
                color,
            );
        if !uses_device_css_clip && tb_needs_clip {
            content.push_str(&color.fill_operator());
            tb_clip_box.push_clip(content);
            content.push_str(&tb_clip_box.rect.rect_path());
            content.push_str("f\n");
            content.push_str("Q\n");
        } else if !uses_device_css_clip {
            content.push_str(&color.fill_operator());
            content.push_str(&tb_border_box.path_or_rect());
            content.push_str("f\n");
        }
        if needs_bg_alpha {
            content.push_str("/GSDefault gs\n");
        }
    }

    // Draw linear gradient if specified
    if phase.paints_decoration()
        && let Some(gradient) = background_gradient
    {
        let gradient = linear_with_background_layer(gradient, tb_layer_box);
        if !tb_text_clip_background
            && gradient.layer_box.attachment != Some(BackgroundAttachment::Local)
        {
            if tb_bg_blended {
                content.push_str("q\n");
                begin_blend_mode(content, ctx.page_ext_gstates, tb_bg_blend_mode);
            }
            // Clip to the background-clip box (rounded if needed).
            if tb_gradient_clip {
                tb_clip_box.push_clip(content);
            }
            let (grad_x, grad_y, grad_w, grad_h) =
                if gradient.layer_box.attachment == Some(BackgroundAttachment::Fixed) {
                    (0.0, 0.0, page_size.width, page_size.height)
                } else {
                    (
                        tb_reference.left,
                        tb_reference.bottom,
                        tb_reference.width,
                        tb_reference.height,
                    )
                };
            render_linear_gradient(
                content,
                &gradient,
                GradientBackdrop::isolated_linear_layer(
                    *background_color,
                    background_radial_gradient.is_some()
                        || background_conic_gradient.is_some()
                        || background_svg.is_some(),
                    tb_bg_blend_mode,
                ),
                grad_x,
                grad_y,
                grad_w,
                grad_h,
                ctx.shadings,
                ctx.shading_counter,
                ctx.text.pdf_writer,
                ctx.text.page_images,
            );
            if tb_gradient_clip {
                content.push_str("Q\n");
            }
            if tb_bg_blended {
                content.push_str("Q\n");
            }
        }
    }

    // Draw radial gradient if specified
    if phase.paints_decoration()
        && let Some(gradient) = background_radial_gradient
    {
        let gradient = radial_with_background_layer(gradient, tb_layer_box);
        if !tb_text_clip_background
            && gradient.layer_box.attachment != Some(BackgroundAttachment::Local)
        {
            if tb_bg_blended {
                content.push_str("q\n");
                begin_blend_mode(content, ctx.page_ext_gstates, tb_bg_blend_mode);
            }
            if tb_gradient_clip {
                tb_clip_box.push_clip(content);
            }
            let (grad_x, grad_y, grad_w, grad_h) =
                if gradient.layer_box.attachment == Some(BackgroundAttachment::Fixed) {
                    (0.0, 0.0, page_size.width, page_size.height)
                } else {
                    (
                        tb_reference.left,
                        tb_reference.bottom,
                        tb_reference.width,
                        tb_reference.height,
                    )
                };
            render_radial_gradient(
                content,
                &gradient,
                grad_x,
                grad_y,
                grad_w,
                grad_h,
                ctx.shadings,
                ctx.shading_counter,
                ctx.text.pdf_writer,
                ctx.text.page_images,
            );
            if tb_gradient_clip {
                content.push_str("Q\n");
            }
            if tb_bg_blended {
                content.push_str("Q\n");
            }
        }
    }

    // Draw conic gradient if specified
    if phase.paints_decoration()
        && let Some(gradient) = background_conic_gradient
    {
        let gradient = conic_with_background_layer(gradient, tb_layer_box);
        if !tb_text_clip_background
            && gradient.layer_box.attachment != Some(BackgroundAttachment::Local)
        {
            if tb_bg_blended {
                content.push_str("q\n");
                begin_blend_mode(content, ctx.page_ext_gstates, tb_bg_blend_mode);
            }
            if tb_gradient_clip {
                tb_clip_box.push_clip(content);
            }
            let (grad_x, grad_y, grad_w, grad_h) =
                if gradient.layer_box.attachment == Some(BackgroundAttachment::Fixed) {
                    (0.0, 0.0, page_size.width, page_size.height)
                } else {
                    (
                        tb_reference.left,
                        tb_reference.bottom,
                        tb_reference.width,
                        tb_reference.height,
                    )
                };
            render_conic_gradient(
                content,
                &gradient,
                grad_x,
                grad_y,
                grad_w,
                grad_h,
                ctx.text.pdf_writer,
                ctx.text.page_images,
            );
            if tb_gradient_clip {
                content.push_str("Q\n");
            }
            if tb_bg_blended {
                content.push_str("Q\n");
            }
        }
    }

    // Draw inset box-shadow (after backgrounds, before content).
    if phase.paints_decoration() {
        render_box_shadows_inset(
            content,
            box_shadow,
            tb_geometry.for_fragment(element.fragmentation.box_fragmentation),
            *tb_radii,
            ctx.page_ext_gstates,
            ctx.bg_alpha_counter,
            ctx.text.pdf_writer,
            ctx.text.page_images,
        );
    }

    // Draw SVG background image if specified.
    // `block_x` / `block_y` are the border-box top-left and
    // `render_width` × `border_box_h` is the border box (border
    // paints inward).  Derive the padding/content boxes by
    // insetting with the per-side border / padding widths.
    if phase.paints_decoration()
        && let Some(svg_tree) = background_svg
    {
        if tb_bg_blended {
            content.push_str("q\n");
            begin_blend_mode(content, ctx.page_ext_gstates, tb_bg_blend_mode);
        }
        let paint = BackgroundPaintContext::new(
            tb_reference.into(),
            tb_clip_box.rect.into(),
            tb_clip_box.radii,
            *background_blur_radius,
            *background_size,
            *background_position,
            *background_repeat,
        );
        let paint = transformed_paint_space.map_or_else(
            || PdfBackgroundPaintContext::local(paint),
            |paint_space| PdfBackgroundPaintContext::in_default_space(paint, paint_space),
        );
        render_svg_background(
            content,
            svg_tree,
            PdfBackgroundResources::new(
                ctx.text.pdf_writer,
                ctx.text.page_images,
                ctx.shadings,
                ctx.shading_counter,
                Some(ctx.page_ext_gstates),
            ),
            paint,
        );
        if tb_bg_blended {
            content.push_str("Q\n");
        }
    }

    // Draw every box border through the shared rounded-ring painter. Text,
    // containers, flex items, and table cells must not disagree about corner
    // ownership or style transitions.
    if phase.paints_decoration() && (border.has_visible() || element.paint.border_image.is_some()) {
        paint_box_decoration(
            content,
            tb_geometry.for_fragment(element.fragmentation.box_fragmentation),
            border,
            *tb_radii,
            element.paint.border_image.as_ref(),
            BorderPaintResources::from_page(ctx),
        );
    }

    // Draw outline if specified (outside the element box).
    // `outline-offset` widens the gap between the border edge and
    // the outline; the centerline sits half the outline width
    // beyond the offset edge so the stroke stays fully outside.
    if phase.paints_decoration() && *outline_width > 0.0 {
        let gap = *tb_outline_offset + *outline_width / 2.0;
        let outline_x = block_x - gap;
        let outline_y = block_bottom - gap;
        let outline_w = render_width + 2.0 * gap;
        let outline_h = border_box_h + 2.0 * gap;
        let (or, og, ob) = outline_color
            .unwrap_or(crate::types::Color::BLACK)
            .to_f32_rgb();
        content.push_str(&PdfRgb::from((or, og, ob)).stroke_operator());
        content.push_str(&format!("{outline_width} w\n"));
        content.push_str(
            &RoundedRect::new(
                PdfRect::new(outline_x, outline_y, outline_w, outline_h),
                tb_radii.grow(gap),
            )
            .path_or_rect(),
        );
        content.push_str("S\n");
    }

    // Open the overflow clip now — AFTER the background, border and
    // outline are painted (so they stay fully visible) and BEFORE
    // the inline text / descendant content (which is clipped to the
    // padding box). Mirrors the nested-TextBlock paint order.
    if needs_clip {
        content.push_str("q\n");
        content.push_str(&overflow_clip_path(
            block_x,
            block_bottom,
            render_width,
            border_box_h,
            tb_geometry.border,
            *tb_radii,
        ));
        content.push_str("W n\n");
    }

    let text_space = if needs_transform {
        PdfTextSpace::Points
    } else {
        PdfTextSpace::page_css(ctx.text.pdf_writer.page_content_transform)
    };
    if phase.paints_contents() {
        text_lines::render_text_block_lines(
            content,
            element,
            tb_geometry,
            frame,
            false,
            text_space,
            ctx,
        );
    }

    // Restore the text box's local overflow clip.
    if needs_clip {
        content.push_str("Q\n");
    }

    tb_group.finish(content, ctx);
}
