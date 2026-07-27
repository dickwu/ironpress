use super::{
    MultiColItem, balance_columns, balanced_buckets_height, column_has_content, column_rule_x,
    column_x,
};
use crate::layout::elements::{
    BlockSize, BoxModel, BoxPaint, ColumnRule, Container, FragmentBox, FragmentBreakQuery,
    FragmentBreakRule, FragmentPlacement, IntoLayoutNode, LayoutElement, LayoutNode, LayoutSize,
    LayoutVisitor, LayoutVisitorMut, MulticolColumn, TextBlock,
};
use crate::layout::engine::{LayoutBorderSide, TextLine};
use crate::layout::flow_metrics::BlockMargins;
use crate::layout::roundoff::{
    equal_with_roundoff, exceeds_with_roundoff, is_positive_with_roundoff,
};
use crate::style::computed::{BorderStyle, ComputedStyle, Position};
use crate::types::{Point, Size, Vector};

/// Distribute `items` into a sequence of per-page column rows for a paginated
/// multicol (CSS Multicol §2). Each page-row is a fresh set of `num_cols` columns
/// filled sequentially to `col_fill_h` (the page's content-box height), and any
/// block that does not fit continues at the top of the next page-row — so content
/// flows column-by-column down a page, then onto the next page, instead of being
/// clipped.
///
/// Reuses [`fragment_columns`] with dynamically growing *virtual* columns,
/// then regroups every `num_cols` virtual columns into one page-row. Returns
/// `(column_children, used_content_height)` per page-row, in page order, with
/// trailing empty rows dropped. A single returned row means the content fits one
/// page (the caller then takes the unchanged single-page path).
#[allow(clippy::too_many_arguments)]
pub(super) fn build_paginated_column_rows(
    items: &[MultiColItem],
    num_cols: usize,
    col_width: f32,
    gap: f32,
    pad_left: f32,
    col_fill_h: f32,
    style: &ComputedStyle,
) -> Vec<(Vec<LayoutNode>, f32)> {
    let indices: Vec<usize> = (0..items.len()).collect();
    let fragmented = fragment_columns(
        items,
        &indices,
        ColumnFragmentation::overflowing(num_cols, col_fill_h),
    );
    let page_rows = fragmented.columns.len().div_ceil(num_cols);

    let rule_active = style.column_rule.used_width() > 0.0 && num_cols > 1;
    let mut rows: Vec<(Vec<LayoutNode>, f32)> = Vec::new();
    for page in 0..page_rows {
        let mut row_children: Vec<LayoutNode> = Vec::new();
        let mut row_max = 0.0f32;
        let mut has_content = false;
        for pc in 0..num_cols {
            let vc = page * num_cols + pc;
            if vc >= fragmented.columns.len() {
                break;
            }
            let frags = &fragmented.columns[vc];
            if frags.is_empty() {
                continue;
            }
            has_content = true;
            row_max = row_max.max(fragmented.used_block_sizes[vc]);
            let col_x = column_x(style, pad_left, col_width, gap, num_cols, pc);
            let mut kids: Vec<LayoutNode> = Vec::new();
            for f in frags {
                kids.push(make_fragment_box(
                    &items[f.item].elements[0],
                    f.placement(0.0, col_width),
                ));
            }
            row_children.push(make_column_container(
                kids,
                pc,
                col_x - pad_left + style.padding.left,
                style.padding.top,
                col_width,
                fragmented.used_block_sizes[vc],
            ));
        }
        if !has_content {
            continue;
        }
        if rule_active {
            let rule_w = style.column_rule.used_width();
            let rule_color = style.column_rule.color.resolve(style.color);
            for c in 0..num_cols - 1 {
                let left = page * num_cols + c;
                let right = left + 1;
                if fragmented.columns.get(left).is_none_or(Vec::is_empty)
                    || fragmented.columns.get(right).is_none_or(Vec::is_empty)
                {
                    continue;
                }
                let gap_center = column_rule_x(style, pad_left, col_width, gap, num_cols, c);
                let rule_x = gap_center - rule_w / 2.0;
                row_children.push(make_rule_container(
                    c,
                    rule_x - pad_left + style.padding.left,
                    style.padding.top,
                    rule_w,
                    row_max,
                    rule_color,
                    style.column_rule.style,
                ));
            }
        }
        rows.push((row_children, row_max));
    }
    rows
}

/// Page-aware balanced multicol fragmentation for `column-fill: balance-all`.
/// Each page row takes the largest document-order run that can be balanced into
/// the available column block-size, then the next page row re-balances the
/// remaining content.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_balanced_paginated_column_rows(
    items: &[MultiColItem],
    num_cols: usize,
    col_width: f32,
    gap: f32,
    pad_left: f32,
    col_fill_h: f32,
    style: &ComputedStyle,
) -> Vec<(Vec<LayoutNode>, f32)> {
    let mut rows: Vec<(Vec<LayoutNode>, f32)> = Vec::new();
    let mut start = 0usize;
    while start < items.len() {
        let mut best_end = start + 1;
        let mut best_buckets = vec![vec![0usize]];
        let mut best_h = items[start].height;

        for end in start + 1..=items.len() {
            let heights: Vec<f32> = items[start..end].iter().map(|it| it.height).collect();
            let buckets = balance_columns(&heights, num_cols);
            let row_h = balanced_buckets_height(&items[start..end], &buckets);
            if !exceeds_with_roundoff(row_h, col_fill_h) || end == start + 1 {
                best_end = end;
                best_buckets = buckets;
                best_h = row_h;
            } else {
                break;
            }
        }

        let mut row_children: Vec<LayoutNode> = Vec::new();
        if style.column_rule.used_width() > 0.0 && num_cols > 1 {
            let rule_w = style.column_rule.used_width();
            let rule_color = style.column_rule.color.resolve(style.color);
            for c in 0..num_cols - 1 {
                if !column_has_content(&best_buckets, c)
                    || !column_has_content(&best_buckets, c + 1)
                {
                    continue;
                }
                let gap_center = column_rule_x(style, pad_left, col_width, gap, num_cols, c);
                let rule_x = gap_center - rule_w / 2.0;
                row_children.push(make_rule_container(
                    c,
                    rule_x - pad_left + style.padding.left,
                    style.padding.top,
                    rule_w,
                    best_h,
                    rule_color,
                    style.column_rule.style,
                ));
            }
        }

        for (c, bucket) in best_buckets.iter().enumerate() {
            if bucket.is_empty() {
                continue;
            }
            let col_x = column_x(style, pad_left, col_width, gap, num_cols, c);
            let mut col_kids: Vec<LayoutNode> = Vec::new();
            let mut col_height = 0.0f32;
            for &idx in bucket {
                col_height += items[start + idx].height;
                col_kids.extend(items[start + idx].elements.clone());
            }
            row_children.push(make_column_container(
                col_kids,
                c,
                col_x - pad_left + style.padding.left,
                style.padding.top,
                col_width,
                col_height,
            ));
        }
        rows.push((row_children, best_h));
        start = best_end;
    }
    rows
}

/// Page-aware multicol fragmentation for flows that include `column-span: all`.
/// Consecutive non-span items are balanced into the remaining column space on the
/// current page row; a span-all item is placed as a full-width band at the current
/// block cursor. When the next segment would overflow, a new page row starts.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_paginated_column_rows_with_spans(
    items: &[MultiColItem],
    num_cols: usize,
    col_width: f32,
    gap: f32,
    pad_left: f32,
    col_fill_h: f32,
    inner_width: f32,
    style: &ComputedStyle,
) -> Vec<(Vec<LayoutNode>, f32)> {
    let mut rows: Vec<(Vec<LayoutNode>, f32)> = Vec::new();
    let mut row_children: Vec<LayoutNode> = Vec::new();
    let mut cursor = 0.0f32;

    let finish_row = |rows: &mut Vec<(Vec<LayoutNode>, f32)>,
                      row_children: &mut Vec<LayoutNode>,
                      cursor: &mut f32| {
        if row_children.is_empty() {
            *cursor = 0.0;
            return;
        }
        rows.push((std::mem::take(row_children), (*cursor).max(0.0)));
        *cursor = 0.0;
    };

    let add_rules = |row_children: &mut Vec<LayoutNode>, run_top: f32, run_h: f32| {
        if style.column_rule.used_width() <= 0.0 || num_cols <= 1 || run_h <= 0.0 {
            return;
        }
        let rule_w = style.column_rule.used_width();
        let rule_color = style.column_rule.color.resolve(style.color);
        for c in 0..num_cols - 1 {
            let gap_center = column_rule_x(style, pad_left, col_width, gap, num_cols, c);
            let rule_x = gap_center - rule_w / 2.0;
            row_children.push(make_rule_container(
                c,
                rule_x - pad_left + style.padding.left,
                style.padding.top + run_top,
                rule_w,
                run_h,
                rule_color,
                style.column_rule.style,
            ));
        }
    };

    let place_balanced_run = |row_children: &mut Vec<LayoutNode>,
                              run: &[MultiColItem],
                              top: f32,
                              truncate_all_trailing: bool|
     -> f32 {
        let heights: Vec<f32> = run.iter().map(|it| it.height).collect();
        let buckets = balance_columns(&heights, num_cols);
        let last_nonempty_col = buckets
            .iter()
            .rposition(|b| !b.is_empty())
            .unwrap_or(usize::MAX);
        let mut run_max_h = 0.0f32;
        for (c, bucket) in buckets.iter().enumerate() {
            if bucket.is_empty() {
                continue;
            }
            let col_x = column_x(style, pad_left, col_width, gap, num_cols, c);
            let mut col_kids: Vec<LayoutNode> = Vec::new();
            let mut col_height = 0.0f32;
            for &idx in bucket {
                col_height += run[idx].height;
                col_kids.extend(run[idx].elements.clone());
            }
            let used_col_height = if truncate_all_trailing || c != last_nonempty_col {
                let trailing_margin = bucket.last().map_or(0.0, |&index| run[index].margin_bottom);
                (col_height - trailing_margin).max(0.0)
            } else {
                col_height
            };
            run_max_h = run_max_h.max(used_col_height);
            row_children.push(make_column_container(
                col_kids,
                c,
                col_x - pad_left + style.padding.left,
                style.padding.top + top,
                col_width,
                col_height,
            ));
        }
        run_max_h
    };

    let mut i = 0usize;
    while i < items.len() {
        if items[i].span_all {
            let band_h = items[i].height;
            if cursor > 0.0 && exceeds_with_roundoff(cursor + band_h, col_fill_h) {
                finish_row(&mut rows, &mut row_children, &mut cursor);
            }
            row_children.push(make_band_container(
                items[i].elements.clone(),
                style.padding.left,
                style.padding.top + cursor,
                inner_width,
                band_h,
            ));
            cursor += band_h;
            i += 1;
            continue;
        }

        let run_start = i;
        while i < items.len() && !items[i].span_all {
            i += 1;
        }
        let run_end = i;
        let mut start = run_start;
        while start < run_end {
            if !exceeds_with_roundoff(col_fill_h, cursor) {
                finish_row(&mut rows, &mut row_children, &mut cursor);
            }
            let remaining = (col_fill_h - cursor).max(0.0);
            let mut best_end = start;
            let mut best_h = 0.0f32;
            for end in start + 1..=run_end {
                let heights: Vec<f32> = items[start..end].iter().map(|it| it.height).collect();
                let buckets = balance_columns(&heights, num_cols);
                let mut max_h = 0.0f32;
                let truncates_at_page_break = end < run_end;
                let last_nonempty_col = buckets
                    .iter()
                    .rposition(|b| !b.is_empty())
                    .unwrap_or(usize::MAX);
                for (c, bucket) in buckets.iter().enumerate() {
                    let mut col_h = 0.0f32;
                    for &idx in bucket {
                        col_h += items[start + idx].height;
                    }
                    if truncates_at_page_break || c != last_nonempty_col {
                        if let Some(&last_idx) = bucket.last() {
                            col_h = (col_h - items[start + last_idx].margin_bottom).max(0.0);
                        }
                    }
                    max_h = max_h.max(col_h);
                }
                if !exceeds_with_roundoff(max_h, remaining) || best_end == start {
                    best_end = end;
                    best_h = max_h;
                } else {
                    break;
                }
            }
            if best_end == start {
                if cursor > 0.0 {
                    finish_row(&mut rows, &mut row_children, &mut cursor);
                    continue;
                }
                best_end = start + 1;
            }
            let top = cursor;
            let placed_h = place_balanced_run(
                &mut row_children,
                &items[start..best_end],
                top,
                best_end < run_end,
            );
            let used_h = placed_h.max(best_h);
            add_rules(&mut row_children, top, used_h);
            cursor += used_h;
            start = best_end;
            if start < run_end {
                finish_row(&mut rows, &mut row_children, &mut cursor);
            }
        }
    }
    finish_row(&mut rows, &mut row_children, &mut cursor);
    rows
}

/// One placed fragment of an item inside a column formatting context.
pub(super) struct ColumnFragment {
    /// Index into the run of the source item this fragment belongs to.
    pub(super) item: usize,
    /// Border-box top of the fragment, relative to the column content top (px).
    pub(super) y: f32,
    /// Border-box height of this fragment (px).
    pub(super) height: f32,
    /// Offset from the source item's border-box top to this fragment's top.
    /// Text continuation must be projected through this offset rather than
    /// repainting the source's first line in every column.
    pub(super) source_top: f32,
    /// This fragment contains the item's top edge (first slice → keep top border).
    pub(super) is_first: bool,
    /// This fragment contains the item's bottom edge (last slice → keep bottom
    /// border + the item's trailing margin extends below it within the column).
    pub(super) is_last: bool,
}

impl ColumnFragment {
    pub(super) fn placement(&self, inline_offset: f32, inline_size: f32) -> BoxFragmentPlacement {
        BoxFragmentPlacement {
            origin: Point::new(inline_offset, self.y),
            size: Size::new(inline_size, self.height),
            source: SourceBlockRange::fragment(self.source_top, self.height, self.is_last),
            edges: FragmentEdges {
                block_start: self.is_first,
                block_end: self.is_last,
            },
        }
    }
}

/// The source-coordinate interval owned by one box fragment.
///
/// The final fragment has no upper bound: content overflowing a definite
/// principal box remains associated with its final fragment instead of
/// enlarging the box or being duplicated in every preceding fragment.
#[derive(Clone, Copy)]
pub(super) struct SourceBlockRange {
    start: f32,
    end: Option<f32>,
}

impl SourceBlockRange {
    const fn fragment(start: f32, block_size: f32, is_last: bool) -> Self {
        Self {
            start,
            end: if is_last {
                None
            } else {
                Some(start + block_size)
            },
        }
    }

    #[cfg(test)]
    pub(super) const fn continuation(start: f32) -> Self {
        Self { start, end: None }
    }

    #[cfg(test)]
    pub(super) const fn bounded(start: f32, end: f32) -> Self {
        Self {
            start,
            end: Some(end),
        }
    }
}

#[derive(Clone, Copy)]
struct FragmentEdges {
    block_start: bool,
    block_end: bool,
}

/// Physical placement and source ownership of one principal-box fragment.
/// This replaces parallel scalar arguments that were easy to transpose or
/// partially update when adding another fragmentation path.
#[derive(Clone, Copy)]
pub(super) struct BoxFragmentPlacement {
    origin: Point,
    size: Size,
    source: SourceBlockRange,
    edges: FragmentEdges,
}

impl BoxFragmentPlacement {
    pub(super) const fn whole(origin: Point, size: Size) -> Self {
        Self {
            origin,
            size,
            source: SourceBlockRange {
                start: 0.0,
                end: None,
            },
            edges: FragmentEdges {
                block_start: true,
                block_end: true,
            },
        }
    }

    const fn is_whole(self) -> bool {
        self.edges.block_start && self.edges.block_end
    }

    const fn physical(self) -> FragmentPlacement {
        FragmentPlacement::in_content_box(Vector::new(self.origin.x, self.origin.y), self.size)
    }
}

#[derive(Clone, Copy)]
enum OverflowColumns {
    Fixed,
    Extend,
}

/// Physical fragmentainer geometry for a multicol run.
#[derive(Clone, Copy)]
pub(super) struct ColumnFragmentation {
    column_count: usize,
    block_size: f32,
    overflow: OverflowColumns,
    balance_probe: bool,
}

impl ColumnFragmentation {
    pub(super) const fn fixed(column_count: usize, block_size: f32) -> Self {
        Self {
            column_count,
            block_size,
            overflow: OverflowColumns::Fixed,
            balance_probe: false,
        }
    }

    pub(super) const fn overflowing(column_count: usize, block_size: f32) -> Self {
        Self {
            column_count,
            block_size,
            overflow: OverflowColumns::Extend,
            balance_probe: false,
        }
    }

    pub(super) const fn balance_probe(column_count: usize, block_size: f32) -> Self {
        Self {
            column_count,
            block_size,
            overflow: OverflowColumns::Extend,
            balance_probe: true,
        }
    }

    const fn allows_overflow(self) -> bool {
        matches!(self.overflow, OverflowColumns::Extend)
    }
}

/// Fragments and used block sizes for one multicol run.
pub(super) struct FragmentedColumns {
    pub(super) columns: Vec<Vec<ColumnFragment>>,
    pub(super) used_block_sizes: Vec<f32>,
}

impl FragmentedColumns {
    fn content_column_count(&self) -> usize {
        self.columns
            .iter()
            .rposition(|column| !column.is_empty())
            .map_or(0, |index| index + 1)
    }
}

/// Find the shortest fragmentainer that holds a sliceable run in the requested
/// number of balanced columns, then materialize those fragments.
///
/// Feasibility is not monotonic in the candidate block size: making a column
/// slightly taller can admit another whole block and temporarily produce more
/// overflow. Grow from the lower bound to the exact used overflow boundary
/// reported by sequential placement instead of applying a binary search to a
/// non-monotonic predicate.
pub(super) fn balance_fragmented_columns(
    items: &[MultiColItem],
    indices: &[usize],
    column_count: usize,
) -> FragmentedColumns {
    if indices.is_empty() || column_count <= 1 {
        let block_size = indices
            .iter()
            .map(|&index| items[index].fragmentation_height)
            .sum();
        return fragment_columns(
            items,
            indices,
            ColumnFragmentation::fixed(column_count.max(1), block_size),
        );
    }

    // A run made entirely of unconstrained `break-inside: avoid` boxes has no
    // legal internal break to discover. CSS Multicol establishes the shortest
    // column block-size first, then fills those columns sequentially; solve
    // that atomic partition exactly instead of feeding it through the
    // descendant-fragment probe. Besides doing needless work, the probe's
    // overflowing columns do not expose the next atomic packing threshold, so
    // using their current used size can skip directly to the one-column upper
    // bound.
    if let Some(block_size) = atomic_balanced_block_size(items, indices, column_count) {
        return fragment_columns(
            items,
            indices,
            ColumnFragmentation::fixed(column_count, block_size),
        );
    }

    // Positive trailing margins may be discarded at a fragmentation break, so
    // only border-box content is a sound lower bound for the balanced height.
    let content_size: f32 = indices
        .iter()
        .map(|&index| {
            (items[index].fragmentation_height - items[index].margin_bottom.max(0.0)).max(0.0)
        })
        .sum();
    let minimum_fragment_size = indices
        .iter()
        .map(|&index| item_minimum_fragment_size(&items[index]))
        .fold(0.0f32, f32::max);
    let total_size: f32 = indices
        .iter()
        .map(|&index| items[index].fragmentation_height.max(0.0))
        .sum();
    if content_size <= 0.0 || !content_size.is_finite() || !total_size.is_finite() {
        return fragment_columns(
            items,
            indices,
            ColumnFragmentation::fixed(column_count, total_size.max(0.0)),
        );
    }

    let lower = (content_size / column_count as f32).max(minimum_fragment_size);
    let upper = total_size.max(lower);
    let mut block_size = lower;
    loop {
        let probe = fragment_columns(
            items,
            indices,
            ColumnFragmentation::balance_probe(column_count, block_size),
        );
        let used = probe
            .used_block_sizes
            .iter()
            .copied()
            .fold(0.0f32, f32::max);
        if probe.content_column_count() <= column_count && used <= block_size {
            break;
        }
        if block_size >= upper {
            // Forced breaks can require overflow columns at every block size.
            return fragment_columns(
                items,
                indices,
                ColumnFragmentation::overflowing(column_count, upper),
            );
        }
        block_size = if used > block_size {
            used.min(upper)
        } else {
            upper
        };
    }

    fragment_columns(
        items,
        indices,
        ColumnFragmentation::fixed(column_count, block_size),
    )
}

/// Resolve the shortest height for a run whose boxes must remain whole.
///
/// Sibling break controls need the full fragmentation solver, so this exact
/// atomic path is used only when `break-inside` is the run's sole constraint.
fn atomic_balanced_block_size(
    items: &[MultiColItem],
    indices: &[usize],
    column_count: usize,
) -> Option<f32> {
    let is_unconstrained_atomic_item = |item: &MultiColItem| {
        item.break_inside_avoid_column
            && !item.break_before_column
            && !item.break_after_column
            && !item.break_before_avoid_column
            && !item.break_after_avoid_column
    };
    if !indices
        .iter()
        .all(|&index| is_unconstrained_atomic_item(&items[index]))
    {
        return None;
    }

    let heights = indices
        .iter()
        .map(|&index| items[index].fragmentation_height.max(0.0))
        .collect::<Vec<_>>();
    let buckets = balance_columns(&heights, column_count);
    Some(
        buckets
            .iter()
            .map(|bucket| bucket.iter().map(|&position| heights[position]).sum())
            .fold(0.0f32, f32::max),
    )
}

/// Distribute items across a semantic column geometry, *fragmenting* a
/// block that crosses a column boundary (css-break-3 + css-multicol-1): the part
/// that fits stays at the bottom of column N, the remainder continues at the top
/// of column N+1. Each column is filled to its block size before the next starts.
///
/// A definite principal box fragments within its hard used extent; visible
/// descendant overflow stays associated with the final fragment. Auto and
/// minimum-sized boxes resolve against their natural descendant extent. The
/// trailing margin follows the principal box and is *truncated* if it would
/// cross the column bottom. Splitting follows `box-decoration-break: slice`:
/// the top slice keeps the top border, the bottom slice keeps the bottom border,
/// and cut edges keep neither.
///
pub(super) fn fragment_columns(
    items: &[MultiColItem],
    indices: &[usize],
    geometry: ColumnFragmentation,
) -> FragmentedColumns {
    let initial_cols = geometry.column_count.max(1);
    let mut cols: Vec<Vec<ColumnFragment>> = (0..initial_cols).map(|_| Vec::new()).collect();
    let mut used: Vec<f32> = vec![0.0; initial_cols];
    if geometry.column_count == 0 || geometry.block_size <= 0.0 {
        // Degenerate: pile everything into column 0 unfragmented.
        let mut y = 0.0f32;
        for &idx in indices {
            let h = (items[idx].fragmentation_height - items[idx].margin_bottom).max(0.0);
            cols[0].push(ColumnFragment {
                item: idx,
                y,
                height: h,
                source_top: 0.0,
                is_first: true,
                is_last: true,
            });
            y += items[idx].fragmentation_height;
        }
        used[0] = y;
        return FragmentedColumns {
            columns: cols,
            used_block_sizes: used,
        };
    }

    let mut col = 0usize;
    let mut y = 0.0f32; // border-box fill cursor within the current column
    let mut placed_in_current_col: Vec<usize> = Vec::new();

    let advance_column = |cols: &mut Vec<Vec<ColumnFragment>>,
                          used: &mut Vec<f32>,
                          col: &mut usize,
                          y: &mut f32,
                          placed_in_current_col: &mut Vec<usize>| {
        if *col + 1 < cols.len() {
            *col += 1;
        } else if geometry.allows_overflow() {
            cols.push(Vec::new());
            used.push(0.0);
            *col += 1;
        }
        *y = 0.0;
        placed_in_current_col.clear();
    };

    let mut groups = Vec::new();
    let mut group_start = 0usize;
    while group_start < indices.len() {
        let mut group_end = group_start + 1;
        while group_end < indices.len() {
            let prev = indices[group_end - 1];
            let next = indices[group_end];
            if items[prev].break_after_column || items[next].break_before_column {
                break;
            }
            if items[prev].break_after_avoid_column || items[next].break_before_avoid_column {
                group_end += 1;
            } else {
                break;
            }
        }
        groups.push(group_start..group_end);
        group_start = group_end;
    }

    for group in groups {
        let group_len = group.end - group.start;
        let group_height: f32 = group
            .clone()
            .map(|i| items[indices[i]].fragmentation_height)
            .sum();
        if group_len > 1
            && y > 0.0
            && exceeds_with_roundoff(y + group_height, geometry.block_size)
            && !exceeds_with_roundoff(group_height, geometry.block_size)
        {
            advance_column(
                &mut cols,
                &mut used,
                &mut col,
                &mut y,
                &mut placed_in_current_col,
            );
        }

        for group_pos in group {
            let idx = indices[group_pos];
            if items[idx].break_before_column && (y > 0.0 || !placed_in_current_col.is_empty()) {
                advance_column(
                    &mut cols,
                    &mut used,
                    &mut col,
                    &mut y,
                    &mut placed_in_current_col,
                );
            }

            place_fragmented_item(
                items,
                idx,
                &mut cols,
                &mut used,
                &mut col,
                &mut y,
                geometry,
                &mut placed_in_current_col,
            );

            if items[idx].break_after_column {
                advance_column(
                    &mut cols,
                    &mut used,
                    &mut col,
                    &mut y,
                    &mut placed_in_current_col,
                );
            }
        }
    }
    FragmentedColumns {
        columns: cols,
        used_block_sizes: used,
    }
}

#[allow(clippy::too_many_arguments)]
fn place_fragmented_item(
    items: &[MultiColItem],
    idx: usize,
    cols: &mut Vec<Vec<ColumnFragment>>,
    used: &mut Vec<f32>,
    col: &mut usize,
    y: &mut f32,
    geometry: ColumnFragmentation,
    placed_in_current_col: &mut Vec<usize>,
) {
    let advance_column = |cols: &mut Vec<Vec<ColumnFragment>>,
                          used: &mut Vec<f32>,
                          col: &mut usize,
                          y: &mut f32,
                          placed_in_current_col: &mut Vec<usize>| {
        if *col + 1 < cols.len() {
            *col += 1;
        } else if geometry.allows_overflow() {
            cols.push(Vec::new());
            used.push(0.0);
            *col += 1;
        }
        *y = 0.0;
        placed_in_current_col.clear();
    };

    let margin = items[idx].margin_bottom.max(0.0);
    let box_h = (items[idx].fragmentation_height - margin).max(0.0);
    let prefer_block_boundaries =
        geometry.balance_probe && !exceeds_with_roundoff(box_h, geometry.block_size);
    let balance_query = |query: FragmentBreakQuery| {
        if prefer_block_boundaries {
            query.block_boundaries_only()
        } else {
            query
        }
    };
    let space_before_break = geometry.block_size - *y;
    let minimum_fragment_size = item_minimum_fragment_size(&items[idx]);
    let normal_break_here = item_block_break(
        &items[idx],
        balance_query(FragmentBreakQuery::latest_before(
            0.0,
            space_before_break,
            FragmentBreakRule::Normal,
        )),
    );
    let normal_break_in_empty_column = item_block_break(
        &items[idx],
        balance_query(FragmentBreakQuery::latest_before(
            0.0,
            geometry.block_size,
            FragmentBreakRule::Normal,
        )),
    );
    let should_move_intact = *y > 0.0
        && exceeds_with_roundoff(*y + box_h, geometry.block_size)
        && ((items[idx].break_inside_avoid_column
            && !exceeds_with_roundoff(box_h, geometry.block_size))
            || (!geometry.balance_probe
                && normal_break_here.is_none()
                && (exceeds_with_roundoff(minimum_fragment_size, space_before_break)
                    || normal_break_in_empty_column.is_some())));
    if should_move_intact {
        advance_column(cols, used, col, y, placed_in_current_col);
    }
    let mut remaining = box_h;
    let mut source_top = 0.0;
    let mut first_slice = true;
    // Place the box, splitting across columns as needed.
    loop {
        let space = geometry.block_size - *y;
        // Start a new column when the current one is full and this is not the
        // last fixed column (or overflow columns are allowed).
        if !is_positive_with_roundoff(space)
            && (*col + 1 < cols.len() || geometry.allows_overflow())
        {
            advance_column(cols, used, col, y, placed_in_current_col);
            continue;
        }
        let space = geometry.block_size - *y;
        let fixed_last_col = !geometry.allows_overflow() && *col + 1 >= cols.len();
        let take = if fixed_last_col {
            remaining
        } else if exceeds_with_roundoff(remaining, space) {
            let limit = source_top + space.max(0.0);
            let normal_break = item_block_break(
                &items[idx],
                balance_query(FragmentBreakQuery::latest_before(
                    source_top,
                    limit,
                    FragmentBreakRule::Normal,
                )),
            );
            let normal_break_in_empty_column = item_block_break(
                &items[idx],
                balance_query(FragmentBreakQuery::latest_before(
                    source_top,
                    source_top + geometry.block_size,
                    FragmentBreakRule::Normal,
                )),
            );
            normal_break
                .or_else(|| {
                    if geometry.balance_probe {
                        return item_block_break(
                            &items[idx],
                            balance_query(FragmentBreakQuery::earliest_after(
                                source_top,
                                limit,
                                FragmentBreakRule::Normal,
                            )),
                        )
                        .or_else(|| {
                            item_block_break(
                                &items[idx],
                                balance_query(FragmentBreakQuery::earliest_after(
                                    source_top,
                                    limit,
                                    FragmentBreakRule::Emergency,
                                )),
                            )
                        });
                    }
                    if normal_break_in_empty_column.is_none() {
                        item_block_break(
                            &items[idx],
                            FragmentBreakQuery::latest_before(
                                source_top,
                                limit,
                                FragmentBreakRule::Emergency,
                            ),
                        )
                    } else {
                        None
                    }
                })
                .map(|break_offset| break_offset - source_top)
                .unwrap_or_else(|| {
                    if geometry.balance_probe
                        && !exceeds_with_roundoff(remaining, geometry.block_size)
                    {
                        remaining
                    } else {
                        remaining.min(space.max(0.0))
                    }
                })
        } else {
            // The shared comparison has already established that the whole
            // remainder fits modulo short-sequence arithmetic roundoff. Keep
            // it whole; taking the raw `min` here would manufacture a tiny
            // continuation fragment and advance the following content.
            remaining
        };
        let is_last_slice = equal_with_roundoff(remaining, take) || fixed_last_col;
        cols[*col].push(ColumnFragment {
            item: idx,
            y: *y,
            height: take,
            source_top,
            is_first: first_slice,
            is_last: is_last_slice,
        });
        *y += take;
        used[*col] = used[*col].max(*y);
        remaining -= take;
        source_top += take;
        first_slice = false;
        if !is_positive_with_roundoff(remaining) || fixed_last_col {
            break;
        }
        // Box continues in the next column.
        advance_column(cols, used, col, y, placed_in_current_col);
    }

    placed_in_current_col.push(idx);
    // Trailing margin follows the box: kept if it fits in the column, else
    // truncated at the fragmentation break (do not carry it to the next col).
    if margin > 0.0 {
        let fixed_last_col = !geometry.allows_overflow() && *col + 1 >= cols.len();
        if !exceeds_with_roundoff(*y + margin, geometry.block_size) || fixed_last_col {
            *y += margin;
            used[*col] = used[*col].max(*y);
        } else {
            // Margin adjoins the column break → truncated; next item starts a
            // fresh column at the top.
            advance_column(cols, used, col, y, placed_in_current_col);
        }
    }
}

fn item_block_break(item: &MultiColItem, query: FragmentBreakQuery) -> Option<f32> {
    let [element] = item.elements.as_slice() else {
        return None;
    };
    element
        .block_fragmentation_source()?
        .find_block_break(query)
}

/// Build one anonymous column fragmentainer at a content-box-local placement.
pub(super) fn make_column_container(
    kids: Vec<LayoutNode>,
    column_index: usize,
    off_left: f32,
    off_top: f32,
    width: f32,
    height: f32,
) -> LayoutNode {
    let principal = empty_container_value(kids, width, height, None);
    MulticolColumn::new(
        principal,
        column_index,
        FragmentPlacement::in_padding_box(Vector::new(off_left, off_top), Size::new(width, height)),
    )
    .boxed()
}

/// Smallest block-size at which the item's next unbreakable visual unit fits.
/// Text breaks between lines, while an unsupported descendant layout remains
/// atomic. A break-inside avoidance request keeps the whole item together.
pub(super) fn item_minimum_fragment_size(item: &MultiColItem) -> f32 {
    if item.break_inside_avoid_column {
        return (item.fragmentation_height - item.margin_bottom.max(0.0)).max(0.0);
    }

    struct MinimumFragmentSize(f32);

    impl LayoutVisitor for MinimumFragmentSize {
        fn visit_text_block(&mut self, element: &TextBlock) {
            let line_size = element
                .lines
                .iter()
                .map(|line| line.height)
                .fold(0.0f32, f32::max);
            self.0 = element.box_model.border.top.width + element.box_model.padding.top + line_size;
        }

        fn visit_container(&mut self, element: &Container) {
            let child_size = element
                .children
                .iter()
                .map(|child| minimum_fragment_size(child.as_ref()))
                .fold(0.0f32, f32::max);
            self.0 =
                element.box_model.border.top.width + element.box_model.padding.top + child_size;
        }
    }

    fn minimum_fragment_size(element: &dyn LayoutElement) -> f32 {
        let mut minimum =
            MinimumFragmentSize(crate::layout::paginate::estimate_element_height(element).max(0.0));
        element.accept(&mut minimum);
        minimum.0
    }

    let [element] = item.elements.as_slice() else {
        return item.fragmentation_height.max(0.0);
    };
    minimum_fragment_size(element.as_ref())
}

/// Whether an item is a single block box (one Container or TextBlock) that the
/// `column-fill: auto` fragmenter can geometrically slice across a column break.
/// Anything else (multiple flattened elements, images, tables, …) is treated as
/// atomic and never split.
pub(super) fn item_is_splittable(item: &MultiColItem) -> bool {
    let [element] = item.elements.as_slice() else {
        return false;
    };
    element.block_fragmentation_source().is_some()
}

/// Remove source text lines assigned to an earlier fragment, retaining a
/// leading blank line when the next source line begins below the fragment edge.
/// Fragment selection supplies legal line boundaries; if independent layout
/// sums leave the edge microscopically inside a line box, ownership stays with
/// the preceding fragment so the continuation can never duplicate that line.
pub(super) fn project_text_lines_into_fragment(
    lines: &mut Vec<TextLine>,
    source_content_top: f32,
    source: SourceBlockRange,
) {
    let source_lines = std::mem::take(lines);
    let mut projected = Vec::with_capacity(source_lines.len() + 1);
    let mut line_top = source_content_top;
    let mut inserted_leading_gap = false;
    for line in source_lines {
        let line_height = line.height;
        let starts_before_end = source
            .end
            .is_none_or(|end| exceeds_with_roundoff(end, line_top));
        if starts_before_end && !exceeds_with_roundoff(source.start, line_top) {
            if !inserted_leading_gap && is_positive_with_roundoff(line_top - source.start) {
                projected.push(TextLine {
                    height: line_top - source.start,
                    ..Default::default()
                });
                inserted_leading_gap = true;
            }
            projected.push(line);
        }
        line_top += line_height;
    }
    *lines = projected;
}

/// Project a cloned container subtree to the source block offset represented by
/// a continuation fragment. Fully consumed children are removed; the first
/// partially consumed child is recursively projected. This preserves document
/// order without repainting the first fragment's descendants in every column.
fn project_container_children(container: &mut Container, source: SourceBlockRange) {
    let source_content_top = container.box_model.border.top.width + container.box_model.padding.top;
    let mut child_top = source_content_top;
    container.children.retain_mut(|child| {
        let child_size = child
            .fragmentable_outer_block_extent()
            .unwrap_or_else(|| crate::layout::paginate::estimate_element_height(child.as_ref()));
        let child_bottom = child_top + child_size;
        let consumed = !exceeds_with_roundoff(child_bottom, source.start);
        let follows_fragment = source
            .end
            .is_some_and(|end| !exceeds_with_roundoff(end, child_top));
        let keep = !consumed && !follows_fragment;
        if keep {
            project_fragment_subtree(
                child.as_mut(),
                SourceBlockRange {
                    start: (source.start - child_top).max(0.0),
                    end: source.end.map(|end| (end - child_top).max(0.0)),
                },
            );
        }
        child_top = child_bottom;
        keep
    });
}

fn project_fragment_subtree(element: &mut dyn LayoutElement, source: SourceBlockRange) {
    struct FragmentSubtreeProjector {
        source: SourceBlockRange,
    }

    impl LayoutVisitorMut for FragmentSubtreeProjector {
        fn visit_text_block(&mut self, element: &mut TextBlock) {
            project_text_lines_into_fragment(
                &mut element.lines,
                element.box_model.border.top.width + element.box_model.padding.top,
                self.source,
            );
        }

        fn visit_container(&mut self, element: &mut Container) {
            project_container_children(element, self.source);
        }
    }

    element.accept_mut(&mut FragmentSubtreeProjector { source });
}

/// Build one positioned fragment box for a `column-fill: auto` slice of an item.
///
/// Clones the item's single block element, projects its text through the source
/// fragment offset, retains its fragmentainer-local placement independently of
/// authored positioning, forces its border-box height to the slice height, and
/// applies `box-decoration-break: slice` borders. The projected subtree assigns
/// content to exactly one fragment; authored visible overflow from the final
/// fragment remains visible.
pub(super) fn make_fragment_box(
    src: &dyn LayoutElement,
    placement: BoxFragmentPlacement,
) -> LayoutNode {
    if placement.is_whole() {
        if let Some(wrapped) = make_whole_text_fragment(src, placement.physical()) {
            return wrapped;
        }
        return retain_whole_fragment(src, placement.physical());
    }
    struct FragmentProjector {
        placement: BoxFragmentPlacement,
    }

    impl LayoutVisitorMut for FragmentProjector {
        fn visit_container(&mut self, element: &mut Container) {
            project_container_children(element, self.placement.source);
            let border = &mut element.box_model.border;
            let padding = &mut element.box_model.padding;
            if !self.placement.edges.block_start {
                border.top = crate::layout::engine::LayoutBorderSide::default();
                padding.top = 0.0;
            }
            if !self.placement.edges.block_end {
                border.bottom = crate::layout::engine::LayoutBorderSide::default();
                padding.bottom = 0.0;
            }
            element.box_model.size.height = BlockSize::definite(self.placement.size.height);
            element.box_model.margins = BlockMargins::ZERO;
        }

        fn visit_text_block(&mut self, element: &mut TextBlock) {
            let border = &mut element.box_model.border;
            let padding = &mut element.box_model.padding;
            project_text_lines_into_fragment(
                &mut element.lines,
                border.top.width + padding.top,
                self.placement.source,
            );
            if !self.placement.edges.block_start {
                border.top = crate::layout::engine::LayoutBorderSide::default();
                padding.top = 0.0;
            }
            if !self.placement.edges.block_end {
                border.bottom = crate::layout::engine::LayoutBorderSide::default();
                padding.bottom = 0.0;
            }
            element.box_model.size.height = BlockSize::definite(fragment_content_height(
                self.placement.size.height,
                border.vertical_width(),
                padding.vertical(),
            ));
            element.box_model.margins = BlockMargins::ZERO;
            element.clipping.rect = None;
        }
    }

    let mut element = src.clone_box();
    element.accept_mut(&mut FragmentProjector { placement });
    FragmentBox::new(element, placement.physical()).boxed()
}

/// Retain an unsliced item in its column without replacing its computed size.
/// Intrinsic, min/max, and overflow widths remain properties of the source box;
/// the column fragmentainer constrains placement, not the box itself.
fn retain_whole_fragment(src: &dyn LayoutElement, placement: FragmentPlacement) -> LayoutNode {
    struct WholeFragment;

    impl LayoutVisitorMut for WholeFragment {
        fn visit_container(&mut self, element: &mut Container) {
            element.box_model.margins = BlockMargins::ZERO;
        }

        fn visit_text_block(&mut self, element: &mut TextBlock) {
            element.box_model.margins = BlockMargins::ZERO;
            element.clipping.rect = None;
        }
    }

    let mut element = src.clone_box();
    element.accept_mut(&mut WholeFragment);
    FragmentBox::new(element, placement).boxed()
}

fn make_whole_text_fragment(
    src: &dyn LayoutElement,
    placement: FragmentPlacement,
) -> Option<LayoutNode> {
    struct WholeTextFragment {
        placement: FragmentPlacement,
        result: Option<LayoutNode>,
    }

    impl LayoutVisitor for WholeTextFragment {
        fn visit_text_block(&mut self, element: &TextBlock) {
            let Some(background) = element.paint.background.color else {
                return;
            };
            if element.box_model.border.has_any()
                || element.box_model.padding != crate::types::EdgeSizes::ZERO
            {
                return;
            }
            let mut text = element.clone();
            text.paint.background.color = None;
            text.box_model.margins = BlockMargins::ZERO;
            text.positioning.scheme = Position::Static;
            text.positioning.insets = crate::types::EdgeSizes::ZERO;
            text.clipping.rect = None;
            let principal = Container {
                children: vec![text.boxed()],
                box_model: BoxModel {
                    size: LayoutSize::fixed(
                        self.placement.size.width,
                        Some(self.placement.size.height),
                    ),
                    ..Default::default()
                },
                paint: BoxPaint {
                    background: crate::layout::elements::BackgroundPaint {
                        color: Some(background),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            };
            self.result = Some(FragmentBox::new(principal.boxed(), self.placement).boxed());
        }
    }

    let mut visitor = WholeTextFragment {
        placement,
        result: None,
    };
    src.accept(&mut visitor);
    visitor.result
}

fn fragment_content_height(border_box_h: f32, border_v: f32, padding_v: f32) -> f32 {
    (border_box_h - border_v - padding_v).max(0.0)
}

/// Build a full-width band (for `column-span: all`) at the current cursor.
pub(super) fn make_band_container(
    kids: Vec<LayoutNode>,
    off_left: f32,
    off_top: f32,
    width: f32,
    height: f32,
) -> LayoutNode {
    FragmentBox::new(
        empty_container_value(kids, width, height, None).boxed(),
        FragmentPlacement::in_padding_box(Vector::new(off_left, off_top), Size::new(width, height)),
    )
    .boxed()
}

/// Build a semantically identified rule spanning a column gap.
pub(super) fn make_rule_container(
    gap_after: usize,
    off_left: f32,
    off_top: f32,
    width: f32,
    height: f32,
    color: crate::types::Color,
    rule_style: BorderStyle,
) -> LayoutNode {
    ColumnRule {
        gap_after,
        placement: FragmentPlacement::in_padding_box(
            Vector::new(off_left, off_top),
            Size::new(width, height),
        ),
        height,
        paint: LayoutBorderSide {
            width,
            color,
            style: rule_style,
            ..Default::default()
        },
    }
    .boxed()
}

fn empty_container_value(
    kids: Vec<LayoutNode>,
    width: f32,
    height: f32,
    bg: Option<crate::types::Color>,
) -> Container {
    Container {
        children: kids,
        box_model: BoxModel {
            size: LayoutSize::fixed(width, Some(height)),
            ..Default::default()
        },
        paint: BoxPaint {
            background: crate::layout::elements::BackgroundPaint {
                color: bg,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

pub(super) fn empty_flow_anchor() -> LayoutNode {
    Container {
        box_model: BoxModel {
            size: LayoutSize::fixed(0.0, Some(0.0)),
            ..Default::default()
        },
        paint: BoxPaint {
            visible: false,
            ..Default::default()
        },
        ..Default::default()
    }
    .boxed()
}
