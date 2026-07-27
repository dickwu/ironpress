//! Column block-size constraints and fill behavior.

use super::distribution::{balance_columns, balanced_buckets_height, fill_columns};
use super::flow::{
    ColumnFragmentation, FragmentedColumns, balance_fragmented_columns, fragment_columns,
};
use super::items::MultiColItem;
use crate::layout::elements::SizeConstraints;
use crate::layout::roundoff::exceeds_with_roundoff;
use crate::style::computed::{BoxSizing, ComputedStyle};

#[derive(Clone, Copy)]
enum ColumnFillMode {
    Balance,
    Sequential,
}

/// Resolved block-axis behavior of one multi-column formatting context.
///
/// The fragmentainer limit is content-box geometry. Keeping it beside the
/// fill mode prevents `height` and `max-height` from constraining the principal
/// box while leaving its anonymous columns and rules unconstrained.
#[derive(Clone, Copy)]
pub(super) struct MulticolBlockFlow {
    mode: ColumnFillMode,
    content_limit: Option<f32>,
    preferred_content: Option<f32>,
    preferred_border_box: Option<f32>,
}

impl MulticolBlockFlow {
    pub(super) fn from_style(style: &ComputedStyle) -> Self {
        let edges = style.padding.vertical() + style.border.vertical_width();
        let to_content_box = |size: f32| match style.box_sizing {
            BoxSizing::BorderBox => (size - edges).max(0.0),
            BoxSizing::ContentBox => size.max(0.0),
        };
        let constraints =
            SizeConstraints::new(style.min_height, style.max_height).map(to_content_box);
        let preferred_content = style
            .height
            .map(to_content_box)
            .map(|height| constraints.constrain(height));
        let content_limit = preferred_content.or_else(|| {
            constraints
                .maximum()
                .map(|maximum| maximum.max(constraints.minimum().unwrap_or_default()))
        });

        Self {
            mode: if style.column_fill_auto {
                ColumnFillMode::Sequential
            } else {
                ColumnFillMode::Balance
            },
            content_limit,
            preferred_content,
            preferred_border_box: preferred_content.map(|height| height + edges),
        }
    }

    pub(super) const fn preferred_border_box(self) -> Option<f32> {
        self.preferred_border_box
    }

    pub(super) fn fragment(
        self,
        items: &[MultiColItem],
        indices: &[usize],
        column_count: usize,
    ) -> FragmentedColumns {
        match (self.mode, self.content_limit) {
            (ColumnFillMode::Sequential, Some(limit)) => fragment_columns(
                items,
                indices,
                ColumnFragmentation::overflowing(column_count, limit),
            ),
            (ColumnFillMode::Sequential, None) | (ColumnFillMode::Balance, None) => {
                balance_fragmented_columns(items, indices, column_count)
            }
            (ColumnFillMode::Balance, Some(limit)) => {
                let balanced = balance_fragmented_columns(items, indices, column_count);
                let used = balanced
                    .used_block_sizes
                    .iter()
                    .copied()
                    .fold(0.0f32, f32::max);
                if exceeds_with_roundoff(used, limit) {
                    fragment_columns(
                        items,
                        indices,
                        ColumnFragmentation::overflowing(column_count, limit),
                    )
                } else {
                    balanced
                }
            }
        }
    }

    pub(super) fn distribute_atomic(
        self,
        items: &[MultiColItem],
        indices: &[usize],
        column_count: usize,
    ) -> Vec<Vec<usize>> {
        let heights = indices
            .iter()
            .map(|&index| items[index].height)
            .collect::<Vec<_>>();
        let balanced = || balance_columns(&heights, column_count);
        match (self.mode, self.content_limit) {
            (ColumnFillMode::Sequential, Some(limit)) => {
                fill_columns(&heights, column_count, limit)
            }
            (ColumnFillMode::Sequential, None) | (ColumnFillMode::Balance, None) => balanced(),
            (ColumnFillMode::Balance, Some(limit)) => {
                let buckets = balanced();
                if exceeds_with_roundoff(balanced_buckets_height(items, &buckets), limit) {
                    fill_columns(&heights, column_count, limit)
                } else {
                    buckets
                }
            }
        }
    }

    /// Column rules are as tall as their anonymous column boxes. A definite
    /// `height` establishes that column height; `max-height` only caps a taller
    /// content-dependent column and never stretches a shorter one.
    pub(super) fn rule_block_size(self, measured: f32) -> f32 {
        self.preferred_content
            .or_else(|| self.content_limit.map(|limit| measured.min(limit)))
            .unwrap_or(measured)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::css::SpecifiedColor;
    use crate::style::computed::{BorderSide, BorderSides};
    use crate::types::EdgeSizes;

    fn bordered_style() -> ComputedStyle {
        ComputedStyle {
            padding: EdgeSizes::uniform(7.0),
            border: BorderSides::uniform(BorderSide::solid(2.0, SpecifiedColor::CurrentColor)),
            box_sizing: BoxSizing::BorderBox,
            ..Default::default()
        }
    }

    #[test]
    fn explicit_height_establishes_the_column_rule_block_size() {
        let flow = MulticolBlockFlow::from_style(&ComputedStyle {
            height: Some(96.0),
            ..bordered_style()
        });

        assert_eq!(flow.preferred_border_box(), Some(96.0));
        assert_eq!(flow.rule_block_size(22.0), 78.0);
    }

    #[test]
    fn maximum_height_caps_but_does_not_stretch_column_rules() {
        let capped = MulticolBlockFlow::from_style(&ComputedStyle {
            max_height: Some(58.0),
            ..bordered_style()
        });

        assert_eq!(capped.preferred_border_box(), None);
        assert_eq!(capped.rule_block_size(47.0), 40.0);
        assert_eq!(capped.rule_block_size(22.0), 22.0);
    }
}
