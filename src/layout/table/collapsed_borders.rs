use super::{query_table_row, update_table_row};
use crate::layout::cells::CollapsedBorderSegment;
#[cfg(test)]
use crate::layout::cells::TableCell;
use crate::layout::elements::{LayoutNode, TableCells};
use crate::layout::engine::{LayoutBorder, LayoutBorderSide};
use crate::style::computed::BorderStyle;
use crate::types::PhysicalSide;

/// CSS table box that supplied one collapsed-border candidate. Declaration
/// order is also the CSS tie-break order from least to most specific.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum CollapsedBorderOrigin {
    #[default]
    Table,
    ColumnGroup,
    Column,
    RowGroup,
    Row,
    Cell,
}

/// One authored border side together with its table-box origin.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct BorderCandidate {
    pub(super) side: LayoutBorderSide,
    pub(super) origin: CollapsedBorderOrigin,
}

/// Border declarations supplied by one row/column track and its containing
/// track group. Group sides that do not bound this track are already zeroed.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct CollapsedBorderTrack {
    pub(super) border: LayoutBorder,
    pub(super) group_border: LayoutBorder,
}

impl CollapsedBorderTrack {
    pub(super) fn row(
        border: LayoutBorder,
        group_border: Option<LayoutBorder>,
        index_in_group: usize,
        group_size: usize,
    ) -> Self {
        let mut group_border = group_border.unwrap_or_default();
        if index_in_group != 0 {
            group_border.top = LayoutBorderSide::default();
        }
        if index_in_group.saturating_add(1) < group_size {
            group_border.bottom = LayoutBorderSide::default();
        }
        Self {
            border,
            group_border,
        }
    }
}

/// All authored sources needed to resolve one collapsed table. Keeping the
/// track declarations together lets spanning cells harmonize against every row
/// and column they cover instead of freezing an incomplete per-cell snapshot.
#[derive(Debug, Clone, Default)]
pub(super) struct CollapsedBorderSources {
    pub(super) table: LayoutBorder,
    pub(super) rows: Vec<CollapsedBorderTrack>,
    pub(super) columns: Vec<CollapsedBorderTrack>,
}

impl CollapsedBorderSources {
    pub(super) fn new(
        table: LayoutBorder,
        columns: impl IntoIterator<Item = CollapsedBorderTrack>,
        direction_rtl: bool,
    ) -> Self {
        let mut columns = columns.into_iter().collect::<Vec<_>>();
        if direction_rtl {
            columns.reverse();
        }
        Self {
            table,
            rows: Vec::new(),
            columns,
        }
    }

    pub(super) fn push_row(&mut self, row: CollapsedBorderTrack) {
        self.rows.push(row);
    }
}

pub(super) fn collapsed_style_rank(style: BorderStyle) -> u8 {
    match style {
        BorderStyle::Double => 8,
        BorderStyle::Solid => 7,
        BorderStyle::Dashed => 6,
        BorderStyle::Dotted => 5,
        BorderStyle::Ridge => 4,
        BorderStyle::Outset => 3,
        BorderStyle::Groove => 2,
        BorderStyle::Inset => 1,
        BorderStyle::Hidden | BorderStyle::None => 0,
    }
}

pub(super) fn collapsed_border_winner(
    first: BorderCandidate,
    second: BorderCandidate,
) -> Option<usize> {
    match (first.side.style, second.side.style) {
        (BorderStyle::Hidden, _) => return Some(0),
        (_, BorderStyle::Hidden) => return Some(1),
        _ => {}
    }
    match (first.side.paints(), second.side.paints()) {
        (false, false) => return None,
        (true, false) => return Some(0),
        (false, true) => return Some(1),
        (true, true) => {}
    }
    match first
        .side
        .width
        .partial_cmp(&second.side.width)
        .unwrap_or(std::cmp::Ordering::Equal)
    {
        std::cmp::Ordering::Greater => Some(0),
        std::cmp::Ordering::Less => Some(1),
        std::cmp::Ordering::Equal => {
            let first_rank = collapsed_style_rank(first.side.style);
            let second_rank = collapsed_style_rank(second.side.style);
            match first_rank.cmp(&second_rank) {
                std::cmp::Ordering::Greater => Some(0),
                std::cmp::Ordering::Less => Some(1),
                std::cmp::Ordering::Equal => {
                    if first.origin >= second.origin {
                        Some(0)
                    } else {
                        Some(1)
                    }
                }
            }
        }
    }
}

fn harmonize_candidates(candidates: impl IntoIterator<Item = BorderCandidate>) -> BorderCandidate {
    candidates
        .into_iter()
        .reduce(
            |winner, candidate| match collapsed_border_winner(winner, candidate) {
                Some(1) => candidate,
                _ => winner,
            },
        )
        .unwrap_or_default()
}

fn resolved_side(candidate: BorderCandidate) -> LayoutBorderSide {
    if candidate.side.style == BorderStyle::Hidden {
        LayoutBorderSide::default()
    } else {
        candidate.side
    }
}

#[cfg(test)]
pub(super) fn apply_table_winning_side(
    cell: &mut TableCell,
    side: PhysicalSide,
    table_side: LayoutBorderSide,
) {
    let cell_candidate = BorderCandidate {
        side: *cell.layout.box_model.border.get(side),
        origin: CollapsedBorderOrigin::Cell,
    };
    let table_candidate = BorderCandidate {
        side: table_side,
        origin: CollapsedBorderOrigin::Table,
    };
    let winner = match collapsed_border_winner(cell_candidate, table_candidate) {
        Some(0) => cell_candidate.side,
        Some(1) => table_candidate.side,
        _ => LayoutBorderSide::default(),
    };
    let winner = if winner.style == BorderStyle::Hidden {
        LayoutBorderSide::default()
    } else {
        winner
    };
    let old_inset = *cell.layout.box_model.border_insets.get(side);
    let new_inset = winner.width / 2.0;
    *cell.layout.box_model.border_insets.get_mut(side) = new_inset;
    *cell.layout.box_model.content_insets.get_mut(side) += new_inset - old_inset;
    *cell.layout.box_model.border.get_mut(side) = winner;
    *cell.table.collapsed_outer_edges.get_mut(side) = winner.paints();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GridEdgeAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CellId {
    row: usize,
    cell: usize,
}

#[derive(Debug, Clone, Copy)]
struct CellPlacement {
    id: CellId,
    row_start: usize,
    row_span: usize,
    column_start: usize,
    column_span: usize,
}

#[derive(Debug, Clone, Copy)]
struct GridBorderRun {
    axis: GridEdgeAxis,
    line: usize,
    track_start: usize,
    track_end: usize,
    owner: Option<CellId>,
    owner_side: PhysicalSide,
    candidate: BorderCandidate,
}

impl GridBorderRun {
    fn overlaps(self, other: Self) -> bool {
        self.axis == other.axis
            && self.line == other.line
            && self.track_start.max(other.track_start) < self.track_end.min(other.track_end)
    }

    fn covers(self, track: usize) -> bool {
        self.track_start <= track && track < self.track_end
    }
}

fn candidate_run(
    placement: CellPlacement,
    side: PhysicalSide,
    candidate: BorderCandidate,
) -> GridBorderRun {
    match side {
        PhysicalSide::Top => GridBorderRun {
            axis: GridEdgeAxis::Horizontal,
            line: placement.row_start,
            track_start: placement.column_start,
            track_end: placement.column_start + placement.column_span,
            owner: Some(placement.id),
            owner_side: side,
            candidate,
        },
        PhysicalSide::Right => GridBorderRun {
            axis: GridEdgeAxis::Vertical,
            line: placement.column_start + placement.column_span,
            track_start: placement.row_start,
            track_end: placement.row_start + placement.row_span,
            owner: Some(placement.id),
            owner_side: side,
            candidate,
        },
        PhysicalSide::Bottom => GridBorderRun {
            axis: GridEdgeAxis::Horizontal,
            line: placement.row_start + placement.row_span,
            track_start: placement.column_start,
            track_end: placement.column_start + placement.column_span,
            owner: Some(placement.id),
            owner_side: side,
            candidate,
        },
        PhysicalSide::Left => GridBorderRun {
            axis: GridEdgeAxis::Vertical,
            line: placement.column_start,
            track_start: placement.row_start,
            track_end: placement.row_start + placement.row_span,
            owner: Some(placement.id),
            owner_side: side,
            candidate,
        },
    }
}

fn harmonize_track_side(
    winner: BorderCandidate,
    track: CollapsedBorderTrack,
    side: PhysicalSide,
    track_origin: CollapsedBorderOrigin,
    group_origin: CollapsedBorderOrigin,
) -> BorderCandidate {
    harmonize_candidates([
        winner,
        BorderCandidate {
            side: *track.group_border.get(side),
            origin: group_origin,
        },
        BorderCandidate {
            side: *track.border.get(side),
            origin: track_origin,
        },
    ])
}

fn harmonize_track_range(
    mut winner: BorderCandidate,
    tracks: &[CollapsedBorderTrack],
    range: std::ops::Range<usize>,
    side: PhysicalSide,
    track_origin: CollapsedBorderOrigin,
    group_origin: CollapsedBorderOrigin,
    reverse: bool,
) -> BorderCandidate {
    if reverse {
        for index in range.rev() {
            if let Some(track) = tracks.get(index).copied() {
                winner = harmonize_track_side(winner, track, side, track_origin, group_origin);
            }
        }
    } else {
        for index in range {
            if let Some(track) = tracks.get(index).copied() {
                winner = harmonize_track_side(winner, track, side, track_origin, group_origin);
            }
        }
    }
    winner
}

fn cell_side_candidate(
    cell_border: LayoutBorder,
    placement: CellPlacement,
    side: PhysicalSide,
    sources: &CollapsedBorderSources,
    direction_rtl: bool,
) -> BorderCandidate {
    let mut winner = BorderCandidate {
        side: *cell_border.get(side),
        origin: CollapsedBorderOrigin::Cell,
    };
    let row_end = placement.row_start.saturating_add(placement.row_span);
    let column_end = placement.column_start.saturating_add(placement.column_span);

    match side {
        PhysicalSide::Top => {
            if let Some(track) = sources.rows.get(placement.row_start).copied() {
                winner = harmonize_track_side(
                    winner,
                    track,
                    side,
                    CollapsedBorderOrigin::Row,
                    CollapsedBorderOrigin::RowGroup,
                );
            }
            if placement.row_start == 0 {
                winner = harmonize_track_range(
                    winner,
                    &sources.columns,
                    placement.column_start..column_end,
                    side,
                    CollapsedBorderOrigin::Column,
                    CollapsedBorderOrigin::ColumnGroup,
                    direction_rtl,
                );
            }
        }
        PhysicalSide::Bottom => {
            if let Some(track) = row_end
                .checked_sub(1)
                .and_then(|index| sources.rows.get(index))
                .copied()
            {
                winner = harmonize_track_side(
                    winner,
                    track,
                    side,
                    CollapsedBorderOrigin::Row,
                    CollapsedBorderOrigin::RowGroup,
                );
            }
            if row_end >= sources.rows.len() {
                winner = harmonize_track_range(
                    winner,
                    &sources.columns,
                    placement.column_start..column_end,
                    side,
                    CollapsedBorderOrigin::Column,
                    CollapsedBorderOrigin::ColumnGroup,
                    direction_rtl,
                );
            }
        }
        PhysicalSide::Left => {
            if let Some(track) = sources.columns.get(placement.column_start).copied() {
                winner = harmonize_track_side(
                    winner,
                    track,
                    side,
                    CollapsedBorderOrigin::Column,
                    CollapsedBorderOrigin::ColumnGroup,
                );
            }
            if placement.column_start == 0 {
                winner = harmonize_track_range(
                    winner,
                    &sources.rows,
                    placement.row_start..row_end,
                    side,
                    CollapsedBorderOrigin::Row,
                    CollapsedBorderOrigin::RowGroup,
                    false,
                );
            }
        }
        PhysicalSide::Right => {
            if let Some(track) = column_end
                .checked_sub(1)
                .and_then(|index| sources.columns.get(index))
                .copied()
            {
                winner = harmonize_track_side(
                    winner,
                    track,
                    side,
                    CollapsedBorderOrigin::Column,
                    CollapsedBorderOrigin::ColumnGroup,
                );
            }
            if column_end >= sources.columns.len() {
                winner = harmonize_track_range(
                    winner,
                    &sources.rows,
                    placement.row_start..row_end,
                    side,
                    CollapsedBorderOrigin::Row,
                    CollapsedBorderOrigin::RowGroup,
                    false,
                );
            }
        }
    }
    winner
}

fn table_border_runs(
    border: LayoutBorder,
    row_count: usize,
    column_count: usize,
) -> [GridBorderRun; 4] {
    [
        GridBorderRun {
            axis: GridEdgeAxis::Horizontal,
            line: 0,
            track_start: 0,
            track_end: column_count,
            owner: None,
            owner_side: PhysicalSide::Top,
            candidate: BorderCandidate {
                side: border.top,
                origin: CollapsedBorderOrigin::Table,
            },
        },
        GridBorderRun {
            axis: GridEdgeAxis::Vertical,
            line: column_count,
            track_start: 0,
            track_end: row_count,
            owner: None,
            owner_side: PhysicalSide::Right,
            candidate: BorderCandidate {
                side: border.right,
                origin: CollapsedBorderOrigin::Table,
            },
        },
        GridBorderRun {
            axis: GridEdgeAxis::Horizontal,
            line: row_count,
            track_start: 0,
            track_end: column_count,
            owner: None,
            owner_side: PhysicalSide::Bottom,
            candidate: BorderCandidate {
                side: border.bottom,
                origin: CollapsedBorderOrigin::Table,
            },
        },
        GridBorderRun {
            axis: GridEdgeAxis::Vertical,
            line: 0,
            track_start: 0,
            track_end: row_count,
            owner: None,
            owner_side: PhysicalSide::Left,
            candidate: BorderCandidate {
                side: border.left,
                origin: CollapsedBorderOrigin::Table,
            },
        },
    ]
}

fn component_indices(runs: &[GridBorderRun], seed: usize, claimed: &mut [bool]) -> Vec<usize> {
    let Some(seed_run) = runs.get(seed).copied() else {
        return Vec::new();
    };
    let mut component = Vec::new();
    let mut pending = vec![seed];
    if let Some(value) = claimed.get_mut(seed) {
        *value = true;
    }
    while let Some(index) = pending.pop() {
        let Some(run) = runs.get(index).copied() else {
            continue;
        };
        component.push(index);
        for (candidate_index, candidate) in runs.iter().copied().enumerate() {
            if claimed.get(candidate_index).copied().unwrap_or(true)
                || candidate.axis != seed_run.axis
                || candidate.line != seed_run.line
                || !run.overlaps(candidate)
            {
                continue;
            }
            if let Some(value) = claimed.get_mut(candidate_index) {
                *value = true;
                pending.push(candidate_index);
            }
        }
    }
    component
}

fn placement_for(placements: &[CellPlacement], id: CellId) -> Option<CellPlacement> {
    placements
        .iter()
        .copied()
        .find(|placement| placement.id == id)
}

fn component_order(
    run: GridBorderRun,
    placements: &[CellPlacement],
    direction_rtl: bool,
) -> (usize, usize, usize) {
    let Some(owner) = run.owner else {
        return (usize::MAX, usize::MAX, usize::MAX);
    };
    let Some(placement) = placement_for(placements, owner) else {
        return (usize::MAX - 1, usize::MAX, usize::MAX);
    };
    let column_order = if direction_rtl {
        usize::MAX - placement.column_start
    } else {
        placement.column_start
    };
    (placement.row_start, column_order, owner.cell)
}

fn preferred_paint_owner(
    runs: &[GridBorderRun],
    component: &[usize],
    axis: GridEdgeAxis,
    track: usize,
) -> Option<GridBorderRun> {
    let preferred_side = match axis {
        GridEdgeAxis::Horizontal => PhysicalSide::Top,
        GridEdgeAxis::Vertical => PhysicalSide::Left,
    };
    component
        .iter()
        .filter_map(|index| runs.get(*index).copied())
        .filter(|run| run.owner.is_some() && run.covers(track))
        .find(|run| run.owner_side == preferred_side)
        .or_else(|| {
            component
                .iter()
                .filter_map(|index| runs.get(*index).copied())
                .find(|run| run.owner.is_some() && run.covers(track))
        })
}

fn push_resolved_segment(
    cells: &mut [TableCells],
    placements: &[CellPlacement],
    owner_run: GridBorderRun,
    track: usize,
    side: LayoutBorderSide,
    outer: bool,
) {
    let Some(owner) = owner_run.owner else {
        return;
    };
    let Some(placement) = placement_for(placements, owner) else {
        return;
    };
    let track_offset = match owner_run.axis {
        GridEdgeAxis::Horizontal => track.saturating_sub(placement.column_start),
        GridEdgeAxis::Vertical => track.saturating_sub(placement.row_start),
    };
    let Some(cell) = cells
        .get_mut(owner.row)
        .and_then(|row| row.cells.get_mut(owner.cell))
    else {
        return;
    };
    let segments = cell.table.collapsed_segments.get_mut(owner_run.owner_side);
    if let Some(previous) = segments.last_mut()
        && previous.track_offset + previous.track_span == track_offset
        && previous.side.same_paint(&side)
    {
        previous.track_span += 1;
    } else {
        segments.push(CollapsedBorderSegment {
            track_offset,
            track_span: 1,
            side,
        });
    }
    if outer && side.paints() {
        *cell
            .table
            .collapsed_outer_edges
            .get_mut(owner_run.owner_side) = true;
    }
}

fn apply_component_geometry(
    cells: &mut [TableCells],
    runs: &[GridBorderRun],
    component: &[usize],
    side: LayoutBorderSide,
) {
    for run in component
        .iter()
        .filter_map(|index| runs.get(*index).copied())
    {
        let Some(owner) = run.owner else {
            continue;
        };
        let Some(cell) = cells
            .get_mut(owner.row)
            .and_then(|row| row.cells.get_mut(owner.cell))
        else {
            continue;
        };
        let used = cell.layout.box_model.border.get_mut(run.owner_side);
        if side.width > used.width {
            *used = side;
        }
    }
}

fn normalize_resolved_cell_geometry(cells: &mut [TableCells]) {
    for cell in cells.iter_mut().flat_map(|row| row.cells.iter_mut()) {
        if cell.span.rows == 0 {
            continue;
        }
        cell.table.collapsed_resolution_complete = true;
        for side in [
            PhysicalSide::Top,
            PhysicalSide::Right,
            PhysicalSide::Bottom,
            PhysicalSide::Left,
        ] {
            let painted = cell
                .table
                .collapsed_segments
                .get(side)
                .iter()
                .map(|segment| segment.side)
                .max_by(|first, second| {
                    first
                        .width
                        .partial_cmp(&second.width)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap_or_default();
            let used = *cell.layout.box_model.border.get(side);
            let representative = if painted.width > used.width {
                painted
            } else {
                used
            };
            let old_inset = *cell.layout.box_model.border_insets.get(side);
            let new_inset = representative.width / 2.0;
            *cell.layout.box_model.border_insets.get_mut(side) = new_inset;
            *cell.layout.box_model.content_insets.get_mut(side) += new_inset - old_inset;
            *cell.layout.box_model.border.get_mut(side) = representative;
        }
    }
}

/// Resolve every shared table grid edge once, including rowspans and colspans.
/// The result is stored as track-relative segments on the cell that owns the
/// single painted copy of each edge section.
pub(super) fn resolve_collapsed_border_grid(
    rows: &mut [LayoutNode],
    sources: &CollapsedBorderSources,
    direction_rtl: bool,
) -> crate::types::EdgeSizes {
    let mut row_nodes = Vec::new();
    let mut cells = Vec::new();
    for (node_index, node) in rows.iter().enumerate() {
        if let Some(content) = query_table_row(node.as_ref(), |row| row.content.clone()) {
            row_nodes.push(node_index);
            cells.push(content);
        }
    }
    if cells.is_empty() {
        return crate::types::EdgeSizes::ZERO;
    }
    let row_count = cells.len();
    let column_count = cells
        .iter()
        .map(|row| row.column_widths.len())
        .max()
        .unwrap_or(0);
    if column_count == 0 {
        return crate::types::EdgeSizes::ZERO;
    }

    let mut placements = Vec::new();
    for (row_index, row) in cells.iter().enumerate() {
        let mut column_start = 0usize;
        for (cell_index, cell) in row.cells.iter().enumerate() {
            let column_span = cell.span.columns.max(1);
            if cell.span.rows > 0 {
                placements.push(CellPlacement {
                    id: CellId {
                        row: row_index,
                        cell: cell_index,
                    },
                    row_start: row_index,
                    row_span: cell.span.rows.min(row_count - row_index),
                    column_start,
                    column_span: column_span.min(column_count.saturating_sub(column_start)),
                });
            }
            column_start = column_start.saturating_add(column_span);
        }
    }

    let mut runs = Vec::new();
    for placement in placements.iter().copied() {
        let Some(cell) = cells
            .get(placement.id.row)
            .and_then(|row| row.cells.get(placement.id.cell))
        else {
            continue;
        };
        for side in [
            PhysicalSide::Top,
            PhysicalSide::Right,
            PhysicalSide::Bottom,
            PhysicalSide::Left,
        ] {
            runs.push(candidate_run(
                placement,
                side,
                cell_side_candidate(
                    cell.layout.box_model.border,
                    placement,
                    side,
                    sources,
                    direction_rtl,
                ),
            ));
        }
    }
    runs.extend(table_border_runs(sources.table, row_count, column_count));
    for cell in cells.iter_mut().flat_map(|row| row.cells.iter_mut()) {
        if cell.span.rows == 0 {
            continue;
        }
        cell.layout.box_model.border = LayoutBorder::default();
        cell.table.collapsed_segments = Default::default();
        cell.table.collapsed_outer_edges = Default::default();
    }

    // Table-root runs participate in every covered outer section, but they do
    // not connect neighboring cell sections into one conflict component. If
    // they did, a winner on the first cell could incorrectly propagate across
    // the entire table edge.
    let mut claimed = runs
        .iter()
        .map(|run| run.owner.is_none())
        .collect::<Vec<_>>();
    for seed in 0..runs.len() {
        if claimed.get(seed).copied().unwrap_or(true) {
            continue;
        }
        let mut component = component_indices(&runs, seed, &mut claimed);
        component.sort_by_key(|index| {
            runs.get(*index)
                .copied()
                .map(|run| component_order(run, &placements, direction_rtl))
                .unwrap_or((usize::MAX, usize::MAX, usize::MAX))
        });
        let Some(first_run) = component
            .first()
            .and_then(|index| runs.get(*index))
            .copied()
        else {
            continue;
        };
        let passive_candidates = runs.iter().filter(|run| {
            run.owner.is_none()
                && run.axis == first_run.axis
                && run.line == first_run.line
                && component.iter().any(|index| {
                    runs.get(*index)
                        .is_some_and(|component_run| component_run.overlaps(**run))
                })
        });
        let winner = harmonize_candidates(
            component
                .iter()
                .filter_map(|index| runs.get(*index).map(|run| run.candidate))
                .chain(passive_candidates.map(|run| run.candidate)),
        );
        let side = resolved_side(winner);
        let track_start = component
            .iter()
            .filter_map(|index| runs.get(*index).map(|run| run.track_start))
            .min()
            .unwrap_or(0);
        let track_end = component
            .iter()
            .filter_map(|index| runs.get(*index).map(|run| run.track_end))
            .max()
            .unwrap_or(track_start);
        let outer = match first_run.axis {
            GridEdgeAxis::Horizontal => first_run.line == 0 || first_run.line == row_count,
            GridEdgeAxis::Vertical => first_run.line == 0 || first_run.line == column_count,
        };
        apply_component_geometry(&mut cells, &runs, &component, side);
        if !side.paints() {
            continue;
        }
        for track in track_start..track_end {
            if let Some(owner) = preferred_paint_owner(&runs, &component, first_run.axis, track) {
                push_resolved_segment(&mut cells, &placements, owner, track, side, outer);
            }
        }
    }
    normalize_resolved_cell_geometry(&mut cells);

    let first_row = cells.first();
    let last_row = cells.last();
    let top = first_row
        .into_iter()
        .flat_map(|row| row.cells.iter())
        .filter(|cell| cell.span.rows != 0)
        .map(|cell| cell.layout.box_model.border.top.width / 2.0)
        .fold(0.0, f32::max);
    let bottom = last_row
        .into_iter()
        .flat_map(|row| row.cells.iter())
        .filter(|cell| cell.span.rows != 0)
        .map(|cell| cell.layout.box_model.border.bottom.width / 2.0)
        .fold(0.0, f32::max);
    let left = cells
        .iter()
        .filter_map(|row| row.cells.iter().find(|cell| cell.span.rows != 0))
        .map(|cell| cell.layout.box_model.border.left.width / 2.0)
        .fold(0.0, f32::max);
    let right = cells
        .iter()
        .filter_map(|row| row.cells.iter().rev().find(|cell| cell.span.rows != 0))
        .map(|cell| cell.layout.box_model.border.right.width / 2.0)
        .fold(0.0, f32::max);
    let outer_insets = crate::types::EdgeSizes::new(top, right, bottom, left);

    for (logical_row, (node_index, content)) in row_nodes.into_iter().zip(cells).enumerate() {
        if let Some(node) = rows.get_mut(node_index) {
            update_table_row(node.as_mut(), |row| {
                row.content = content;
                if logical_row == 0 {
                    row.flow.internal.start = outer_insets.top;
                }
            });
        }
    }
    outer_insets
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: f32) -> LayoutBorderSide {
        LayoutBorderSide {
            width,
            style: BorderStyle::Solid,
            ..Default::default()
        }
    }

    #[test]
    fn rowspan_harmonizes_every_contiguous_row_origin() {
        let sources = CollapsedBorderSources {
            rows: vec![
                CollapsedBorderTrack {
                    border: LayoutBorder {
                        left: solid(2.0),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                CollapsedBorderTrack {
                    border: LayoutBorder {
                        left: solid(8.0),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ],
            columns: vec![CollapsedBorderTrack::default()],
            ..Default::default()
        };
        let placement = CellPlacement {
            id: CellId { row: 0, cell: 0 },
            row_start: 0,
            row_span: 2,
            column_start: 0,
            column_span: 1,
        };

        let candidate = cell_side_candidate(
            LayoutBorder::default(),
            placement,
            PhysicalSide::Left,
            &sources,
            false,
        );

        assert_eq!(candidate.side.width, 8.0);
        assert_eq!(candidate.origin, CollapsedBorderOrigin::Row);
    }

    #[test]
    fn colspan_harmonizes_every_contiguous_column_origin() {
        let sources = CollapsedBorderSources {
            rows: vec![CollapsedBorderTrack::default()],
            columns: vec![
                CollapsedBorderTrack {
                    border: LayoutBorder {
                        top: solid(2.0),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                CollapsedBorderTrack {
                    border: LayoutBorder {
                        top: solid(8.0),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let placement = CellPlacement {
            id: CellId { row: 0, cell: 0 },
            row_start: 0,
            row_span: 1,
            column_start: 0,
            column_span: 2,
        };

        let candidate = cell_side_candidate(
            LayoutBorder::default(),
            placement,
            PhysicalSide::Top,
            &sources,
            false,
        );

        assert_eq!(candidate.side.width, 8.0);
        assert_eq!(candidate.origin, CollapsedBorderOrigin::Column);
    }
}
