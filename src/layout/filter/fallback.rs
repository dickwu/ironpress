//! Ordered primitive fallback for filter sources outside the offscreen painter.
//!
//! This path preserves CSS filter-list order even though it can only express
//! the result with existing layout and PDF primitives. In particular, a
//! `drop-shadow()` is affected by colour functions which follow it, but never
//! by colour functions which precede it.

use crate::layout::elements::{
    BoxModel, BoxPaint, Container, FlexRow, LayoutElement, LayoutNode, LayoutVisitorMut,
    Positioning, TextBlock, visit_layout_tree_mut,
};
use crate::layout::engine::{FlexCell, LayoutBorder, TextRun, estimate_element_height};
use crate::style::computed::{BoxShadow, ColorSource, FilterOperation};

use super::ResolvedFilter;

impl ResolvedFilter {
    /// Apply a source-ordered primitive approximation when group rasterization
    /// is unavailable for a laid-out root box.
    pub(crate) fn apply_primitive_fallback(&self, elements: &mut [LayoutNode]) {
        let fallback = PrimitiveFilterFallback {
            linear_rgb: self.linear_rgb,
        };
        for operation in &self.operations {
            fallback.apply_to_layout(elements, operation);
        }
    }

    /// Apply the same ordered contract to a direct flex item represented by a
    /// cell rather than an independent layout node.
    pub(crate) fn apply_flex_cell_fallback(&self, cell: &mut FlexCell) {
        let fallback = PrimitiveFilterFallback {
            linear_rgb: self.linear_rgb,
        };
        for operation in &self.operations {
            fallback.apply_to_flex_cell(cell, operation);
        }
    }
}

struct PrimitiveFilterFallback {
    linear_rgb: bool,
}

impl PrimitiveFilterFallback {
    fn apply_to_layout(&self, elements: &mut [LayoutNode], operation: &FilterOperation) {
        if changes_color(operation) {
            for element in elements {
                apply_color_to_element(element.as_mut(), operation, self.linear_rgb);
            }
            return;
        }

        match *operation {
            FilterOperation::Blur(_)
            | FilterOperation::DropShadow(_)
            | FilterOperation::MorphologyDilate(_) => {
                for element in elements {
                    apply_geometry_to_element(element.as_mut(), operation);
                }
            }
            FilterOperation::Offset {
                dx,
                keep_source,
                region_x,
                region_width,
                ..
            } => {
                for element in elements {
                    apply_offset_to_element(
                        element.as_mut(),
                        dx,
                        keep_source,
                        region_x,
                        region_width,
                    );
                }
            }
            FilterOperation::Flood {
                color,
                region_x,
                region_y,
                region_width,
                region_height,
            } => {
                for element in elements {
                    apply_flood_to_element(
                        element.as_mut(),
                        color,
                        region_x,
                        region_y,
                        region_width,
                        region_height,
                    );
                }
            }
            _ => {}
        }
    }

    fn apply_to_flex_cell(&self, cell: &mut FlexCell, operation: &FilterOperation) {
        if changes_color(operation) {
            apply_color_to_flex_cell(cell, operation, self.linear_rgb);
            return;
        }

        match *operation {
            FilterOperation::Blur(radius) => {
                cell.paint.background.layers.blur_radius =
                    cell.paint.background.layers.blur_radius.max(radius);
            }
            FilterOperation::DropShadow(shadow) => cell.paint.shadows.push(BoxShadow {
                offset_x: shadow.dx,
                offset_y: shadow.dy,
                blur: shadow.blur,
                spread: 0.0,
                color: shadow.color,
                color_source: ColorSource::Absolute,
                inset: false,
            }),
            FilterOperation::MorphologyDilate(radius) => {
                if let Some(color) = cell.paint.background.color {
                    cell.paint.shadows.push(dilation_shadow(radius, color));
                }
            }
            FilterOperation::Flood {
                color,
                region_x,
                region_y,
                region_width,
                region_height,
            } => cell.paint.shadows.extend(flood_shadows(
                cell.width,
                cell.natural_height,
                color,
                region_x,
                region_y,
                region_width,
                region_height,
            )),
            _ => {}
        }
    }
}

fn changes_color(operation: &FilterOperation) -> bool {
    !operation.is_visual_identity()
        && !matches!(
            operation,
            FilterOperation::Blur(_)
                | FilterOperation::Flood { .. }
                | FilterOperation::Offset { .. }
                | FilterOperation::DropShadow(_)
                | FilterOperation::MorphologyDilate(_)
        )
}

fn apply_geometry_to_element(element: &mut dyn LayoutElement, operation: &FilterOperation) {
    struct GeometryFallback<'a>(&'a FilterOperation);

    impl GeometryFallback<'_> {
        fn apply(&self, paint: &mut BoxPaint) {
            match *self.0 {
                FilterOperation::Blur(radius) => {
                    paint.background.layers.blur_radius =
                        paint.background.layers.blur_radius.max(radius);
                }
                FilterOperation::DropShadow(shadow) => paint.shadows.push(BoxShadow {
                    offset_x: shadow.dx,
                    offset_y: shadow.dy,
                    blur: shadow.blur,
                    spread: 0.0,
                    color: shadow.color,
                    color_source: ColorSource::Absolute,
                    inset: false,
                }),
                FilterOperation::MorphologyDilate(radius) => {
                    if let Some(color) = paint.background.color {
                        paint.shadows.push(dilation_shadow(radius, color));
                    }
                }
                _ => {}
            }
        }
    }

    impl LayoutVisitorMut for GeometryFallback<'_> {
        fn visit_text_block(&mut self, element: &mut TextBlock) {
            self.apply(&mut element.paint);
        }

        fn visit_container(&mut self, element: &mut Container) {
            self.apply(&mut element.paint);
        }

        fn visit_flex_row(&mut self, element: &mut FlexRow) {
            self.apply(&mut element.paint);
        }
    }

    element.accept_mut(&mut GeometryFallback(operation));
}

fn dilation_shadow(radius: f32, color: crate::types::Color) -> BoxShadow {
    BoxShadow {
        offset_x: 0.0,
        offset_y: 0.0,
        blur: 0.0,
        spread: radius,
        color,
        color_source: ColorSource::Absolute,
        inset: false,
    }
}

fn apply_offset_to_element(
    element: &mut dyn LayoutElement,
    dx: f32,
    keep_source: bool,
    region_x: f32,
    region_width: f32,
) {
    struct OffsetFallback {
        dx: f32,
        keep_source: bool,
        region_x: f32,
        region_width: f32,
    }

    impl OffsetFallback {
        fn apply(&self, box_model: &mut BoxModel, positioning: &mut Positioning) {
            let Some(width) = box_model.size.width.fixed_value() else {
                return;
            };
            let Some((left, right)) = clipped_offset_bounds(
                width,
                self.dx,
                self.keep_source,
                self.region_x,
                self.region_width,
            ) else {
                return;
            };
            positioning.insets.left += left;
            box_model.size.width = crate::layout::elements::InlineSize::fixed(right - left);
        }

        fn applies_to(paint: &BoxPaint) -> bool {
            paint.background.color.is_some()
        }
    }

    impl LayoutVisitorMut for OffsetFallback {
        fn visit_text_block(&mut self, element: &mut TextBlock) {
            if element.lines.is_empty() && Self::applies_to(&element.paint) {
                self.apply(&mut element.box_model, &mut element.positioning);
            }
        }

        fn visit_container(&mut self, element: &mut Container) {
            if element.children.is_empty() && Self::applies_to(&element.paint) {
                self.apply(&mut element.box_model, &mut element.positioning);
            }
        }
    }

    element.accept_mut(&mut OffsetFallback {
        dx,
        keep_source,
        region_x,
        region_width,
    });
}

fn clipped_offset_bounds(
    width: f32,
    dx: f32,
    keep_source: bool,
    region_x: f32,
    region_width: f32,
) -> Option<(f32, f32)> {
    if width <= 0.0 {
        return None;
    }
    let region_left = region_x * width;
    let region_right = (region_x + region_width) * width;
    let shifted_left = dx.max(region_left);
    let shifted_right = (dx + width).min(region_right);
    if keep_source {
        let right = width.max(shifted_right);
        (right > 0.0).then_some((0.0, right))
    } else if shifted_right > shifted_left {
        Some((shifted_left, shifted_right))
    } else {
        None
    }
}

fn apply_flood_to_element(
    element: &mut dyn LayoutElement,
    color: crate::types::Color,
    region_x: f32,
    region_y: f32,
    region_width: f32,
    region_height: f32,
) {
    struct FloodFallback {
        color: crate::types::Color,
        region_x: f32,
        region_y: f32,
        region_width: f32,
        region_height: f32,
    }

    impl FloodFallback {
        fn apply(&self, box_model: &BoxModel, paint: &mut BoxPaint, content_height: f32) {
            let width = box_model.size.width.fixed_value().unwrap_or_default();
            let height = box_model
                .size
                .height
                .resolve(box_model.padding.vertical() + content_height)
                + box_model.border.vertical_width();
            if width > 0.0 && height > 0.0 {
                paint.shadows.extend(flood_shadows(
                    width,
                    height,
                    self.color,
                    self.region_x,
                    self.region_y,
                    self.region_width,
                    self.region_height,
                ));
            }
        }
    }

    impl LayoutVisitorMut for FloodFallback {
        fn visit_text_block(&mut self, element: &mut TextBlock) {
            let content_height = element.lines.iter().map(|line| line.height).sum();
            self.apply(&element.box_model, &mut element.paint, content_height);
        }

        fn visit_container(&mut self, element: &mut Container) {
            let content_height = element
                .children
                .iter()
                .map(|child| estimate_element_height(child.as_ref()))
                .sum();
            self.apply(&element.box_model, &mut element.paint, content_height);
        }
    }

    element.accept_mut(&mut FloodFallback {
        color,
        region_x,
        region_y,
        region_width,
        region_height,
    });
}

fn flood_shadows(
    width: f32,
    height: f32,
    color: crate::types::Color,
    region_x: f32,
    region_y: f32,
    region_width: f32,
    region_height: f32,
) -> Vec<BoxShadow> {
    let left = (-region_x * width).max(0.0);
    let right = ((region_x + region_width - 1.0) * width).max(0.0);
    let top = (-region_y * height).max(0.0);
    let bottom = ((region_y + region_height - 1.0) * height).max(0.0);
    let vertical_spread = top.max(bottom);
    let vertical_offset = (bottom - top) * 0.5;
    let make_shadow = |offset_x: f32, spread: f32| BoxShadow {
        offset_x,
        offset_y: vertical_offset,
        blur: 0.0,
        spread,
        color,
        color_source: ColorSource::Absolute,
        inset: false,
    };
    let mut shadows = vec![make_shadow(0.0, vertical_spread)];
    if left > vertical_spread {
        shadows.push(make_shadow(-(left - vertical_spread), vertical_spread));
    }
    if right > vertical_spread {
        shadows.push(make_shadow(right - vertical_spread, vertical_spread));
    }
    shadows
}

fn apply_color_to_element(
    element: &mut dyn LayoutElement,
    operation: &FilterOperation,
    linear_rgb: bool,
) {
    struct ColorFallback<'a> {
        operation: &'a FilterOperation,
        linear_rgb: bool,
    }

    impl ColorFallback<'_> {
        fn apply_box(&self, box_model: &mut BoxModel, paint: &mut BoxPaint) {
            if let Some(color) = &mut paint.background.color {
                *color = filtered_color(*color, self.operation, self.linear_rgb);
            }
            apply_color_to_border(&mut box_model.border, self.operation, self.linear_rgb);
            for shadow in &mut paint.shadows {
                shadow.color = filtered_color(shadow.color, self.operation, self.linear_rgb);
            }
            if let Some(color) = &mut paint.outline.color {
                *color = filtered_color(*color, self.operation, self.linear_rgb);
            }
        }
    }

    impl LayoutVisitorMut for ColorFallback<'_> {
        fn visit_text_block(&mut self, element: &mut TextBlock) {
            self.apply_box(&mut element.box_model, &mut element.paint);
            for line in &mut element.lines {
                for run in &mut line.runs {
                    apply_color_to_run(run, self.operation, self.linear_rgb);
                }
            }
        }

        fn visit_container(&mut self, element: &mut Container) {
            self.apply_box(&mut element.box_model, &mut element.paint);
        }

        fn visit_flex_row(&mut self, element: &mut FlexRow) {
            self.apply_box(&mut element.box_model, &mut element.paint);
            for cell in &mut element.content.cells {
                apply_color_to_flex_cell(cell, self.operation, self.linear_rgb);
            }
        }
    }

    visit_layout_tree_mut(
        element,
        &mut ColorFallback {
            operation,
            linear_rgb,
        },
    );
}

fn apply_color_to_flex_cell(cell: &mut FlexCell, operation: &FilterOperation, linear_rgb: bool) {
    if let Some(color) = &mut cell.paint.background.color {
        *color = filtered_color(*color, operation, linear_rgb);
    }
    apply_color_to_border(&mut cell.border, operation, linear_rgb);
    for shadow in &mut cell.paint.shadows {
        shadow.color = filtered_color(shadow.color, operation, linear_rgb);
    }
    for line in &mut cell.lines {
        for run in &mut line.runs {
            apply_color_to_run(run, operation, linear_rgb);
        }
    }
    for element in &mut cell.nested_elements {
        apply_color_to_element(element.as_mut(), operation, linear_rgb);
    }
}

fn apply_color_to_run(run: &mut TextRun, operation: &FilterOperation, linear_rgb: bool) {
    run.color = filtered_color(run.color, operation, linear_rgb);
    if let Some(color) = run.decoration_color {
        run.decoration_color = Some(filtered_color(color, operation, linear_rgb));
    }
    if run.metadata.emphasis.mark {
        run.metadata.emphasis.color =
            filtered_color(run.metadata.emphasis.color, operation, linear_rgb);
    }
    if let Some(color) = run.background_color {
        run.background_color = Some(filtered_color(color, operation, linear_rgb));
    }
    for shadow in &mut run.text_shadow {
        shadow.color = filtered_color(shadow.color, operation, linear_rgb);
    }
    if let Some(inline_box) = &mut run.inline_box {
        if let Some(color) = inline_box.background_color {
            inline_box.background_color = Some(filtered_color(color, operation, linear_rgb));
        }
        apply_color_to_border(&mut inline_box.border, operation, linear_rgb);
        for line in &mut inline_box.lines {
            for run in &mut line.runs {
                apply_color_to_run(run, operation, linear_rgb);
            }
        }
    }
}

fn apply_color_to_border(border: &mut LayoutBorder, operation: &FilterOperation, linear_rgb: bool) {
    for side in [
        &mut border.top,
        &mut border.right,
        &mut border.bottom,
        &mut border.left,
    ] {
        side.color = filtered_color(side.color, operation, linear_rgb);
    }
}

fn filtered_color(
    color: crate::types::Color,
    operation: &FilterOperation,
    linear_rgb: bool,
) -> crate::types::Color {
    let (red, green, blue, alpha) = crate::render::filter::apply_operations_to_color(
        color.to_f32_rgba(),
        std::slice::from_ref(operation),
        linear_rgb,
    );
    crate::types::Color::from_srgb(red, green, blue, alpha)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::elements::{BackgroundPaint, IntoLayoutNode, LayoutElementTestExt};
    use crate::style::computed::DropShadow;
    use crate::types::Color;

    fn ordered_filter(operations: Vec<FilterOperation>) -> ResolvedFilter {
        ResolvedFilter {
            operations,
            linear_rgb: false,
        }
    }

    fn source() -> Vec<LayoutNode> {
        vec![
            Container {
                paint: BoxPaint {
                    background: BackgroundPaint {
                        color: Some(Color::from_srgb(0.91, 0.96, 1.0, 1.0)),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            }
            .boxed(),
        ]
    }

    fn shadow() -> DropShadow {
        DropShadow {
            dx: 1.5,
            dy: 0.75,
            blur: 0.0,
            color: Color::from_srgb(0.56, 0.64, 0.68, 1.0),
        }
    }

    fn resulting_shadow_color(elements: &[LayoutNode]) -> Color {
        elements[0]
            .inspect_container(|container| container.paint.shadows[0].color)
            .expect("the fallback source remains a container")
    }

    #[test]
    fn earlier_color_functions_do_not_recolor_a_later_drop_shadow() {
        let mut elements = source();
        let shadow = shadow();
        ordered_filter(vec![
            FilterOperation::Grayscale(0.18),
            FilterOperation::Contrast(1.08),
            FilterOperation::DropShadow(shadow),
        ])
        .apply_primitive_fallback(&mut elements);

        assert_eq!(resulting_shadow_color(&elements), shadow.color);
    }

    #[test]
    fn later_color_functions_recolor_an_existing_drop_shadow() {
        let mut elements = source();
        let shadow = shadow();
        ordered_filter(vec![
            FilterOperation::DropShadow(shadow),
            FilterOperation::Grayscale(1.0),
        ])
        .apply_primitive_fallback(&mut elements);

        assert_ne!(resulting_shadow_color(&elements), shadow.color);
    }
}
