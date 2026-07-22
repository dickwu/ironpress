use super::*;
use crate::layout::elements::{Image, Svg};

pub(super) fn render_image_child(
    content: &mut String,
    child: &Image,
    child_index: usize,
    flow: &ContainerFlowContext<'_>,
    position: FlowPosition,
    ctx: &mut PageRenderContext<'_>,
) -> FlowPosition {
    let x = flow.frame.content_origin.x;
    let flow_top_by_index = flow.flow_top_by_index;
    let FlowPosition {
        mut y,
        mut cursor_y,
        previous_margin_bottom: mut prev_margin_bottom,
    } = position;
    let img_w = &child.geometry.size.width;
    let img_h = &child.geometry.size.height;
    let img_mt = &child.geometry.flow.margins.start;
    let img_mb = &child.geometry.flow.margins.end;
    let img_extra_end = child.geometry.flow.extra_end;
    let offset_top = &child.positioning.insets.top;
    let offset_left = &child.positioning.insets.left;
    let planned_flow_top = flow_top_by_index.get(&child_index).copied();
    let flow_top = if let Some(top) = planned_flow_top {
        top
    } else {
        cursor_y -= collapsed_margin_top_extra(*img_mt, prev_margin_bottom);
        cursor_y
    };
    let render_x = x + offset_left;
    let box_top = flow_top - offset_top;
    let box_bottom = box_top - img_h;
    let image_box = PdfRect::new(render_x, box_bottom, *img_w, *img_h);
    paint_image_box(content, child, image_box, ctx);
    if planned_flow_top.is_none() {
        cursor_y -= img_h + img_extra_end + img_mb;
        y = cursor_y;
    }
    prev_margin_bottom = *img_mb;

    FlowPosition::new(y, cursor_y, prev_margin_bottom)
}

pub(super) fn render_svg_child(
    content: &mut String,
    child: &Svg,
    child_index: usize,
    flow: &ContainerFlowContext<'_>,
    position: FlowPosition,
    ctx: &mut PageRenderContext<'_>,
) -> FlowPosition {
    let x = flow.frame.content_origin.x;
    let flow_top_by_index = flow.flow_top_by_index;
    let FlowPosition {
        y: _,
        mut cursor_y,
        previous_margin_bottom: mut prev_margin_bottom,
    } = position;
    let mut y;
    let svg_w = &child.geometry.size.width;
    let svg_h = &child.geometry.size.height;
    let svg_mt = &child.geometry.flow.margins.start;
    let svg_mb = &child.geometry.flow.margins.end;
    let svg_extra_end = child.geometry.flow.extra_end;
    let offset_top = &child.positioning.insets.top;
    let offset_left = &child.positioning.insets.left;
    let planned_flow_top = flow_top_by_index.get(&child_index).copied();
    if let Some(top) = planned_flow_top {
        y = top;
    } else {
        cursor_y -= collapsed_margin_top_extra(*svg_mt, prev_margin_bottom);
        y = cursor_y;
    }
    let svg_x = x + offset_left;
    let svg_y = y - offset_top - svg_h;
    let svg_box = PdfRect::new(svg_x, svg_y, *svg_w, *svg_h);
    paint_svg_box(content, child, svg_box, ctx);
    if planned_flow_top.is_none() {
        cursor_y -= svg_h + svg_extra_end + svg_mb;
        y = cursor_y;
    }
    prev_margin_bottom = *svg_mb;

    FlowPosition::new(y, cursor_y, prev_margin_bottom)
}
