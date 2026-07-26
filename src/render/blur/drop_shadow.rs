//! CSS `filter: drop-shadow()` source and shadow compositing.

use super::*;

struct DropShadowSurface {
    painted: image::RgbaImage,
    kernel: Option<FilterBlurKernel>,
    offset: DeviceOffset,
    padding: u32,
    width: u32,
    height: u32,
    scale: f32,
}

impl DropShadowSurface {
    fn new(
        source: &image::RgbaImage,
        display_w_pt: f32,
        display_h_pt: f32,
        shadow: DropShadow,
        image_rendering: ImageRendering,
        filter_dpi: f32,
    ) -> Option<Self> {
        if display_w_pt <= 0.0
            || display_h_pt <= 0.0
            || !shadow.blur.is_finite()
            || !shadow.dx.is_finite()
            || !shadow.dy.is_finite()
            || source.width() == 0
            || source.height() == 0
        {
            return None;
        }
        let scale = filter_dpi_scale(filter_dpi);
        let content_width = filter_raster_pixels(display_w_pt, scale)?;
        let content_height = filter_raster_pixels(display_h_pt, scale)?;
        let painted = rasterize_image_buffer(
            source,
            display_w_pt,
            display_h_pt,
            image_rendering,
            filter_dpi,
        )?;
        let kernel = (shadow.blur > 0.0)
            .then(|| FilterBlurKernel::new(shadow.blur, filter_dpi))
            .flatten();
        let offset = DeviceOffset::from_points(shadow.dx, shadow.dy, scale);
        let padding = kernel
            .map_or(1, |kernel| kernel.padding_px)
            .max(offset.padding()?)
            .checked_add(1)?;
        Some(Self {
            painted,
            kernel,
            offset,
            padding,
            width: padded_pixels(content_width, padding)?,
            height: padded_pixels(content_height, padding)?,
            scale,
        })
    }

    fn shadow_layer(self, shadow: DropShadow) -> Option<(image::RgbaImage, Self)> {
        let mut layer = image::RgbaImage::new(self.width, self.height);
        let [r, g, b, alpha] = shadow.color.to_rgba8();
        let color = [r, g, b];
        paint_translated_alpha(
            &mut layer,
            &self.painted,
            self.padding,
            self.offset,
            color,
            f32::from(alpha) / 255.0,
        );
        let layer = match self.kernel {
            Some(kernel) => blur_css_filter(&layer, kernel)?,
            None => layer,
        };
        Some((layer, self))
    }

    fn overflow_pt(&self) -> f32 {
        self.padding as f32 / self.scale * PT_PER_PX
    }
}

/// Build a `drop-shadow()` replacement raster containing both the shadow and the
/// source image on one configured-resolution filter surface.
pub(crate) fn drop_shadow_image(
    source: &image::RgbaImage,
    display_w_pt: f32,
    display_h_pt: f32,
    shadow: DropShadow,
    image_rendering: ImageRendering,
    filter_dpi: f32,
) -> Option<BlurredRaster> {
    let (mut composed, surface) = DropShadowSurface::new(
        source,
        display_w_pt,
        display_h_pt,
        shadow,
        image_rendering,
        filter_dpi,
    )?
    .shadow_layer(shadow)?;

    for y in 0..surface.painted.height() {
        for x in 0..surface.painted.width() {
            let src = *surface.painted.get_pixel(x, y);
            if src[3] == 0 {
                continue;
            }
            let dx0 = x + surface.padding;
            let dy0 = y + surface.padding;
            let bg = *composed.get_pixel(dx0, dy0);
            composed.put_pixel(dx0, dy0, over(src, bg));
        }
    }

    let overflow_pt = surface.overflow_pt();
    let asset = rgba_to_png_alpha_asset(composed, filter_dpi)?;
    Some(BlurredRaster { asset, overflow_pt })
}

/// A CSS filter offset converted to the device-pixel coordinates of its filter
/// surface. Keeping both axes together prevents accidental mixed rounding.
#[derive(Clone, Copy)]
struct DeviceOffset {
    x: f32,
    y: f32,
}

impl DeviceOffset {
    fn from_points(x: f32, y: f32, device_scale: f32) -> Self {
        Self {
            x: x / PT_PER_PX * device_scale,
            y: y / PT_PER_PX * device_scale,
        }
    }

    fn padding(self) -> Option<u32> {
        nonnegative_pixel_ceil(self.x.abs())
            .zip(nonnegative_pixel_ceil(self.y.abs()))
            .map(|(horizontal, vertical)| horizontal.max(vertical))
    }
}

/// Translate a source alpha field without discarding fractional device-pixel
/// offsets. Depositing each source sample into its four destination neighbours
/// is bilinear resampling of the alpha field; a single rounded placement makes
/// a 0.5-pixel CSS offset visibly wider on opposite sides of a hard shadow.
fn paint_translated_alpha(
    target: &mut image::RgbaImage,
    source: &image::RgbaImage,
    padding: u32,
    offset: DeviceOffset,
    color: [u8; 3],
    opacity: f32,
) {
    let width = target.width();
    let height = target.height();
    let mut alpha = vec![0.0; target.pixels().len()];
    for y in 0..source.height() {
        for x in 0..source.width() {
            let coverage = f32::from(source.get_pixel(x, y)[3]) / 255.0 * opacity;
            if coverage == 0.0 {
                continue;
            }
            let destination_x = x as f32 + padding as f32 + offset.x;
            let destination_y = y as f32 + padding as f32 + offset.y;
            let left = destination_x.floor() as i32;
            let top = destination_y.floor() as i32;
            let horizontal = destination_x - left as f32;
            let vertical = destination_y - top as f32;
            for (dy, vertical_weight) in [(0, 1.0 - vertical), (1, vertical)] {
                for (dx, horizontal_weight) in [(0, 1.0 - horizontal), (1, horizontal)] {
                    let target_x = left + dx;
                    let target_y = top + dy;
                    if target_x < 0
                        || target_y < 0
                        || target_x >= width as i32
                        || target_y >= height as i32
                    {
                        continue;
                    }
                    let index = target_y as usize * width as usize + target_x as usize;
                    alpha[index] += coverage * horizontal_weight * vertical_weight;
                }
            }
        }
    }
    for (pixel, coverage) in target.pixels_mut().zip(alpha) {
        let alpha = (coverage.clamp(0.0, 1.0) * 255.0).round() as u8;
        if alpha != 0 {
            *pixel = image::Rgba([color[0], color[1], color[2], alpha]);
        }
    }
}

/// Source-over composite of `src` onto `bg`, both straight-alpha RGBA8.
fn over(src: image::Rgba<u8>, bg: image::Rgba<u8>) -> image::Rgba<u8> {
    let sa = src[3] as f32 / 255.0;
    let ba = bg[3] as f32 / 255.0;
    let oa = sa + ba * (1.0 - sa);
    if oa <= 0.0 {
        return image::Rgba([0, 0, 0, 0]);
    }
    let blend = |s: u8, b: u8| {
        let s = s as f32;
        let b = b as f32;
        ((s * sa + b * ba * (1.0 - sa)) / oa)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    image::Rgba([
        blend(src[0], bg[0]),
        blend(src[1], bg[1]),
        blend(src[2], bg[2]),
        (oa * 255.0).round() as u8,
    ])
}
