use super::*;
use crate::layout::elements::{GridRow, LayoutVisitor, TableRow};

fn table_row_height(element: &dyn LayoutElement) -> Option<f32> {
    #[derive(Default)]
    struct Height(Option<f32>);

    impl LayoutVisitor for Height {
        fn visit_table_row(&mut self, element: &TableRow) {
            self.0 = Some(compute_row_height(&element.content.cells));
        }
    }

    let mut height = Height::default();
    element.accept(&mut height);
    height.0
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum FirstMarginState {
    #[default]
    Pending,
    Resolved,
}

/// Entry state for a contiguous run of internal table/grid rows.
/// Reordered painting receives positions from the shared flow planner, where
/// the first table margin has already been collapsed; source-order painting
/// resolves it here. The distinction is explicit so a margin is never applied
/// twice merely because stacking order required a planning pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct NestedRowsFlow {
    position: FlowPosition,
    first_margin: FirstMarginState,
}

impl NestedRowsFlow {
    pub(super) const fn pending(position: FlowPosition) -> Self {
        Self {
            position,
            first_margin: FirstMarginState::Pending,
        }
    }

    pub(super) const fn resolved(position: FlowPosition) -> Self {
        Self {
            position,
            first_margin: FirstMarginState::Resolved,
        }
    }
}

struct NestedRowsRenderer<'call, 'fonts> {
    content: &'call mut String,
    origin_x: f32,
    cursor_y: f32,
    page_ext_gstates: &'call mut Vec<(String, f32)>,
    bg_alpha_counter: &'call mut usize,
    custom_fonts: &'fonts HashMap<String, TtfFont>,
    prepared_custom_fonts: &'fonts PreparedCustomFonts,
    page_shadings: &'call mut Vec<ShadingEntry>,
    shading_counter: &'call mut usize,
    pdf_writer: &'call mut PdfWriter,
    page_images: &'call mut Vec<ImageRef>,
    annotations: &'call mut Vec<LinkAnnotation>,
    stacking: &'call mut StackingTraversal,
    page_paint_box: PdfRect,
    page_height: f32,
    previous_margin_bottom: f32,
    first_margin: FirstMarginState,
    row_heights: Vec<Option<f32>>,
    element_index: usize,
    handled: bool,
}

impl LayoutVisitor for NestedRowsRenderer<'_, '_> {
    fn visit_table_row(&mut self, element: &TableRow) {
        self.handled = true;
        let content = &mut *self.content;
        let origin_x = self.origin_x;
        let mut cursor_y = self.cursor_y;
        let page_ext_gstates = &mut *self.page_ext_gstates;
        let bg_alpha_counter = &mut *self.bg_alpha_counter;
        let custom_fonts = self.custom_fonts;
        let prepared_custom_fonts = self.prepared_custom_fonts;
        let page_shadings = &mut *self.page_shadings;
        let shading_counter = &mut *self.shading_counter;
        let pdf_writer = &mut *self.pdf_writer;
        let page_images = &mut *self.page_images;
        let annotations = &mut *self.annotations;
        let page_paint_box = self.page_paint_box;
        let page_height = self.page_height;
        let cells = &element.content.cells;
        let col_widths = &element.content.column_widths;
        let border_collapse = &element.formatting.border_collapse;
        let border_spacing = &element.formatting.border_spacing;
        let outer_margins = element.flow.margins;
        let internal_spacing = element.flow.internal;
        let flow_extra_bottom = element.flow.extra_end;
        let offset_left = element.grid_inline_offset();
        if self.first_margin == FirstMarginState::Pending {
            cursor_y -=
                collapsed_margin_top_extra(outer_margins.start, self.previous_margin_bottom);
        }
        self.first_margin = FirstMarginState::Pending;
        cursor_y -= internal_spacing.start;
        let spacing = if *border_collapse == BorderCollapse::Collapse {
            0.0
        } else {
            *border_spacing
        };
        let row_y = cursor_y;
        let row_origin_x = table_row_origin_x(origin_x, offset_left);
        let row_height = compute_row_height(cells);
        let baseline_shifts = row_baseline_shifts(cells, custom_fonts);
        let mut col_pos: usize = 0;
        let stacking_scope = StackingScope::for_element(element);
        let mut stacking_plan = StackingPaintPlan::default();
        for (cell_idx, cell) in cells.iter().enumerate() {
            if cell.span.rows == 0 {
                col_pos += cell.span.columns;
                continue;
            }
            let marker = self.stacking.marker();
            let mut cell_content = String::new();
            'paint_cell: {
                let content = &mut cell_content;
                let (cell_x, cell_w) = table_cell_geometry(
                    col_widths,
                    col_pos,
                    cell.span.columns,
                    spacing,
                    row_origin_x,
                );
                let (horizontal_border_left, horizontal_border_right) =
                    collapsed_table_horizontal_border_span(
                        cell,
                        *border_collapse,
                        col_pos == 0,
                        cell_x,
                        cell_x + cell_w,
                    );
                let cell_height = row_height
                    + self
                        .row_heights
                        .iter()
                        .skip(self.element_index + 1)
                        .take(cell.span.rows.saturating_sub(1))
                        .flatten()
                        .sum::<f32>();
                let cell_geometry = BoxGeometry::from_layout(
                    PdfRect::new(cell_x, row_y - cell_height, cell_w, cell_height),
                    &cell.layout.box_model.border,
                    cell.layout.box_model.content_insets,
                );
                let cell_border_box = cell_geometry
                    .border_box
                    .rounded(cell.layout.paint.border_radii);
                let cell_group = {
                    let mut cell_ctx = PageRenderContext::new(
                        pdf_writer,
                        page_images,
                        custom_fonts,
                        prepared_custom_fonts,
                        page_shadings,
                        shading_counter,
                        page_ext_gstates,
                        bg_alpha_counter,
                        annotations,
                        page_paint_box,
                        page_height,
                    );
                    PaintGroupScope::begin(
                        content,
                        &cell.layout,
                        cell_geometry.for_fragment(Default::default()),
                        &mut cell_ctx,
                    )
                };
                let filtered = {
                    let mut cell_ctx = PageRenderContext::new(
                        pdf_writer,
                        page_images,
                        custom_fonts,
                        prepared_custom_fonts,
                        page_shadings,
                        shading_counter,
                        page_ext_gstates,
                        bg_alpha_counter,
                        annotations,
                        page_paint_box,
                        page_height,
                    );
                    paint_box_filter_output(content, &cell.layout, cell_geometry, &mut cell_ctx)
                };
                if filtered {
                    let mut cell_ctx = PageRenderContext::new(
                        pdf_writer,
                        page_images,
                        custom_fonts,
                        prepared_custom_fonts,
                        page_shadings,
                        shading_counter,
                        page_ext_gstates,
                        bg_alpha_counter,
                        annotations,
                        page_paint_box,
                        page_height,
                    );
                    cell_group.finish(content, &mut cell_ctx);
                    break 'paint_cell;
                }
                if !cell.table.hide_if_empty {
                    render_box_shadows(
                        content,
                        &cell.layout.paint.shadows,
                        cell_geometry.for_fragment(Default::default()),
                        cell.layout.paint.border_radii,
                        page_ext_gstates,
                        bg_alpha_counter,
                        pdf_writer,
                        page_images,
                    );
                }
                // Draw cell background
                if let Some(background) = cell
                    .layout
                    .paint
                    .background
                    .color
                    .filter(|_| !cell.table.hide_if_empty)
                {
                    let (r, g, b, a) = background.to_f32_rgba();
                    let needs_alpha = a < 1.0;
                    if needs_alpha {
                        let gs_name = format!("GScca{bg_alpha_counter}");
                        *bg_alpha_counter += 1;
                        page_ext_gstates.push((gs_name.clone(), a));
                        content.push_str(&format!("/{gs_name} gs\n"));
                    }
                    content.push_str(&format!("{r} {g} {b} rg\n"));
                    content.push_str(&cell_border_box.path_or_rect());
                    content.push_str("f\n");
                    if needs_alpha {
                        content.push_str("/GSDefault gs\n");
                    }
                }
                if !cell.table.hide_if_empty {
                    let mut cell_ctx = PageRenderContext::new(
                        pdf_writer,
                        page_images,
                        custom_fonts,
                        prepared_custom_fonts,
                        page_shadings,
                        shading_counter,
                        page_ext_gstates,
                        bg_alpha_counter,
                        annotations,
                        page_paint_box,
                        page_height,
                    );
                    paint_cell_gradient_backgrounds(
                        content,
                        &cell.layout,
                        cell_geometry,
                        &mut cell_ctx,
                    );
                    render_box_shadows_inset(
                        content,
                        &cell.layout.paint.shadows,
                        cell_geometry.for_fragment(Default::default()),
                        cell.layout.paint.border_radii,
                        cell_ctx.page_ext_gstates,
                        cell_ctx.bg_alpha_counter,
                        cell_ctx.text.pdf_writer,
                        cell_ctx.text.page_images,
                    );
                }

                let separate = *border_collapse == BorderCollapse::Separate;
                let border = &cell.layout.box_model.border;
                if separate
                    && (border.has_any() || cell.layout.paint.border_image.is_some())
                    && !cell.table.hide_if_empty
                {
                    paint_box_decoration(
                        content,
                        cell_geometry.for_fragment(Default::default()),
                        border,
                        cell.layout.paint.border_radii,
                        cell.layout.paint.border_image.as_ref(),
                        BorderPaintResources {
                            shadings: page_shadings,
                            shading_counter,
                            page_ext_gstates,
                            alpha_counter: bg_alpha_counter,
                            pdf_writer,
                            page_images,
                        },
                    );
                } else if (border.has_any() || cell.table.has_resolved_collapsed_borders())
                    && !cell.table.hide_if_empty
                {
                    if !separate && cell.table.has_resolved_collapsed_borders() {
                        paint_resolved_collapsed_cell_borders(
                            content,
                            cell,
                            CollapsedCellTrackGeometry {
                                border_box: PdfRect::new(
                                    cell_x,
                                    row_y - cell_height,
                                    cell_w,
                                    cell_height,
                                ),
                                column_widths: col_widths,
                                column_start: col_pos,
                                row_heights: &self.row_heights,
                                row_element_index: self.element_index,
                                current_row_height: row_height,
                            },
                            page_ext_gstates,
                            bg_alpha_counter,
                        );
                    } else {
                        let inset = |w: f32| if separate { w / 2.0 } else { 0.0 };
                        let x1 = cell_x;
                        let x2 = cell_x + cell_w;
                        let y_top = row_y;
                        let y_bottom = row_y - cell_height;
                        let (vertical_top, vertical_bottom) = collapsed_table_vertical_border_span(
                            cell,
                            *border_collapse,
                            y_top,
                            y_bottom,
                        );
                        if border.top.width > 0.0 {
                            let side = border.top;
                            let y = y_top - inset(border.top.width);
                            paint_table_cell_border_line(
                                content,
                                &side,
                                PhysicalSide::Top,
                                horizontal_border_left,
                                y,
                                horizontal_border_right,
                                y,
                                page_ext_gstates,
                                bg_alpha_counter,
                            );
                        }
                        if border.right.width > 0.0 {
                            let collapsed_right = !separate
                                && cell.table.collapsed_outer_edges.right
                                && border.right.paints();
                            let side = border.right;
                            if collapsed_right {
                                let w = side.width;
                                paint_collapsed_outer_right_border(
                                    content,
                                    &side,
                                    x2 - w / 2.0,
                                    vertical_bottom,
                                    w,
                                    vertical_top - vertical_bottom,
                                    page_ext_gstates,
                                    bg_alpha_counter,
                                );
                            } else {
                                let x = x2 - inset(border.right.width);
                                paint_table_cell_border_line(
                                    content,
                                    &side,
                                    PhysicalSide::Right,
                                    x,
                                    vertical_top,
                                    x,
                                    vertical_bottom,
                                    page_ext_gstates,
                                    bg_alpha_counter,
                                );
                            }
                        }
                        if border.bottom.width > 0.0 {
                            let side = border.bottom;
                            let y = y_bottom + inset(border.bottom.width);
                            paint_table_cell_border_line(
                                content,
                                &side,
                                PhysicalSide::Bottom,
                                horizontal_border_left,
                                y,
                                horizontal_border_right,
                                y,
                                page_ext_gstates,
                                bg_alpha_counter,
                            );
                        }
                        if border.left.width > 0.0 {
                            let side = border.left;
                            let x = x1 + inset(border.left.width);
                            paint_table_cell_border_line(
                                content,
                                &side,
                                PhysicalSide::Left,
                                x,
                                vertical_top,
                                x,
                                vertical_bottom,
                                page_ext_gstates,
                                bg_alpha_counter,
                            );
                        }
                    }
                }

                let mut page_context = PageRenderContext::new(
                    pdf_writer,
                    page_images,
                    custom_fonts,
                    prepared_custom_fonts,
                    page_shadings,
                    shading_counter,
                    page_ext_gstates,
                    bg_alpha_counter,
                    annotations,
                    page_paint_box,
                    page_height,
                );
                page_context.stacking = self.stacking.fork();
                render_cell_content(
                    content,
                    &cell.layout,
                    CellRenderBox::new(PdfPoint::new(cell_x, row_y), cell_w, row_height)
                        .with_baseline_shift(baseline_shifts.get(cell_idx).copied().unwrap_or(0.0)),
                    &mut page_context,
                );
                cell_group.finish(content, &mut page_context);
                self.stacking.restore(page_context.stacking.take_since(0));
            }
            let descendants = self.stacking.take_since(marker);
            self.stacking.commit(
                stacking_scope,
                content,
                &mut stacking_plan,
                cell.layout.stacking_level(),
                cell_content,
                descendants,
            );
            col_pos += cell.span.columns;
        }
        if stacking_scope.is_local() {
            self.stacking.paint_plan(stacking_plan, content);
        }
        cursor_y -= row_height + internal_spacing.end + flow_extra_bottom + outer_margins.end;
        self.cursor_y = cursor_y;
        self.previous_margin_bottom = outer_margins.end;
    }

    fn visit_grid_row(&mut self, element: &GridRow) {
        self.handled = true;
        let content = &mut *self.content;
        let origin_x = self.origin_x;
        let mut cursor_y = self.cursor_y;
        let page_ext_gstates = &mut *self.page_ext_gstates;
        let bg_alpha_counter = &mut *self.bg_alpha_counter;
        let custom_fonts = self.custom_fonts;
        let prepared_custom_fonts = self.prepared_custom_fonts;
        let page_shadings = &mut *self.page_shadings;
        let shading_counter = &mut *self.shading_counter;
        let pdf_writer = &mut *self.pdf_writer;
        let page_images = &mut *self.page_images;
        let annotations = &mut *self.annotations;
        let page_paint_box = self.page_paint_box;
        let page_height = self.page_height;
        let cells = &element.content.cells;
        let col_widths = &element.content.column_widths;
        let gap = &element.content.gap;
        let grid_border = &element.box_model.border;
        let grid_padding = &element.box_model.padding;
        let margin_top = &element.box_model.margins.start;
        let margin_bottom = &element.box_model.margins.end;
        self.previous_margin_bottom = 0.0;
        if self.first_margin == FirstMarginState::Pending {
            cursor_y -= margin_top;
        }
        self.first_margin = FirstMarginState::Pending;
        let row_y = cursor_y;
        let row_height =
            compute_grid_row_height(cells) + grid_padding.vertical() + grid_border.vertical_width();
        let grid_total_w: f32 = col_widths.iter().sum::<f32>()
            + gap * col_widths.len().saturating_sub(1) as f32
            + grid_padding.horizontal()
            + grid_border.horizontal_width();
        let grid_geometry = BoxGeometry::from_layout(
            PdfRect::from_top(origin_x, row_y, grid_total_w, row_height),
            grid_border,
            *grid_padding,
        );
        paint_box_decoration(
            content,
            grid_geometry.for_fragment(Default::default()),
            grid_border,
            CornerRadii::ZERO,
            None,
            BorderPaintResources {
                shadings: page_shadings,
                shading_counter,
                page_ext_gstates,
                alpha_counter: bg_alpha_counter,
                pdf_writer,
                page_images,
            },
        );

        let cell_row_y = grid_geometry.content_box().top();
        let cell_content_h = compute_grid_row_height(cells);
        let stacking_scope = StackingScope::for_element(element);
        let mut stacking_plan = StackingPaintPlan::default();
        for cell in cells.iter() {
            let column_start = cell.placement.column_start;
            let span = cell.placement.column_span.max(1);
            let marker = self.stacking.marker();
            let mut cell_content = String::new();
            'paint_cell: {
                let content = &mut cell_content;
                let track_x = grid_geometry.content_box().left
                    + col_widths.iter().take(column_start).sum::<f32>()
                    + gap * column_start as f32;
                let track_w: f32 = col_widths.iter().skip(column_start).take(span).sum::<f32>()
                    + gap * span.saturating_sub(1) as f32;

                // The painted box (background + border) either fills the
                // track cell or, for grid items with an explicit smaller
                // size, is inset per justify-items/align-items.
                let (box_x, box_y, box_w, box_h) = match cell.placement.inset {
                    Some(ins) => (
                        track_x + ins.offset.x,
                        cell_row_y - ins.offset.y - ins.size.height,
                        ins.size.width,
                        ins.size.height,
                    ),
                    None => (
                        track_x,
                        cell_row_y - cell_content_h,
                        track_w,
                        cell_content_h,
                    ),
                };
                let cell_geometry = BoxGeometry::from_layout(
                    PdfRect::new(box_x, box_y, box_w, box_h),
                    &cell.layout.box_model.border,
                    cell.layout.box_model.content_insets,
                );
                let cell_border_box = cell_geometry
                    .border_box
                    .rounded(cell.layout.paint.border_radii);
                let cell_content_box = cell_geometry.content_box();
                let cell_group = {
                    let mut cell_ctx = PageRenderContext::new(
                        pdf_writer,
                        page_images,
                        custom_fonts,
                        prepared_custom_fonts,
                        page_shadings,
                        shading_counter,
                        page_ext_gstates,
                        bg_alpha_counter,
                        annotations,
                        page_paint_box,
                        page_height,
                    );
                    PaintGroupScope::begin(
                        content,
                        &cell.layout,
                        cell_geometry.for_fragment(Default::default()),
                        &mut cell_ctx,
                    )
                };

                let filtered = {
                    let mut filter_ctx = PageRenderContext::new(
                        pdf_writer,
                        page_images,
                        custom_fonts,
                        prepared_custom_fonts,
                        page_shadings,
                        shading_counter,
                        page_ext_gstates,
                        bg_alpha_counter,
                        annotations,
                        page_paint_box,
                        page_height,
                    );
                    paint_box_filter_output(content, &cell.layout, cell_geometry, &mut filter_ctx)
                };
                if filtered {
                    let mut cell_ctx = PageRenderContext::new(
                        pdf_writer,
                        page_images,
                        custom_fonts,
                        prepared_custom_fonts,
                        page_shadings,
                        shading_counter,
                        page_ext_gstates,
                        bg_alpha_counter,
                        annotations,
                        page_paint_box,
                        page_height,
                    );
                    cell_group.finish(content, &mut cell_ctx);
                    break 'paint_cell;
                }
                render_box_shadows(
                    content,
                    &cell.layout.paint.shadows,
                    cell_geometry.for_fragment(Default::default()),
                    cell.layout.paint.border_radii,
                    page_ext_gstates,
                    bg_alpha_counter,
                    pdf_writer,
                    page_images,
                );

                // Draw cell background
                if let Some(background) = cell.layout.paint.background.color {
                    let (r, g, b, a) = background.to_f32_rgba();
                    let needs_alpha = a < 1.0;
                    if needs_alpha {
                        let gs_name = format!("GScca{bg_alpha_counter}");
                        *bg_alpha_counter += 1;
                        page_ext_gstates.push((gs_name.clone(), a));
                        content.push_str(&format!("/{gs_name} gs\n"));
                    }
                    content.push_str(&format!("{r} {g} {b} rg\n"));
                    content.push_str(&cell_border_box.path_or_rect());
                    content.push_str("f\n");
                    if needs_alpha {
                        content.push_str("/GSDefault gs\n");
                    }
                }

                // Draw cell gradient backgrounds. A grid item is a block
                // container, so a `background: linear/radial/conic-gradient`
                // paints across the cell's border box just like any block
                // (css-backgrounds-3 §3), clipped to the painted box.
                {
                    let mut cell_ctx = PageRenderContext::new(
                        pdf_writer,
                        page_images,
                        custom_fonts,
                        prepared_custom_fonts,
                        page_shadings,
                        shading_counter,
                        page_ext_gstates,
                        bg_alpha_counter,
                        annotations,
                        page_paint_box,
                        page_height,
                    );
                    paint_cell_gradient_backgrounds(
                        content,
                        &cell.layout,
                        cell_geometry,
                        &mut cell_ctx,
                    );
                    render_box_shadows_inset(
                        content,
                        &cell.layout.paint.shadows,
                        cell_geometry.for_fragment(Default::default()),
                        cell.layout.paint.border_radii,
                        cell_ctx.page_ext_gstates,
                        cell_ctx.bg_alpha_counter,
                        cell_ctx.text.pdf_writer,
                        cell_ctx.text.page_images,
                    );
                }

                // Draw the cell border through the shared rounded ring.
                paint_box_decoration(
                    content,
                    cell_geometry.for_fragment(Default::default()),
                    &cell.layout.box_model.border,
                    cell.layout.paint.border_radii,
                    cell.layout.paint.border_image.as_ref(),
                    BorderPaintResources {
                        shadings: page_shadings,
                        shading_counter,
                        page_ext_gstates,
                        alpha_counter: bg_alpha_counter,
                        pdf_writer,
                        page_images,
                    },
                );

                // Render cell text
                let cell_inner_w = cell_content_box.width;
                let mut baseline_cursor = TextBaselineCursor::new(cell_content_box.top());
                for line in &cell.layout.content.lines {
                    let metrics = line_box_metrics(line, custom_fonts);
                    let text_y = baseline_cursor.next_horizontal(metrics);
                    let text_content: String =
                        line.runs.iter().map(|run| run.text.as_str()).collect();
                    if text_content.is_empty() {
                        continue;
                    }
                    let merged = merge_runs(&line.runs);
                    let line_width: f32 = merged
                        .iter()
                        .map(|run| estimate_run_width_with_fonts(run, custom_fonts))
                        .sum();
                    let text_x = match cell.layout.alignment.inline {
                        TextAlign::Right => {
                            cell_content_box.left + (cell_inner_w - line_width).max(0.0)
                        }
                        TextAlign::Center => {
                            cell_content_box.left + ((cell_inner_w - line_width) / 2.0).max(0.0)
                        }
                        _ => cell_content_box.left,
                    };
                    let mut lx = text_x;
                    for (run_index, run) in merged.iter().enumerate() {
                        if run.text.is_empty() {
                            continue;
                        }
                        let run_width = estimate_run_width_with_fonts(run, custom_fonts);
                        let previous = merged[..run_index].iter().rev().find(|previous| {
                            previous.inline_box.is_none() && !previous.text.is_empty()
                        });
                        let decoration =
                            HorizontalRunDecorations::new(run, lx, run_width, text_y, custom_fonts)
                                .continuing_after(previous);
                        let rw = decoration.paint_text(
                            content,
                            crate::layout::text::line_primary_font_size(&merged),
                            prepared_custom_fonts,
                            0.0,
                            pdf_writer,
                            page_images,
                        );
                        lx += rw;
                    }
                }

                // Render the cell's nested block children (e.g. a grid
                // item's inner <div>), clipped to the cell's padding box when
                // the item has `overflow: hidden`/`clip`/`scroll`/`auto`.
                if !cell.layout.content.children.is_empty() {
                    let text_h: f32 = cell
                        .layout
                        .content
                        .lines
                        .iter()
                        .map(|line| line.height)
                        .sum();
                    let nested_clip = cell.placement.clips;
                    let clip_command = nested_clip.then(|| {
                        cell_geometry
                            .padding_box()
                            .rounded(CornerRadii::ZERO)
                            .clip_command()
                    });
                    if let Some(command) = &clip_command {
                        content.push_str(command);
                    }
                    let nested_x = cell_content_box.left;
                    let nested_w = cell_content_box.width;
                    let nested_y = cell_content_box.top() - text_h;
                    let mut nested_abs: HashMap<usize, PdfPoint> = HashMap::new();
                    let mut child_ctx = PageRenderContext::new(
                        pdf_writer,
                        page_images,
                        custom_fonts,
                        prepared_custom_fonts,
                        page_shadings,
                        shading_counter,
                        page_ext_gstates,
                        bg_alpha_counter,
                        annotations,
                        page_paint_box,
                        page_height,
                    );
                    child_ctx.stacking = self.stacking.fork();
                    if let Some(command) = &clip_command {
                        child_ctx.stacking.push_clip(command.clone());
                    }
                    render_container_children(
                        content,
                        &cell.layout.content.children,
                        ContainerFrame::new(
                            PdfPoint::new(nested_x, nested_y),
                            nested_w,
                            PdfPoint::new(
                                nested_x - cell.layout.box_model.content_insets.left,
                                nested_y + cell.layout.box_model.content_insets.top,
                            ),
                        ),
                        &mut nested_abs,
                        &mut child_ctx,
                        ContainerRenderOptions {
                            stacking_scope: if cell.layout.establishes_stacking_context() {
                                StackingScope::Local
                            } else {
                                StackingScope::Ancestor
                            },
                            ..Default::default()
                        },
                    );
                    if nested_clip {
                        child_ctx.stacking.pop_clip();
                        content.push_str("Q\n");
                    }
                    self.stacking.restore(child_ctx.stacking.take_since(0));
                }

                let mut cell_ctx = PageRenderContext::new(
                    pdf_writer,
                    page_images,
                    custom_fonts,
                    prepared_custom_fonts,
                    page_shadings,
                    shading_counter,
                    page_ext_gstates,
                    bg_alpha_counter,
                    annotations,
                    page_paint_box,
                    page_height,
                );
                cell_group.finish(content, &mut cell_ctx);
            }
            let descendants = self.stacking.take_since(marker);
            self.stacking.commit(
                stacking_scope,
                content,
                &mut stacking_plan,
                cell.layout.stacking_level(),
                cell_content,
                descendants,
            );
        }
        if stacking_scope.is_local() {
            self.stacking.paint_plan(stacking_plan, content);
        }
        cursor_y -= row_height + margin_bottom;
        self.cursor_y = cursor_y;
        self.previous_margin_bottom = 0.0;
    }
}

/// Render table and grid rows at a resolved flow origin.
///
/// Page roots, ordinary containers, table cells, and grid/flex descendants all
/// enter this same painter; only the flow origin differs.
pub(super) fn render_rows(
    content: &mut String,
    elements: &[&dyn LayoutElement],
    origin_x: f32,
    flow: NestedRowsFlow,
    ctx: &mut PageRenderContext<'_>,
) -> FlowPosition {
    let row_heights = elements
        .iter()
        .map(|element| table_row_height(*element))
        .collect();
    let mut renderer = NestedRowsRenderer {
        content,
        origin_x,
        cursor_y: flow.position.cursor_y,
        page_ext_gstates: ctx.page_ext_gstates,
        bg_alpha_counter: ctx.bg_alpha_counter,
        custom_fonts: ctx.text.custom_fonts,
        prepared_custom_fonts: ctx.text.prepared_custom_fonts,
        page_shadings: ctx.shadings,
        shading_counter: ctx.shading_counter,
        pdf_writer: ctx.text.pdf_writer,
        page_images: ctx.text.page_images,
        annotations: ctx.text.annotations,
        stacking: &mut ctx.stacking,
        page_paint_box: ctx.paint_box,
        page_height: ctx.text.page_height,
        previous_margin_bottom: flow.position.previous_margin_bottom,
        first_margin: flow.first_margin,
        row_heights,
        element_index: 0,
        handled: false,
    };
    for (element_index, &element) in elements.iter().enumerate() {
        renderer.element_index = element_index;
        renderer.handled = false;
        element.accept(&mut renderer);
        if !renderer.handled {
            renderer.cursor_y -= crate::layout::paginate::estimate_element_height(element);
            renderer.previous_margin_bottom = 0.0;
        }
    }
    FlowPosition::new(
        renderer.cursor_y,
        renderer.cursor_y,
        renderer.previous_margin_bottom,
    )
}
