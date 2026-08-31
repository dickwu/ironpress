//! Memoization of the table auto-sizing pass's per-cell measurements.
//!
//! Auto table layout walks every cell twice — once to size the columns and once
//! to place the content — and measuring a cell that contains a nested table
//! flattens that entire nested table just to read its width. The two walks
//! therefore compound with nesting: a cell `d` tables deep is re-measured on the
//! order of `2^d` times, all of it redundant because the measurement is a pure
//! function of the cell subtree and the width it is measured against. This memo
//! reduces that to one measurement per `(cell, table inner width)`.

use std::collections::HashMap;

use crate::parser::dom::ElementNode;

/// The two intrinsic widths the auto-sizing pass derives for one cell.
///
/// Both are already grown from the content box to the cell's border box, so a
/// column track consumes them without knowing how the cell was measured.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CellContentWidths {
    /// Max-content contribution: the width at which the cell's content takes
    /// its preferred number of lines.
    pub(crate) preferred: f32,
    /// Min-content contribution: the narrowest track the cell tolerates before
    /// its content overflows.
    pub(crate) min: f32,
}

/// Identity of one cell measurement.
///
/// The measured widths depend on the cell subtree and on the table inner width
/// the cell was measured against — that width bounds wrapping inside the cell —
/// so both take part in the key.
///
/// The cell is identified by its address rather than by its markup. Structure
/// is not identity here: sibling cells are routinely identical as markup yet
/// select different rules through `:nth-child` and inherit different styles from
/// their position, so only the node itself distinguishes them.
///
/// Addresses are sound as keys because a [`TableCellSizingMemo`] is owned by one
/// top-level layout call and dropped with it, while the DOM it indexes outlives
/// that call. An address recorded during a layout therefore always names a live
/// node of that same DOM, and an address freed after a layout can never be
/// looked up again, because the memo that held it is already gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CellSizingKey {
    cell: usize,
    /// The table inner width as raw bits. `f32` is neither `Eq` nor `Hash`, and
    /// bit identity is the conservative reading of "the same width": widths that
    /// differ in any bit — `0.0` and `-0.0` among them — simply miss and are
    /// measured again.
    inner_width: u32,
}

impl CellSizingKey {
    pub(crate) fn new(cell: &ElementNode, inner_width: f32) -> Self {
        Self {
            cell: std::ptr::from_ref(cell) as usize,
            inner_width: inner_width.to_bits(),
        }
    }
}

/// Per-layout memo of the table auto-sizing pass's cell measurements.
///
/// The top-level layout entry point owns one instance and lends it to the
/// traversal through the layout environment, which ties the memo's lifetime to
/// the lifetime of the DOM whose addresses it records.
///
/// The auto-sizing pass stores and trusts an entry only while no CSS counter or
/// quote context is live around the measurement. A measurement that reads
/// counters is not a function of the cell and the width alone, and one that
/// writes them has an effect a cache hit would silently drop.
#[derive(Debug, Default)]
pub(crate) struct TableCellSizingMemo {
    measured: HashMap<CellSizingKey, CellContentWidths>,
}

impl TableCellSizingMemo {
    pub(crate) fn get(&self, key: &CellSizingKey) -> Option<CellContentWidths> {
        self.measured.get(key).copied()
    }

    pub(crate) fn insert(&mut self, key: CellSizingKey, widths: CellContentWidths) {
        self.measured.insert(key, widths);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::dom::HtmlTag;

    const WIDTHS: CellContentWidths = CellContentWidths {
        preferred: 120.0,
        min: 40.0,
    };

    #[test]
    fn an_unmeasured_cell_misses() {
        let cell = ElementNode::new(HtmlTag::Td);
        let memo = TableCellSizingMemo::default();

        assert_eq!(memo.get(&CellSizingKey::new(&cell, 300.0)), None);
    }

    #[test]
    fn a_measured_cell_hits_at_the_width_it_was_measured_against() {
        let cell = ElementNode::new(HtmlTag::Td);
        let mut memo = TableCellSizingMemo::default();

        memo.insert(CellSizingKey::new(&cell, 300.0), WIDTHS);

        assert_eq!(memo.get(&CellSizingKey::new(&cell, 300.0)), Some(WIDTHS));
    }

    #[test]
    fn a_second_inner_width_is_measured_separately() {
        let cell = ElementNode::new(HtmlTag::Td);
        let mut memo = TableCellSizingMemo::default();

        memo.insert(CellSizingKey::new(&cell, 300.0), WIDTHS);

        assert_eq!(memo.get(&CellSizingKey::new(&cell, 150.0)), None);
    }

    #[test]
    fn structurally_identical_cells_are_measured_separately() {
        let measured = ElementNode::new(HtmlTag::Td);
        let twin = ElementNode::new(HtmlTag::Td);
        let mut memo = TableCellSizingMemo::default();

        memo.insert(CellSizingKey::new(&measured, 300.0), WIDTHS);

        assert_eq!(memo.get(&CellSizingKey::new(&twin, 300.0)), None);
    }

    #[test]
    fn a_later_measurement_replaces_an_earlier_one() {
        let cell = ElementNode::new(HtmlTag::Td);
        let remeasured = CellContentWidths {
            preferred: 80.0,
            min: 20.0,
        };
        let mut memo = TableCellSizingMemo::default();

        memo.insert(CellSizingKey::new(&cell, 300.0), WIDTHS);
        memo.insert(CellSizingKey::new(&cell, 300.0), remeasured);

        assert_eq!(
            memo.get(&CellSizingKey::new(&cell, 300.0)),
            Some(remeasured)
        );
    }
}
