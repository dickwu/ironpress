use super::*;
use crate::layout::elements::FlexRow;

pub(super) fn render_flex_child(
    content: &mut String,
    child: &FlexRow,
    child_index: usize,
    flow: &ContainerFlowContext<'_>,
    position: FlowPosition,
    abs_origins: &mut HashMap<usize, PdfPoint>,
    ctx: &mut PageRenderContext<'_>,
) -> FlowPosition {
    let x = flow.frame.content_origin.x + child.inline_offset.value();
    let flow_top_by_index = flow.flow_top_by_index;
    let FlowPosition {
        y: _,
        mut cursor_y,
        previous_margin_bottom: mut prev_margin_bottom,
    } = position;
    let mut y;
    let cells = &child.content.cells;
    let flex_mt = &child.box_model.margins.start;
    let flex_mb = &child.box_model.margins.end;
    let background_color = &child.paint.background.color;
    let border = &child.box_model.border;
    let flex_border_radii = &child.paint.border_radii;
    let box_shadow = &child.paint.shadows;
    let background_gradient = &child.paint.background.layers.gradient;
    let background_radial_gradient = &child.paint.background.layers.radial_gradient;
    let background_conic_gradient = &child.paint.background.layers.conic_gradient;
    let background_svg = &child.paint.background.layers.svg;
    let background_blur_radius = &child.paint.background.layers.blur_radius;
    let flex_bg_size = &child.paint.background.layers.size;
    let flex_bg_pos = &child.paint.background.layers.position;
    let flex_bg_repeat = &child.paint.background.layers.repeat;
    let flex_bg_origin = &child.paint.background.layers.origin;
    let flex_padding = &child.box_model.padding;
    let flex_row_h = &child.content.row_height;
    let align_items = &child.content.alignment;
    let flex_positioned_depth = &child.positioning.containing_block_depth;
    let planned_flow_top = flow_top_by_index.get(&child_index).copied();
    if let Some(top) = planned_flow_top {
        y = top;
    } else {
        cursor_y -= collapsed_margin_top_extra(*flex_mt, prev_margin_bottom);
        y = cursor_y;
    }
    let row_h = crate::layout::engine::estimate_element_height(child) - flex_mt - flex_mb;

    // The flex container honors its explicit width: paint its
    // background at `container_width` (already clamped to the
    // layout-time available width), not the full available width.
    // Mirrors the top-level FlexRow arm; without this a `width:Npx`
    // flex box painted its background across the whole content width.
    let flex_w = child.box_model.size.resolve_width(flow.frame.width);
    let flex_geometry = BoxGeometry::from_layout(
        PdfRect::from_top(x, y, flex_w, row_h),
        border,
        *flex_padding,
    );
    let flex_border_box = flex_geometry.border_box.rounded(*flex_border_radii);
    let flex_group = PaintGroupScope::begin(
        content,
        child,
        flex_geometry.for_fragment(Default::default()),
        ctx,
    );

    // A flex container that establishes a containing block records its
    // padding-box origin under its `positioned_depth`.
    if *flex_positioned_depth > 0 {
        let padding_box = flex_geometry.padding_box();
        abs_origins.insert(
            *flex_positioned_depth,
            PdfPoint::new(padding_box.left, padding_box.top()),
        );
    }

    render_box_shadows(
        content,
        box_shadow,
        flex_geometry.for_fragment(Default::default()),
        *flex_border_radii,
        ctx.page_ext_gstates,
        ctx.bg_alpha_counter,
        ctx.text.pdf_writer,
        ctx.text.page_images,
    );

    // Draw flex row background
    if let Some(color) = background_color {
        let (r, g, b, a) = color.to_f32_rgba();
        let needs_alpha = a < 1.0;
        if needs_alpha {
            let gs_name = format!("GScca{}", ctx.bg_alpha_counter);
            *ctx.bg_alpha_counter += 1;
            ctx.page_ext_gstates.push((gs_name.clone(), a));
            content.push_str(&format!("/{gs_name} gs\n"));
        }
        content.push_str(&format!("{r} {g} {b} rg\n"));
        content.push_str(&flex_border_box.path_or_rect());
        content.push_str("f\n");
        if needs_alpha {
            content.push_str("/GSDefault gs\n");
        }
    }

    if let Some(gradient) = background_gradient {
        let gradient = linear_with_background_layer(
            gradient,
            background_layer_box(*flex_bg_size, *flex_bg_pos, *flex_bg_repeat),
        );
        flex_border_box.push_clip(content);
        render_linear_gradient(
            content,
            &gradient,
            GradientBackdrop::isolated_linear_layer(
                *background_color,
                background_radial_gradient.is_some()
                    || background_conic_gradient.is_some()
                    || background_svg.is_some(),
                child.paint.background.blend_mode.background_layer(0),
            ),
            flex_geometry.border_box.left,
            flex_geometry.border_box.bottom,
            flex_geometry.border_box.width,
            flex_geometry.border_box.height,
            ctx.shadings,
            ctx.shading_counter,
            ctx.text.pdf_writer,
            ctx.text.page_images,
        );
        content.push_str("Q\n");
    }

    if let Some(gradient) = background_radial_gradient {
        let gradient = radial_with_background_layer(
            gradient,
            background_layer_box(*flex_bg_size, *flex_bg_pos, *flex_bg_repeat),
        );
        let rounded_clip = flex_border_box.push_rounded_clip(content);
        render_radial_gradient(
            content,
            &gradient,
            flex_geometry.border_box.left,
            flex_geometry.border_box.bottom,
            flex_geometry.border_box.width,
            flex_geometry.border_box.height,
            ctx.shadings,
            ctx.shading_counter,
            ctx.text.pdf_writer,
            ctx.text.page_images,
        );
        if rounded_clip {
            content.push_str("Q\n");
        }
    }

    if let Some(gradient) = background_conic_gradient {
        let gradient = conic_with_background_layer(
            gradient,
            background_layer_box(*flex_bg_size, *flex_bg_pos, *flex_bg_repeat),
        );
        let rounded_clip = flex_border_box.push_rounded_clip(content);
        render_conic_gradient(
            content,
            &gradient,
            flex_geometry.border_box.left,
            flex_geometry.border_box.bottom,
            flex_geometry.border_box.width,
            flex_geometry.border_box.height,
            ctx.text.pdf_writer,
            ctx.text.page_images,
        );
        if rounded_clip {
            content.push_str("Q\n");
        }
    }

    render_box_shadows_inset(
        content,
        box_shadow,
        flex_geometry.for_fragment(Default::default()),
        *flex_border_radii,
        ctx.page_ext_gstates,
        ctx.bg_alpha_counter,
        ctx.text.pdf_writer,
        ctx.text.page_images,
    );

    if let Some(svg_tree) = background_svg {
        let origin_box = flex_geometry.background_origin_box(*flex_bg_origin);
        render_svg_background(
            content,
            svg_tree,
            PdfBackgroundResources::new(
                ctx.text.pdf_writer,
                ctx.text.page_images,
                ctx.shadings,
                ctx.shading_counter,
                Some(&mut *ctx.page_ext_gstates),
            ),
            PdfBackgroundPaintContext::local(BackgroundPaintContext::new(
                origin_box.into(),
                flex_geometry.border_box.into(),
                *flex_border_radii,
                *background_blur_radius,
                *flex_bg_size,
                *flex_bg_pos,
                *flex_bg_repeat,
            )),
        );
    }

    // Draw the flex container's own border. Mirrors the top-level
    // FlexRow arm; the nested arm previously painted the background
    // but never the container border, so a bordered flex box nested
    // inside a block lost its frame entirely.
    if border.has_any() || child.paint.border_image.is_some() {
        paint_box_decoration(
            content,
            flex_geometry.for_fragment(Default::default()),
            border,
            *flex_border_radii,
            child.paint.border_image.as_ref(),
            BorderPaintResources::from_page(ctx),
        );
    }

    // Render flex cells. Anchor each cell to its layout-computed
    // main-axis offset (which folds in justify-content spacing and
    // `gap`) instead of accumulating widths — mirrors the top-level
    // FlexRow arm. Without this, nested flex rows packed left and
    // ignored justify-content/gap entirely.
    let cell_base_x = flex_geometry.content_box().left;
    let content_y = flex_geometry.content_box().top();

    let stacking_scope = StackingScope::for_element(child);
    let mut stacking_plan = StackingPaintPlan::default();
    for cell in cells {
        let marker = ctx.stacking.marker();
        let mut cell_content = String::new();
        'paint_cell: {
            let content = &mut cell_content;
            let cell_w = cell.width;
            let cell_x = cell_base_x + cell.x_offset;
            // Cross-axis (vertical) placement per align-items/align-self,
            // mirroring the top-level FlexRow arm. Stretch fills the line
            // cross size; otherwise the cell keeps its natural height and
            // is anchored at start/end/center. Without this the nested arm
            // force-stretched every cell to the full row height.
            // `align-self` on the item overrides the container's
            // `align-items` unless it is `auto`.
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
            let cross_geometry = cell.cross_geometry(*flex_row_h, *align_items, baseline_shift);
            let cell_h = cross_geometry.size;
            let cell_y_shift = cross_geometry.offset;
            let cell_top = content_y - cell_y_shift;
            let cell_bottom = cell_top - cell_h;
            let cell_geometry = BoxGeometry::from_layout(
                PdfRect::new(cell_x, cell_bottom, cell_w, cell_h),
                &cell.border,
                cell.padding,
            );
            let cell_border_box = cell_geometry.border_box.rounded(cell.paint.border_radii);
            let cell_shadows = FlexCellShadows::new(cell, cell_geometry);
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
            // Draw cell background
            if let Some(color) = cell.paint.background.color {
                let (cr, cg, cb, ca) = color.to_f32_rgba();
                let needs_alpha = ca < 1.0;
                if needs_alpha {
                    let gs_name = format!("GScca{}", ctx.bg_alpha_counter);
                    *ctx.bg_alpha_counter += 1;
                    ctx.page_ext_gstates.push((gs_name.clone(), ca));
                    content.push_str(&format!("/{gs_name} gs\n"));
                }
                content.push_str(&format!("{cr} {cg} {cb} rg\n"));
                content.push_str(&cell_border_box.path_or_rect());
                content.push_str("f\n");
                if needs_alpha {
                    content.push_str("/GSDefault gs\n");
                }
            }
            cell_shadows.paint_inset(content, ctx);
            // Draw cell border through the shared rounded-ring painter.
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
            // Draw cell text. Seat it relative to the cell's *content
            // box*, not its border box: the content origin is the
            // border-box top-left (`cell_top`, `cell_x`) inset by the
            // cell's top/left border and padding. This mirrors the
            // `flex_cell_baseline` model (`border-top + padding-top
            // + ...`) and the top-level FlexRow arm; without the inset the
            // text sat at the border-box top-left, painting it too high
            // and too far left.
            let content_box = cell_geometry.content_box();
            let content_left = content_box.left;
            let content_w = content_box.width;
            let mut baseline_cursor = TextBaselineCursor::new(content_box.top());
            for line in &cell.lines {
                let metrics = line_box_metrics(line, ctx.text.custom_fonts);
                let text_y = baseline_cursor.next_horizontal(metrics);
                let merged = merge_runs(&line.runs);
                let line_width: f32 = merged
                    .iter()
                    .map(|r| estimate_run_width_with_fonts(r, ctx.text.custom_fonts))
                    .sum();
                let text_x = match cell.text_align {
                    TextAlign::Right => content_left + (content_w - line_width).max(0.0),
                    TextAlign::Center => content_left + (content_w - line_width).max(0.0) / 2.0,
                    _ => content_left,
                };
                let mut lx = text_x;
                for (run_index, run) in merged.iter().enumerate() {
                    let run_width = estimate_run_width_with_fonts(run, ctx.text.custom_fonts);
                    let previous = merged[..run_index].iter().rev().find(|previous| {
                        previous.inline_box.is_none() && !previous.text.is_empty()
                    });
                    let decoration = HorizontalRunDecoration::new(
                        run,
                        lx,
                        run_width,
                        text_y,
                        ctx.text.custom_fonts,
                    )
                    .continuing_after(previous, lx);
                    let rw = decoration.paint_text(
                        content,
                        crate::layout::text::line_primary_font_size(&merged),
                        ctx.text.prepared_custom_fonts,
                        0.0,
                        ctx.text.pdf_writer,
                        ctx.text.page_images,
                    );
                    lx += rw;
                }
            }
            // Render nested elements in flex cells (tables, containers)
            if !cell.nested_elements.is_empty() {
                let text_h: f32 = cell.lines.iter().map(|l| l.height).sum();
                let nested_y = content_box.top() - text_h;
                let mut abs_origins: HashMap<usize, PdfPoint> = HashMap::new();
                render_container_children(
                    content,
                    &cell.nested_elements,
                    ContainerFrame::new(
                        PdfPoint::new(content_box.left, nested_y),
                        content_box.width,
                        PdfPoint::new(
                            content_box.left - cell.padding.left,
                            nested_y + cell.padding.top,
                        ),
                    ),
                    &mut abs_origins,
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
    if planned_flow_top.is_none() {
        cursor_y -= row_h + flex_mb;
        y = cursor_y;
    }
    prev_margin_bottom = *flex_mb;

    FlowPosition::new(y, cursor_y, prev_margin_bottom)
}
