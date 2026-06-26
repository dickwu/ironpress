use crate::parser::css::{AncestorInfo, CssRule, CssValue, SelectorContext};
use crate::parser::dom::{DomNode, ElementNode, HtmlTag};
use crate::parser::ttf::TtfFont;
use crate::style::computed::{
    BorderCollapse, ComputedStyle, Display, FontStyle, FontWeight, TableLayout, TextAlign,
    VerticalAlign, WhiteSpace, compute_style_with_context,
};
use std::collections::HashMap;

use super::context::{LayoutContext, LayoutEnv, ParentBox, Viewport};
use super::engine::{
    CounterState, LayoutBorder, LayoutElement, TextLine, TextRun, collects_as_inline_text,
    flatten_element, has_background_paint, recurses_as_layout_child,
};
use super::paginate::{estimate_element_height, table_row_content_width};
use super::text::{
    TextWrapOptions, collapse_whitespace, estimate_word_width, resolve_style_font_family,
    resolved_line_height_factor, wrap_text_runs,
};

/// A table cell ready for rendering.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TableCell {
    pub lines: Vec<TextLine>,
    pub nested_rows: Vec<LayoutElement>,
    pub bold: bool,
    pub background_color: Option<(f32, f32, f32, f32)>,
    pub padding_top: f32,
    pub padding_right: f32,
    pub padding_bottom: f32,
    pub padding_left: f32,
    /// Number of columns this cell spans (default 1).
    pub colspan: usize,
    /// Number of rows this cell spans (default 1).
    pub rowspan: usize,
    /// Per-side border specification.
    pub border: LayoutBorder,
    /// Text alignment within the cell.
    pub text_align: TextAlign,
    /// Vertical alignment within the row box.
    pub vertical_align: VerticalAlign,
    /// Resolved CSS `height`/`min-height` floor for this cell's box, in points.
    /// Includes any height inherited from the owning `<tr>` and any surplus
    /// distributed from a fixed-height `<table>`. `0.0` means content-driven.
    pub min_height: f32,
}

pub(crate) fn table_cell_content_height(cell: &TableCell) -> f32 {
    let text_h: f32 = cell.lines.iter().map(|l| l.height).sum();
    let nested_h: f32 = cell.nested_rows.iter().map(estimate_element_height).sum();
    cell.padding_top + text_h + nested_h + cell.padding_bottom
}

/// The cell's box height: the larger of its natural content height and any
/// CSS-specified height floor. Row height aggregates this across cells, while
/// `table_cell_content_height` stays content-only so `vertical-align` can offset
/// content within the (possibly taller) box.
pub(crate) fn table_cell_box_height(cell: &TableCell) -> f32 {
    table_cell_content_height(cell).max(cell.min_height)
}

/// Resolve a `<table>`/`<tr>`/`<td>` element's CSS `height` (and `min-height`,
/// `max-height`) to a concrete points value used as a row/cell height floor.
///
/// Percentage heights resolve against `containing_height` (the table's own
/// resolved height) when one is known, mirroring the block-flow resolver in
/// `helpers.rs`. Returns `0.0` when no height is specified, which is the
/// content-driven default for ordinary tables.
fn resolve_specified_height(style: &ComputedStyle, containing_height: Option<f32>) -> f32 {
    let mut h = style.height;
    if let Some(cb) = containing_height
        && let Some(percent) = style.percentage_sizing.height
    {
        h = Some(cb * percent / 100.0);
    }
    if let Some(min_h) = style.min_height {
        h = Some(h.map_or(min_h, |v| v.max(min_h)));
    }
    if let Some(cb) = containing_height
        && let Some(percent) = style.percentage_sizing.min_height
    {
        let min_h = cb * percent / 100.0;
        h = Some(h.map_or(min_h, |v| v.max(min_h)));
    }
    if let Some(max_h) = style.max_height {
        h = h.map(|v| v.min(max_h));
    }
    if let Some(cb) = containing_height
        && let Some(percent) = style.percentage_sizing.max_height
    {
        let max_h = cb * percent / 100.0;
        h = h.map(|v| v.min(max_h));
    }
    h.unwrap_or(0.0).max(0.0)
}

/// Parse a width for a `<col>` / `<colgroup>` element.
///
/// Valid inline `width` declarations take precedence. Malformed inline
/// declarations are ignored so the `width` attribute can still act as a
/// fallback. `width: auto` explicitly clears the width.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TableTrackWidth {
    Points(f32),
    Percent(f32),
}

fn resolve_table_percentage_width(table_width: f32, percent: f32) -> f32 {
    // Percentage `<col>` and `<colgroup>` widths resolve against the table
    // width itself. Border-spacing is applied later when laying out the cells
    // so it must not shrink the percentage basis.
    table_width * percent
}

impl TableTrackWidth {
    fn resolve(self, table_width: f32) -> f32 {
        match self {
            Self::Points(width) => width,
            Self::Percent(percent) => resolve_table_percentage_width(table_width, percent),
        }
    }
}

fn compute_column_style(
    el: &ElementNode,
    parent_style: &ComputedStyle,
    rules: &[CssRule],
    ancestors: &[AncestorInfo],
    child_index: usize,
    sibling_count: usize,
) -> ComputedStyle {
    let classes = el.class_list();
    let selector_ctx = SelectorContext {
        ancestors: ancestors.to_vec(),
        child_index,
        sibling_count,
        preceding_siblings: Vec::new(),
    };
    compute_style_with_context(
        el.tag,
        el.style_attr(),
        parent_style,
        rules,
        el.tag_name(),
        &classes,
        el.id(),
        &el.attributes,
        &selector_ctx,
    )
}

fn parse_element_width(el: &ElementNode) -> Option<TableTrackWidth> {
    if let Some(inline_width) = parse_element_inline_width(el) {
        return inline_width;
    }
    el.attributes
        .get("width")
        .and_then(|val| parse_table_track_width(val))
}

fn parse_element_inline_width(el: &ElementNode) -> Option<Option<TableTrackWidth>> {
    if let Some(style_str) = el.style_attr() {
        let mut last_inline_width = None;
        for decl in style_str.split(';').map(str::trim) {
            if let Some((prop, val)) = decl.split_once(':') {
                if prop.trim().eq_ignore_ascii_case("width") {
                    let val = strip_important(val).trim();
                    last_inline_width = parse_inline_width_value(val).or(last_inline_width);
                }
            }
        }
        return last_inline_width;
    }
    None
}

fn parse_col_width(
    col_el: &ElementNode,
    parent_style: &ComputedStyle,
    rules: &[CssRule],
    ancestors: &[AncestorInfo],
    child_index: usize,
    sibling_count: usize,
) -> Option<TableTrackWidth> {
    let computed_style = compute_column_style(
        col_el,
        parent_style,
        rules,
        ancestors,
        child_index,
        sibling_count,
    );
    if let Some(inline_width) = parse_column_inline_width(col_el, computed_style.width) {
        return inline_width;
    }
    computed_style
        .width
        .map(TableTrackWidth::Points)
        .or_else(|| {
            col_el
                .attributes
                .get("width")
                .and_then(|val| parse_table_track_width(val))
        })
}

fn parse_column_inline_width(
    el: &ElementNode,
    computed_width: Option<f32>,
) -> Option<Option<TableTrackWidth>> {
    let style_str = el.style_attr()?;
    let inline = crate::parser::css::parse_inline_style(style_str);
    match inline.get("width") {
        Some(CssValue::Keyword(k)) if k.eq_ignore_ascii_case("auto") => Some(None),
        Some(_) => computed_width.map(|width| Some(TableTrackWidth::Points(width))),
        None => None,
    }
}

fn parse_percent_width(val: &str) -> Option<f32> {
    let pct_str = val.trim().strip_suffix('%')?;
    pct_str.trim().parse::<f32>().ok().map(|pct| pct / 100.0)
}

fn parse_table_track_width(val: &str) -> Option<TableTrackWidth> {
    if let Some(percent) = parse_percent_width(val) {
        return Some(TableTrackWidth::Percent(percent));
    }
    match crate::parser::css::parse_length(val) {
        Some(CssValue::Length(width)) => Some(TableTrackWidth::Points(width)),
        _ => None,
    }
}

fn parse_inline_width_value(val: &str) -> Option<Option<TableTrackWidth>> {
    if val.eq_ignore_ascii_case("auto") {
        return Some(None);
    }
    parse_table_track_width(val).map(Some).or_else(|| {
        crate::parser::css::parse_length(val)
            .is_some()
            .then_some(None)
    })
}

fn strip_important(val: &str) -> &str {
    val.strip_suffix("!important")
        .map(str::trim_end)
        .unwrap_or(val)
}

fn parse_col_span(el: &ElementNode) -> usize {
    el.attributes
        .get("span")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1)
        .clamp(1, 1000)
}

fn assign_explicit_col_widths(
    explicit_col_widths: &mut [Option<TableTrackWidth>],
    col_idx: &mut usize,
    span: usize,
    width: Option<TableTrackWidth>,
) {
    for slot in explicit_col_widths.iter_mut().skip(*col_idx).take(span) {
        *slot = width;
    }
    *col_idx = col_idx.saturating_add(span);
}

fn resolve_table_inner_width(style: &ComputedStyle, available_width: f32) -> f32 {
    let containing_width = (available_width - style.margin.left - style.margin.right).max(0.0);
    style
        .width
        .or_else(|| {
            style
                .percentage_sizing
                .width
                .map(|percent| containing_width * percent / 100.0)
        })
        .map_or(containing_width, |width| {
            width.min(containing_width).max(0.0)
        })
}

fn uses_fixed_table_layout(style: &ComputedStyle) -> bool {
    style.table_layout == TableLayout::Fixed
        && (style.width.is_some() || style.percentage_sizing.width.is_some())
}

fn resolve_cell_track_width(
    cell_el: &ElementNode,
    cell_style: &ComputedStyle,
    table_width: f32,
) -> Option<f32> {
    parse_element_width(cell_el)
        .map(|width| width.resolve(table_width))
        .or(cell_style.width)
}

fn apply_cell_width_to_columns(
    col_widths: &mut [Option<f32>],
    start: usize,
    colspan: usize,
    width: f32,
) {
    if colspan == 0 || start >= col_widths.len() {
        return;
    }
    let per_column_width = width / colspan as f32;
    for slot in col_widths.iter_mut().skip(start).take(colspan) {
        *slot = Some(slot.map_or(per_column_width, |existing| existing.max(per_column_width)));
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_fixed_table_columns(
    table_style: &ComputedStyle,
    table_width: f32,
    rows: &[&ElementNode],
    row_section_indices: &[usize],
    row_section_sizes: &[usize],
    row_section_elements: &[Option<&ElementNode>],
    row_section_child_indices: &[usize],
    row_section_sibling_counts: &[usize],
    table_ancestors: &[AncestorInfo],
    explicit_col_widths: &[Option<TableTrackWidth>],
    num_cols: usize,
    rules: &[CssRule],
) -> Vec<f32> {
    let mut col_widths: Vec<Option<f32>> = explicit_col_widths
        .iter()
        .map(|width| width.map(|specified| specified.resolve(table_width)))
        .collect();

    if let Some(first_row) = rows.first() {
        let mut row_ancestors = table_ancestors.to_vec();
        if let Some(section_el) = row_section_elements.first().copied().flatten() {
            row_ancestors.push(AncestorInfo {
                element: section_el,
                child_index: row_section_child_indices.first().copied().unwrap_or(0),
                sibling_count: row_section_sibling_counts.first().copied().unwrap_or(0),
                preceding_siblings: Vec::new(),
            });
        }
        let row_selector_ctx = SelectorContext {
            ancestors: row_ancestors,
            child_index: row_section_indices.first().copied().unwrap_or(0),
            sibling_count: row_section_sizes.first().copied().unwrap_or(1),
            preceding_siblings: Vec::new(),
        };
        let row_classes = first_row.class_list();
        let mut row_style = compute_style_with_context(
            first_row.tag,
            first_row.style_attr(),
            table_style,
            rules,
            first_row.tag_name(),
            &row_classes,
            first_row.id(),
            &first_row.attributes,
            &row_selector_ctx,
        );
        row_style.width = Some(table_width);

        let mut col_pos = 0usize;
        for child in &first_row.children {
            let DomNode::Element(cell_el) = child else {
                continue;
            };
            if cell_el.tag != HtmlTag::Td && cell_el.tag != HtmlTag::Th {
                continue;
            }
            let colspan = cell_el
                .attributes
                .get("colspan")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(1)
                .max(1);

            let cell_classes = cell_el.class_list();
            let mut cell_ancestors = row_selector_ctx.ancestors.clone();
            cell_ancestors.push(AncestorInfo {
                element: first_row,
                child_index: row_selector_ctx.child_index,
                sibling_count: row_selector_ctx.sibling_count,
                preceding_siblings: Vec::new(),
            });
            let cell_selector_ctx = SelectorContext {
                ancestors: cell_ancestors,
                child_index: col_pos,
                sibling_count: num_cols,
                preceding_siblings: Vec::new(),
            };
            let cell_style = compute_style_with_context(
                cell_el.tag,
                cell_el.style_attr(),
                &row_style,
                rules,
                cell_el.tag_name(),
                &cell_classes,
                cell_el.id(),
                &cell_el.attributes,
                &cell_selector_ctx,
            );

            if let Some(width) = resolve_cell_track_width(cell_el, &cell_style, table_width) {
                apply_cell_width_to_columns(&mut col_widths, col_pos, colspan, width);
            }

            col_pos = col_pos.saturating_add(colspan);
            if col_pos >= num_cols {
                break;
            }
        }
    }

    let assigned_width: f32 = col_widths.iter().flatten().copied().sum();
    let unresolved_count = col_widths.iter().filter(|width| width.is_none()).count();
    if unresolved_count > 0 {
        let remaining_width = (table_width - assigned_width).max(0.0);
        let default_width = remaining_width / unresolved_count as f32;
        for width in &mut col_widths {
            if width.is_none() {
                *width = Some(default_width);
            }
        }
    }

    let mut resolved_widths: Vec<f32> = col_widths
        .into_iter()
        .map(|width| width.unwrap_or(0.0))
        .collect();
    let resolved_total: f32 = resolved_widths.iter().sum();
    let used_table_width = table_width.max(resolved_total);
    if used_table_width > resolved_total && !resolved_widths.is_empty() {
        let extra_per_column = (used_table_width - resolved_total) / resolved_widths.len() as f32;
        for width in &mut resolved_widths {
            *width += extra_per_column;
        }
    }

    if resolved_widths.iter().all(|width| *width <= 0.0) && num_cols > 0 {
        return vec![table_width / num_cols as f32; num_cols];
    }

    resolved_widths
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn flatten_table(
    el: &ElementNode,
    style: &ComputedStyle,
    available_width: f32,
    output: &mut Vec<LayoutElement>,
    ancestors: &[AncestorInfo],
    table_child_index: usize,
    table_sibling_count: usize,
    env: &mut LayoutEnv,
) {
    let rules = env.rules;
    let fonts = env.fonts;
    let counter_state = &mut *env.counter_state;
    let inner_width = resolve_table_inner_width(style, available_width);

    // Build ancestor chain: everything above + the table element itself.
    let mut table_ancestors: Vec<AncestorInfo> = ancestors.to_vec();
    table_ancestors.push(AncestorInfo {
        element: el,
        child_index: table_child_index,
        sibling_count: table_sibling_count,
        preceding_siblings: Vec::new(),
    });

    // Collect all <tr> elements (from direct children, thead, tbody, tfoot).
    // Track section-relative indices so nth-child counts within each section
    // (thead, tbody, tfoot) as browsers do, not globally.
    // Also track the section element so descendant selectors can see it.
    let mut rows: Vec<&ElementNode> = Vec::new();
    let mut row_section_indices: Vec<usize> = Vec::new();
    let mut row_section_sizes: Vec<usize> = Vec::new();
    let mut row_section_elements: Vec<Option<&ElementNode>> = Vec::new();
    let mut row_section_child_indices: Vec<usize> = Vec::new();
    let mut row_section_sibling_counts: Vec<usize> = Vec::new();
    let section_count = el
        .children
        .iter()
        .filter(|c| matches!(c, DomNode::Element(_)))
        .count();
    for (section_child_idx, child) in el.children.iter().enumerate() {
        if let DomNode::Element(child_el) = child {
            match child_el.tag {
                HtmlTag::Tr => {
                    // Direct <tr> child of <table> — standalone section
                    let idx = rows.len();
                    rows.push(child_el);
                    row_section_indices.push(idx);
                    row_section_sizes.push(1);
                    row_section_elements.push(None);
                    row_section_child_indices.push(section_child_idx);
                    row_section_sibling_counts.push(section_count);
                }
                HtmlTag::Thead | HtmlTag::Tbody | HtmlTag::Tfoot => {
                    let section_rows: Vec<&ElementNode> = child_el
                        .children
                        .iter()
                        .filter_map(|gc| {
                            if let DomNode::Element(g) = gc {
                                if g.tag == HtmlTag::Tr {
                                    return Some(g);
                                }
                            }
                            None
                        })
                        .collect();
                    let section_size = section_rows.len();
                    for (i, gc) in section_rows.into_iter().enumerate() {
                        rows.push(gc);
                        row_section_indices.push(i);
                        row_section_sizes.push(section_size);
                        row_section_elements.push(Some(child_el));
                        row_section_child_indices.push(section_child_idx);
                        row_section_sibling_counts.push(section_count);
                    }
                }
                _ => {}
            }
        }
    }

    if rows.is_empty() {
        return;
    }

    // Determine column count from the widest row, accounting for colspan
    let num_cols = rows
        .iter()
        .map(|row| {
            row.children
                .iter()
                .filter_map(|c| {
                    if let DomNode::Element(e) = c {
                        if e.tag == HtmlTag::Td || e.tag == HtmlTag::Th {
                            let colspan = e
                                .attributes
                                .get("colspan")
                                .and_then(|v| v.parse::<usize>().ok())
                                .unwrap_or(1)
                                .max(1);
                            return Some(colspan);
                        }
                    }
                    None
                })
                .sum::<usize>()
        })
        .max()
        .unwrap_or(1);

    let mut column_parent_style = style.clone();
    column_parent_style.width = Some(inner_width);

    // --- Extract explicit column widths from <colgroup>/<col> elements ---
    let mut explicit_col_widths: Vec<Option<TableTrackWidth>> = vec![None; num_cols];
    {
        let mut col_idx = 0usize;
        for (section_child_idx, child) in el.children.iter().enumerate() {
            if let DomNode::Element(child_el) = child {
                match child_el.tag {
                    HtmlTag::Colgroup => {
                        let cols: Vec<&ElementNode> = child_el
                            .children
                            .iter()
                            .filter_map(|gc| match gc {
                                DomNode::Element(g) if g.tag == HtmlTag::Col => Some(g),
                                _ => None,
                            })
                            .collect();
                        let colgroup_style = compute_column_style(
                            child_el,
                            &column_parent_style,
                            rules,
                            &table_ancestors,
                            section_child_idx,
                            section_count,
                        );
                        if !cols.is_empty() {
                            let mut colgroup_basis_style = colgroup_style.clone();
                            colgroup_basis_style.width = Some(inner_width);
                            let mut colgroup_ancestors = table_ancestors.clone();
                            colgroup_ancestors.push(AncestorInfo {
                                element: child_el,
                                child_index: section_child_idx,
                                sibling_count: section_count,
                                preceding_siblings: Vec::new(),
                            });
                            let col_sibling_count = cols.len();
                            for (col_child_idx, col_el) in cols.into_iter().enumerate() {
                                assign_explicit_col_widths(
                                    &mut explicit_col_widths,
                                    &mut col_idx,
                                    parse_col_span(col_el),
                                    parse_col_width(
                                        col_el,
                                        &colgroup_basis_style,
                                        rules,
                                        &colgroup_ancestors,
                                        col_child_idx,
                                        col_sibling_count,
                                    ),
                                );
                            }
                            continue;
                        }
                        assign_explicit_col_widths(
                            &mut explicit_col_widths,
                            &mut col_idx,
                            parse_col_span(child_el),
                            parse_col_width(
                                child_el,
                                &column_parent_style,
                                rules,
                                &table_ancestors,
                                section_child_idx,
                                section_count,
                            ),
                        );
                    }
                    HtmlTag::Col => {
                        assign_explicit_col_widths(
                            &mut explicit_col_widths,
                            &mut col_idx,
                            parse_col_span(child_el),
                            parse_col_width(
                                child_el,
                                &column_parent_style,
                                rules,
                                &table_ancestors,
                                section_child_idx,
                                section_count,
                            ),
                        );
                    }
                    _ => continue,
                }
            }
        }
    }
    let has_explicit_widths = explicit_col_widths.iter().any(|width| width.is_some());
    // When `border-collapse: separate`, horizontal `border-spacing` is drawn between
    // every pair of adjacent cells AND on the outer edges, so the space available for
    // the N columns is `inner_width - (N+1) * border_spacing`. Without this reduction
    // the columns are distributed across the full width and the table overflows by
    // exactly `(N+1) * border_spacing` on the right.
    let columns_width = if matches!(
        style.border_collapse,
        crate::style::computed::BorderCollapse::Separate
    ) && style.border_spacing > 0.0
        && num_cols > 0
    {
        (inner_width - (num_cols as f32 + 1.0) * style.border_spacing).max(0.0)
    } else {
        inner_width
    };
    let col_widths: Vec<f32> = if uses_fixed_table_layout(style) {
        resolve_fixed_table_columns(
            style,
            columns_width,
            &rows,
            &row_section_indices,
            &row_section_sizes,
            &row_section_elements,
            &row_section_child_indices,
            &row_section_sibling_counts,
            &table_ancestors,
            &explicit_col_widths,
            num_cols,
            rules,
        )
    } else {
        // --- Auto-sizing pass: measure preferred content width for each column ---
        let min_col_width: f32 = 30.0;
        let mut preferred_widths: Vec<f32> = vec![0.0; num_cols];

        for (sizing_row_idx, row) in rows.iter().enumerate() {
            let row_classes = row.class_list();
            // Build ancestors for the row: table + optional section element
            let mut sizing_row_ancestors = table_ancestors.clone();
            if let Some(section_el) = row_section_elements[sizing_row_idx] {
                sizing_row_ancestors.push(AncestorInfo {
                    element: section_el,
                    child_index: row_section_child_indices[sizing_row_idx],
                    sibling_count: row_section_sibling_counts[sizing_row_idx],
                    preceding_siblings: Vec::new(),
                });
            }
            let sizing_row_ctx = SelectorContext {
                ancestors: sizing_row_ancestors,
                child_index: row_section_indices[sizing_row_idx],
                sibling_count: row_section_sizes[sizing_row_idx],
                preceding_siblings: Vec::new(),
            };
            let mut row_style = compute_style_with_context(
                row.tag,
                row.style_attr(),
                style,
                rules,
                row.tag_name(),
                &row_classes,
                row.id(),
                &row.attributes,
                &sizing_row_ctx,
            );
            row_style.width = Some(inner_width);
            let mut col_pos: usize = 0;
            for child in &row.children {
                if let DomNode::Element(cell_el) = child {
                    if cell_el.tag == HtmlTag::Td || cell_el.tag == HtmlTag::Th {
                        let colspan = cell_el
                            .attributes
                            .get("colspan")
                            .and_then(|v| v.parse::<usize>().ok())
                            .unwrap_or(1)
                            .max(1);
                        let cell_classes = cell_el.class_list();
                        let mut cell_sizing_ancestors = sizing_row_ctx.ancestors.clone();
                        cell_sizing_ancestors.push(AncestorInfo {
                            element: row,
                            child_index: row_section_indices[sizing_row_idx],
                            sibling_count: row_section_sizes[sizing_row_idx],
                            preceding_siblings: Vec::new(),
                        });
                        let cell_sizing_ctx = SelectorContext {
                            ancestors: cell_sizing_ancestors,
                            child_index: col_pos,
                            sibling_count: num_cols,
                            preceding_siblings: Vec::new(),
                        };
                        let cell_style = compute_style_with_context(
                            cell_el.tag,
                            cell_el.style_attr(),
                            &row_style,
                            rules,
                            cell_el.tag_name(),
                            &cell_classes,
                            cell_el.id(),
                            &cell_el.attributes,
                            &cell_sizing_ctx,
                        );
                        let mut runs = Vec::new();
                        let mut nested_rows = Vec::new();
                        let recurse_descendants = cell_el.children.iter().any(
                            |node| matches!(node, DomNode::Element(e) if recurses_as_layout_child(e.tag)),
                        );
                        let mut text_ancestors = cell_sizing_ctx.ancestors.clone();
                        text_ancestors.push(AncestorInfo {
                            element: cell_el,
                            child_index: col_pos,
                            sibling_count: num_cols,
                            preceding_siblings: Vec::new(),
                        });
                        collect_table_cell_content_inner(
                            &cell_el.children,
                            &cell_style,
                            &mut runs,
                            &mut nested_rows,
                            None,
                            rules,
                            fonts,
                            false,
                            recurse_descendants,
                            recurse_descendants,
                            &text_ancestors,
                            inner_width.max(1.0),
                            counter_state,
                        );
                        // Estimate content width using estimate_word_width for accurate
                        // measurement. Use the maximum of (full text width, longest word
                        // width) to avoid hyphenation of short columns like "Unit Price".
                        let content_width: f32 = runs
                            .iter()
                            .map(|run| {
                                // Atomic inline-block boxes (fill-in underlines /
                                // checkbox squares) reserve their explicit width so
                                // the column is wide enough not to truncate them.
                                if let Some(box_width) = run.box_width {
                                    return box_width;
                                }
                                // Measure full text width using estimate_word_width
                                let full_width = estimate_word_width(
                                    &run.text,
                                    run.font_size,
                                    &run.font_family,
                                    run.bold,
                                    run.italic,
                                    fonts,
                                );
                                // Also ensure the column is at least as wide as
                                // the longest word to prevent hyphenation.
                                let longest_word_width = run
                                    .text
                                    .split_whitespace()
                                    .map(|w| {
                                        estimate_word_width(
                                            w,
                                            run.font_size,
                                            &run.font_family,
                                            run.bold,
                                            run.italic,
                                            fonts,
                                        )
                                    })
                                    .fold(0.0f32, f32::max);
                                full_width.max(longest_word_width)
                            })
                            .sum();
                        let nested_width = nested_rows
                            .iter()
                            .map(table_row_content_width)
                            .fold(0.0f32, f32::max);
                        let total_preferred = content_width.max(nested_width)
                            + cell_style.padding.left
                            + cell_style.padding.right;
                        if colspan == 1 {
                            if col_pos < num_cols {
                                preferred_widths[col_pos] =
                                    preferred_widths[col_pos].max(total_preferred);
                            }
                        } else {
                            let per_col = total_preferred / colspan as f32;
                            for i in 0..colspan {
                                if col_pos + i < num_cols {
                                    preferred_widths[col_pos + i] =
                                        preferred_widths[col_pos + i].max(per_col);
                                }
                            }
                        }
                        col_pos += colspan;
                    }
                }
            }
        }

        for width in &mut preferred_widths {
            if *width < min_col_width {
                *width = min_col_width;
            }
        }

        if has_explicit_widths {
            preferred_widths
                .iter()
                .zip(explicit_col_widths.iter())
                .map(|(preferred, explicit)| {
                    explicit
                        .map(|width| width.resolve(columns_width).max(min_col_width))
                        .unwrap_or_else(|| preferred.max(min_col_width))
                })
                .collect()
        } else {
            let total_preferred: f32 = preferred_widths.iter().sum();
            if total_preferred <= columns_width {
                let extra = columns_width - total_preferred;
                if total_preferred > 0.0 && extra > 0.0 {
                    preferred_widths
                        .iter()
                        .map(|width| width + (width / total_preferred) * extra)
                        .collect()
                } else {
                    preferred_widths
                }
            } else {
                let scale = columns_width / total_preferred;
                preferred_widths
                    .iter()
                    .map(|width| (width * scale).max(min_col_width))
                    .collect()
            }
        }
    };

    // Resolve the table's own CSS height. Row/cell percentage heights resolve
    // against it; any surplus beyond the summed row heights is distributed back
    // across the rows so a fixed-height table fills its box.
    let table_specified_height = resolve_specified_height(style, Some(style.viewport_height));
    let row_height_basis = if table_specified_height > 0.0 {
        Some(table_specified_height)
    } else {
        None
    };

    // Build layout rows, tracking cells occupied by rowspan from previous rows.
    // Each entry in `occupied` tracks the remaining rowspan count for that column.
    let mut occupied: Vec<usize> = vec![0; num_cols];
    let mut is_first = true;
    // Collect this table's rows locally so the table-height surplus can be
    // distributed across them before they are appended to `output`.
    let mut table_rows: Vec<LayoutElement> = Vec::new();
    for (row_idx, row) in rows.iter().enumerate() {
        let row_classes = row.class_list();
        // Use section-relative index for nth-child matching (browsers count
        // within thead/tbody/tfoot, not globally across all rows).
        let section_idx = row_section_indices[row_idx];
        let section_size = row_section_sizes[row_idx];
        // Build ancestors for the row: table + optional section element
        let mut row_ancestors = table_ancestors.clone();
        if let Some(section_el) = row_section_elements[row_idx] {
            row_ancestors.push(AncestorInfo {
                element: section_el,
                child_index: row_section_child_indices[row_idx],
                sibling_count: row_section_sibling_counts[row_idx],
                preceding_siblings: Vec::new(),
            });
        }
        let row_selector_ctx = SelectorContext {
            ancestors: row_ancestors,
            child_index: section_idx,
            sibling_count: section_size,
            preceding_siblings: Vec::new(),
        };
        let mut row_style = compute_style_with_context(
            row.tag,
            row.style_attr(),
            style,
            rules,
            row.tag_name(),
            &row_classes,
            row.id(),
            &row.attributes,
            &row_selector_ctx,
        );
        row_style.width = Some(inner_width);
        // A `<tr style="height">` floor applies to every cell in the row.
        let row_specified_height = resolve_specified_height(&row_style, row_height_basis);
        let mut cells = Vec::new();

        // Current logical column position in the grid
        let mut col_pos: usize = 0;
        let mut child_iter = row.children.iter().filter_map(|child| {
            if let DomNode::Element(cell_el) = child {
                if cell_el.tag == HtmlTag::Td || cell_el.tag == HtmlTag::Th {
                    return Some(cell_el);
                }
            }
            None
        });

        // Process cells, skipping occupied positions and inserting phantom cells
        let mut next_cell = child_iter.next();
        while col_pos < num_cols {
            if occupied[col_pos] > 0 {
                // This position is occupied by a rowspan from a previous row.
                // Insert a phantom cell (rowspan = 0) as a placeholder.
                let span_cols = {
                    // Count how many consecutive occupied columns share this rowspan
                    let remaining = occupied[col_pos];
                    let mut count = 1;
                    while col_pos + count < num_cols && occupied[col_pos + count] == remaining {
                        count += 1;
                    }
                    count
                };
                cells.push(TableCell {
                    lines: Vec::new(),
                    nested_rows: Vec::new(),
                    bold: false,
                    background_color: None,
                    padding_top: 0.0,
                    padding_right: 0.0,
                    padding_bottom: 0.0,
                    padding_left: 0.0,
                    colspan: span_cols,
                    rowspan: 0, // phantom cell marker
                    border: LayoutBorder::default(),
                    text_align: TextAlign::Left,
                    vertical_align: VerticalAlign::Baseline,
                    min_height: row_specified_height,
                });
                for i in 0..span_cols {
                    occupied[col_pos + i] -= 1;
                }
                col_pos += span_cols;
                continue;
            }

            // Place the next real cell at this position
            let Some(cell_el) = next_cell else { break };
            next_cell = child_iter.next();

            let colspan = cell_el
                .attributes
                .get("colspan")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1)
                .max(1);
            let rowspan = cell_el
                .attributes
                .get("rowspan")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1)
                .max(1);

            let cell_classes = cell_el.class_list();
            let mut cell_ancestors = row_selector_ctx.ancestors.clone();
            cell_ancestors.push(AncestorInfo {
                element: row,
                child_index: section_idx,
                sibling_count: section_size,
                preceding_siblings: Vec::new(),
            });
            let cell_selector_ctx = SelectorContext {
                ancestors: cell_ancestors,
                child_index: col_pos,
                sibling_count: num_cols,
                preceding_siblings: Vec::new(),
            };
            let cell_style = compute_style_with_context(
                cell_el.tag,
                cell_el.style_attr(),
                &row_style,
                rules,
                cell_el.tag_name(),
                &cell_classes,
                cell_el.id(),
                &cell_el.attributes,
                &cell_selector_ctx,
            );
            // Compute effective width from auto-sized column widths
            let effective_width: f32 = col_widths.iter().skip(col_pos).take(colspan).copied().sum();
            let cell_inner = effective_width - cell_style.padding.left - cell_style.padding.right;
            let mut cell_content_style = cell_style.clone();
            cell_content_style.width = Some(cell_inner.max(0.0));

            let mut runs = Vec::new();
            let mut nested_rows = Vec::new();
            let recurse_descendants = cell_el
                .children
                .iter()
                .any(|node| matches!(node, DomNode::Element(e) if recurses_as_layout_child(e.tag)));
            let mut text_ancestors = cell_selector_ctx.ancestors.clone();
            text_ancestors.push(AncestorInfo {
                element: cell_el,
                child_index: col_pos,
                sibling_count: num_cols,
                preceding_siblings: Vec::new(),
            });
            let (block_margin_top, block_margin_bottom) = table_cell_edge_block_margins(
                &cell_el.children,
                &cell_content_style,
                rules,
                &text_ancestors,
            );
            collect_table_cell_content_inner(
                &cell_el.children,
                &cell_content_style,
                &mut runs,
                &mut nested_rows,
                None,
                rules,
                fonts,
                false,
                recurse_descendants,
                recurse_descendants,
                &text_ancestors,
                cell_inner.max(1.0),
                counter_state,
            );
            let lines = wrap_text_runs(
                runs,
                TextWrapOptions::new(
                    cell_inner.max(1.0),
                    cell_style.font_size,
                    resolved_line_height_factor(&cell_style, fonts),
                    cell_style.overflow_wrap,
                )
                .with_rtl(cell_style.direction_rtl),
                fonts,
            );

            let bg = cell_style
                .background_color
                .or(row_style.background_color)
                .map(|c: crate::types::Color| c.to_f32_rgba());

            // The cell's height floor is the larger of its own CSS height and
            // the row's height; either expands the row beyond its content.
            let cell_specified_height = resolve_specified_height(&cell_style, row_height_basis);

            cells.push(TableCell {
                lines,
                nested_rows,
                bold: cell_style.font_weight == FontWeight::Bold,
                background_color: bg,
                padding_top: cell_style.padding.top + block_margin_top,
                padding_right: cell_style.padding.right,
                padding_bottom: cell_style.padding.bottom + block_margin_bottom,
                padding_left: cell_style.padding.left,
                colspan,
                rowspan,
                border: LayoutBorder::from_computed(&cell_style.border),
                text_align: cell_style.text_align,
                vertical_align: cell_style.vertical_align,
                min_height: cell_specified_height.max(row_specified_height),
            });

            // Mark subsequent rows as occupied if rowspan > 1
            if rowspan > 1 {
                for i in 0..colspan {
                    if col_pos + i < num_cols {
                        occupied[col_pos + i] = rowspan - 1;
                    }
                }
            }

            col_pos += colspan;
        }

        if !cells.is_empty() {
            let is_header = row_section_elements[row_idx]
                .map(|s| s.tag == HtmlTag::Thead)
                .unwrap_or(false);
            table_rows.push(LayoutElement::TableRow {
                cells,
                col_widths: col_widths.clone(),
                margin_top: if is_first {
                    style.margin.top
                } else if style.border_collapse == BorderCollapse::Separate {
                    style.border_spacing
                } else {
                    0.0
                },
                margin_bottom: 0.0,
                border_collapse: style.border_collapse,
                border_spacing: style.border_spacing,
                is_header,
            });
            is_first = false;
        }
    }

    // Add bottom margin after the last row
    if let Some(LayoutElement::TableRow { margin_bottom, .. }) = table_rows.last_mut() {
        *margin_bottom = style.margin.bottom;
    }

    // When the table specifies a height larger than the sum of its rows,
    // distribute the surplus across the rows (proportionally to their current
    // heights) so the table fills its box. Tables with no specified height are
    // left untouched, keeping the common content-driven case unchanged.
    distribute_table_height_surplus(&mut table_rows, table_specified_height);

    output.append(&mut table_rows);
}

/// Grow each row so the rows together fill `table_height` when the table
/// specifies a height beyond their natural sum. The surplus is shared in
/// proportion to each row's current height (falling back to an even split when
/// every row is zero-height). A `0.0` or already-satisfied target is a no-op,
/// preserving content-driven tables.
fn distribute_table_height_surplus(table_rows: &mut [LayoutElement], table_height: f32) {
    if table_height <= 0.0 || table_rows.is_empty() {
        return;
    }
    let row_heights: Vec<f32> = table_rows
        .iter()
        .map(|row| match row {
            LayoutElement::TableRow { cells, .. } => cells
                .iter()
                .map(table_cell_box_height)
                .fold(0.0f32, f32::max),
            _ => 0.0,
        })
        .collect();
    let total: f32 = row_heights.iter().sum();
    let surplus = table_height - total;
    if surplus <= 0.0 {
        return;
    }
    let row_count = table_rows.len() as f32;
    for (row, &row_h) in table_rows.iter_mut().zip(row_heights.iter()) {
        let extra = if total > 0.0 {
            surplus * (row_h / total)
        } else {
            surplus / row_count
        };
        let target = row_h + extra;
        if let LayoutElement::TableRow { cells, .. } = row {
            for cell in cells.iter_mut() {
                // Phantom cells (rowspan == 0) belong to a rowspan from an
                // earlier row; leave their height to that origin row.
                if cell.rowspan == 0 {
                    continue;
                }
                cell.min_height = cell.min_height.max(target);
            }
        }
    }
}

fn table_cell_edge_block_margins(
    nodes: &[DomNode],
    parent_style: &ComputedStyle,
    rules: &[CssRule],
    ancestors: &[AncestorInfo],
) -> (f32, f32) {
    let element_sibling_count = nodes
        .iter()
        .filter(|node| matches!(node, DomNode::Element(_)))
        .count();

    let mut first_margin_top = None;
    let mut last_margin_bottom = None;

    for (node_index, node) in nodes.iter().enumerate() {
        let DomNode::Element(element) = node else {
            continue;
        };
        if element.tag == HtmlTag::Br
            || element.tag == HtmlTag::Table
            || element.children.is_empty()
        {
            continue;
        }

        let child_index = nodes[..node_index]
            .iter()
            .filter(|node| matches!(node, DomNode::Element(_)))
            .count();
        let preceding_siblings = nodes[..node_index]
            .iter()
            .filter_map(|node| match node {
                DomNode::Element(element) => Some((
                    element.tag_name().to_string(),
                    element
                        .class_list()
                        .into_iter()
                        .map(str::to_string)
                        .collect(),
                )),
                _ => None,
            })
            .collect();
        let selector_ctx = SelectorContext {
            ancestors: ancestors.to_vec(),
            child_index,
            sibling_count: element_sibling_count,
            preceding_siblings,
        };
        let child_style = compute_style_with_context(
            element.tag,
            element.style_attr(),
            parent_style,
            rules,
            element.tag_name(),
            &element.class_list(),
            element.id(),
            &element.attributes,
            &selector_ctx,
        );
        if child_style.display == Display::Inline {
            continue;
        }

        first_margin_top.get_or_insert(child_style.margin.top);
        last_margin_bottom = Some(child_style.margin.bottom);
    }

    (
        first_margin_top.unwrap_or(0.0),
        last_margin_bottom.unwrap_or(0.0),
    )
}

/// Concatenate all descendant text of a node subtree into one string. Used to
/// build the single atomic text run for a `display:inline-block` box inside a
/// table cell (its box/border/size are carried on the run; its text is its
/// flattened content). Whitespace collapsing and `&nbsp;`→space handling are
/// applied by the caller via `collapse_whitespace`.
fn flatten_descendant_text(nodes: &[DomNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            DomNode::Text(text) => out.push_str(text),
            DomNode::Element(el) => out.push_str(&flatten_descendant_text(&el.children)),
        }
    }
    out
}

/// Whether an inline-block element carries a renderable atomic box: an explicit
/// `width`/`height`, or any non-zero border. Such boxes (form fill-in
/// underlines, checkbox squares) must render their geometry rather than being
/// flattened to plain text runs that drop the border and size.
fn inline_block_has_box(style: &ComputedStyle) -> bool {
    style.display == Display::InlineBlock
        && (style.width.is_some() || style.height.is_some() || style.border.bottom.width > 0.0)
}

#[allow(clippy::too_many_arguments)]
fn collect_table_cell_content_inner(
    nodes: &[DomNode],
    parent_style: &ComputedStyle,
    runs: &mut Vec<TextRun>,
    nested_rows: &mut Vec<LayoutElement>,
    link_url: Option<&str>,
    rules: &[CssRule],
    fonts: &HashMap<String, TtfFont>,
    inline_parent: bool,
    recurse_blocks: bool,
    suppress_direct_text_padding: bool,
    ancestors: &[AncestorInfo],
    available_width: f32,
    counter_state: &mut CounterState,
) {
    let preserve_ws = matches!(
        parent_style.white_space,
        WhiteSpace::Pre | WhiteSpace::PreWrap
    );
    let element_sibling_count = nodes
        .iter()
        .filter(|node| matches!(node, DomNode::Element(_)))
        .count();

    for (node_index, node) in nodes.iter().enumerate() {
        match node {
            DomNode::Text(text) => {
                let processed = if preserve_ws {
                    text.clone()
                } else {
                    collapse_whitespace(text)
                };
                // Apply CSS text-transform
                let processed = match parent_style.text_transform {
                    crate::style::computed::TextTransform::Uppercase => processed.to_uppercase(),
                    crate::style::computed::TextTransform::Lowercase => processed.to_lowercase(),
                    crate::style::computed::TextTransform::Capitalize => {
                        let mut result = String::with_capacity(processed.len());
                        let mut prev_is_space = true;
                        for c in processed.chars() {
                            if prev_is_space && c.is_alphabetic() {
                                for uc in c.to_uppercase() {
                                    result.push(uc);
                                }
                            } else {
                                result.push(c);
                            }
                            prev_is_space = c.is_whitespace();
                        }
                        result
                    }
                    crate::style::computed::TextTransform::None => processed,
                };
                if !processed.is_empty() {
                    let (bg, pad, br) = if (inline_parent || recurse_blocks) && !preserve_ws {
                        let pad = if suppress_direct_text_padding {
                            (0.0, 0.0)
                        } else {
                            (parent_style.padding.left, parent_style.padding.top)
                        };
                        (
                            parent_style.background_color.map(|c| c.to_f32_rgba()),
                            pad,
                            parent_style.border_radius,
                        )
                    } else {
                        (None, (0.0, 0.0), 0.0)
                    };
                    push_text_run(
                        runs,
                        TextRun {
                            text: processed,
                            font_size: parent_style.font_size,
                            bold: parent_style.font_weight == FontWeight::Bold,
                            italic: parent_style.font_style == FontStyle::Italic,
                            underline: parent_style.text_decoration_underline,
                            line_through: parent_style.text_decoration_line_through,
                            overline: parent_style.text_decoration_overline,
                            color: parent_style.color.to_f32_rgb(),
                            link_url: link_url.map(String::from),
                            font_family: resolve_style_font_family(parent_style, fonts),
                            background_color: bg,
                            padding: pad,
                            border_radius: br,
                            box_width: None,
                            box_height: None,
                            border_bottom: None,
                        },
                    );
                }
            }
            DomNode::Element(el) => {
                let child_index = nodes[..node_index]
                    .iter()
                    .filter(|node| matches!(node, DomNode::Element(_)))
                    .count();
                let preceding_siblings = nodes[..node_index]
                    .iter()
                    .filter_map(|node| match node {
                        DomNode::Element(element) => Some((
                            element.tag_name().to_string(),
                            element
                                .class_list()
                                .into_iter()
                                .map(str::to_string)
                                .collect(),
                        )),
                        _ => None,
                    })
                    .collect();
                let classes = el.class_list();
                let selector_ctx = SelectorContext {
                    ancestors: ancestors.to_vec(),
                    child_index,
                    sibling_count: element_sibling_count,
                    preceding_siblings,
                };
                let style = compute_style_with_context(
                    el.tag,
                    el.style_attr(),
                    parent_style,
                    rules,
                    el.tag_name(),
                    &classes,
                    el.id(),
                    &el.attributes,
                    &selector_ctx,
                );
                if style.display == Display::None {
                    continue;
                }
                // Atomic inline-block box (form fill-in underline / checkbox
                // square): emit ONE text run carrying the box geometry so the
                // border and explicit width/height survive, flowing inline with
                // sibling text. Without this the box is flattened to plain runs
                // that drop the border and size. Only applies to SVG-free inline
                // blocks; SVG/Img keep their dedicated branches below.
                if el.tag != HtmlTag::Svg && el.tag != HtmlTag::Img && inline_block_has_box(&style)
                {
                    let raw = flatten_descendant_text(&el.children);
                    let collapsed = collapse_whitespace(&raw);
                    // Keep a single space when empty so the run survives merge /
                    // wrap (which skip empty text); its advance comes from the
                    // box_width, not the glyph.
                    let text = if collapsed.is_empty() {
                        " ".to_string()
                    } else {
                        collapsed
                    };
                    let bottom = style.border.bottom;
                    let border_bottom = if bottom.width > 0.0 {
                        let rgb = bottom
                            .color
                            .map(|c| c.to_f32_rgb())
                            .unwrap_or_else(|| style.color.to_f32_rgb());
                        Some((bottom.width, rgb))
                    } else {
                        None
                    };
                    push_text_run(
                        runs,
                        TextRun {
                            text,
                            font_size: style.font_size,
                            bold: style.font_weight == FontWeight::Bold,
                            italic: style.font_style == FontStyle::Italic,
                            underline: style.text_decoration_underline,
                            line_through: style.text_decoration_line_through,
                            overline: style.text_decoration_overline,
                            color: style.color.to_f32_rgb(),
                            link_url: link_url.map(String::from),
                            font_family: resolve_style_font_family(&style, fonts),
                            background_color: style.background_color.map(|c| c.to_f32_rgba()),
                            padding: (0.0, 0.0),
                            border_radius: style.border_radius,
                            box_width: style.width,
                            box_height: style.height,
                            border_bottom,
                        },
                    );
                    continue;
                }
                let url = if el.tag == HtmlTag::A {
                    el.attributes.get("href").map(|s| s.as_str()).or(link_url)
                } else {
                    link_url
                };
                let mut child_ancestors = ancestors.to_vec();
                child_ancestors.push(AncestorInfo {
                    element: el,
                    child_index,
                    sibling_count: element_sibling_count,
                    preceding_siblings: Vec::new(),
                });
                if el.tag == HtmlTag::Table {
                    let mut inner_env = LayoutEnv {
                        rules,
                        fonts,
                        counter_state,
                    };
                    flatten_table(
                        el,
                        &style,
                        available_width,
                        nested_rows,
                        &child_ancestors,
                        child_index,
                        element_sibling_count,
                        &mut inner_env,
                    );
                } else if el.tag == HtmlTag::Svg
                    || el.tag == HtmlTag::Img
                    || (recurse_blocks
                        && style.display != Display::Inline
                        && el.tag != HtmlTag::Br
                        && el.children.is_empty()
                        && (has_background_paint(&style)
                            || style.border.has_any()
                            || style.box_shadow.is_some()
                            || style.aspect_ratio.is_some()
                            || style.height.is_some()
                            || style.width.is_some()))
                {
                    let cell_ctx = LayoutContext {
                        viewport: Viewport {
                            width: available_width,
                            height: f32::INFINITY,
                        },
                        parent: ParentBox {
                            content_width: available_width,
                            content_height: None,
                            font_size: parent_style.font_size,
                        },
                        containing_block: None,
                        root_font_size: parent_style.root_font_size,
                    };
                    let mut inner_env = LayoutEnv {
                        rules,
                        fonts,
                        counter_state,
                    };
                    flatten_element(
                        el,
                        parent_style,
                        &cell_ctx,
                        nested_rows,
                        None,
                        ancestors,
                        0,
                        child_index,
                        element_sibling_count,
                        &selector_ctx.preceding_siblings,
                        &mut inner_env,
                    );
                } else if recurse_blocks || collects_as_inline_text(el.tag) || el.tag == HtmlTag::Br
                {
                    if el.tag == HtmlTag::Br {
                        push_line_break_run(runs, parent_style, fonts);
                    } else {
                        collect_table_cell_content_inner(
                            &el.children,
                            &style,
                            runs,
                            nested_rows,
                            url,
                            rules,
                            fonts,
                            collects_as_inline_text(el.tag),
                            recurse_blocks,
                            false,
                            &child_ancestors,
                            available_width,
                            counter_state,
                        );
                        if recurse_blocks && style.display != Display::Inline && !runs.is_empty() {
                            push_line_break_run(runs, &style, fonts);
                        }
                    }
                }
            }
        }
    }
}

fn push_text_run(runs: &mut Vec<TextRun>, run: TextRun) {
    runs.push(run);
}

fn push_line_break_run(
    runs: &mut Vec<TextRun>,
    style: &ComputedStyle,
    fonts: &HashMap<String, TtfFont>,
) {
    push_text_run(
        runs,
        TextRun {
            text: "\n".to_string(),
            font_size: style.font_size,
            bold: false,
            italic: false,
            underline: false,
            line_through: false,
            overline: false,
            color: (0.0, 0.0, 0.0),
            link_url: None,
            font_family: resolve_style_font_family(style, fonts),
            background_color: None,
            padding: (0.0, 0.0),
            border_radius: 0.0,
            box_width: None,
            box_height: None,
            border_bottom: None,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentage_max_height_without_height_does_not_create_floor() {
        let mut style = ComputedStyle::default();
        style.percentage_sizing.max_height = Some(50.0);

        assert_eq!(resolve_specified_height(&style, Some(200.0)), 0.0);
    }

    #[test]
    fn percentage_max_height_clamps_existing_height() {
        let mut style = ComputedStyle {
            height: Some(150.0),
            ..Default::default()
        };
        style.percentage_sizing.max_height = Some(50.0);

        assert_eq!(resolve_specified_height(&style, Some(200.0)), 100.0);
    }
}
