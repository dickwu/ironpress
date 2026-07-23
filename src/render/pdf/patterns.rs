use super::geometry::{PdfMatrix, PdfPoint, PdfRect, PdfVector};
use super::transforms::{PageContentTransform, PdfPaintSpace};
use super::{ImageRef, PdfResourceUsage, PdfWriter};
use crate::render::background::{BackgroundRepeatModes, BackgroundTilePattern};
use crate::render::pdf_syntax::format_pdf_number;
use crate::render::pdf_syntax::format_pdf_number_fixed;
use crate::render::shading::{PdfGradientStops, PdfShadingKind, build_shading_function};
use crate::style::computed::{BackgroundRepeat, GradientLayerBox};
use crate::types::Size;
use crate::util::{AxisRepeatPattern, AxisRepeatPlacements, RasterDimensions};

/// Above this many cells, a PDF tiling pattern remains the compact rendering.
/// `space` and `round` normally produce only a few cells; painting that bounded
/// grid directly avoids a renderer-visible seam at every PDF pattern boundary.
const MAX_DIRECT_DISTRIBUTED_TILES: usize = 256;

pub(super) type RepeatModes = BackgroundRepeatModes;

#[derive(Debug, Clone, Copy)]
pub(super) struct LayerTilePattern {
    paint_box: PdfRect,
    x: AxisRepeatPattern,
    y: AxisRepeatPattern,
    distributed_repeat: bool,
}

impl LayerTilePattern {
    pub(super) const fn new(
        paint_box: PdfRect,
        x: AxisRepeatPattern,
        y: AxisRepeatPattern,
    ) -> Self {
        Self {
            paint_box,
            x,
            y,
            distributed_repeat: false,
        }
    }

    pub(super) const fn with_distributed_repeat(mut self, distributed_repeat: bool) -> Self {
        self.distributed_repeat = distributed_repeat;
        self
    }

    pub(super) fn tile_size(self) -> PdfVector {
        PdfVector::new(self.x.tile_size(), self.y.tile_size())
    }

    pub(super) fn first_tile(self) -> Option<PdfRect> {
        let origin = PdfPoint::new(
            self.x.placements(0.0, self.paint_box.width)?.next()?,
            self.y.placements(0.0, self.paint_box.height)?.next()?,
        );
        let size = self.tile_size();
        Some(PdfRect::new(
            self.paint_box.left + origin.x,
            self.paint_box.top() - origin.y - size.y,
            size.x,
            size.y,
        ))
    }

    pub(super) fn is_single(self) -> bool {
        self.x.is_single_in(0.0, self.paint_box.width)
            && self.y.is_single_in(0.0, self.paint_box.height)
    }

    pub(super) fn paint_box(self) -> PdfRect {
        self.paint_box
    }

    pub(super) fn sample(self, point: PdfPoint) -> Option<PdfPoint> {
        Some(PdfPoint::new(
            self.x.sample(point.x)?,
            self.y.sample(point.y)?,
        ))
    }

    fn tiles(self) -> Option<LayerTilePlacements> {
        Some(LayerTilePlacements {
            paint_box: self.paint_box,
            tile_size: self.tile_size(),
            x: self.x.placements(0.0, self.paint_box.width)?,
            y_pattern: self.y,
            current_x: None,
            y: None,
        })
    }

    fn has_at_most_direct_tiles(self) -> bool {
        let Some(mut tiles) = self.tiles() else {
            return false;
        };
        for _ in 0..=MAX_DIRECT_DISTRIBUTED_TILES {
            if tiles.next().is_none() {
                return true;
            }
        }
        false
    }

    pub(super) fn pdf_pattern(self, bbox: PdfRect) -> Option<PdfTilingPattern> {
        let axis_step = |pattern: AxisRepeatPattern, extent: f32| {
            pattern.stride().or_else(|| {
                let first = pattern.first();
                let size = pattern.tile_size();
                let step = (extent - first).max(first + size).max(size);
                (step.is_finite() && step > 0.0).then_some(step)
            })
        };
        let first_tile = self.first_tile()?;
        Some(PdfTilingPattern {
            bbox,
            paint_box: PdfRect::new(0.0, 0.0, self.paint_box.width, self.paint_box.height),
            step: PdfVector::new(
                axis_step(self.x, self.paint_box.width)?,
                axis_step(self.y, self.paint_box.height)?,
            ),
            transform: PdfMatrix::translate(PdfPoint::new(
                first_tile.left - self.paint_box.left,
                first_tile.bottom - self.paint_box.bottom,
            )),
        })
    }

    pub(super) fn pdf_raster_pattern(self, source: RasterDimensions) -> Option<PdfTilingPattern> {
        let tile_size = self.tile_size();
        let mut pattern = self.pdf_pattern(PdfRect::new(0.0, 0.0, tile_size.x, tile_size.y))?;
        let scale = tile_size
            .component_quotient(PdfVector::new(source.width as f32, source.height as f32))?;
        pattern.bbox = PdfRect::new(0.0, 0.0, source.width as f32, source.height as f32);
        pattern.step = pattern.step.component_quotient(scale)?;
        pattern.transform = pattern.transform * PdfMatrix::scale(scale);
        Some(pattern)
    }

    /// Describe a raster cell directly in default page space. Chromium anchors
    /// the pattern axes to the transformed page bounds, then carries the tile
    /// phase in the cell BBox instead of in the matrix translation.
    pub(super) fn pdf_page_raster_pattern(
        self,
        source: RasterDimensions,
        paint_space: PdfPaintSpace,
    ) -> Option<PdfTilingPattern> {
        let first_tile = self.first_tile()?;
        let source_size = PdfVector::new(source.width as f32, source.height as f32);
        let scale = self.tile_size().component_quotient(source_size)?;
        let step = PdfVector::new(
            self.x.stride().unwrap_or(self.paint_box.width),
            self.y.stride().unwrap_or(self.paint_box.height),
        )
        .component_quotient(scale)?;
        let placement = paint_space.raster_cell_to_default(
            PdfPoint::new(first_tile.left, first_tile.top()),
            PdfVector::new(scale.x, -scale.y),
        )?;
        let placed = placement.placed;
        let transform = placement.pattern_transform;
        let pattern_origin = transform.inverse()?.transform_point(placed.translation);
        Some(PdfTilingPattern {
            bbox: PdfRect::new(
                pattern_origin.x,
                pattern_origin.y,
                source_size.x,
                source_size.y,
            ),
            paint_box: self.paint_box,
            step,
            transform,
        })
    }
}

struct LayerTilePlacements {
    paint_box: PdfRect,
    tile_size: PdfVector,
    x: AxisRepeatPlacements,
    y_pattern: AxisRepeatPattern,
    current_x: Option<f32>,
    y: Option<AxisRepeatPlacements>,
}

impl Iterator for LayerTilePlacements {
    type Item = PdfRect;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(y) = &mut self.y
                && let Some(y) = y.next()
            {
                let x = self.current_x?;
                return Some(PdfRect::new(
                    self.paint_box.left + x,
                    self.paint_box.top() - y - self.tile_size.y,
                    self.tile_size.x,
                    self.tile_size.y,
                ));
            }
            let x = self.x.next()?;
            self.current_x = Some(x);
            self.y = self.y_pattern.placements(0.0, self.paint_box.height);
        }
    }
}

/// Paint a small, bounded `space`/`round` grid without a PDF tiling pattern.
/// Each cell is still rendered by the normal vector/raster gradient path; this
/// merely puts its clip directly in page content so viewers cannot anti-alias
/// one pattern cell against the next.
pub(super) fn paint_distributed_tiles(
    content: &mut String,
    pattern: LayerTilePattern,
    mut paint: impl FnMut(&mut String, PdfRect),
) -> bool {
    if !pattern.distributed_repeat || !pattern.has_at_most_direct_tiles() {
        return false;
    }
    let Some(tiles) = pattern.tiles() else {
        return false;
    };
    content.push_str("q\n");
    content.push_str(&pattern.paint_box.rect_path());
    content.push_str("W n\n");
    for tile in tiles {
        paint(content, tile);
    }
    content.push_str("Q\n");
    true
}

pub(super) fn gradient_layer_pattern(
    layer_box: &GradientLayerBox,
    paint_box: PdfRect,
) -> Option<LayerTilePattern> {
    let PdfRect { width, height, .. } = paint_box;
    let tiles = BackgroundTilePattern::resolve(
        layer_box.size.unwrap_or_default(),
        layer_box.position.unwrap_or_default(),
        layer_box.repeat.unwrap_or(BackgroundRepeat::Repeat),
        Size::new(width, height),
    )?;
    let (horizontal, vertical) = tiles.axes();
    Some(
        LayerTilePattern::new(paint_box, horizontal, vertical)
            .with_distributed_repeat(tiles.has_distributed_repeat()),
    )
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct PdfTilingPattern {
    pub(super) bbox: PdfRect,
    pub(super) paint_box: PdfRect,
    pub(super) step: PdfVector,
    pub(super) transform: PdfMatrix,
}

#[derive(Debug)]
pub(super) struct PdfTilingPatternEntry {
    pub(super) pattern_id: usize,
    pub(super) target: PdfTilingPatternTarget,
    pub(super) resources: PdfResourceUsage,
}

#[derive(Debug)]
pub(super) enum PdfTilingPatternTarget {
    Page { name: String },
    Form { object_id: usize },
}

#[derive(Debug)]
pub(super) struct PdfPatternEntry {
    pub(super) name: String,
    pub(super) object_id: usize,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum PdfPatternGeometryFormat {
    Shortest,
    SixDecimals,
}

impl PdfPatternGeometryFormat {
    fn number(self, value: f32) -> String {
        if matches!(self, Self::Shortest) {
            return format_pdf_number(value);
        }
        let mut text = format!("{value:.6}");
        while text.contains('.') && text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
        if text == "-0" {
            return "0".to_string();
        }
        text
    }
}

#[derive(Debug, Clone, Copy)]
struct PdfCircle {
    center: PdfPoint,
    radius: f32,
}

impl PdfCircle {
    const fn new(center: PdfPoint, radius: f32) -> Self {
        Self { center, radius }
    }
}

#[derive(Debug, Clone, Copy)]
enum PdfShadingGeometry {
    Axial { start: PdfPoint, end: PdfPoint },
    Radial { start: PdfCircle, end: PdfCircle },
}

impl PdfShadingGeometry {
    fn pdf(self) -> (PdfShadingKind, [f32; 6], usize) {
        match self {
            Self::Axial { start, end } => (
                PdfShadingKind::Axial,
                [start.x, start.y, end.x, end.y, 0.0, 0.0],
                4,
            ),
            Self::Radial { start, end } => (
                PdfShadingKind::Radial,
                [
                    start.center.x,
                    start.center.y,
                    start.radius,
                    end.center.x,
                    end.center.y,
                    end.radius,
                ],
                6,
            ),
        }
    }
}

pub(super) struct PdfShadingPattern {
    geometry: PdfShadingGeometry,
    transform: PdfMatrix,
    stops: PdfGradientStops,
    geometry_format: PdfPatternGeometryFormat,
}

pub(super) struct PdfFunctionPattern {
    transform: PdfMatrix,
    domain: PdfRect,
    calculator: String,
}

impl PdfFunctionPattern {
    pub(super) fn new(transform: PdfMatrix, domain: PdfRect, calculator: String) -> Option<Self> {
        (transform.is_invertible() && !domain.is_empty()).then_some(Self {
            transform,
            domain,
            calculator,
        })
    }
}

impl PdfShadingPattern {
    pub(super) const fn axial(
        start: PdfPoint,
        end: PdfPoint,
        transform: PdfMatrix,
        stops: PdfGradientStops,
        geometry_format: PdfPatternGeometryFormat,
    ) -> Self {
        Self {
            geometry: PdfShadingGeometry::Axial { start, end },
            transform,
            stops,
            geometry_format,
        }
    }

    pub(super) const fn radial(
        center: PdfPoint,
        end_radius: f32,
        transform: PdfMatrix,
        stops: PdfGradientStops,
        geometry_format: PdfPatternGeometryFormat,
    ) -> Self {
        Self {
            geometry: PdfShadingGeometry::Radial {
                start: PdfCircle::new(center, 0.0),
                end: PdfCircle::new(center, end_radius),
            },
            transform,
            stops,
            geometry_format,
        }
    }
}

pub(super) fn paint_tiling_pattern(content: &mut String, form: &ImageRef, rect: PdfRect) {
    content.push_str(&format!(
        "q\n1 0 0 1 {left} {bottom} cm\n/{name} Do\nQ\n",
        left = rect.left,
        bottom = rect.bottom,
        name = form.name,
    ));
}

pub(super) fn paint_shading_pattern(content: &mut String, name: &str, tile: PdfRect) {
    content.push_str("q\n/Pattern cs\n");
    content.push_str(&format!("/{name} scn\n"));
    content.push_str(&tile.rect_path());
    content.push_str("f\nQ\n");
}

pub(super) fn paint_page_tiling_pattern(content: &mut String, name: &str, rect: PdfRect) {
    content.push_str("q\n/Pattern cs\n");
    content.push_str(&format!("/{name} scn\n"));
    content.push_str(&rect.rect_path());
    content.push_str("f\nQ\n");
}

pub(super) fn paint_css_box_pattern(
    content: &mut String,
    page: PageContentTransform,
    name: &str,
    rect: PdfRect,
) -> Option<()> {
    let origin = PdfPoint::new(rect.left, rect.top());
    let size = PdfVector::new(
        rect.width / crate::fonts::PT_PER_CSS_PX,
        rect.height / crate::fonts::PT_PER_CSS_PX,
    );
    let operator = page.css_box_operator(origin)?;
    content.push_str("q\n");
    content.push_str(&operator);
    content.push_str(&format!(
        "/Pattern cs\n/{name} scn\n0 0 {} {} re\nf\nQ\n",
        size.x, size.y,
    ));
    Some(())
}

pub(super) fn paint_css_page_pattern(
    content: &mut String,
    page_transform: PageContentTransform,
    name: &str,
    rect: PdfRect,
) -> Option<()> {
    let page = page_transform.page_bounds()?;
    let scale = crate::fonts::PT_PER_CSS_PX;
    let left = (rect.left - page.left) / scale;
    let top = (page.top() - rect.top()) / scale;
    let width = rect.width / scale;
    let height = rect.height / scale;
    content.push_str("q\n");
    content.push_str(&page_transform.css_box_operator(PdfPoint::new(page.left, page.top()))?);
    content.push_str(&format!(
        "/Pattern cs\n/{name} scn\n{left} {top} {width} {height} re\nf\nQ\n",
    ));
    Some(())
}

impl PdfWriter {
    /// Wrap one masked solid in a page-space pattern cell. PDF soft-mask
    /// coverage at the painted box edge depends on the cell and its consumer
    /// being composited as one transparency primitive; applying the mask
    /// directly to the consumer path produces a second, observably different
    /// edge rasterization.
    pub(super) fn add_masked_solid_page_pattern(
        &mut self,
        paint_box: PdfRect,
        mask: &str,
        color: (f32, f32, f32),
    ) -> Option<String> {
        const CELL_GAP: f32 = 2.0;

        let cell_box = self
            .page_content_transform
            .page_bounds()
            .unwrap_or(paint_box);
        let color_pattern = self.add_premultiplied_solid_pattern(cell_box, color)?;
        let stream = format!(
            "/{mask} gs\n/Pattern CS/Pattern cs\n/{color_pattern} SCN/{color_pattern} scn\n{}f*\n",
            cell_box.rect_path(),
        );
        self.add_page_tiling_pattern(
            stream,
            PdfTilingPattern {
                bbox: cell_box,
                paint_box: cell_box,
                step: PdfVector::new(cell_box.width + CELL_GAP, cell_box.height + CELL_GAP),
                ..Default::default()
            },
        )
    }

    /// Emit the function-based color half of a premultiplied transparent-to-
    /// solid gradient. The companion luminosity mask carries alpha; this
    /// calculator reconstructs straight RGB from the same premultiplied ramp,
    /// matching the PDF transparency model at every sample.
    fn add_premultiplied_solid_pattern(
        &mut self,
        page: PdfRect,
        color: (f32, f32, f32),
    ) -> Option<String> {
        if page.is_empty() {
            return None;
        }
        let (red, green, blue) = color;
        let exact = [red, green, blue].map(|value| format_pdf_number_fixed(f64::from(value), 8));
        let terminal = [red, green, blue].map(|value| format_pdf_number_fixed(f64::from(value), 4));
        let calculator = format!(
            "{{pop\n\
dup 0 le {{pop 0 0 0 0 0}} if\n\
dup dup 0 gt exch 1 le and {{\n\
0 sub dup {} mul exch dup {} mul exch dup {} mul exch \n\
0}} if\n\
0 gt {{{} {} {} 1}} if\n\
dup abs 0.00001 lt{{ pop pop pop pop 0 0 0 }}{{ dup 3 1 roll div 4 1 roll dup 3 1 roll div 4 1 roll div 3 1 roll}} ifelse\n\
}}\n",
            exact[0], exact[1], exact[2], terminal[0], terminal[1], terminal[2],
        );
        let domain_y = format_pdf_number_fixed(f64::from(page.height / page.width), 8);
        let function_id = self.next_id();
        self.objects.push(format!(
            "{function_id} 0 obj\n<< /FunctionType 4 /Domain [0 1 0 {domain_y}] /Range [0 1 0 1 0 1] /Length {} >>\nstream\n",
            calculator.len(),
        ));
        self.binary_objects
            .insert(function_id, calculator.into_bytes());

        let object_id = self.next_id();
        let name = format!("PSh{}", self.pdf_patterns.len());
        self.objects.push(format!(
            "{object_id} 0 obj\n<< /Type /Pattern /PatternType 2 /Matrix [{width} 0 0 -{width} {left} {top}] /Shading << /Domain [0 1 0 {domain_y}] /Function {function_id} 0 R /ShadingType 1 /ColorSpace /DeviceRGB >> >>\nendobj",
            width = page.width,
            left = page.left,
            top = page.top(),
        ));
        self.pdf_patterns.push(PdfPatternEntry {
            name: name.clone(),
            object_id,
        });
        Some(name)
    }

    pub(super) fn add_shading_pattern(&mut self, pattern: PdfShadingPattern) -> String {
        let PdfShadingPattern {
            geometry,
            transform,
            stops,
            geometry_format,
        } = pattern;
        let (kind, coordinates, coordinate_count) = geometry.pdf();
        let object_id = self.next_id();
        let name = format!("PSh{}", self.pdf_patterns.len());
        let coordinates = coordinates[..coordinate_count]
            .iter()
            .copied()
            .map(|value| geometry_format.number(value))
            .collect::<Vec<_>>()
            .join(" ");
        let matrix = transform
            .components()
            .into_iter()
            .map(|value| geometry_format.number(value))
            .collect::<Vec<_>>()
            .join(" ");
        let function = build_shading_function(&stops);
        self.objects.push(format!(
            "{object_id} 0 obj\n<< /Type /Pattern /PatternType 2 /Matrix [{matrix}] /Shading << /ShadingType {} /ColorSpace /DeviceRGB /Coords [{coordinates}] /Function {function} /Extend [true true] >> >>\nendobj",
            kind.pdf_type(),
        ));
        self.pdf_patterns.push(PdfPatternEntry {
            name: name.clone(),
            object_id,
        });
        name
    }

    pub(super) fn add_function_pattern(&mut self, pattern: PdfFunctionPattern) -> String {
        let PdfFunctionPattern {
            transform,
            domain,
            calculator,
        } = pattern;
        let function_id = self.next_id();
        self.objects.push(format!(
            "{function_id} 0 obj\n<< /FunctionType 4 /Domain [{}] /Range [0 1 0 1 0 1] /Length {} >>\nstream\n",
            domain
                .xy_domain()
                .into_iter()
                .map(format_pdf_number)
                .collect::<Vec<_>>()
                .join(" "),
            calculator.len(),
        ));
        self.binary_objects
            .insert(function_id, calculator.into_bytes());

        let object_id = self.next_id();
        let name = format!("PSh{}", self.pdf_patterns.len());
        let matrix = transform
            .components()
            .into_iter()
            .map(format_pdf_number)
            .collect::<Vec<_>>()
            .join(" ");
        let domain = domain
            .xy_domain()
            .into_iter()
            .map(format_pdf_number)
            .collect::<Vec<_>>()
            .join(" ");
        self.objects.push(format!(
            "{object_id} 0 obj\n<< /Type /Pattern /PatternType 2 /Matrix [{matrix}] /Shading << /Domain [{domain}] /Function {function_id} 0 R /ShadingType 1 /ColorSpace /DeviceRGB >> >>\nendobj",
        ));
        self.pdf_patterns.push(PdfPatternEntry {
            name: name.clone(),
            object_id,
        });
        name
    }

    pub(super) fn add_tiling_pattern(
        &mut self,
        stream: String,
        pattern: PdfTilingPattern,
    ) -> Option<ImageRef> {
        let (pattern_id, resources) = self.add_tiling_pattern_object(stream, pattern)?;
        let form_id = self.next_id();
        let form_name = format!("Fm{form_id}");
        let form_stream = format!(
            "q\n/Pattern cs\n/Cell scn\n0 0 {width} {height} re\nf\nQ\n",
            width = pattern.paint_box.width,
            height = pattern.paint_box.height,
        );
        self.objects.push(format!(
            "{form_id} 0 obj\n<< /Type /XObject /Subtype /Form /FormType 1 /BBox [0 0 {width} {height}] /Resources << /Pattern << /Cell {pattern_id} 0 R >> >> /Length {len} >>\nstream\n",
            width = pattern.paint_box.width,
            height = pattern.paint_box.height,
            len = form_stream.len(),
        ));
        self.binary_objects
            .insert(form_id, form_stream.into_bytes());
        self.tiling_patterns.push(PdfTilingPatternEntry {
            pattern_id,
            target: PdfTilingPatternTarget::Form { object_id: form_id },
            resources,
        });
        Some(ImageRef {
            name: form_name,
            obj_id: form_id,
        })
    }

    pub(super) fn add_page_tiling_pattern(
        &mut self,
        stream: String,
        pattern: PdfTilingPattern,
    ) -> Option<String> {
        let (pattern_id, resources) = self.add_tiling_pattern_object(stream, pattern)?;
        let name = format!("PT{}", self.tiling_patterns.len());
        self.tiling_patterns.push(PdfTilingPatternEntry {
            pattern_id,
            target: PdfTilingPatternTarget::Page { name: name.clone() },
            resources,
        });
        Some(name)
    }

    fn add_tiling_pattern_object(
        &mut self,
        stream: String,
        pattern: PdfTilingPattern,
    ) -> Option<(usize, PdfResourceUsage)> {
        if pattern.bbox.is_empty()
            || pattern.paint_box.is_empty()
            || !pattern.step.is_positive()
            || !pattern.transform.is_invertible()
        {
            return None;
        }
        let PdfVector {
            x: step_x,
            y: step_y,
        } = pattern.step;
        let [a, b, c, d, e, f] = pattern.transform.components();
        let matrix = (pattern.transform != PdfMatrix::IDENTITY)
            .then(|| format!(" /Matrix [{a} {b} {c} {d} {e} {f}]"))
            .unwrap_or_default();
        let resources = PdfResourceUsage::from_stream(&stream);
        let pattern_id = self.next_id();
        let bytes = stream.into_bytes();
        self.objects.push(format!(
            "{pattern_id} 0 obj\n<< /Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 /BBox [{left} {bottom} {right} {top}] /XStep {step_x} /YStep {step_y}{matrix} /Resources __IP_PATTERN_CELL_RESOURCES__ /Length {len} >>\nstream\n",
            left = pattern.bbox.left,
            bottom = pattern.bbox.bottom,
            right = pattern.bbox.right(),
            top = pattern.bbox.top(),
            matrix = matrix,
            len = bytes.len(),
        ));
        self.binary_objects.insert(pattern_id, bytes);
        Some((pattern_id, resources))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::AxisRepeatMode;

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-4, "{actual} != {expected}");
    }

    #[test]
    fn page_pattern_carries_tile_phase_in_its_bbox() {
        let paint_box = PdfRect::new(44.25, 42.75, 91.5, 55.5);
        let pattern = LayerTilePattern::new(
            paint_box,
            AxisRepeatPattern::new(AxisRepeatMode::Repeat, 0.0, 13.5, paint_box.width).unwrap(),
            AxisRepeatPattern::new(AxisRepeatMode::Repeat, 0.0, 10.5, paint_box.height).unwrap(),
        );
        let to_default = PdfMatrix::new(
            PdfVector::new(1.032_809_1, -0.315_761_45),
            PdfVector::new(0.268_981_96, 0.879_800_4),
            PdfPoint::new(-19.095_955, 29.968_697),
        );
        let page_bounds = PdfRect::new(0.0, 0.0, 180.0, 138.0);

        let pdf_pattern = pattern
            .pdf_page_raster_pattern(
                RasterDimensions {
                    width: 4,
                    height: 4,
                },
                PdfPaintSpace::new(to_default, PageContentTransform::default(), page_bounds),
            )
            .unwrap();

        assert_eq!(pdf_pattern.step, PdfVector::new(4.0, 4.0));
        assert_eq!(pdf_pattern.bbox.width, 4.0);
        assert_eq!(pdf_pattern.bbox.height, 4.0);
        assert!(pdf_pattern.transform.y_axis.y < 0.0);

        let page_in_pattern =
            page_bounds.transformed_bounds(pdf_pattern.transform.inverse().unwrap());
        assert_close(page_in_pattern.left, 0.0);
        assert_close(page_in_pattern.bottom, 0.0);

        let tile_anchor = pdf_pattern.transform.transform_point(PdfPoint::new(
            pdf_pattern.bbox.left,
            pdf_pattern.bbox.bottom,
        ));
        let expected_anchor =
            to_default.transform_point(PdfPoint::new(paint_box.left, paint_box.top()));
        assert_close(tile_anchor.x, expected_anchor.x);
        assert_close(tile_anchor.y, expected_anchor.y);
    }

    #[test]
    fn distributed_gradient_tiles_keep_css_space_and_round_geometry() {
        let paint_box = PdfRect::new(0.0, 0.0, 135.0, 67.5);
        let pattern = LayerTilePattern::new(
            paint_box,
            AxisRepeatPattern::new(AxisRepeatMode::Space, 0.0, 30.0, paint_box.width).unwrap(),
            AxisRepeatPattern::new(AxisRepeatMode::Round, 0.0, 18.0, paint_box.height).unwrap(),
        )
        .with_distributed_repeat(true);

        let tiles: Vec<_> = pattern.tiles().unwrap().collect();
        assert_eq!(tiles.len(), 16);
        assert_eq!(tiles[0], PdfRect::new(0.0, 50.625, 30.0, 16.875));
        assert_eq!(tiles[4], PdfRect::new(35.0, 50.625, 30.0, 16.875));
        assert_eq!(tiles[15], PdfRect::new(105.0, 0.0, 30.0, 16.875));
        assert!(pattern.has_at_most_direct_tiles());
    }

    #[test]
    fn distributed_gradient_tiles_fall_back_before_expanding_a_large_grid() {
        let paint_box = PdfRect::new(0.0, 0.0, 1_000.0, 1_000.0);
        let pattern = LayerTilePattern::new(
            paint_box,
            AxisRepeatPattern::new(AxisRepeatMode::Space, 0.0, 1.0, paint_box.width).unwrap(),
            AxisRepeatPattern::new(AxisRepeatMode::Round, 0.0, 1.0, paint_box.height).unwrap(),
        )
        .with_distributed_repeat(true);

        assert!(!pattern.has_at_most_direct_tiles());
    }
}
