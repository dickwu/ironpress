use super::patterns::LayerTilePattern;
use super::*;

pub(super) fn render_distributed_linear_gradient_raster(
    content: &mut String,
    gradient: &LinearGradient,
    pattern: LayerTilePattern,
    pdf_writer: &mut PdfWriter,
    page_images: &mut Vec<ImageRef>,
) -> bool {
    let paint_box = pattern.paint_box();
    let Some(page) = pdf_writer.page_content_transform.page_bounds() else {
        return false;
    };
    let Some(dimensions) = RasterDimensions::from_point_scales(page.width, page.height, 1.0, 1.0)
    else {
        return false;
    };
    if dimensions.width > MAX_RASTER_TILE_EDGE || dimensions.height > MAX_RASTER_TILE_EDGE {
        return false;
    }
    let tile = pattern.tile_size();
    let Some(sampler) = crate::render::gradient_sampling::LinearGradientSampler::resolve(
        gradient,
        crate::types::Size::new(tile.x, tile.y),
    ) else {
        return false;
    };
    let image = image::RgbaImage::from_fn(dimensions.width, dimensions.height, |px, py| {
        let local = PdfPoint::new(
            page.left + px as f32 - paint_box.left,
            paint_box.top() - (page.top() - py as f32),
        );
        pattern
            .sample(local)
            .map_or(image::Rgba([0, 0, 0, 0]), |sample| {
                image::Rgba(
                    sampler
                        .sample(crate::types::Point::new(sample.x, sample.y))
                        .to_rgba8(),
                )
            })
    });
    draw_gradient_raster_tile(content, pdf_writer, page_images, &image, page);
    true
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_linear_gradient_tile_raster(
    content: &mut String,
    gradient: &LinearGradient,
    rect: PdfRect,
    pdf_writer: &mut PdfWriter,
    page_images: &mut Vec<ImageRef>,
) {
    let PdfRect { width, height, .. } = rect;
    let Some(dimensions) =
        gradient_raster_dimensions(width, height, pdf_writer.opts.raster_quality.filter_dpi)
    else {
        return;
    };
    let Some(sampler) = crate::render::gradient_sampling::LinearGradientSampler::resolve(
        gradient,
        crate::types::Size::new(width, height),
    ) else {
        return;
    };
    draw_tiled_gradient_raster(
        content,
        pdf_writer,
        page_images,
        dimensions,
        rect,
        |px, py| {
            let fx = (px as f32 + 0.5) * width / dimensions.width as f32;
            let fy = (py as f32 + 0.5) * height / dimensions.height as f32;
            image::Rgba(sampler.sample(crate::types::Point::new(fx, fy)).to_rgba8())
        },
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_radial_gradient_tile_raster(
    content: &mut String,
    gradient: &RadialGradient,
    rect: PdfRect,
    pdf_writer: &mut PdfWriter,
    page_images: &mut Vec<ImageRef>,
) {
    let PdfRect { width, height, .. } = rect;
    let Some(dimensions) =
        gradient_raster_dimensions(width, height, pdf_writer.opts.raster_quality.filter_dpi)
    else {
        return;
    };
    let Some(geometry) = RadialGradientGeometry::resolve(gradient, PdfVector::new(width, height))
    else {
        return;
    };
    let Some(ramp) = gradient.ramp.resolve(geometry.stop_basis()) else {
        return;
    };
    let center = geometry.point_center();
    let radii = geometry.point_radii();
    draw_tiled_gradient_raster(
        content,
        pdf_writer,
        page_images,
        dimensions,
        rect,
        |px, py| {
            let fx = (px as f32 + 0.5) * width / dimensions.width as f32;
            let fy = (py as f32 + 0.5) * height / dimensions.height as f32;
            let nx = (fx - center.x) / radii.x;
            let ny = (fy - center.y) / radii.y;
            let t = (nx * nx + ny * ny).sqrt();
            rgba_to_pixel(ramp.sample(t))
        },
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_conic_gradient_tile_raster(
    content: &mut String,
    gradient: &ConicGradient,
    rect: PdfRect,
    pdf_writer: &mut PdfWriter,
    page_images: &mut Vec<ImageRef>,
) {
    let PdfRect { width, height, .. } = rect;
    let Some(dimensions) =
        gradient_raster_dimensions(width, height, pdf_writer.opts.raster_quality.filter_dpi)
    else {
        return;
    };
    let cx = gradient.center.x.resolve(width);
    let cy = gradient.center.y.resolve(height);
    let from = gradient.from_angle.to_radians();
    let Some(ramp) = gradient.ramp.resolve(1.0) else {
        return;
    };
    draw_tiled_gradient_raster(
        content,
        pdf_writer,
        page_images,
        dimensions,
        rect,
        |px, py| {
            rgba_to_pixel(ResolvedGradientRamp::average_samples(
                [
                    (0.25_f32, 0.25_f32),
                    (0.75, 0.25),
                    (0.25, 0.75),
                    (0.75, 0.75),
                ]
                .map(|(ox, oy)| {
                    let fx = (px as f32 + ox) * width / dimensions.width as f32;
                    let fy = (py as f32 + oy) * height / dimensions.height as f32;
                    let dx = fx - cx;
                    let dy = fy - cy;
                    let angle = (dx.atan2(-dy) - from).rem_euclid(std::f32::consts::TAU);
                    let t = angle / std::f32::consts::TAU;
                    ramp.sample(t)
                }),
            ))
        },
    );
}
