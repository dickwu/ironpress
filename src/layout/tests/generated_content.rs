use super::support::{layout_pages, visible_runs};
use crate::layout::elements::test_support::LayoutElementTestExt;
use crate::layout::elements::{Container, Image, LayoutVisitor, TextBlock};
use crate::layout::engine::visit_layout_tree;
use crate::style::computed::Position;

#[test]
fn generated_content_survives_grid_item_block_content_layout() {
    let runs = visible_runs(
        r#"<style>
            .grid { display: grid; grid-template-columns: 100pt; }
            .item::before { content: "before"; }
            .item::after { content: "after"; }
        </style>
        <div class="grid">
            <div class="item"><div>body</div></div>
        </div>"#,
    );
    let text = runs.iter().map(|run| run.text.as_str()).collect::<String>();

    assert!(text.contains("before"), "missing ::before in {text:?}");
    assert!(text.contains("body"), "missing principal child in {text:?}");
    assert!(text.contains("after"), "missing ::after in {text:?}");
}

#[test]
fn empty_string_generated_box_retains_absolute_background_and_filter() {
    #[derive(Default)]
    struct GeneratedBox {
        found: bool,
        text_boxes: Vec<(Position, bool, bool)>,
        images: Vec<(Position, bool)>,
    }

    impl LayoutVisitor for GeneratedBox {
        fn visit_text_block(&mut self, block: &TextBlock) {
            let has_background = block.paint.background.color.is_some();
            let has_filter = block
                .paint
                .filter
                .as_ref()
                .is_some_and(|filter| filter.has_composited_output());
            self.text_boxes
                .push((block.positioning.scheme, has_background, has_filter));
            self.found |=
                block.positioning.scheme == Position::Absolute && has_background && has_filter;
        }

        fn visit_image(&mut self, image: &Image) {
            let has_filter_overflow = !image.paint.raster_overflow.is_zero();
            self.images
                .push((image.positioning.scheme, has_filter_overflow));
            self.found |= image.positioning.scheme == Position::Absolute && has_filter_overflow;
        }
    }

    let pages = layout_pages(
        r#"<style>
            .wrap { position: relative; width: 72pt; height: 72pt; }
            .wrap::before {
                content: "";
                position: absolute;
                inset: 0;
                width: 72pt;
                height: 72pt;
                background: #336699;
                filter: blur(4pt);
            }
        </style><div class="wrap"></div>"#,
    );
    let mut generated = GeneratedBox::default();
    for (_, element) in &pages[0].elements {
        visit_layout_tree(element.as_ref(), &mut generated);
    }

    assert!(
        generated.found,
        "content: \"\" must generate its styled pseudo-element box; text={:?}, images={:?}",
        generated.text_boxes, generated.images,
    );
}

#[test]
fn generated_block_remains_inside_its_absolute_originating_box() {
    #[derive(Default)]
    struct AbsoluteGeneratedBox(Vec<(f32, f32, f32, usize)>);

    impl LayoutVisitor for AbsoluteGeneratedBox {
        fn visit_container(&mut self, container: &Container) {
            if container.positioning.scheme == Position::Absolute
                && container.children.iter().any(|child| {
                    child
                        .inspect_text(|block| block.paint.background.color.is_some())
                        .unwrap_or(false)
                })
            {
                self.0.push((
                    container.positioning.insets.left,
                    container.positioning.insets.top,
                    container.box_model.size.width.resolve(0.0),
                    container.children.len(),
                ));
            }
        }
    }

    let pages = layout_pages(
        r#"<style>
            * { margin:0; padding:0; box-sizing:border-box }
            .panel { position:relative; width:120pt; height:108pt }
            .top { position:absolute; left:12pt; top:9pt; width:72pt; height:72pt }
            .top::before {
                content:"";
                display:block;
                width:72pt;
                height:72pt;
                background:#ef233c;
            }
        </style><div class="panel"><div class="top"></div></div>"#,
    );
    let mut boxes = AbsoluteGeneratedBox::default();
    for (_, element) in &pages[0].elements {
        visit_layout_tree(element.as_ref(), &mut boxes);
    }

    assert_eq!(boxes.0, [(12.0, 9.0, 72.0, 1)]);
}
