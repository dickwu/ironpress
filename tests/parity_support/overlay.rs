//! Classed-diff overlay rendering (spec §3.3 item 2 + §3.3 item 2 edge overlay).
//!
//! `render_classed_overlay` maps every unequal `PixelClass` to its original
//! full-page coordinate on a blank page-sized canvas. The committed `.diff.png`
//! therefore shows only WHAT differed and WHERE, without a crop, reference-page
//! content, or a registration step:
//!   Missing = magenta, Extra = green, ColorErr = blue.
//! It additionally outlines each surviving `DiffRegion`'s bbox in its dominant-
//! class colour (the "edge/region overlay" of §3.3 item 2), so the single image
//! both shows the per-pixel classes AND frames the connected blobs the region
//! table lists. The legend in the HTML report (`report::render_legend`) maps the
//! exact same colours back to classes.
//!
//! The colour table is the single source of truth: `class_rgb` is consumed both
//! here (pixel fill + region frame) and by the HTML legend swatches, so the
//! overlay and its legend can never drift apart. No external deps.

use image::{Rgba, RgbaImage};

use super::compare::{ClassMap, DiffRegion, PixelClass};
use super::config::CSS_PX;

/// The overlay colour for a `PixelClass` (spec §3.3 item 2). The SINGLE source of
/// truth for both the rendered overlay and the HTML legend, so they stay in sync.
pub(crate) fn class_rgb(c: PixelClass) -> [u8; 3] {
    match c {
        // Faint grey for the legend-only Match swatch. Matching pixels are
        // deliberately absent from a diff image.
        PixelClass::Match => [245, 245, 245],
        // Blue: aligned recolour / wrong colour value (incl. colour-space drift).
        PixelClass::ColorErr => [40, 80, 255],
        // Magenta: reference paints, candidate is paper-white.
        PixelClass::Missing => [230, 0, 230],
        // Green: candidate paints where the reference is blank.
        PixelClass::Extra => [0, 200, 60],
    }
}

/// Human label for a `PixelClass`, for the legend (spec §3.3 item 3).
pub(crate) fn class_label(c: PixelClass) -> &'static str {
    c.as_str()
}

/// The legend rows, in the precedence order the classifier assigns them. Shared by
/// the overlay (it is exhaustive over `PixelClass`) and the HTML legend so both
/// describe the same palette.
pub(crate) const LEGEND_ORDER: [PixelClass; 4] = [
    PixelClass::Missing,
    PixelClass::Extra,
    PixelClass::ColorErr,
    PixelClass::Match,
];

/// Render the full-page classed diff overlay (spec §3.3 item 2).
///
/// `page_dimensions` is the original shared page canvas, not a crop.
/// `crop_origin` identifies the same-coordinate origin of `cm`: segmentation can
/// work on a compact union internally, but every visual artifact remains in
/// full-page coordinates. Matching pixels stay blank; unequal pixels use their
/// exact-class colour, and representative region frames stay aligned to the same
/// page coordinates.
pub(crate) fn render_classed_overlay(
    cm: &ClassMap,
    regions: &[DiffRegion],
    page_dimensions: (u32, u32),
    crop_origin: (u32, u32),
) -> RgbaImage {
    let (page_width, page_height) = page_dimensions;
    let mut out = RgbaImage::from_pixel(page_width, page_height, Rgba([255; 4]));

    // 1. Paint unequal crop pixels at their original page coordinates. `Match`
    // remains blank so a diff cannot be mistaken for a reference render.
    for y in 0..cm.h {
        for x in 0..cm.w {
            let c = cm.px[(y as usize) * (cm.w as usize) + x as usize];
            if c == PixelClass::Match {
                continue;
            }
            let Some(page_x) = crop_origin.0.checked_add(x) else {
                continue;
            };
            let Some(page_y) = crop_origin.1.checked_add(y) else {
                continue;
            };
            if page_x >= page_width || page_y >= page_height {
                continue;
            }
            let [r, g, b] = class_rgb(c);
            out.put_pixel(page_x, page_y, Rgba([r, g, b, 255]));
        }
    }

    // 2. Region bbox frames in the dominant-class colour (the edge/region
    // overlay). `bbox_css` is relative to the compact analysis crop, so convert
    // it back to the full shared page before framing.
    for region in regions {
        let [r, g, b] = class_rgb(region.class);
        let frame = Rgba([r, g, b, 255]);
        let Some(x0) = page_coordinate(region.bbox_css[0], crop_origin.0, page_width) else {
            continue;
        };
        let Some(y0) = page_coordinate(region.bbox_css[1], crop_origin.1, page_height) else {
            continue;
        };
        let Some(x1) = page_coordinate(region.bbox_css[2], crop_origin.0, page_width) else {
            continue;
        };
        let Some(y1) = page_coordinate(region.bbox_css[3], crop_origin.1, page_height) else {
            continue;
        };
        for x in x0..=x1 {
            out.put_pixel(x, y0, frame);
            out.put_pixel(x, y1, frame);
        }
        for y in y0..=y1 {
            out.put_pixel(x0, y, frame);
            out.put_pixel(x1, y, frame);
        }
    }

    out
}

/// Convert a crop-relative CSS coordinate back to an in-bounds page pixel.
///
/// A zero-sized page and a crop whose origin lies beyond that page have no
/// drawable coordinate. Otherwise the relative component is bounded *before*
/// adding the origin, so a representative frame that reaches the padded edge
/// remains visible instead of overflowing past the page.
fn page_coordinate(relative_css: f64, crop_origin: u32, page_extent: u32) -> Option<u32> {
    let page_max = page_extent.checked_sub(1)?;
    let remaining = page_max.checked_sub(crop_origin)?;
    let relative_px = (relative_css * CSS_PX)
        .round()
        .clamp(0.0, f64::from(remaining)) as u32;
    Some(crop_origin + relative_px)
}

#[cfg(test)]
mod tests {
    use image::{ImageBuffer, Rgba, RgbaImage};

    use super::render_classed_overlay;
    use crate::parity_support::compare::{ClassMap, CoverageEvidence, DiffRegion, PixelClass};
    use crate::parity_support::config::CSS_PX;

    fn region(class: PixelClass, bbox_css: [f64; 4]) -> DiffRegion {
        DiffRegion {
            bbox_css,
            class,
            area_px: 1,
            longest_span_px: 1,
            area_pct: 0.0,
            modal_drgba: [0; 4],
            delta_e: 0.0,
            interior_color_px: 0,
            coverage: CoverageEvidence::default(),
            large_color_component_is_balanced: false,
            max_direct_delta_e: 0.0,
        }
    }

    #[test]
    fn overlay_keeps_a_blank_full_page_and_places_crop_pixels_at_their_source_coordinates() {
        let mut reference = ImageBuffer::from_pixel(8, 6, Rgba([255, 255, 255, 255]));
        reference.put_pixel(0, 0, Rgba([12, 34, 56, 255]));
        let class_map = ClassMap {
            w: 2,
            h: 2,
            px: vec![
                PixelClass::Match,
                PixelClass::Missing,
                PixelClass::Extra,
                PixelClass::ColorErr,
            ],
        };

        let overlay = render_classed_overlay(&class_map, &[], reference.dimensions(), (3, 2));

        assert_eq!(overlay.dimensions(), reference.dimensions());
        assert_eq!(overlay.get_pixel(0, 0).0, [255; 4]);
        assert_eq!(overlay.get_pixel(3, 2).0, [255; 4]);
        assert_eq!(overlay.get_pixel(4, 2).0, [230, 0, 230, 255]);
        assert_eq!(overlay.get_pixel(3, 3).0, [0, 200, 60, 255]);
        assert_eq!(overlay.get_pixel(4, 3).0, [40, 80, 255, 255]);
    }

    #[test]
    fn region_frames_keep_their_crop_offset_on_a_full_page() {
        let reference = ImageBuffer::from_pixel(7, 5, Rgba([255, 255, 255, 255]));
        let class_map = ClassMap {
            w: 1,
            h: 1,
            px: vec![PixelClass::Match],
        };
        let regions = [region(
            PixelClass::Missing,
            [0.0, 0.0, 1.0 / CSS_PX, 1.0 / CSS_PX],
        )];

        let overlay = render_classed_overlay(&class_map, &regions, reference.dimensions(), (4, 2));

        for (x, y) in [(4, 2), (5, 2), (4, 3), (5, 3)] {
            assert_eq!(overlay.get_pixel(x, y).0, [230, 0, 230, 255]);
        }
        assert_eq!(overlay.get_pixel(0, 0).0, [255; 4]);
    }

    #[test]
    fn padded_page_edges_are_framed_without_an_out_of_bounds_coordinate() {
        // `compare_v2` white-pads unequal candidate/reference canvases before
        // invoking the overlay. This is the larger padded reference canvas.
        let padded_reference = ImageBuffer::from_pixel(7, 5, Rgba([255, 255, 255, 255]));
        let class_map = ClassMap {
            w: 2,
            h: 2,
            px: vec![PixelClass::Match; 4],
        };
        let regions = [region(PixelClass::Extra, [0.0, 0.0, 10.0, 10.0])];

        let overlay =
            render_classed_overlay(&class_map, &regions, padded_reference.dimensions(), (5, 3));

        assert_eq!(overlay.dimensions(), (7, 5));
        assert_eq!(overlay.get_pixel(5, 3).0, [0, 200, 60, 255]);
        assert_eq!(overlay.get_pixel(6, 4).0, [0, 200, 60, 255]);
    }

    #[test]
    fn zero_sized_page_never_attempts_to_draw_a_frame() {
        let reference = RgbaImage::new(0, 0);
        let class_map = ClassMap {
            w: 0,
            h: 0,
            px: Vec::new(),
        };
        let regions = [region(PixelClass::ColorErr, [0.0; 4])];

        let overlay = render_classed_overlay(&class_map, &regions, reference.dimensions(), (0, 0));

        assert_eq!(overlay.dimensions(), (0, 0));
    }
}
