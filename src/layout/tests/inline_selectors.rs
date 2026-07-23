use crate::style::computed::VerticalAlign;
use crate::types::Color;

use super::support::{layout_pages, visible_runs};

#[test]
fn nested_inline_runs_keep_their_real_sibling_selector_positions() {
    let runs = visible_runs(
        r#"<style>
            .node { font-size: 16px; }
            .node > .own > .token:first-of-type {
                font-size: .72em;
                vertical-align: super;
            }
            .node > .own > .token:nth-of-type(2) { color: #c1121f; }
        </style>
        <div class="node"><div class="own"><span class="token">Ag</span><span class="token">Bb</span></div></div>"#,
    );

    assert_eq!(runs.len(), 2, "runs: {runs:#?}");
    assert_eq!(runs[0].text, "Ag");
    assert_eq!(runs[0].vertical_align, VerticalAlign::Super);
    assert!(runs[0].font_size < runs[1].font_size, "runs: {runs:#?}");
    assert_eq!(runs[1].text, "Bb");
    assert_eq!(runs[1].vertical_align, VerticalAlign::Baseline);
    assert_eq!(runs[1].color, Color::rgb(193, 18, 31));
}

#[test]
fn anonymous_table_fixup_keeps_authored_ancestor_and_type_position() {
    let runs = visible_runs(
        r#"<style>
            .node { display: table; }
            .node > .own > .token { display: table-cell; }
            .node > .own > .token:nth-of-type(2) { color: #c1121f; }
        </style>
        <div class="node"><div class="own"><span class="token">Ag</span><span class="token">Bb</span></div></div>"#,
    );

    assert_eq!(runs.len(), 2, "runs: {runs:#?}");
    assert_eq!(runs[0].text, "Ag");
    assert_eq!(runs[1].text, "Bb");
    assert_eq!(runs[1].color, Color::rgb(193, 18, 31));
}

#[test]
fn positioned_inline_is_out_of_flow_and_resolves_against_its_ancestor() {
    use crate::layout::elements::{LayoutElement, LayoutVisitor, TextBlock};

    #[derive(Debug)]
    struct Snapshot {
        text: String,
        positioning: crate::layout::elements::Positioning,
    }

    #[derive(Default)]
    struct Collector(Vec<Snapshot>);

    impl LayoutVisitor for Collector {
        fn visit_text_block(&mut self, block: &TextBlock) {
            let text = block
                .lines
                .iter()
                .flat_map(|line| line.runs.iter().map(|run| run.text.as_str()))
                .collect::<String>();
            if !text.is_empty() {
                self.0.push(Snapshot {
                    text,
                    positioning: block.positioning.clone(),
                });
            }
        }
    }

    let pages = layout_pages(
        r#"<style>
            .cb { position: relative; width: 126px; height: 68px; }
            .own { height: 22px; }
            .token:last-of-type { position: absolute; right: 4px; bottom: 4px; }
        </style>
        <div class="cb"><div class="own"><span class="token">Ag</span><span class="token">Bb</span></div></div>"#,
    );
    fn collect_tree(
        element: &dyn LayoutElement,
        depth: usize,
        snapshots: &mut Vec<(usize, Snapshot)>,
    ) {
        let mut collector = Collector::default();
        element.accept(&mut collector);
        for snapshot in collector.0 {
            snapshots.push((depth, snapshot));
        }
        element.visit_children(&mut |child| collect_tree(child, depth + 1, snapshots));
    }

    let mut snapshots = Vec::new();
    for page in pages {
        for (_, element) in page.elements {
            collect_tree(element.as_ref(), 0, &mut snapshots);
        }
    }
    assert_eq!(snapshots.len(), 2, "snapshots: {snapshots:#?}");

    let (_, in_flow) = &snapshots[0];
    assert_eq!(in_flow.text, "Ag");
    assert_eq!(
        in_flow.positioning.scheme,
        crate::style::computed::Position::Static
    );

    let (_, positioned) = &snapshots[1];
    assert_eq!(positioned.text, "Bb");
    assert_eq!(
        positioned.positioning.scheme,
        crate::style::computed::Position::Absolute
    );
    let containing_block = positioned
        .positioning
        .containing_block
        .expect("absolute inline containing block");
    assert!((containing_block.width - 94.5).abs() < 0.001);
    assert!((containing_block.height - 51.0).abs() < 0.001);
    assert!((positioned.positioning.insets.left - 77.496).abs() < 0.001);
    assert!((positioned.positioning.insets.top - 33.0).abs() < 0.001);
}

#[test]
fn positioned_inline_inside_flex_item_uses_the_general_item_context() {
    use crate::layout::elements::{LayoutElement, LayoutVisitor, TextBlock};
    use crate::style::computed::Position;

    #[derive(Default)]
    struct Collector(Vec<(String, crate::layout::elements::Positioning)>);

    impl LayoutVisitor for Collector {
        fn visit_text_block(&mut self, block: &TextBlock) {
            let text = block
                .lines
                .iter()
                .flat_map(|line| line.runs.iter().map(|run| run.text.as_str()))
                .collect::<String>();
            if !text.is_empty() {
                self.0.push((text, block.positioning.clone()));
            }
        }
    }

    fn collect(element: &dyn LayoutElement, snapshots: &mut Collector) {
        element.accept(snapshots);
        element.visit_children(&mut |child| collect(child, snapshots));
    }

    let pages = layout_pages(
        r#"<style>
            .cb {
                display: flex;
                position: relative;
                width: 126px;
                height: 68px;
                align-items: center;
                justify-content: center;
            }
            .own { height: 22px; }
            .cb > .own > .token:last-of-type {
                position: absolute;
                right: 4px;
                bottom: 4px;
            }
        </style>
        <div class="cb"><div class="own"><span class="token">Ag</span><span class="token">Bb</span></div></div>"#,
    );
    let mut snapshots = Collector::default();
    for page in pages {
        for (_, element) in page.elements {
            collect(element.as_ref(), &mut snapshots);
        }
    }

    let in_flow = snapshots
        .0
        .iter()
        .find(|(text, _)| text == "Ag")
        .expect("in-flow flex item text");
    let positioned = snapshots
        .0
        .iter()
        .find(|(text, _)| text == "Bb")
        .expect("positioned flex descendant");
    assert_eq!(in_flow.1.scheme, Position::Static);
    assert_eq!(positioned.1.scheme, Position::Absolute);
    assert!(
        positioned.1.insets.left > in_flow.1.insets.left + 40.0,
        "snapshots: {:#?}",
        snapshots.0
    );
    assert!(
        positioned.1.insets.top > in_flow.1.insets.top + 20.0,
        "snapshots: {:#?}",
        snapshots.0
    );
}
