use super::support::layout_pages_at;
use crate::layout::elements::{Container, LayoutElementTestExt, LayoutVisitor, TextBlock};
use crate::layout::engine::{
    DocumentGeometry, layout_with_rules_and_fonts_raster_quality, visit_layout_tree,
};
use crate::parser::css::parse_stylesheet;
use crate::parser::html::parse_html_with_styles;
use crate::style::raster_quality::RasterQuality;
use crate::types::{Color, Margin, PageSize, Size};
use std::collections::HashMap;

fn page() -> PageSize {
    PageSize::new(240.0, 120.0)
}

fn root_carrier(display: &str, tracks: &str) -> String {
    format!(
        r#"<style>
            * {{ box-sizing:border-box; margin:0; padding:0; }}
            body {{ display:{display}; {tracks} width:240pt; height:120pt; gap:6pt; padding:12pt; }}
            .item {{ width:60pt; height:60pt; }}
        </style>
        <div class="item"></div><div class="item"></div><div class="item"></div>"#,
    )
}

#[test]
fn root_flex_formatting_context_is_one_page_carrier() {
    let pages = layout_pages_at(&root_carrier("flex", ""), page());
    assert_eq!(pages.len(), 1);
}

#[test]
fn root_grid_formatting_context_is_one_page_carrier() {
    let pages = layout_pages_at(
        &root_carrier(
            "grid",
            "grid-template-columns:repeat(3,60pt); grid-template-rows:60pt;",
        ),
        page(),
    );
    assert_eq!(pages.len(), 1);
}

#[test]
fn viewport_math_uses_page_area_before_projected_body_padding() {
    #[derive(Default)]
    struct BarWidth(Option<f32>);

    impl LayoutVisitor for BarWidth {
        fn visit_text_block(&mut self, block: &TextBlock) {
            if block.paint.background.color == Some(Color::rgb(17, 138, 178)) {
                self.0 = block.box_model.size.width.fixed_value();
            }
        }

        fn visit_container(&mut self, container: &Container) {
            if container.paint.background.color == Some(Color::rgb(17, 138, 178)) {
                self.0 = container.box_model.size.width.fixed_value();
            }
        }
    }

    let markup = r#"
        <style>
            * { box-sizing:border-box; margin:0 }
            body { padding:18px }
            .bar { width:calc(50vmin - 180px); height:20px; background:#118ab2 }
        </style>
        <div class="bar"></div>
    "#;
    let document = parse_html_with_styles(markup).expect("valid viewport fixture");
    let rules = document
        .stylesheets
        .iter()
        .flat_map(|stylesheet| parse_stylesheet(stylesheet))
        .collect::<Vec<_>>();
    let page = PageSize::new(360.0, 468.0);
    let projected_body_padding = Margin::uniform(13.5);
    let pages = layout_with_rules_and_fonts_raster_quality(
        &document.nodes,
        DocumentGeometry::new(page, projected_body_padding)
            .with_initial_containing_block(Size::new(360.0, 468.0)),
        &rules,
        &HashMap::new(),
        None,
        0.0,
        Default::default(),
        RasterQuality::default(),
    );
    let mut width = BarWidth::default();
    for (_, element) in &pages[0].elements {
        visit_layout_tree(element.as_ref(), &mut width);
    }

    // 50vmin = 180pt from the 360pt page area; 180px = 135pt.
    assert_eq!(width.0, Some(45.0));
}

#[test]
fn fixed_height_sibling_boxes_keep_their_collapsed_block_margin() {
    let pages = layout_pages_at(
        r#"<style>
            * { box-sizing:border-box; margin:0 }
            .lane { width:100px; height:54px; margin-bottom:10px; border:3px solid #111 }
        </style>
        <div class="lane"></div><div class="lane"></div>"#,
        PageSize::new(360.0, 468.0),
    );
    let lanes = pages[0]
        .elements
        .iter()
        .filter_map(|(y, element)| {
            element.inspect_container(|container| {
                (
                    *y,
                    container.box_model.margins,
                    container.box_model.size.height.used(),
                )
            })
        })
        .collect::<Vec<_>>();

    assert_eq!(lanes.len(), 2);
    assert_eq!(lanes[0].1.end, 7.5);
    assert_eq!(lanes[1].0 - lanes[0].0, 48.0);
}
