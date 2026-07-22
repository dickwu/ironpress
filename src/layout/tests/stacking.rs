use super::support::layout_pages;
use crate::layout::elements::{GridRow, LayoutVisitor, StackingLevel, StackingParticipant};
use crate::layout::engine::{layout_element_paint_order, visit_layout_tree};

#[derive(Default)]
struct PaintedGridItems(Vec<(usize, StackingLevel)>);

impl LayoutVisitor for PaintedGridItems {
    fn visit_grid_row(&mut self, row: &GridRow) {
        self.0.extend(
            row.content
                .cells
                .iter()
                .filter(|cell| cell.layout.paint.background.color.is_some())
                .map(|cell| (cell.placement.column_start, cell.layout.stacking_level())),
        );
    }
}

#[test]
fn overlapping_grid_items_remain_independent_and_in_document_order() {
    let pages = layout_pages(
        r#"
        <style>
            .grid { display:grid; grid-template-columns:100px; grid-template-rows:80px; }
            .item { grid-area:1 / 1 / 2 / 2; }
            .first { background:red; z-index:2; }
            .second { background:green; z-index:1; }
        </style>
        <div class="grid">
            <div class="item first"></div>
            <div class="item second"></div>
        </div>
        "#,
    );
    let mut items = PaintedGridItems::default();
    for page in pages {
        for (_, element) in page.elements {
            visit_layout_tree(element.as_ref(), &mut items);
        }
    }
    assert_eq!(
        items.0,
        [
            (0, StackingLevel::positive(2)),
            (0, StackingLevel::positive(1)),
        ]
    );
}

#[test]
fn pagination_preserves_source_order_for_the_paint_planner() {
    let pages = layout_pages(
        r#"
        <div style="position:relative; z-index:2; width:20px; height:20px; background:red"></div>
        <div style="position:relative; z-index:1; width:20px; height:20px; background:green"></div>
        "#,
    );
    let levels: Vec<_> = pages[0]
        .elements
        .iter()
        .map(|(_, element)| layout_element_paint_order(element.as_ref()))
        .collect();
    assert_eq!(
        levels,
        [StackingLevel::positive(2), StackingLevel::positive(1)]
    );
}
