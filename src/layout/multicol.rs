//! CSS multi-column layout (`column-count` / `column-width` / `columns`).
//!
//! Implements a column-major *balanced* flow: the container's block-level
//! children are laid out top-to-bottom filling column 1, then column 2, etc.,
//! with the content distributed so the columns end up roughly equal height.
//! Each column is emitted as an absolutely positioned [`Container`]
//! at its computed x-offset inside the multicol element's padding box, so the
//! columns sit side-by-side without participating in the parent's vertical flow.
//!
//! Supported:
//! - `column-count`, `column-width`, and the `columns` shorthand (used-column
//!   count derived per the CSS spec when both/either are present).
//! - `column-gap` (with a `normal` default of 1em).
//! - `column-rule` painted as a vertical stroke centered in each gap; `solid`
//!   paints as a filled bar, `dashed`/`dotted`/`double` as the matching styled
//!   line. The rule spans the full content box of a definite-height container.
//! - `column-span: all` — a child spans every column as a full-width band that
//!   breaks the balanced flow (content before/after balances independently).
//! - `column-fill`: `balance` (default, equal column heights) and `auto`
//!   (sequential fill to the container height, last column short). Under
//!   `auto` with a definite height, a simple block box that crosses a column
//!   boundary is *fragmented* (css-break-3 + css-multicol-1): the part that
//!   fits stays at the bottom of column N and the remainder continues at the
//!   top of N+1, with `box-decoration-break: slice` borders at the cut.
//! - Margins adjoining a column fragmentation break are truncated (css-break-3
//!   §4.2): the trailing `margin-bottom` of the last box in every column except
//!   the last (in document order) is dropped from the column's used height.
//! - Balanced columns are filled sequentially after resolving their shortest
//!   legal block size. Class-A descendant boundaries are preferred over line
//!   splits, and `break-inside: avoid` keeps a box whole whenever it can fit.

mod distribution;
mod flow;
mod geometry;

use distribution::{
    balance_columns, balanced_buckets_height, fill_columns, max_vertical_rl_item_height,
};
use flow::{
    BoxFragmentPlacement, ColumnFragmentation, balance_fragmented_columns,
    build_balanced_paginated_column_rows, build_paginated_column_rows,
    build_paginated_column_rows_with_spans, empty_flow_anchor, fragment_columns,
    item_is_splittable, make_band_container, make_column_container, make_fragment_box,
    make_rule_container,
};
use geometry::{column_has_content, column_rule_x, column_x, resolve_columns};

use crate::layout::elements::{
    BlockSize, BoxFragmentation, BoxModel, BoxPaint, Container, InlineOffset, IntoLayoutNode,
    LayoutElement, LayoutNode, LayoutSize, LayoutVisitor, MulticolContainer, OverflowBehavior,
    PageBreak, Positioning, TextBlock,
};
use crate::layout::flow_metrics::BlockMargins;
use crate::parser::css::{
    AncestorInfo, CssValue, PseudoElement, SelectorContext, parse_inline_style,
    selector_matches_with_context, specificity,
};
use crate::parser::dom::{DomNode, ElementNode};
use crate::style::computed::{ComputedStyle, compute_pseudo_element_style_with_font_metrics};
use crate::types::{Point, Size};

use super::context::{LayoutContext, LayoutEnv};
use super::engine::{
    ElementSiblingContext, LayoutBorder, LayoutTreeContext, PageBreakSide, flatten_element,
};
use super::helpers::build_pseudo_block;
use super::paginate::estimate_element_height;
use super::roundoff::is_positive_with_roundoff;

/// A single laid-out top-level child of the multicol element.
struct MultiColItem {
    /// The flattened layout elements for this child (usually one Container or
    /// TextBlock, but text/anonymous content may produce several).
    elements: Vec<LayoutNode>,
    /// Outer (margin-box) height used for balancing.
    height: f32,
    /// Principal outer extent available to fragmentation. Definite boxes keep
    /// their hard used extent; content-dependent boxes resolve against their
    /// natural descendant extent.
    fragmentation_height: f32,
    /// Outer (margin-box) width used for vertical writing-mode block flow.
    width: f32,
    /// The item's trailing `margin-bottom` (the last in-flow element's bottom
    /// margin). Truncated at a column-fragment break per css-break-3 §4.2.
    margin_bottom: f32,
    /// `column-span: all` — render as a full-width band, not inside a column.
    span_all: bool,
    /// `break-before: column` — force this item to the next column.
    break_before_column: bool,
    /// `break-after: column` — force following content to the next column.
    break_after_column: bool,
    /// `break-before: avoid-column` / `avoid` — avoid a column break before it.
    break_before_avoid_column: bool,
    /// `break-after: avoid-column` / `avoid` — avoid a column break after it.
    break_after_avoid_column: bool,
    /// `break-inside: avoid-column` / `avoid` — keep the item in one column if
    /// it fits there.
    break_inside_avoid_column: bool,
}

impl MultiColItem {
    fn from_layout(elements: Vec<LayoutNode>, info: ChildMulticolInfo) -> Self {
        let height = info.definite_outer_height.unwrap_or_else(|| {
            elements
                .iter()
                .map(|element| multicol_item_element_height(element.as_ref()))
                .sum()
        });
        let width = info.definite_outer_width.unwrap_or_else(|| {
            elements
                .iter()
                .map(|element| multicol_item_element_width(element.as_ref()))
                .fold(0.0, f32::max)
        });
        let margin_bottom = element_trailing_margin_bottom(&elements);
        let fragmentation_height = elements
            .iter()
            .map(|element| {
                element
                    .fragmentable_outer_block_extent()
                    .unwrap_or_else(|| multicol_item_element_height(element.as_ref()))
            })
            .sum::<f32>()
            .max(height);
        Self {
            elements,
            height,
            fragmentation_height,
            width,
            margin_bottom,
            span_all: info.span_all,
            break_before_column: info.breaks.before_force,
            break_after_column: info.breaks.after_force,
            break_before_avoid_column: info.breaks.before_avoid,
            break_after_avoid_column: info.breaks.after_avoid,
            break_inside_avoid_column: info.breaks.inside_avoid,
        }
    }
}

struct ChildMulticolInfo {
    span_all: bool,
    definite_outer_height: Option<f32>,
    definite_outer_width: Option<f32>,
    breaks: ColumnBreakInfo,
}

impl ChildMulticolInfo {
    fn from_style(style: &ComputedStyle, breaks: ColumnBreakInfo) -> Self {
        let definite_outer_height = style.height.map(|height| {
            let border_padding = style.border.vertical_width() + style.padding.vertical();
            let border_box = if style.box_sizing == crate::style::computed::BoxSizing::BorderBox {
                height
            } else {
                height + border_padding
            };
            style.margin.top + border_box + style.margin.bottom
        });
        let definite_outer_width = style.width.map(|width| {
            let border_padding = style.border.horizontal_width() + style.padding.horizontal();
            let border_box = if style.box_sizing == crate::style::computed::BoxSizing::BorderBox {
                width
            } else {
                width + border_padding
            };
            style.margin.left + border_box + style.margin.right
        });
        Self {
            span_all: style.column_span_all,
            definite_outer_height,
            definite_outer_width,
            breaks,
        }
    }
}

/// Geometry of one principal multicol box fragment.
///
/// Keeping the semantic [`BlockSize`] beside the physical inline size prevents
/// measured auto/minimum heights from becoming fixed-height overflow boxes at
/// the pagination boundary.
#[derive(Clone, Copy)]
struct MulticolFragmentGeometry {
    size: LayoutSize,
    margins: BlockMargins,
    inline_offset: InlineOffset,
}

impl MulticolFragmentGeometry {
    const fn new(
        inline_size: f32,
        block_size: BlockSize,
        inline_offset: InlineOffset,
        margins: BlockMargins,
    ) -> Self {
        Self {
            size: LayoutSize::fixed_inline(inline_size, block_size),
            margins,
            inline_offset,
        }
    }
}

#[derive(Clone, Copy)]
struct ColumnRuleSpan {
    gap_after: usize,
    inline_offset: f32,
    block_offset: f32,
    block_size: f32,
}

#[derive(Clone, Copy, Default)]
struct ColumnBreakInfo {
    before_force: bool,
    after_force: bool,
    before_avoid: bool,
    after_avoid: bool,
    inside_avoid: bool,
}

/// The trailing `margin-bottom` of a laid-out item: the bottom margin of its
/// last in-flow (non-absolute) layout element, which is what adjoins a column
/// fragmentation break. Returns 0.0 for elements that carry no bottom margin.
fn element_trailing_margin_bottom(elements: &[LayoutNode]) -> f32 {
    for el in elements.iter().rev() {
        if el
            .positioning_owner()
            .is_some_and(|owner| owner.positioning().scheme.is_absolute())
        {
            continue;
        }
        if let Some(holder) = el.margin_holder() {
            return holder.margins().end;
        }
    }
    0.0
}

fn multicol_item_element_height(element: &dyn LayoutElement) -> f32 {
    #[derive(Default)]
    struct HeightVisitor(Option<f32>);

    impl LayoutVisitor for HeightVisitor {
        fn visit_text_block(&mut self, element: &TextBlock) {
            if !element.positioning.scheme.is_absolute()
                && let Some(height) = element.box_model.size.height.used()
            {
                self.0 = Some(
                    element.box_model.margins.total()
                        + height
                        + element.box_model.border.vertical_width(),
                );
            }
        }

        fn visit_container(&mut self, element: &Container) {
            if !element.positioning.scheme.is_absolute()
                && let Some(height) = element.box_model.size.height.used()
            {
                self.0 = Some(element.box_model.margins.total() + height);
            }
        }
    }

    let mut visitor = HeightVisitor::default();
    element.accept(&mut visitor);
    visitor
        .0
        .unwrap_or_else(|| estimate_element_height(element))
}

fn multicol_item_element_width(element: &dyn LayoutElement) -> f32 {
    #[derive(Default)]
    struct WidthVisitor(f32);

    impl LayoutVisitor for WidthVisitor {
        fn visit_text_block(&mut self, element: &TextBlock) {
            if !element.positioning.scheme.is_absolute()
                && let Some(width) = element.box_model.size.width.fixed_value()
            {
                self.0 = width + element.box_model.border.horizontal_width();
            }
        }

        fn visit_container(&mut self, element: &Container) {
            if !element.positioning.scheme.is_absolute() {
                self.0 = element
                    .box_model
                    .size
                    .width
                    .fixed_value()
                    .unwrap_or_default();
            }
        }
    }

    let mut visitor = WidthVisitor::default();
    element.accept(&mut visitor);
    visitor.0
}

/// Lay out a multi-column container, replacing the previous grid-emulation path.
pub(crate) fn layout_multicol_container(
    el: &ElementNode,
    style: &ComputedStyle,
    ctx: &LayoutContext,
    output: &mut Vec<LayoutNode>,
    ancestors: &[AncestorInfo],
    positioned_depth: usize,
    env: &mut LayoutEnv,
) {
    let available_width = ctx.available_width();
    let border_pad_w = style.border.horizontal_width() + style.padding.horizontal();

    // Content-box (inner) width: explicit `width` wins (resolving box-sizing),
    // else available width minus padding.
    let inner_width = match style.width {
        Some(w) => {
            if style.box_sizing == crate::style::computed::BoxSizing::BorderBox {
                (w - border_pad_w).max(0.0)
            } else {
                w
            }
        }
        None => (available_width - style.padding.horizontal()).max(0.0),
    };
    let border_box_w = inner_width + border_pad_w;

    let inline_offset = InlineOffset::resolve_block_start(style, available_width, border_box_w);

    // Column gap: `normal` resolves to 1em (the element's font-size, in pt).
    let gap = if style.column_gap_is_normal {
        style.font_size
    } else if let Some(frac) = style.column_gap_pct {
        inner_width * frac
    } else {
        style.column_gap
    };

    // Resolve the used number of columns and the per-column width, following the
    // CSS multicol "pseudo-algorithm" (simplified): with both column-count N and
    // column-width W, use up to N columns each at least W wide; with only one,
    // derive the other from the available inner width.
    let (num_cols, col_width) = resolve_columns(style, inner_width, gap);
    if num_cols < 1 {
        return;
    }

    // ---- Lay out each top-level child into its own buffer at column width ----
    let col_ctx = ctx.with_parent(col_width, None, style.font_size);
    let full_ctx = ctx.with_parent(inner_width, None, style.font_size);

    let mut child_ancestors: Vec<AncestorInfo> = ancestors.to_vec();
    child_ancestors.push(AncestorInfo {
        element: el,
        child_index: 0,
        sibling_count: 0,
        preceding_siblings: Vec::new(),
        following_siblings: Vec::new(),
        is_empty: false,
    });

    let element_count = el
        .children
        .iter()
        .filter(|n| matches!(n, DomNode::Element(_)))
        .count();

    let mut items: Vec<MultiColItem> = Vec::new();
    let container_selector_ctx = SelectorContext {
        ancestors: ancestors.to_vec(),
        ..Default::default()
    };
    let container_classes = el.class_list();
    let before_style = compute_pseudo_element_style_with_font_metrics(
        style,
        env.rules,
        el.tag_name(),
        &container_classes,
        el.id(),
        &el.attributes,
        &container_selector_ctx,
        PseudoElement::Before,
        env.font_metrics(),
    );
    if let Some(pseudo_style) = before_style.as_ref() {
        let pseudo = build_pseudo_block(
            pseudo_style,
            el,
            col_width,
            env.fonts,
            None,
            positioned_depth,
            env.counter_state,
            false,
        );
        items.push(MultiColItem::from_layout(
            vec![pseudo],
            ChildMulticolInfo::from_style(pseudo_style, ColumnBreakInfo::default()),
        ));
    }
    let mut element_index = 0usize;
    let mut preceding_siblings: Vec<(String, Vec<String>)> = Vec::new();
    for node in &el.children {
        let DomNode::Element(child_el) = node else {
            continue;
        };
        // Decide span-all by computing the child's style cheaply.
        let child_info = child_multicol_info(
            child_el,
            style,
            env,
            &child_ancestors,
            element_index,
            element_count,
            &preceding_siblings,
        );
        let item_ctx = if child_info.span_all {
            &full_ctx
        } else {
            &col_ctx
        };

        let mut buf: Vec<LayoutNode> = Vec::new();
        flatten_element(
            child_el,
            LayoutTreeContext::new(style, item_ctx, &child_ancestors)
                .with_positioned_ancestor_depth(positioned_depth)
                .for_element(
                    ElementSiblingContext::new(element_index, element_count)
                        .with_neighbors(&preceding_siblings, &[]),
                ),
            &mut buf,
            env,
        );
        items.push(MultiColItem::from_layout(buf, child_info));

        preceding_siblings.push((
            child_el.tag_name().to_string(),
            child_el
                .class_list()
                .iter()
                .map(|s| s.to_string())
                .collect(),
        ));
        element_index += 1;
    }
    let after_style = compute_pseudo_element_style_with_font_metrics(
        style,
        env.rules,
        el.tag_name(),
        &container_classes,
        el.id(),
        &el.attributes,
        &container_selector_ctx,
        PseudoElement::After,
        env.font_metrics(),
    );
    if let Some(pseudo_style) = after_style.as_ref() {
        let pseudo = build_pseudo_block(
            pseudo_style,
            el,
            col_width,
            env.fonts,
            None,
            positioned_depth,
            env.counter_state,
            false,
        );
        items.push(MultiColItem::from_layout(
            vec![pseudo],
            ChildMulticolInfo::from_style(pseudo_style, ColumnBreakInfo::default()),
        ));
    }

    // ---- Distribute items into columns (balanced, span-all as a band) -------
    // The output is a sequence of "segments": each segment is either a
    // full-width band (one span-all item) or a balanced multicol run. We track
    // the running vertical cursor so successive segments stack.
    let pad_left = style.border.left.used_width() + style.padding.left;
    let pad_top = style.border.top.used_width() + style.padding.top;
    // Column/band/rule containers are emitted as `Position::Absolute` children of
    // the multicol wrapper. The renderer places absolute children at the wrapper's
    // PADDING-box origin (CSS §10.1), so their offsets must be padding-box-relative:
    // strip the wrapper border from the border-box-relative cursors below. The
    // height accounting (`cursor_y`/`max_bottom`) stays in border-box coordinates.
    let bl = style.border.left.used_width();
    let bt = style.border.top.used_width();

    // Explicit border-box height (if any) resolved up front so the column rule
    // can span the full content box of a definite-height multicol container
    // (CSS Multicol §6: the rule is as tall as the column box, and in a
    // definite-height container the columns fill the content box).
    let explicit_border_box_h = style.height.map(|h| {
        if style.box_sizing == crate::style::computed::BoxSizing::BorderBox {
            h
        } else {
            h + style.border.vertical_width() + style.padding.vertical()
        }
    });

    if style.writing_mode.is_vertical() {
        let natural_block_size = explicit_border_box_h.unwrap_or_else(|| {
            pad_top
                + max_vertical_rl_item_height(&items)
                + style.padding.bottom
                + style.border.bottom.used_width()
        });
        let block_size = BlockSize::from_style(style, natural_block_size);
        let mut x_cursor = pad_left + inner_width;
        let mut column_children: Vec<LayoutNode> = Vec::new();
        for item in &items {
            if item.elements.len() != 1 {
                continue;
            }
            let item_w = if item.width > 0.0 {
                item.width
            } else {
                col_width
            };
            x_cursor -= item_w;
            column_children.push(make_fragment_box(
                &item.elements[0],
                BoxFragmentPlacement::whole(
                    Point::new(x_cursor - bl, pad_top - bt),
                    Size::new(item_w, item.height),
                ),
            ));
        }
        output.push(emit_multicol_wrapper(
            style,
            column_children,
            MulticolFragmentGeometry::new(
                border_box_w,
                block_size,
                inline_offset,
                BlockMargins::new(style.margin.top, style.margin.bottom),
            ),
        ));
        output.push(
            PageBreak {
                side: PageBreakSide::Any,
                page_name: None,
            }
            .boxed(),
        );
        output.push(empty_flow_anchor());
        return;
    }

    // ---- Page-aware column fragmentation (CSS Multicol §2 + Fragmentation 3) -
    // An auto-height, in-flow multicol whose column content is taller than the
    // page's content box is a *nested* fragmentation context: its columns fill to
    // the page height, then continue as a fresh row of column boxes on the next
    // page (CSS Multicol §2 — "a column box never splits across pages"). Without
    // this the whole multicol would be one atomic box that overflows off the page
    // bottom and is clipped (data loss). Only the genuinely-overflowing case takes
    // this path; everything that fits on one page (the entire existing corpus)
    // produces a single page-row and falls through to the byte-for-byte
    // single-page layout below.
    let page_content_h = ctx.available_height();
    let wrapper_v_extra = style.border.vertical_width() + style.padding.vertical();
    let col_fill_h = (page_content_h - wrapper_v_extra).max(0.0);
    let in_flow = style.position.is_in_flow() && style.float == crate::style::computed::Float::None;
    let all_splittable = !items.is_empty() && items.iter().all(item_is_splittable);
    let no_span = items.iter().all(|it| !it.span_all);
    if explicit_border_box_h.is_none()
        && in_flow
        && no_span
        && all_splittable
        && num_cols >= 1
        && is_positive_with_roundoff(col_fill_h)
    {
        let rows = if style.column_fill_auto {
            build_paginated_column_rows(
                &items, num_cols, col_width, gap, pad_left, bl, pad_top, bt, col_fill_h, style,
            )
        } else {
            build_balanced_paginated_column_rows(
                &items, num_cols, col_width, gap, pad_left, bl, pad_top, bt, col_fill_h, style,
            )
        };
        // Only fragment when the content genuinely spills past one page (more than
        // one page-row). A single row means it fits — fall through unchanged.
        if rows.len() > 1 {
            let last = rows.len() - 1;
            for (i, (row_children, row_max)) in rows.into_iter().enumerate() {
                let is_last = i == last;
                // Non-final page-rows fill the whole page (so the next row breaks
                // onto a fresh page); the final row shrink-wraps its content.
                let block_h = if is_last {
                    pad_top + row_max + style.padding.bottom + style.border.bottom.used_width()
                } else {
                    page_content_h
                };
                // Only the first fragment carries the top margin and only the last
                // carries the bottom margin (the box is a single flow element).
                let mt = if i == 0 { style.margin.top } else { 0.0 };
                let mb = if is_last { style.margin.bottom } else { 0.0 };
                output.push(emit_multicol_wrapper(
                    style,
                    row_children,
                    MulticolFragmentGeometry::new(
                        border_box_w,
                        BlockSize::fragment(block_h),
                        inline_offset,
                        BlockMargins::new(mt, mb),
                    ),
                ));
            }
            return;
        }
    }
    if explicit_border_box_h.is_none()
        && in_flow
        && !no_span
        && num_cols >= 1
        && is_positive_with_roundoff(col_fill_h)
    {
        let rows = build_paginated_column_rows_with_spans(
            &items,
            num_cols,
            col_width,
            gap,
            pad_left,
            bl,
            pad_top,
            bt,
            col_fill_h,
            inner_width,
            style,
        );
        if rows.len() > 1 {
            let last = rows.len() - 1;
            for (i, (row_children, row_max)) in rows.into_iter().enumerate() {
                let is_last = i == last;
                let block_h = if is_last {
                    pad_top + row_max + style.padding.bottom + style.border.bottom.used_width()
                } else {
                    page_content_h
                };
                let mt = if i == 0 { style.margin.top } else { 0.0 };
                let mb = if is_last { style.margin.bottom } else { 0.0 };
                output.push(emit_multicol_wrapper(
                    style,
                    row_children,
                    MulticolFragmentGeometry::new(
                        border_box_w,
                        BlockSize::fragment(block_h),
                        inline_offset,
                        BlockMargins::new(mt, mb),
                    ),
                ));
            }
            return;
        }
    }

    let mut column_children: Vec<LayoutNode> = Vec::new();
    // A pending rule span recorded per balanced run.
    // Emitted after the loop so a single-run, definite-height container can have
    // its rules stretched to the full content-box height.
    let mut rule_spans: Vec<ColumnRuleSpan> = Vec::new();
    let mut run_count = 0usize;
    let mut cursor_y = pad_top; // distance from border-box top to current band top
    let mut max_bottom = pad_top;

    let mut i = 0usize;
    while i < items.len() {
        if items[i].span_all {
            // Full-width band spanning all columns.
            let band_h = items[i].height;
            let band = make_band_container(
                std::mem::take(&mut items[i].elements),
                pad_left - bl,
                cursor_y - bt,
                inner_width,
                band_h,
            );
            column_children.push(band);
            cursor_y += band_h;
            max_bottom = max_bottom.max(cursor_y);
            i += 1;
            continue;
        }

        // Gather a run of consecutive non-span items.
        let run_start = i;
        while i < items.len() && !items[i].span_all {
            i += 1;
        }
        let run = &mut items[run_start..i];

        // `column-fill: auto` with a definite height fills each column to the
        // content-box height in turn, *fragmenting* a block that crosses a column
        // boundary (the part that fits stays in column N, the rest continues at
        // the top of N+1). Only used when every item in the run is a simple,
        // slice-able block box; otherwise (or for `balance`) fall back to the
        // atomic bucket distribution.
        let fill_h_auto = match (style.column_fill_auto, explicit_border_box_h) {
            (true, Some(height)) => {
                Some((height - style.padding.vertical() - style.border.vertical_width()).max(0.0))
            }
            _ => None,
        };
        let use_fragmentation =
            num_cols > 1 && run.iter().all(item_is_splittable) && !run.is_empty();

        let mut run_max_h = 0.0f32;
        let mut run_nonempty_cols = vec![false; num_cols];
        if use_fragmentation {
            // ---- Fragmenting fill path (auto or balance) -------------------
            let run_indices: Vec<usize> = (0..run.len()).collect();
            let fragmented = match fill_h_auto {
                Some(fill_h) => fragment_columns(
                    run,
                    &run_indices,
                    ColumnFragmentation::overflowing(num_cols, fill_h),
                ),
                None => balance_fragmented_columns(run, &run_indices, num_cols),
            };
            if fragmented.columns.len() > run_nonempty_cols.len() {
                run_nonempty_cols.resize(fragmented.columns.len(), false);
            }
            for (c, frags) in fragmented.columns.iter().enumerate() {
                if frags.is_empty() {
                    continue;
                }
                run_nonempty_cols[c] = true;
                run_max_h = run_max_h.max(fragmented.used_block_sizes[c]);
                let col_x = column_x(style, pad_left, col_width, gap, num_cols, c);
                let mut col_kids: Vec<LayoutNode> = Vec::new();
                for f in frags {
                    col_kids.push(make_fragment_box(
                        &run[f.item].elements[0],
                        f.placement(0.0, col_width),
                    ));
                }
                column_children.push(make_column_container(
                    col_kids,
                    c,
                    col_x - bl,
                    cursor_y - bt,
                    col_width,
                    fragmented.used_block_sizes[c],
                ));
            }
        } else {
            // ---- Atomic bucket path (balance, or non-slice-able auto) ------
            let heights: Vec<f32> = run.iter().map(|it| it.height).collect();
            let buckets = match fill_h_auto {
                Some(fill_h) => fill_columns(&heights, num_cols, fill_h),
                None => balance_columns(&heights, num_cols),
            };

            // The last non-empty column in document order is the natural end of
            // the run's content: its trailing margin is NOT adjoining a
            // fragmentation break and is kept. Every earlier column ends at a
            // column break, so the bottom margin of its last item is truncated
            // (css-break-3 §4.2). This shortens those columns' used height, which
            // is what the container's auto height (`run_max_h`) is measured
            // against — matching Chrome.
            let last_nonempty_col = buckets
                .iter()
                .rposition(|b| !b.is_empty())
                .unwrap_or(usize::MAX);

            for (c, bucket) in buckets.iter().enumerate() {
                if bucket.is_empty() {
                    continue;
                }
                run_nonempty_cols[c] = true;
                let col_x = column_x(style, pad_left, col_width, gap, num_cols, c);
                let mut col_kids: Vec<LayoutNode> = Vec::new();
                let mut col_height = 0.0f32;
                for &idx in bucket {
                    col_height += run[idx].height;
                    col_kids.append(&mut run[idx].elements);
                }
                // Used column height for sizing: drop the trailing margin at a break.
                let used_col_height = if c != last_nonempty_col {
                    let trailing_margin =
                        bucket.last().map_or(0.0, |&index| run[index].margin_bottom);
                    (col_height - trailing_margin).max(0.0)
                } else {
                    col_height
                };
                run_max_h = run_max_h.max(used_col_height);
                column_children.push(make_column_container(
                    col_kids,
                    c,
                    col_x - bl,
                    cursor_y - bt,
                    col_width,
                    col_height,
                ));
            }
        }

        // Record one rule span per gap for this run; the final height is decided
        // after the loop (full content box for a single definite-height run,
        // otherwise the run's column-content height).
        if style.column_rule.used_width() > 0.0 && num_cols > 1 {
            let rule_w = style.column_rule.used_width();
            for c in 0..run_nonempty_cols.len().saturating_sub(1) {
                let has_left = run_nonempty_cols.get(c).copied().unwrap_or(false);
                let has_right = run_nonempty_cols.get(c + 1).copied().unwrap_or(false);
                if !has_left || !has_right {
                    continue;
                }
                let gap_center = column_rule_x(style, pad_left, col_width, gap, num_cols, c);
                let rule_x = gap_center - rule_w / 2.0;
                rule_spans.push(ColumnRuleSpan {
                    gap_after: c,
                    inline_offset: rule_x,
                    block_offset: cursor_y,
                    block_size: run_max_h,
                });
            }
        }
        run_count += 1;

        cursor_y += run_max_h;
        max_bottom = max_bottom.max(cursor_y);
    }

    // Emit the recorded column rules. Per CSS Multicol §6 the rule is as tall as
    // the column box. In a definite-height container with a single balanced run
    // the columns fill the content box, so the rule spans from the content-box
    // top (`pad_top`) to its bottom (matching Chrome, which paints the rule the
    // full height of the box rather than only the filled content).
    if !rule_spans.is_empty() {
        let rule_w = style.column_rule.used_width();
        let rule_color = style.column_rule.color.resolve(style.color);
        // Content-box bottom (border-box coords) when the height is definite.
        let content_box_bottom = explicit_border_box_h
            .map(|bh| bh - style.padding.bottom - style.border.bottom.used_width());
        let mut rule_children: Vec<LayoutNode> = Vec::new();
        for span in rule_spans {
            let (rule_top, rule_h) = match content_box_bottom {
                // Single balanced run + definite height: span the whole content
                // box (top padding edge → bottom padding edge).
                Some(bottom) if run_count == 1 => {
                    (pad_top, (bottom - pad_top).max(span.block_size))
                }
                // Multiple runs (broken by span-all bands) or auto height: the
                // rule is as tall as this run's columns.
                _ => (span.block_offset, span.block_size),
            };
            rule_children.push(make_rule_container(
                span.gap_after,
                span.inline_offset - bl,
                rule_top - bt,
                rule_w,
                rule_h,
                rule_color,
                style.column_rule.style,
            ));
        }
        rule_children.extend(column_children);
        column_children = rule_children;
    }

    // ---- Outer container height --------------------------------------------
    // An explicit height wins; otherwise size to the tallest column run plus
    // the bottom padding.
    let content_box_h = max_bottom + style.padding.bottom + style.border.bottom.used_width();
    let block_size = BlockSize::from_style(style, content_box_h);

    // ---- Emit the wrapping container ---------------------------------------
    output.push(emit_multicol_wrapper(
        style,
        column_children,
        MulticolFragmentGeometry::new(
            border_box_w,
            block_size,
            inline_offset,
            BlockMargins::new(style.margin.top, style.margin.bottom),
        ),
    ));
}

/// Build one multicol wrapper [`Container`] holding `column_children`.
/// (the absolutely-positioned columns/bands/rules). Shared by the single-page
/// layout and by each page-row of the paginated path, which override the wrapper's
/// `block_height` (full page for a continuing fragment, shrink-wrapped for the
/// last) and its `margin_top`/`margin_bottom` (the box is one flow element, so only
/// the first fragment keeps the top margin and only the last keeps the bottom).
fn emit_multicol_wrapper(
    style: &ComputedStyle,
    column_children: Vec<LayoutNode>,
    geometry: MulticolFragmentGeometry,
) -> LayoutNode {
    MulticolContainer::new(Container {
        children: column_children,
        box_model: BoxModel {
            size: geometry.size,
            margins: geometry.margins,
            padding: style.padding,
            border: LayoutBorder::from_computed(&style.border, style.color),
        },
        paint: BoxPaint::from_style(style, geometry.size),
        flow: crate::layout::elements::BlockFlow {
            float: style.float,
            clear: style.clear,
        },
        positioning: Positioning::from_style(style).with_resolved_insets(crate::types::EdgeSizes {
            left: geometry.inline_offset.value(),
            ..Default::default()
        }),
        fragmentation: BoxFragmentation {
            decoration: crate::style::computed::BoxDecorationBreak::Slice,
            ..Default::default()
        },
        overflow: OverflowBehavior {
            combined: style.overflow,
            x: style.overflow_x,
            y: style.overflow_y,
        },
    })
    .boxed()
}

/// Compute multicol-relevant child metadata by resolving its style.
#[allow(clippy::too_many_arguments)]
fn child_multicol_info(
    child_el: &ElementNode,
    parent_style: &ComputedStyle,
    env: &LayoutEnv,
    child_ancestors: &[AncestorInfo],
    child_index: usize,
    sibling_count: usize,
    preceding_siblings: &[(String, Vec<String>)],
) -> ChildMulticolInfo {
    use crate::style::computed::compute_style_with_context;
    let classes = child_el.class_list();
    let selector_ctx = SelectorContext {
        ancestors: child_ancestors.to_vec(),
        child_index,
        sibling_count,
        preceding_siblings: preceding_siblings.to_vec(),
        following_siblings: Vec::new(),
        is_empty: false,
    };
    let cs = compute_style_with_context(
        child_el.tag,
        child_el.style_attr(),
        parent_style,
        env.rules,
        child_el.tag_name(),
        &classes,
        child_el.id(),
        &child_el.attributes,
        &selector_ctx,
    );
    let breaks = resolve_child_column_breaks(
        child_el,
        env.rules,
        child_el.tag_name(),
        &classes,
        child_el.id(),
        &child_el.attributes,
        &selector_ctx,
    );
    ChildMulticolInfo::from_style(&cs, breaks)
}

fn resolve_child_column_breaks(
    child_el: &ElementNode,
    rules: &[crate::parser::css::CssRule],
    tag_name: &str,
    classes: &[&str],
    id: Option<&str>,
    attributes: &std::collections::HashMap<String, String>,
    selector_ctx: &SelectorContext,
) -> ColumnBreakInfo {
    let mut matched: Vec<(u32, &crate::parser::css::CssRule)> = Vec::new();
    for rule in rules {
        if rule.pseudo_element.is_some() {
            continue;
        }
        if selector_matches_with_context(
            &rule.selector,
            tag_name,
            classes,
            id,
            attributes,
            selector_ctx,
        ) {
            matched.push((specificity(&rule.selector), rule));
        }
    }
    matched.sort_by_key(|(spec, _)| *spec);

    let inline_map = child_el.style_attr().map(parse_inline_style);
    let mut breaks = ColumnBreakInfo::default();
    for (_, rule) in &matched {
        apply_column_break_declarations(&mut breaks, &rule.declarations, false);
    }
    if let Some(inline) = &inline_map {
        apply_column_break_declarations(&mut breaks, inline, false);
    }
    for (_, rule) in &matched {
        apply_column_break_declarations(&mut breaks, &rule.declarations, true);
    }
    if let Some(inline) = &inline_map {
        apply_column_break_declarations(&mut breaks, inline, true);
    }
    breaks
}

fn apply_column_break_declarations(
    breaks: &mut ColumnBreakInfo,
    declarations: &crate::parser::css::StyleMap,
    important: bool,
) {
    if declarations.is_important("break-before") == important {
        if let Some(CssValue::Keyword(value)) = declarations.get("break-before") {
            apply_before_column_break_value(breaks, value);
        }
    }
    if declarations.is_important("break-after") == important {
        if let Some(CssValue::Keyword(value)) = declarations.get("break-after") {
            apply_after_column_break_value(breaks, value);
        }
    }
    if declarations.is_important("break-inside") == important {
        if let Some(CssValue::Keyword(value)) = declarations.get("break-inside") {
            breaks.inside_avoid = is_column_avoid_break(value);
        }
    }
    if declarations.is_important("page-break-inside") == important {
        if let Some(CssValue::Keyword(value)) = declarations.get("page-break-inside") {
            breaks.inside_avoid = value == "avoid";
        }
    }
}

fn apply_before_column_break_value(breaks: &mut ColumnBreakInfo, value: &str) {
    breaks.before_force = value == "column";
    breaks.before_avoid = is_column_avoid_break(value);
}

fn apply_after_column_break_value(breaks: &mut ColumnBreakInfo, value: &str) {
    breaks.after_force = value == "column";
    breaks.after_avoid = is_column_avoid_break(value);
}

fn is_column_avoid_break(value: &str) -> bool {
    matches!(value, "avoid" | "avoid-column")
}

#[cfg(test)]
mod tests;
