use super::*;
use crate::layout::elements::FlexRow;

pub(in crate::render::pdf) fn render_flex_row(
    content: &mut String,
    element: &FlexRow,
    frame: PageElementFrame<'_>,
    phase: ElementPaintPhase,
    ctx: &mut PageRenderContext<'_>,
) {
    let page_size = frame.page_size;
    let margin = frame.margin;
    let y_pos = &frame.y_pos;
    let elem_idx = frame.element_index;
    let cells = &element.content.cells;
    let row_height = &element.content.row_height;
    let flex_offset_left = element.inline_offset.value();
    let background_color = &element.paint.background.color;
    let container_width = element.box_model.size.resolve_width(frame.available_width);
    let padding = &element.box_model.padding;
    let border = &element.box_model.border;
    let flex_radii = &element.paint.border_radii;
    let box_shadow = &element.paint.shadows;
    let background_gradient = &element.paint.background.layers.gradient;
    let background_radial_gradient = &element.paint.background.layers.radial_gradient;
    let background_conic_gradient = &element.paint.background.layers.conic_gradient;
    let background_svg = &element.paint.background.layers.svg;
    let background_blur_radius = &element.paint.background.layers.blur_radius;
    let flex_bg_size = &element.paint.background.layers.size;
    let flex_bg_pos = &element.paint.background.layers.position;
    let flex_bg_repeat = &element.paint.background.layers.repeat;
    let flex_bg_origin = &element.paint.background.layers.origin;
    let align_items = &element.content.alignment;
    let row_y = page_size.height - margin.top - y_pos;
    let flow_height = element.box_model.size.height.resolve(*row_height);
    let full_height = padding.vertical() + flow_height + border.vertical_width();
    // Inline-axis origin of the flex container's border box: the
    // page content-left plus the container's own resolved
    // horizontal margin / auto-centering (see `FlexRow.offset_left`).
    let flex_left = margin.left + flex_offset_left;
    // Inline-axis origin of the flex *content* box: in-flow cells
    // begin inside the container's left border (CSS box model — a
    // cell's `x_offset` is measured from the content box, so the
    // border-left width must be added, mirroring the cross-axis
    // `text_area_top` which already subtracts `border.top.width`).
    let cells_left = flex_left + border.left.width;
    let flex_geometry = BoxGeometry::from_layout(
        PdfRect::new(flex_left, row_y - full_height, container_width, full_height),
        border,
        *padding,
    );
    let flex_box = flex_geometry.border_box.rounded(*flex_radii);
    let flex_group = PaintGroupScope::begin(
        content,
        element,
        flex_geometry.for_fragment(Default::default()),
        ctx,
    );

    if phase.paints_decoration() {
        // Draw box shadow with blur
        render_box_shadows(
            content,
            box_shadow,
            flex_geometry.for_fragment(Default::default()),
            *flex_radii,
            ctx.page_ext_gstates,
            ctx.bg_alpha_counter,
            ctx.text.pdf_writer,
            ctx.text.page_images,
        );

        // Draw container background
        if let Some(background) = background_color {
            let (r, g, b, a) = background.to_f32_rgba();
            let needs_flex_bg_alpha = a < 1.0;
            if needs_flex_bg_alpha {
                let gs_name = format!("GSfbg{elem_idx}");
                ctx.page_ext_gstates.push((gs_name.clone(), a));
                content.push_str(&format!("/{gs_name} gs\n"));
            }
            content.push_str(&format!("{r} {g} {b} rg\n"));
            content.push_str(&flex_box.path_or_rect());
            content.push_str("f\n");
            if needs_flex_bg_alpha {
                content.push_str("/GSDefault gs\n");
            }
        }

        // Draw container linear gradient
        if let Some(gradient) = background_gradient {
            let gradient = linear_with_background_layer(
                gradient,
                background_layer_box(*flex_bg_size, *flex_bg_pos, *flex_bg_repeat),
            );
            flex_box.push_clip(content);
            render_linear_gradient(
                content,
                &gradient,
                GradientBackdrop::isolated_linear_layer(
                    *background_color,
                    background_radial_gradient.is_some()
                        || background_conic_gradient.is_some()
                        || background_svg.is_some(),
                    element.paint.background.blend_mode.background_layer(0),
                ),
                flex_box.rect.left,
                flex_box.rect.bottom,
                flex_box.rect.width,
                flex_box.rect.height,
                ctx.shadings,
                ctx.shading_counter,
                ctx.text.pdf_writer,
                ctx.text.page_images,
            );
            content.push_str("Q\n");
        }

        // Draw container radial gradient
        if let Some(gradient) = background_radial_gradient {
            let gradient = radial_with_background_layer(
                gradient,
                background_layer_box(*flex_bg_size, *flex_bg_pos, *flex_bg_repeat),
            );
            let clipped = flex_box.push_rounded_clip(content);
            render_radial_gradient(
                content,
                &gradient,
                flex_box.rect.left,
                flex_box.rect.bottom,
                flex_box.rect.width,
                flex_box.rect.height,
                ctx.shadings,
                ctx.shading_counter,
                ctx.text.pdf_writer,
                ctx.text.page_images,
            );
            if clipped {
                content.push_str("Q\n");
            }
        }

        // Draw container conic gradient
        if let Some(gradient) = background_conic_gradient {
            let gradient = conic_with_background_layer(
                gradient,
                background_layer_box(*flex_bg_size, *flex_bg_pos, *flex_bg_repeat),
            );
            let clipped = flex_box.push_rounded_clip(content);
            render_conic_gradient(
                content,
                &gradient,
                flex_box.rect.left,
                flex_box.rect.bottom,
                flex_box.rect.width,
                flex_box.rect.height,
                ctx.text.pdf_writer,
                ctx.text.page_images,
            );
            if clipped {
                content.push_str("Q\n");
            }
        }

        // Draw inset box-shadow for flex container (after backgrounds).
        render_box_shadows_inset(
            content,
            box_shadow,
            flex_geometry.for_fragment(Default::default()),
            *flex_radii,
            ctx.page_ext_gstates,
            ctx.bg_alpha_counter,
            ctx.text.pdf_writer,
            ctx.text.page_images,
        );

        // Draw SVG background image for flex container
        if let Some(svg_tree) = background_svg {
            let reference = flex_geometry.background_origin_box(*flex_bg_origin);
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
                PdfBackgroundPaintContext::local(BackgroundPaintContext::new(
                    reference.into(),
                    flex_box.rect.into(),
                    flex_box.radii,
                    *background_blur_radius,
                    *flex_bg_size,
                    *flex_bg_pos,
                    *flex_bg_repeat,
                )),
            );
        }

        // Draw border
        if border.has_any() || element.paint.border_image.is_some() {
            paint_box_decoration(
                content,
                flex_geometry.for_fragment(Default::default()),
                border,
                *flex_radii,
                element.paint.border_image.as_ref(),
                BorderPaintResources::from_page(ctx),
            );
        }
    }

    if !phase.paints_contents() {
        flex_group.finish(content, ctx);
        return;
    }

    // Render each flex cell at its computed x-offset
    let text_area_top = row_y - border.top.width - padding.top;

    // Flex order is already resolved by layout. Traverse in that order for
    // geometry, then schedule each item in the flex container's nearest CSS
    // stacking context. Non-context items allow their positioned descendants
    // to escape just like ordinary block ancestors.
    let stacking_scope = StackingScope::for_element(element);
    let mut stacking_plan = StackingPaintPlan::default();
    for cell in cells {
        let marker = ctx.stacking.marker();
        let mut cell_content = String::new();
        'paint_cell: {
            let content = &mut cell_content;
            let cell_x = cells_left + padding.left + cell.x_offset;
            // For single-line rows `line_cross_size == row_height`.
            // For multi-line wrap, each cell's line_cross_size is its
            // own flex line height, so alignment is per-line.
            // Compute per-cell height and vertical offset based on the
            // effective cross-axis alignment. Pagination uses the same domain
            // helper when it propagates fragmentainer space into descendants.
            let effective_align = flex_cell_align(cell, *align_items);
            let baseline_shift = if effective_align == AlignItems::Baseline {
                match (
                    flex_cell_baseline(cell, ctx.text.custom_fonts),
                    flex_line_max_baseline(
                        cells,
                        cell.line_id,
                        *align_items,
                        ctx.text.custom_fonts,
                    ),
                ) {
                    (Some(own), Some(max)) => (max - own).max(0.0),
                    _ => 0.0,
                }
            } else {
                0.0
            };
            let cross_geometry = cell.cross_geometry(*row_height, *align_items, baseline_shift);
            let cell_render_h = cell
                .fragmentation
                .fragment_block_extent
                .unwrap_or(cross_geometry.size);
            let cell_y_shift = cross_geometry.offset;
            let cell_geometry = BoxGeometry::from_layout(
                PdfRect::new(
                    cell_x,
                    text_area_top - cell_y_shift - cell_render_h,
                    cell.width,
                    cell_render_h,
                ),
                &cell.border,
                cell.padding,
            );
            let cell_box = cell_geometry.border_box.rounded(cell.paint.border_radii);
            let cell_shadows = FlexCellShadows::new(cell, cell_geometry);
            let cell_inner_w = cell_geometry.content_box().width;
            let cell_group = PaintGroupScope::begin(
                content,
                cell,
                cell_geometry.for_fragment(cell.fragmentation.box_fragmentation),
                ctx,
            );
            if paint_cell_filter_output(content, &cell.paint, cell_geometry, ctx) {
                cell_group.finish(content, ctx);
                break 'paint_cell;
            }
            cell_shadows.paint_outset(content, ctx);

            if cell.paint.background.layers.blur_radius > 0.0
                && cell.lines.is_empty()
                && cell.nested_elements.is_empty()
                && cell.paint.background.layers.gradient.is_none()
                && cell.paint.background.layers.radial_gradient.is_none()
                && cell.paint.background.layers.conic_gradient.is_none()
                && cell.paint.background.layers.svg.is_none()
                && cell.paint.border_radii.is_zero()
                && let Some(blurred) = crate::render::blur::blur_box(
                    cell.width,
                    cell_render_h,
                    cell.paint.background.color,
                    &cell.border,
                    cell.paint.background.layers.blur_radius,
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
                let cell_bg_x = cells_left + padding.left + cell.x_offset;
                let cell_bg_y = text_area_top - cell_y_shift - cell_render_h;
                content.push_str(&format!(
                    "q\n{w} 0 0 {h} {ix} {iy} cm\n/{name} Do\nQ\n",
                    w = cell.width + 2.0 * ov,
                    h = cell_render_h + 2.0 * ov,
                    ix = cell_bg_x - ov,
                    iy = cell_bg_y - ov,
                    name = img_name,
                ));
                ctx.text.page_images.push(ImageRef {
                    name: img_name,
                    obj_id: img_obj_id,
                });
                cell_group.finish(content, ctx);
                break 'paint_cell;
            }

            // Draw cell background
            if let Some(background) = cell.paint.background.color {
                let (r, g, b, a) = background.to_f32_rgba();
                let needs_fcell_bg_alpha = a < 1.0;
                if needs_fcell_bg_alpha {
                    let gs_name = format!("GSfcbg{}", ctx.bg_alpha_counter);
                    *ctx.bg_alpha_counter += 1;
                    ctx.page_ext_gstates.push((gs_name.clone(), a));
                    content.push_str(&format!("/{gs_name} gs\n"));
                }
                content.push_str(&format!("{r} {g} {b} rg\n"));
                content.push_str(&cell_box.path_or_rect());
                content.push_str("f\n");
                if needs_fcell_bg_alpha {
                    content.push_str("/GSDefault gs\n");
                }
            }

            cell_shadows.paint_inset(content, ctx);

            // Draw cell borders through the same geometry used by every other box.
            if cell.border.has_any() || cell.paint.border_image.is_some() {
                paint_box_decoration(
                    content,
                    cell_geometry.for_fragment(cell.fragmentation.box_fragmentation),
                    &cell.border,
                    cell.paint.border_radii,
                    cell.paint.border_image.as_ref(),
                    BorderPaintResources::from_page(ctx),
                );
            }

            // Draw cell linear gradient
            if let Some(gradient) = &cell.paint.background.layers.gradient {
                let clipped = cell_box.push_rounded_clip(content);
                render_linear_gradient(
                    content,
                    gradient,
                    GradientBackdrop::isolated_linear_layer(
                        cell.paint.background.color,
                        cell.paint.background.layers.radial_gradient.is_some()
                            || cell.paint.background.layers.conic_gradient.is_some()
                            || cell.paint.background.layers.svg.is_some(),
                        crate::style::computed::BlendMode::Normal,
                    ),
                    cell_box.rect.left,
                    cell_box.rect.bottom,
                    cell_box.rect.width,
                    cell_box.rect.height,
                    ctx.shadings,
                    ctx.shading_counter,
                    ctx.text.pdf_writer,
                    ctx.text.page_images,
                );
                if clipped {
                    content.push_str("Q\n");
                }
            }

            // Draw cell radial gradient
            if let Some(gradient) = &cell.paint.background.layers.radial_gradient {
                let clipped = cell_box.push_rounded_clip(content);
                render_radial_gradient(
                    content,
                    gradient,
                    cell_box.rect.left,
                    cell_box.rect.bottom,
                    cell_box.rect.width,
                    cell_box.rect.height,
                    ctx.shadings,
                    ctx.shading_counter,
                    ctx.text.pdf_writer,
                    ctx.text.page_images,
                );
                if clipped {
                    content.push_str("Q\n");
                }
            }

            // Draw cell conic gradient
            if let Some(gradient) = &cell.paint.background.layers.conic_gradient {
                let clipped = cell_box.push_rounded_clip(content);
                render_conic_gradient(
                    content,
                    gradient,
                    cell_box.rect.left,
                    cell_box.rect.bottom,
                    cell_box.rect.width,
                    cell_box.rect.height,
                    ctx.text.pdf_writer,
                    ctx.text.page_images,
                );
                if clipped {
                    content.push_str("Q\n");
                }
            }

            if let Some(svg_tree) = &cell.paint.background.layers.svg {
                let reference =
                    cell_geometry.background_origin_box(cell.paint.background.layers.origin);
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
                    PdfBackgroundPaintContext::local(BackgroundPaintContext::new(
                        reference.into(),
                        cell_box.rect.into(),
                        cell_box.radii,
                        cell.paint.background.layers.blur_radius,
                        cell.paint.background.layers.size,
                        cell.paint.background.layers.position,
                        cell.paint.background.layers.repeat,
                    )),
                );
            }

            // Render cell text
            let mut baseline_cursor = TextBaselineCursor::new(
                text_area_top - cell_y_shift - cell.border.top.width - cell.padding.top,
            );
            for line in &cell.lines {
                let metrics = line_box_metrics(line, ctx.text.custom_fonts);
                let text_y = baseline_cursor.next_horizontal(metrics);
                let line_annotation_top = text_y + metrics.ascender + metrics.half_leading;
                let line_annotation_bottom = text_y - metrics.descender - metrics.half_leading;
                let text_content: String = line.runs.iter().map(|r| r.text.as_str()).collect();
                if text_content.is_empty() {
                    continue;
                }
                let merged = merge_runs(&line.runs);
                // Calculate line width for text-align
                let line_width: f32 = merged
                    .iter()
                    .map(|r| {
                        let w = estimate_run_width_with_fonts(r, ctx.text.custom_fonts);
                        w + r.padding.horizontal()
                    })
                    .sum();
                let first_pad = line.runs.first().map_or(0.0, |r| r.padding.left);
                let text_x = match cell.text_align {
                    TextAlign::Right => {
                        cell_x
                            + cell.border.left.width
                            + cell.padding.left
                            + (cell_inner_w - line_width).max(0.0)
                            + first_pad
                    }
                    TextAlign::Center => {
                        cell_x
                            + cell.border.left.width
                            + cell.padding.left
                            + ((cell_inner_w - line_width) / 2.0).max(0.0)
                            + first_pad
                    }
                    _ => cell_x + cell.border.left.width + cell.padding.left,
                };
                let mut x = text_x;
                for (run_index, run) in merged.iter().enumerate() {
                    if run.text.is_empty() {
                        continue;
                    }
                    let rw = estimate_run_width_with_fonts(run, ctx.text.custom_fonts);
                    let previous = merged[..run_index].iter().rev().find(|previous| {
                        previous.inline_box.is_none() && !previous.text.is_empty()
                    });
                    let decoration =
                        HorizontalRunDecorations::new(run, x, rw, text_y, ctx.text.custom_fonts)
                            .continuing_after(previous);

                    // Draw background rectangle for inline spans
                    if let Some(background) = run.background_color {
                        let (br, bgc, bb, ba) = background.to_f32_rgba();
                        let needs_inline_bg_alpha = ba < 1.0;
                        if needs_inline_bg_alpha {
                            let gs_name = format!("GSfiba{}", ctx.bg_alpha_counter);
                            *ctx.bg_alpha_counter += 1;
                            ctx.page_ext_gstates.push((gs_name.clone(), ba));
                            content.push_str(&format!("/{gs_name} gs\n"));
                        }
                        let rx = x - run.padding.left;
                        let rw2 = rw + run.padding.horizontal();
                        let (ry, rh) = inline_background_y_and_height(
                            run,
                            text_y,
                            run.padding,
                            ctx.text.custom_fonts,
                        );
                        content.push_str(&format!("{br} {bgc} {bb} rg\n"));
                        content.push_str(
                            &PdfRect::new(rx, ry, rw2, rh)
                                .rounded(run.border_radii)
                                .path_or_rect(),
                        );
                        content.push_str("f\n");
                        if needs_inline_bg_alpha {
                            content.push_str("/GSDefault gs\n");
                        }
                    }

                    decoration.paint_text(
                        content,
                        crate::layout::text::line_primary_font_size(&merged),
                        ctx.text.prepared_custom_fonts,
                        0.0,
                        ctx.text.pdf_writer,
                        ctx.text.page_images,
                    );

                    if decoration_is_emphasis(run) {
                        render_text_emphasis_marks(
                            content,
                            run,
                            x,
                            text_y,
                            run.metadata.emphasis.color,
                            ctx.text.custom_fonts,
                            ctx.text.prepared_custom_fonts,
                            ctx.text.pdf_writer,
                            ctx.text.page_images,
                        );
                    }

                    if let Some(annotation) = text_run_link_annotation(
                        run,
                        PdfRect::new(
                            x,
                            line_annotation_bottom,
                            rw,
                            line_annotation_top - line_annotation_bottom,
                        ),
                    ) {
                        ctx.text.annotations.push(annotation);
                    }

                    x += rw;
                }
            }

            // Render nested elements (tables, images, SVGs, blocks,
            // etc. inside flex/inline-block items) through the shared
            // block child renderer so variant support matches normal
            // container children.
            if !cell.nested_elements.is_empty() {
                let text_h: f32 = cell.lines.iter().map(|l| l.height).sum();
                let (nested_x, nested_y, nested_w, padding_origin) = match cell.nested_origin {
                    FlexNestedOrigin::ContentBox => {
                        let x = cell_x + cell.border.left.width + cell.padding.left;
                        let y = text_area_top
                            - cell_y_shift
                            - cell.border.top.width
                            - cell.padding.top
                            - text_h;
                        (
                            x,
                            y,
                            (cell.width
                                - cell.border.horizontal_width()
                                - cell.padding.horizontal())
                            .max(0.0),
                            PdfPoint::new(x - cell.padding.left, y + cell.padding.top + text_h),
                        )
                    }
                    FlexNestedOrigin::TableBorderBox => {
                        let y = text_area_top - cell_y_shift - text_h;
                        (cell_x, y, cell.width, PdfPoint::new(cell_x, y + text_h))
                    }
                };
                let mut nested_abs_origins: HashMap<usize, PdfPoint> = HashMap::new();
                render_container_children(
                    content,
                    &cell.nested_elements,
                    ContainerFrame::new(
                        PdfPoint::new(nested_x, nested_y),
                        nested_w,
                        padding_origin,
                    ),
                    &mut nested_abs_origins,
                    ctx,
                    ContainerRenderOptions {
                        stacking_scope: if cell.establishes_stacking_context() {
                            StackingScope::Local
                        } else {
                            StackingScope::Ancestor
                        },
                        ..Default::default()
                    },
                );
            }

            cell_group.finish(content, ctx);
        }
        let descendants = ctx.stacking.take_since(marker);
        ctx.stacking.commit(
            stacking_scope,
            content,
            &mut stacking_plan,
            cell.stacking_level(),
            cell_content,
            descendants,
        );
    }
    if stacking_scope.is_local() {
        ctx.stacking.paint_plan(stacking_plan, content);
    }
    flex_group.finish(content, ctx);
}
