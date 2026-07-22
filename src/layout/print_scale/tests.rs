use super::{PrintContentScale, assign_page_print_scales};
use crate::layout::elements::{
    InlineOffset, IntoLayoutNode, ProgressBar, TableCells, TableFormatting, TableInlineGeometry,
    TableRow,
};
use crate::layout::engine::Page;
use crate::style::computed::BorderCollapse;
use crate::types::{Color, Margin, PageSize, Size};

#[test]
fn shrinks_only_a_finite_normal_flow_overflow() {
    assert_eq!(
        PrintContentScale::from_flow_width(252.0, 255.0).factor(),
        84.0 / 85.0
    );
    assert!(PrintContentScale::from_flow_width(252.0, 252.0).is_identity());
    assert!(PrintContentScale::from_flow_width(252.0, f32::NAN).is_identity());
}

#[test]
fn page_margins_do_not_reduce_the_print_fit_width_twice() {
    let mut pages = vec![Page {
        elements: vec![(
            0.0,
            ProgressBar {
                fraction: 1.0,
                size: Size::new(184.0, 1.0),
                colors: crate::layout::elements::ProgressColors {
                    fill: Color::BLACK,
                    track: Color::WHITE,
                },
                ..ProgressBar::default()
            }
            .boxed(),
        )],
        margin_override: Some(Margin::new(0.0, 0.0, 0.0, 40.0)),
        ..Page::default()
    }];

    assign_page_print_scales(&mut pages, PageSize::new(184.0, 120.0));

    assert!(pages[0].print_content_scale.is_identity());
}

#[test]
fn collapsed_table_spacing_does_not_trigger_print_fit_scaling() {
    let mut pages = vec![Page {
        elements: vec![(
            0.0,
            TableRow {
                content: TableCells {
                    column_widths: vec![120.0, 120.0],
                    ..TableCells::default()
                },
                formatting: TableFormatting {
                    border_collapse: BorderCollapse::Collapse,
                    border_spacing: 1.5,
                },
                inline: TableInlineGeometry::new(InlineOffset::ZERO, InlineOffset::ZERO)
                    .with_box_extent(240.0),
                ..TableRow::default()
            }
            .boxed(),
        )],
        ..Page::default()
    }];

    assign_page_print_scales(&mut pages, PageSize::new(240.0, 180.0));

    assert!(pages[0].print_content_scale.is_identity());
}
