use super::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct MaskLayerPaint {
    pub(super) tile: PdfRect,
    pub(super) clip: PdfRect,
}

impl MaskLayerPaint {
    pub(super) fn resolve(layer: &MaskLayer, geometry: PaintBoxGeometry) -> Option<Self> {
        let origin = geometry.shape_box(layer.origin);
        let clip = geometry.shape_box(layer.clip);
        let resolve_axis = |value: f32, is_percent: bool, extent: f32| {
            if is_percent {
                extent * value / 100.0
            } else {
                value
            }
        };
        let (width, height) = match layer.layer_box.size {
            Some(BackgroundSize::Explicit {
                width,
                height,
                width_is_percent,
                height_is_percent,
            })
            | Some(BackgroundSize::ExplicitAuto {
                width: Some(width),
                height,
                width_is_percent,
                height_is_percent,
            }) => (
                resolve_axis(width, width_is_percent, origin.width),
                height.map_or(origin.height, |value| {
                    resolve_axis(value, height_is_percent, origin.height)
                }),
            ),
            Some(BackgroundSize::ExplicitAuto {
                width: None,
                height: Some(height),
                height_is_percent,
                ..
            }) => (
                origin.width,
                resolve_axis(height, height_is_percent, origin.height),
            ),
            _ => (origin.width, origin.height),
        };
        if width <= 0.0 || height <= 0.0 || clip.is_empty() {
            return None;
        }
        let (offset_x, offset_y) = match layer.layer_box.position {
            Some(position) => (
                if position.x_is_percent {
                    (origin.width - width) * position.x
                } else {
                    position.x
                },
                if position.y_is_percent {
                    (origin.height - height) * position.y
                } else {
                    position.y
                },
            ),
            None => (0.0, 0.0),
        };
        Some(Self {
            tile: PdfRect::from_top(
                origin.left + offset_x,
                origin.top() - offset_y,
                width,
                height,
            ),
            clip,
        })
    }
}

#[cfg(test)]
mod geometry_consumer_tests {
    use super::*;

    pub(super) fn asymmetric_geometry() -> PaintBoxGeometry {
        PaintBoxGeometry::new(
            PdfRect::from_top(10.0, 200.0, 100.0, 80.0),
            EdgeSizes::new(3.0, 5.0, 7.0, 11.0),
            EdgeSizes::new(13.0, 17.0, 19.0, 23.0),
        )
    }

    pub(super) fn solid_svg_layer() -> MaskLayer {
        MaskLayer {
            source: MaskLayerSource::Svg(std::sync::Arc::new(
                br#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><rect width="1" height="1" fill="white"/></svg>"#.to_vec(),
            )),
            mode: MaskMode::Alpha,
            layer_box: crate::style::computed::GradientLayerBox {
                repeat: Some(BackgroundRepeat::NoRepeat),
                ..Default::default()
            },
            origin: crate::style::computed::ShapeBox::Content,
            clip: crate::style::computed::ShapeBox::Padding,
            composite: MaskComposite::Add,
        }
    }

    #[test]
    pub(super) fn asymmetric_mask_layer_resolves_vector_tile_and_clip_rects() {
        let mut layer = solid_svg_layer();
        layer.layer_box.size = Some(BackgroundSize::Explicit {
            width: 50.0,
            height: Some(10.0),
            width_is_percent: true,
            height_is_percent: false,
        });
        layer.layer_box.position = Some(BackgroundPosition {
            x: 0.25,
            y: 4.0,
            x_is_percent: true,
            y_is_percent: false,
        });

        let paint = MaskLayerPaint::resolve(&layer, asymmetric_geometry()).unwrap();
        assert_eq!(paint.tile, PdfRect::new(49.5, 170.0, 22.0, 10.0));
        assert_eq!(paint.clip, PdfRect::new(21.0, 127.0, 84.0, 70.0));
    }

    #[test]
    pub(super) fn asymmetric_mask_raster_converts_to_top_down_only_at_pixel_boundary() {
        let geometry = asymmetric_geometry();
        let grid = MaskRasterGrid::new(
            RasterDimensions {
                width: 100,
                height: 80,
            },
            geometry.border_box.width,
            geometry.border_box.height,
        )
        .unwrap();
        let coverage = rasterize_mask_layer(
            &solid_svg_layer(),
            grid.full_window(),
            geometry,
            &crate::parser::svg::SvgDefs::default(),
        )
        .unwrap();
        let sample = |x: usize, y: usize| coverage[y * 100 + x];

        assert_eq!(sample(34, 16), 255);
        assert_eq!(sample(77, 53), 255);
        assert_eq!(sample(33, 16), 0);
        assert_eq!(sample(34, 15), 0);
        assert_eq!(sample(78, 53), 0);
        assert_eq!(sample(77, 54), 0);
    }

    #[test]
    pub(super) fn asymmetric_content_box_clip_path_uses_pdf_top_and_bottom_edges_exactly() {
        let clip = crate::style::computed::ClipPath::Inset {
            top: (2.0, false).into(),
            right: (3.0, false).into(),
            bottom: (5.0, false).into(),
            left: (7.0, false).into(),
            radii: CornerRadii::ZERO,
            geometry_box: crate::style::computed::ShapeBox::Content,
        };
        let mut content = String::new();
        push_clip_path(
            &mut content,
            &clip,
            None,
            asymmetric_geometry().for_fragment(Default::default()),
        );

        assert_eq!(content, "51 151 34 31 re\nW n\n");
    }
}

pub(super) fn resolve_len_percent(v: LengthPercent, extent: f32) -> f32 {
    v.resolve(extent)
}

pub(super) fn resolve_clip_radius(
    radius: crate::style::computed::ClipRadius,
    w: f32,
    h: f32,
    cx: f32,
    cy: f32,
) -> f32 {
    match radius {
        crate::style::computed::ClipRadius::Length(lp) => {
            resolve_len_percent(lp, (w * w + h * h).sqrt() / std::f32::consts::SQRT_2)
        }
        crate::style::computed::ClipRadius::Extent(extent) => match extent {
            crate::style::computed::ShapeExtent::ClosestSide => cx.min(w - cx).min(cy.min(h - cy)),
            crate::style::computed::ShapeExtent::FarthestSide => cx.max(w - cx).max(cy.max(h - cy)),
            crate::style::computed::ShapeExtent::ClosestCorner => {
                let dx = cx.min(w - cx);
                let dy = cy.min(h - cy);
                (dx * dx + dy * dy).sqrt()
            }
            crate::style::computed::ShapeExtent::FarthestCorner => {
                let dx = cx.max(w - cx);
                let dy = cy.max(h - cy);
                (dx * dx + dy * dy).sqrt()
            }
        },
    }
}

pub(super) fn push_clip_path(
    content: &mut String,
    clip: &crate::style::computed::ClipPath,
    svg_defs: Option<&crate::parser::svg::SvgDefs>,
    geometry: FragmentPaintGeometry,
) {
    use crate::style::computed::ClipPath;
    let geometry = geometry.shape_reference();
    match clip {
        ClipPath::Circle {
            r,
            cx,
            cy,
            geometry_box,
        } => {
            let reference = geometry.shape_box(*geometry_box);
            let cx_off = resolve_len_percent(*cx, reference.width);
            let cy_off = resolve_len_percent(*cy, reference.height);
            let cxp = reference.left + cx_off;
            let cyp = reference.top() - cy_off;
            let rad = resolve_clip_radius(*r, reference.width, reference.height, cx_off, cy_off);
            PdfEllipse::circle(PdfPoint::new(cxp, cyp), rad).push_path(content);
        }
        ClipPath::Ellipse {
            rx,
            ry,
            cx,
            cy,
            geometry_box,
        } => {
            let reference = geometry.shape_box(*geometry_box);
            let off_x = resolve_len_percent(*cx, reference.width);
            let off_y = resolve_len_percent(*cy, reference.height);
            let cxp = reference.left + off_x;
            let cyp = reference.top() - off_y;
            let resolve_r =
                |r: crate::style::computed::ClipRadius, axis: f32, other: f32, off: f32| match r {
                    crate::style::computed::ClipRadius::Length(lp) => resolve_len_percent(lp, axis),
                    crate::style::computed::ClipRadius::Extent(
                        crate::style::computed::ShapeExtent::ClosestSide,
                    ) => off.min(axis - off),
                    crate::style::computed::ClipRadius::Extent(
                        crate::style::computed::ShapeExtent::FarthestSide,
                    ) => off.max(axis - off),
                    crate::style::computed::ClipRadius::Extent(_) => {
                        (axis * axis + other * other).sqrt() * 0.5
                    }
                };
            PdfEllipse::new(
                PdfPoint::new(cxp, cyp),
                PdfVector::new(
                    resolve_r(*rx, reference.width, reference.height, off_x),
                    resolve_r(*ry, reference.height, reference.width, off_y),
                ),
            )
            .push_path(content);
        }
        ClipPath::Inset {
            top,
            right,
            bottom,
            left: l,
            radii,
            geometry_box,
        } => {
            let reference = geometry.shape_box(*geometry_box);
            let x0 = reference.left + resolve_len_percent(*l, reference.width);
            let x1 = reference.right() - resolve_len_percent(*right, reference.width);
            let y1 = reference.top() - resolve_len_percent(*top, reference.height);
            let y0 = reference.bottom + resolve_len_percent(*bottom, reference.height);
            let (rw, rh) = ((x1 - x0).max(0.0), (y1 - y0).max(0.0));
            content.push_str(&PdfRect::new(x0, y0, rw, rh).rounded(*radii).path_or_rect());
        }
        ClipPath::Polygon {
            points,
            even_odd,
            geometry_box,
        } => {
            let reference = geometry.shape_box(*geometry_box);
            for (i, (px, py)) in points.iter().enumerate() {
                let x = reference.left + resolve_len_percent(*px, reference.width);
                let y = reference.top() - resolve_len_percent(*py, reference.height);
                content.push_str(&format!("{x} {y} {}\n", if i == 0 { "m" } else { "l" }));
            }
            content.push_str("h\n");
            if *even_odd {
                content.push_str("W* n\n");
                return;
            }
        }
        ClipPath::Path {
            commands,
            geometry_box,
        } => {
            let reference = geometry.shape_box(*geometry_box);
            for cmd in commands {
                match *cmd {
                    crate::parser::svg::PathCommand::MoveTo(x, y) => content.push_str(&format!(
                        "{} {} m\n",
                        reference.left + x * 0.75,
                        reference.top() - y * 0.75
                    )),
                    crate::parser::svg::PathCommand::LineTo(x, y) => content.push_str(&format!(
                        "{} {} l\n",
                        reference.left + x * 0.75,
                        reference.top() - y * 0.75
                    )),
                    crate::parser::svg::PathCommand::CubicTo(x1, y1, x2, y2, x, y) => content
                        .push_str(&format!(
                            "{} {} {} {} {} {} c\n",
                            reference.left + x1 * 0.75,
                            reference.top() - y1 * 0.75,
                            reference.left + x2 * 0.75,
                            reference.top() - y2 * 0.75,
                            reference.left + x * 0.75,
                            reference.top() - y * 0.75
                        )),
                    crate::parser::svg::PathCommand::QuadTo(x1, y1, x, y) => {
                        let cx1 = reference.left + x1 * 0.75;
                        let cy1 = reference.top() - y1 * 0.75;
                        content.push_str(&format!(
                            "{cx1} {cy1} {} {} {} {} c\n",
                            reference.left + x * 0.75,
                            reference.top() - y * 0.75,
                            reference.left + x * 0.75,
                            reference.top() - y * 0.75
                        ));
                    }
                    crate::parser::svg::PathCommand::ClosePath => content.push_str("h\n"),
                }
            }
        }
        ClipPath::Rect {
            x,
            y,
            width,
            height,
            radii,
            geometry_box,
        } => {
            let reference = geometry.shape_box(*geometry_box);
            let rw = resolve_len_percent(*width, reference.width);
            let rh = resolve_len_percent(*height, reference.height);
            let rect = PdfRect::from_top(
                reference.left + resolve_len_percent(*x, reference.width),
                reference.top() - resolve_len_percent(*y, reference.height),
                rw,
                rh,
            );
            content.push_str(&rect.rounded(*radii).path_or_rect());
        }
        ClipPath::Url(id) => {
            let border_box = geometry.border_box;
            if let Some(defs) = svg_defs.filter(|defs| defs.clip_paths.contains_key(id)) {
                crate::render::svg_to_pdf::render_css_clip_path_reference(
                    id,
                    defs,
                    border_box.left,
                    border_box.top(),
                    border_box.width,
                    border_box.height,
                    content,
                );
                return;
            } else {
                content.push_str(&border_box.rect_path());
            }
        }
    }
    content.push_str("W n\n");
}
