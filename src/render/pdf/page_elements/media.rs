use super::*;
use crate::layout::elements::{Image, Svg};

fn paint_image_effect_raster(
    content: &mut String,
    effect: &crate::layout::engine::ImageEffectRaster,
    image_box: PdfRect,
    rendering: crate::style::computed::ImageRendering,
    ctx: &mut PageRenderContext<'_>,
) {
    let effect_box = image_box.outset_uniform(effect.overflow);
    let image_id = ctx.text.pdf_writer.add_layout_image_object(
        &effect.image,
        effect_box.width,
        effect_box.height,
        rendering,
    );
    let image_name = format!("Im{image_id}");
    push_raster_xobject(
        content,
        &image_name,
        effect_box,
        &effect.image,
        ctx.text.pdf_writer,
    );
    ctx.text.page_images.push(ImageRef {
        name: image_name,
        obj_id: image_id,
    });
}

pub(in crate::render::pdf) fn render_image(
    content: &mut String,
    element: &Image,
    frame: PageElementFrame<'_>,
    ctx: &mut PageRenderContext<'_>,
) {
    let image_box = PdfRect::from_top(
        frame.margin.left + element.positioning.insets.left,
        frame.page_size.height - frame.margin.top - frame.y_pos - element.positioning.insets.top,
        element.geometry.size.width,
        element.geometry.size.height,
    );
    if ctx.text.pdf_writer.opts.occlusion_cull {
        let raster = if element.paint.raster_overflow.is_zero() {
            let overflow = element
                .paint
                .filter_effect
                .as_ref()
                .map_or(0.0, |effect| effect.overflow);
            image_box.outset_uniform(overflow)
        } else {
            image_box.outset(element.paint.raster_overflow)
        };
        if raster_is_occluded(&frame.occlusion_coverers, raster, frame.element_index) {
            return;
        }
    }
    paint_image_box(content, element, image_box, ctx);
}

/// Paint one replaced raster box using the complete shared effects contract.
pub(in crate::render::pdf) fn paint_image_box(
    content: &mut String,
    element: &Image,
    image_box: PdfRect,
    ctx: &mut PageRenderContext<'_>,
) {
    let image = &element.source;
    let geometry = BoxGeometry::from_layout(image_box, &element.geometry.border, EdgeSizes::ZERO);
    let fragment_geometry = geometry.for_fragment(Default::default());
    let raster_overflow = element.paint.raster_overflow;
    if !raster_overflow.is_zero() {
        let expanded_box = image_box.outset(raster_overflow);
        let group = PaintGroupScope::begin(content, element, fragment_geometry, ctx);
        let image_id = ctx.text.pdf_writer.add_image_object_with_interpolation(
            &image.data,
            image.source_width,
            image.source_height,
            image.format,
            image.png_metadata.as_ref(),
            PdfImageInterpolation::for_css_image_rendering(element.sampling.rendering),
        );
        let image_name = format!("Im{image_id}");
        push_raster_xobject(
            content,
            &image_name,
            expanded_box,
            image,
            ctx.text.pdf_writer,
        );
        ctx.text.page_images.push(ImageRef {
            name: image_name,
            obj_id: image_id,
        });
        group.finish(content, ctx);
        return;
    }

    let group = PaintGroupScope::begin(content, element, fragment_geometry, ctx);
    if let Some(effect) = &element.paint.filter_effect {
        paint_image_effect_raster(content, effect, image_box, element.sampling.rendering, ctx);
    }
    if let Some(background) = element.paint.background_color
        && background.alpha() > 0.0
    {
        let (r, g, b, a) = background.to_f32_rgba();
        let needs_alpha = a < 1.0;
        if needs_alpha {
            let state = format!("GSimage{}", ctx.bg_alpha_counter);
            *ctx.bg_alpha_counter += 1;
            ctx.page_ext_gstates.push((state.clone(), a));
            content.push_str(&format!("/{state} gs\n"));
        }
        content.push_str(&format!(
            "{r} {g} {b} rg\n{}f\n",
            image_box.rounded(element.paint.border_radii).path_or_rect()
        ));
        if needs_alpha {
            content.push_str("/GSDefault gs\n");
        }
    }

    let image_content = geometry.padding_box();
    let sliced = element
        .sampling
        .source_crop
        .and_then(|crop| crate::layout::images::crop_raster_asset(image, crop));
    let source = sliced.as_ref().unwrap_or(image);
    let placement = crate::layout::images::compute_image_placement(
        image_content.width,
        image_content.height,
        source.source_width,
        source.source_height,
        element.sampling.object_fit,
        element.sampling.object_position,
    );
    let image_id = ctx.text.pdf_writer.add_layout_image_object(
        source,
        placement.width,
        placement.height,
        element.sampling.rendering,
    );
    let image_name = format!("Im{image_id}");
    if placement.clip {
        content.push_str("q\n");
        content.push_str(&format!("{}W n\n", image_content.rect_path()));
    }
    push_raster_xobject(
        content,
        &image_name,
        PdfRect::new(
            image_content.left + placement.offset_x,
            image_content.top() - placement.offset_y - placement.height,
            placement.width,
            placement.height,
        ),
        source,
        ctx.text.pdf_writer,
    );
    if placement.clip {
        content.push_str("Q\n");
    }
    ctx.text.page_images.push(ImageRef {
        name: image_name,
        obj_id: image_id,
    });
    paint_box_decoration(
        content,
        fragment_geometry,
        &element.geometry.border,
        element.paint.border_radii,
        element.paint.border_image.as_ref(),
        BorderPaintResources::from_page(ctx),
    );
    group.finish(content, ctx);
}

pub(in crate::render::pdf) fn render_svg(
    content: &mut String,
    element: &Svg,
    frame: PageElementFrame<'_>,
    ctx: &mut PageRenderContext<'_>,
) {
    let page_size = frame.page_size;
    let margin = frame.margin;
    let y_pos = &frame.y_pos;
    let elem_idx = frame.element_index;
    let occlusion_coverers = frame.occlusion_coverers;
    let width = &element.geometry.size.width;
    let height = &element.geometry.size.height;
    let offset_top = &element.positioning.insets.top;
    let offset_left = &element.positioning.insets.left;
    let svg_x = margin.left + offset_left;
    // PDF y-axis is bottom-up, SVG is top-down
    let svg_y = page_size.height - margin.top - y_pos - height - offset_top;

    // Occlusion culling (default off): skip embedding the SVG (and
    // any rasters it would register) when a later opaque coverer
    // fully hides its box.
    if ctx.text.pdf_writer.opts.occlusion_cull {
        let raster = PdfRect::new(svg_x, svg_y, *width, *height);
        if raster_is_occluded(&occlusion_coverers, raster, elem_idx) {
            return;
        }
    }
    paint_svg_box(
        content,
        element,
        PdfRect::new(svg_x, svg_y, *width, *height),
        ctx,
    );
}

/// Paint one replaced SVG box after layout has chosen its absolute PDF-space
/// rectangle. Page-flow and nested-flow callers deliberately share this
/// function so neither path can implement a reduced effects contract.
pub(in crate::render::pdf) fn paint_svg_box(
    content: &mut String,
    element: &Svg,
    svg_box: PdfRect,
    ctx: &mut PageRenderContext<'_>,
) {
    let geometry = BoxGeometry::from_layout(svg_box, &element.geometry.border, EdgeSizes::ZERO);
    let group = PaintGroupScope::begin(
        content,
        element,
        geometry.for_fragment(Default::default()),
        ctx,
    );
    if let Some(background) = element.paint.background_color
        && background.alpha() > 0.0
    {
        let (r, g, b, a) = background.to_f32_rgba();
        let needs_alpha = a < 1.0;
        if needs_alpha {
            let state = format!("GSsvg{}", ctx.bg_alpha_counter);
            *ctx.bg_alpha_counter += 1;
            ctx.page_ext_gstates.push((state.clone(), a));
            content.push_str(&format!("/{state} gs\n"));
        }
        content.push_str(&format!(
            "{r} {g} {b} rg\n{}f\n",
            svg_box.rounded(element.paint.border_radii).path_or_rect()
        ));
        if needs_alpha {
            content.push_str("/GSDefault gs\n");
        }
    }

    let svg_content = geometry.padding_box();
    let content_w = svg_content.width;
    let content_h = svg_content.height;
    content.push_str("q\n");
    // Position on page and flip y-axis for SVG coordinates
    content.push_str(&format!(
        "1 0 0 -1 {} {} cm\n",
        svg_content.left,
        svg_content.top()
    ));
    if let Some(placement) = crate::render::svg_geometry::compute_replaced_svg_placement(
        &element.tree,
        crate::types::Size::new(content_w, content_h),
        element.replaced,
    ) {
        content.push_str("q\n");
        content.push_str(&format!("0 0 {content_w} {content_h} re\nW n\n"));
        content.push_str(&format!(
            "{sx} 0 0 {sy} {tx} {ty} cm\n",
            sx = placement.scale_x,
            sy = placement.scale_y,
            tx = placement.translate_x,
            ty = placement.translate_y,
        ));
        {
            let mut image_sink = SvgPageImageSink {
                pdf_writer: ctx.text.pdf_writer,
                page_images: ctx.text.page_images,
            };
            let mut resources = crate::render::svg_to_pdf::SvgPdfResources {
                shadings: ctx.shadings,
                shading_counter: ctx.shading_counter,
                ext_gstates: Some(ctx.page_ext_gstates),
                image_sink: Some(&mut image_sink),
                raster_scale_x: placement.scale_x.abs(),
                raster_scale_y: placement.scale_y.abs(),
                custom_fonts: Some(ctx.text.custom_fonts),
                prepared_custom_fonts: Some(ctx.text.prepared_custom_fonts),
            };
            crate::render::svg_to_pdf::render_svg_tree_with_resources(
                &element.tree,
                content,
                &mut resources,
            );
        }
        content.push_str("Q\n");
    }
    content.push_str("Q\n");
    paint_box_decoration(
        content,
        geometry.for_fragment(Default::default()),
        &element.geometry.border,
        element.paint.border_radii,
        element.paint.border_image.as_ref(),
        BorderPaintResources::from_page(ctx),
    );
    group.finish(content, ctx);
}
