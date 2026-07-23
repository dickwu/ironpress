use super::geometry::{PdfMatrix, PdfPoint, PdfRect, PdfVector};
use crate::layout::print_scale::PrintContentScale;
use crate::render::pdf_syntax::{format_pdf_number, format_pdf_number_fixed};

/// A local paint coordinate system together with the visible default-page
/// bounds it maps into.
#[derive(Debug, Clone, Copy)]
pub(super) struct PdfPaintSpace {
    local_to_layout: PdfMatrix,
    page_content: PageContentTransform,
    pub(super) page_bounds: PdfRect,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PdfRasterCellPlacement {
    pub(super) placed: PdfMatrix,
    pub(super) pattern_transform: PdfMatrix,
}

impl PdfPaintSpace {
    pub(super) const fn new(
        local_to_layout: PdfMatrix,
        page_content: PageContentTransform,
        page_bounds: PdfRect,
    ) -> Self {
        Self {
            local_to_layout,
            page_content,
            page_bounds,
        }
    }

    /// Map one raster source cell through the same staged print coordinate
    /// hierarchy emitted in the page content stream. Keeping the point→device
    /// and device→page multiplications in their authored order matches the PDF
    /// graphics-state calculation and avoids flattening them into a near-
    /// identity scale before the tiling pattern is constructed.
    pub(super) fn raster_cell_to_default(
        self,
        origin: PdfPoint,
        source_scale: PdfVector,
    ) -> Option<PdfRasterCellPlacement> {
        if self.page_content.is_identity() {
            let placed = self.local_to_layout
                * PdfMatrix::translate(origin)
                * PdfMatrix::scale(source_scale);
            let linear = PdfMatrix::new(placed.x_axis, placed.y_axis, PdfPoint::ORIGIN);
            let bounds = self.page_bounds.transformed_bounds(linear.inverse()?);
            return Some(PdfRasterCellPlacement {
                placed,
                pattern_transform: PdfMatrix::new(
                    linear.x_axis,
                    linear.y_axis,
                    linear.transform_point(PdfPoint::new(bounds.left, bounds.bottom)),
                ),
            });
        }

        const POINTS_PER_CSS_PIXEL: f32 = 0.75;
        let css_to_device = (PageContentTransform::POINT_TO_DEVICE as f32) * POINTS_PER_CSS_PIXEL;
        let device_to_page = PageContentTransform::DEVICE_TO_PAGE as f32;
        let scale_x = source_scale.x / POINTS_PER_CSS_PIXEL;
        let scale_y = -source_scale.y / POINTS_PER_CSS_PIXEL;
        let staged_device = |value: f32, scale: f32| value * css_to_device * scale;
        let device_x_axis = PdfVector::new(
            staged_device(self.local_to_layout.x_axis.x, scale_x),
            -staged_device(self.local_to_layout.x_axis.y, scale_x),
        );
        let device_y_axis = PdfVector::new(
            -staged_device(self.local_to_layout.y_axis.x, scale_y),
            staged_device(self.local_to_layout.y_axis.y, scale_y),
        );
        let device_linear = PdfMatrix::new(device_x_axis, device_y_axis, PdfPoint::ORIGIN);
        let page_height = self.page_content.page_size?.y;
        let point_to_device = PageContentTransform::POINT_TO_DEVICE as f32;
        let device_page_bounds = PdfRect::new(
            self.page_bounds.left * point_to_device,
            (page_height - self.page_bounds.top()) * point_to_device,
            self.page_bounds.width * point_to_device,
            self.page_bounds.height * point_to_device,
        );
        let pattern_bounds = device_page_bounds.transformed_bounds(device_linear.inverse()?);
        let device_pattern_transform = PdfMatrix::new(
            device_x_axis,
            device_y_axis,
            device_linear
                .transform_point(PdfPoint::new(pattern_bounds.left, pattern_bounds.bottom)),
        );
        let device_to_default = |matrix: PdfMatrix| {
            PdfMatrix::new(
                PdfVector::new(
                    matrix.x_axis.x * device_to_page,
                    -matrix.x_axis.y * device_to_page,
                ),
                PdfVector::new(
                    matrix.y_axis.x * device_to_page,
                    -matrix.y_axis.y * device_to_page,
                ),
                PdfPoint::new(
                    matrix.translation.x * device_to_page,
                    page_height - matrix.translation.y * device_to_page,
                ),
            )
        };
        let anchor = self
            .page_content
            .transform_point(self.local_to_layout.transform_point(origin));
        let mut placed = device_to_default(device_linear);
        placed.translation = anchor;
        Some(PdfRasterCellPlacement {
            placed,
            pattern_transform: device_to_default(device_pattern_transform),
        })
    }
}

/// Mapping from point-based layout space and, where a paint operation needs
/// it, the print-device coordinate system. Normal page content stays in its
/// native page-point space: composing device-scale inverses there causes
/// one-device-pixel edge differences in an otherwise identical PDF.
#[derive(Debug, Clone, Copy)]
pub(super) struct PageContentTransform {
    page_size: Option<PdfVector>,
    content_scale: PrintContentScale,
}

/// Physical page boundaries reached by a clip rectangle.
///
/// A physical-page clip needs correction only on the axes it reaches.
/// Correcting the unrelated axis perturbs interior CSS edges by a device row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct PageEdgeContact {
    left: bool,
    right: bool,
    top: bool,
    bottom: bool,
}

impl PageEdgeContact {
    pub(super) fn any(self) -> bool {
        self.left || self.right || self.top || self.bottom
    }

    fn touches_horizontal_axis(self) -> bool {
        self.left || self.right
    }

    fn touches_vertical_axis(self) -> bool {
        self.top || self.bottom
    }
}

/// The print-device coordinate system nested inside a page content stream.
///
/// Page content normally paints in layout points. A few PDF primitives need
/// to retain Chromium's authored device-space hierarchy instead of flattening
/// it into layout space first; this value owns that coordinate transition.
#[derive(Debug, Clone, Copy)]
pub(super) struct PdfDeviceSpace {
    page_height: f64,
}

impl PdfDeviceSpace {
    pub(super) const CSS_TO_DEVICE: f64 = PageContentTransform::POINT_TO_DEVICE * 0.75;

    /// Cancel only the point-to-device stage installed by
    /// [`PageContentTransform::operator`]. The outer device-to-page stage stays
    /// active, so following coordinates are expressed in print-device units.
    pub(super) fn enter_operator(self) -> String {
        let device_to_point = PageContentTransform::POINT_TO_DEVICE.recip();
        format!(
            "{device_to_point} 0 0 -{device_to_point} 0 {} cm\n",
            self.page_height
        )
    }

    /// Correct the page/device hand-off on the physical clip axes. This
    /// preserves deliberate print-fit scaling and leaves interior edges in the
    /// untouched axis at their authored coordinate.
    pub(super) fn layout_edge_correction_operator(self, edges: PageEdgeContact) -> String {
        let outer_layout_scale =
            PageContentTransform::DEVICE_TO_PAGE * PageContentTransform::POINT_TO_DEVICE;
        let inverse = outer_layout_scale.recip();
        let scale_x = edges
            .touches_horizontal_axis()
            .then_some(inverse)
            .unwrap_or(1.0);
        let scale_y = edges
            .touches_vertical_axis()
            .then_some(inverse)
            .unwrap_or(1.0);
        let translate_y = if edges.touches_vertical_axis() {
            -self.page_height * (1.0 - outer_layout_scale) * inverse
        } else {
            0.0
        };
        format!("{scale_x} 0 0 {scale_y} 0 {translate_y} cm\n")
    }

    pub(super) const fn page_height(self) -> f64 {
        self.page_height
    }

    /// Convert a layout rectangle into Chromium's top-down print-device
    /// coordinates. This is the coordinate system used for a browser clip
    /// before it enters CSS-pixel paint coordinates.
    pub(super) fn layout_rect(self, rect: PdfRect) -> PdfRect {
        let scale = PageContentTransform::POINT_TO_DEVICE as f32;
        PdfRect::new(
            rect.left * scale,
            (self.page_height as f32 - rect.top()) * scale,
            rect.width * scale,
            rect.height * scale,
        )
    }

    /// Convert a layout rectangle into absolute, top-down CSS-pixel page
    /// coordinates. The caller must have entered [`Self::css_page_operator`].
    pub(super) fn css_page_rect(self, rect: PdfRect) -> PdfRect {
        let scale = crate::fonts::PT_PER_CSS_PX;
        PdfRect::new(
            rect.left / scale,
            (self.page_height as f32 - rect.top()) / scale,
            rect.width / scale,
            rect.height / scale,
        )
    }

    /// Enter absolute top-down CSS-pixel page coordinates after
    /// [`Self::enter_operator`]. Together these retain Chromium's two-stage
    /// device-clip then CSS-paint structure instead of flattening it into a
    /// single point-space fill.
    pub(super) fn css_page_operator(self) -> String {
        let scale = Self::CSS_TO_DEVICE;
        format!("{scale} 0 0 {scale} 0 0 cm\n")
    }
}

/// Text serialization coordinates. Layout stays in PDF points; this type
/// changes only how a text run is written into its local PDF graphics scope.
///
/// Browser-produced PDFs commonly retain top-down CSS pixels for text and
/// apply the 0.75pt conversion in the surrounding graphics state. Keeping that
/// representation explicit avoids losing edge coverage by flattening small text
/// directly into point-space font sizes and origins.
#[derive(Debug, Clone, Copy, Default)]
pub(super) enum PdfTextSpace {
    #[default]
    Points,
    PageCss {
        page_height: f32,
    },
}

impl PdfTextSpace {
    pub(super) fn page_css(page: PageContentTransform) -> Self {
        if !page.content_scale.is_identity() {
            return Self::Points;
        }
        page.page_size.map_or(Self::Points, |size| Self::PageCss {
            page_height: size.y,
        })
    }

    pub(super) fn begin_operator(self) -> Option<String> {
        let Self::PageCss { page_height } = self else {
            return None;
        };
        let scale = crate::fonts::PT_PER_CSS_PX;
        Some(format!("q\n{scale} 0 0 -{scale} 0 {page_height} cm\n"))
    }

    pub(super) const fn end_operator(self) -> Option<&'static str> {
        match self {
            Self::Points => None,
            Self::PageCss { .. } => Some("Q\n"),
        }
    }

    pub(super) const fn is_page_css(self) -> bool {
        matches!(self, Self::PageCss { .. })
    }

    pub(super) fn point(self, point: PdfPoint) -> PdfPoint {
        match self {
            Self::Points => point,
            Self::PageCss { page_height } => PdfPoint::new(
                point.x / crate::fonts::PT_PER_CSS_PX,
                (page_height - point.y) / crate::fonts::PT_PER_CSS_PX,
            ),
        }
    }

    pub(super) fn length(self, length: f32) -> f32 {
        match self {
            Self::Points => length,
            Self::PageCss { .. } => length / crate::fonts::PT_PER_CSS_PX,
        }
    }

    /// Sign of text-space Y relative to layout-space Y.
    pub(super) const fn y_axis(self) -> f32 {
        match self {
            Self::Points => 1.0,
            Self::PageCss { .. } => -1.0,
        }
    }
}

impl PageContentTransform {
    pub(super) const DEVICE_TO_PAGE: f64 = 0.24;
    pub(super) const POINT_TO_DEVICE: f64 = 1.0 / 0.24;
    /// Skia serializes the 300-DPI device-to-page float with this value in a
    /// browser-produced PDF content stream. Retaining that authored matrix is
    /// significant to device-edge coverage even though it differs from 0.24
    /// by far less than any layout-space precision.
    const DEVICE_TO_PAGE_OPERATOR: f64 = 0.23999999;

    pub(super) fn print(page_size: PdfVector) -> Self {
        Self {
            page_size: Some(page_size),
            ..Default::default()
        }
    }

    /// Apply browser print-to-page fitting to normal-flow content. Page
    /// decorations deliberately remain outside this transform.
    pub(super) const fn with_content_scale(mut self, scale: PrintContentScale) -> Self {
        self.content_scale = scale;
        self
    }

    pub(super) fn page_bounds(self) -> Option<PdfRect> {
        let size = self.page_size?;
        Some(PdfRect::new(0.0, 0.0, size.x, size.y))
    }

    /// Snap the absolute edges of a laid-out CSS box to Chromium's print-paint
    /// CSS-pixel grid.
    ///
    /// Chromium retains subpixel geometry during layout, then rounds each
    /// absolute box edge in top-down page coordinates before applying CSS
    /// transforms. Rounding a width or height instead would move the far edge
    /// independently and create seams between adjacent boxes.
    pub(super) fn snap_layout_box(self, rect: PdfRect) -> PdfRect {
        let Some(page_size) = self.page_size else {
            return rect;
        };
        let snap = crate::fonts::round_to_css_pixel;
        let left = snap(rect.left);
        let right = snap(rect.right());
        let top_from_page = snap(page_size.y - rect.top());
        let bottom_from_page = snap(page_size.y - rect.bottom);
        let top = page_size.y - top_from_page;
        let bottom = page_size.y - bottom_from_page;
        PdfRect::new(left, bottom, right - left, top - bottom)
    }

    /// Snap a horizontal text baseline in top-down page coordinates while the
    /// line-flow cursor itself remains fractional.
    pub(super) fn snap_horizontal_baseline(self, baseline: f32) -> f32 {
        let Some(page_size) = self.page_size else {
            return baseline;
        };
        page_size.y - crate::fonts::round_to_css_pixel(page_size.y - baseline)
    }

    /// Enclose a layout rectangle in the physical print-device grid.
    ///
    /// Chromium creates soft-mask surfaces in integer print-device pixels. A
    /// fractional layout edge must therefore cover the complete edge device
    /// pixel rather than truncate it before the mask is composited.
    pub(super) fn enclosing_device_bounds(self, rect: PdfRect) -> PdfRect {
        if self.page_size.is_none() {
            return rect;
        }

        let scale = self.scale();
        let translate_y = self.translate_y();
        let grid = Self::DEVICE_TO_PAGE;
        let snap_down = |value: f32| (f64::from(value) / grid).floor() * grid;
        let snap_up = |value: f32| (f64::from(value) / grid).ceil() * grid;
        let left = snap_down(rect.left * scale as f32) / scale;
        let bottom = snap_down(rect.bottom * scale as f32 + translate_y as f32) / scale
            - translate_y / scale;
        let right = snap_up(rect.right() * scale as f32) / scale;
        let top =
            snap_up(rect.top() * scale as f32 + translate_y as f32) / scale - translate_y / scale;

        PdfRect::new(
            left as f32,
            bottom as f32,
            (right - left) as f32,
            (top - bottom) as f32,
        )
    }

    /// Enter CSS-pixel coordinates for a box from the native page-point space.
    pub(super) fn css_box_operator(self, origin: PdfPoint) -> Option<String> {
        self.page_size?;
        let relative_scale = crate::fonts::PT_PER_CSS_PX;
        Some(format!(
            "{relative_scale} 0 0 -{relative_scale} {} {} cm\n",
            origin.x, origin.y,
        ))
    }

    pub(super) fn operator(self) -> String {
        let Some(page_size) = self.page_size else {
            return "1 0 0 1 0 0 cm\n".to_owned();
        };
        let page_height = f64::from(page_size.y);
        let point_to_device = Self::POINT_TO_DEVICE;
        let device_height = page_height * point_to_device;
        let page_height = format_pdf_number(page_size.y);
        let point_to_device = format_pdf_number_fixed(point_to_device, 15);
        let device_height = format_pdf_number_fixed(device_height, 15);
        let mut operator = format!(
            "{} 0 0 -{} 0 {page_height} cm\n\
             {point_to_device} 0 0 -{point_to_device} 0 {device_height} cm\n",
            Self::DEVICE_TO_PAGE_OPERATOR,
            Self::DEVICE_TO_PAGE_OPERATOR,
        );
        let content_scale = f64::from(self.content_scale.factor());
        if self.content_scale.is_identity() {
            return operator;
        }
        operator.push_str(&format!(
            "{content_scale} 0 0 {content_scale} 0 {} cm\n",
            f64::from(page_size.y) * (1.0 - content_scale),
        ));
        operator
    }

    pub(super) fn device_space(self) -> Option<PdfDeviceSpace> {
        self.page_size.map(|page_size| PdfDeviceSpace {
            page_height: f64::from(page_size.y),
        })
    }

    /// Whether a layout-space rectangle reaches a physical page boundary.
    /// Only those boundary clips need the browser's device-space coverage
    /// semantics; interior border-box fills retain their ordinary PDF path.
    pub(super) fn page_edge_contact(self, rect: PdfRect) -> PageEdgeContact {
        const EDGE_EPSILON: f32 = 0.000_1;
        let Some(page_size) = self.page_size else {
            return PageEdgeContact::default();
        };
        PageEdgeContact {
            left: rect.left.abs() <= EDGE_EPSILON,
            top: rect.top().abs() <= EDGE_EPSILON,
            right: (rect.right() - page_size.x).abs() <= EDGE_EPSILON,
            bottom: (rect.bottom - page_size.y).abs() <= EDGE_EPSILON,
        }
    }

    pub(super) fn inverse_operator(self) -> String {
        let scale = self.scale();
        let translate_y = self.translate_y();
        let inverse = scale.recip();
        format!("{inverse} 0 0 {inverse} 0 {} cm\n", -translate_y * inverse)
    }

    pub(super) fn is_identity(self) -> bool {
        self.page_size.is_none()
    }

    fn transform_point(self, point: PdfPoint) -> PdfPoint {
        let Some(page_size) = self.page_size else {
            return point;
        };
        let point_to_device = Self::POINT_TO_DEVICE as f32;
        let device_to_page = Self::DEVICE_TO_PAGE as f32;
        let page_height = page_size.y;
        let device_height = page_height * point_to_device;
        let device = PdfPoint::new(
            point.x * point_to_device,
            device_height - point.y * point_to_device,
        );
        let page_point = PdfPoint::new(
            device.x * device_to_page,
            page_height - device.y * device_to_page,
        );
        let scale = self.content_scale.factor();
        PdfPoint::new(
            page_point.x * scale,
            page_height - (page_height - page_point.y) * scale,
        )
    }

    pub(super) fn transform_rect(self, rect: PdfRect) -> PdfRect {
        if self.content_scale.is_identity() {
            return rect;
        }
        let lower_left = self.transform_point(PdfPoint::new(rect.left, rect.bottom));
        let upper_right = self.transform_point(PdfPoint::new(rect.right(), rect.top()));
        PdfRect::new(
            lower_left.x,
            lower_left.y,
            upper_right.x - lower_left.x,
            upper_right.y - lower_left.y,
        )
    }

    fn scale(self) -> f64 {
        self.page_size.map_or(1.0, |_| {
            Self::DEVICE_TO_PAGE * Self::POINT_TO_DEVICE * f64::from(self.content_scale.factor())
        })
    }

    fn translate_y(self) -> f64 {
        self.page_size
            .map_or(0.0, |size| f64::from(size.y) * (1.0 - self.scale()))
    }
}

impl Default for PageContentTransform {
    fn default() -> Self {
        Self {
            page_size: None,
            content_scale: PrintContentScale::default(),
        }
    }
}

/// Reduce a CSS affine transform to a PDF-space matrix around its resolved
/// transform-origin. CSS's downward Y axis is conjugated once at this boundary.
#[derive(Debug, Clone, Copy)]
pub(super) struct ResolvedPdfTransform {
    matrix: PdfMatrix,
    components: [f64; 6],
}

impl ResolvedPdfTransform {
    pub(super) const fn matrix(self) -> PdfMatrix {
        self.matrix
    }
}

pub(super) fn resolve_css_transform(
    transform: &crate::style::computed::Transform,
    pivot: PdfPoint,
    box_size: PdfVector,
) -> ResolvedPdfTransform {
    let [a, b, c, d, e, f] = transform
        .to_css_matrix(crate::style::computed::CssVector::new(
            f64::from(box_size.x),
            f64::from(box_size.y),
        ))
        .components();
    // Resolve the origin conjugation once in f64. Repeated f32 matrix
    // multiplication loses several ULPs in the translation even though the
    // authored transform and pivot are unchanged; that is visible at printed
    // device edges. The PDF matrix itself remains the renderer's compact f32
    // type after this single final conversion.
    let px = f64::from(pivot.x);
    let py = f64::from(pivot.y);
    let translated_x = px + e - a * px + c * py;
    let translated_y = py - f + b * px - d * py;
    let matrix = PdfMatrix::new(
        PdfVector::new(a as f32, -b as f32),
        PdfVector::new(-c as f32, d as f32),
        PdfPoint::new(translated_x as f32, translated_y as f32),
    );
    ResolvedPdfTransform {
        matrix,
        components: [a, -b, -c, d, translated_x, translated_y],
    }
}

pub(super) fn push_resolved_transform_cm(content: &mut String, transform: ResolvedPdfTransform) {
    let [a, b, c, d, e, f] = transform
        .components
        .map(|value| if value == 0.0 { 0.0 } else { value });
    content.push_str(&format!("{a} {b} {c} {d} {e} {f} cm\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_box_transition_reconstructs_direct_print_scale_exactly() {
        let transform = PageContentTransform::print(PdfVector::new(180.0, 72.0));
        let operator = transform
            .css_box_operator(PdfPoint::new(0.0, 72.0))
            .unwrap();
        let values = operator
            .split_ascii_whitespace()
            .take(6)
            .map(|value| value.parse::<f64>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(values[0], f64::from(crate::fonts::PT_PER_CSS_PX));
        assert_eq!(-values[3], f64::from(crate::fonts::PT_PER_CSS_PX));
    }

    #[test]
    fn print_transform_owns_complete_page_geometry() {
        let transform = PageContentTransform::print(PdfVector::new(180.0, 72.0));
        assert_eq!(
            transform.page_bounds(),
            Some(PdfRect::new(0.0, 0.0, 180.0, 72.0))
        );
    }

    #[test]
    fn print_operator_retains_the_device_then_point_hierarchy() {
        let operator = PageContentTransform::print(PdfVector::new(180.0, 72.0)).operator();
        assert_eq!(
            operator,
            "0.23999999 0 0 -0.23999999 0 72 cm\n\
             4.166666666666667 0 0 -4.166666666666667 0 300 cm\n"
        );
    }

    #[test]
    fn print_device_bounds_enclose_fractional_edges() {
        let transform = PageContentTransform::print(PdfVector::new(228.0, 168.0));
        let bounds = transform.enclosing_device_bounds(PdfRect::new(0.0, 3.0, 165.0, 165.0));

        assert!((bounds.left - 0.0).abs() < 0.000_1);
        assert!((bounds.bottom - 2.88).abs() < 0.000_1);
        assert!((bounds.right() - 165.12).abs() < 0.000_1);
        assert!((bounds.top() - 168.0).abs() < 0.000_1);
    }

    #[test]
    fn print_device_space_preserves_device_clip_and_css_paint_coordinates() {
        let device = PageContentTransform::print(PdfVector::new(138.0, 102.0))
            .device_space()
            .expect("print pages have a device coordinate system");
        let rect = PdfRect::new(36.75, 41.25, 61.5, 24.0);

        assert_eq!(
            device.layout_rect(rect),
            PdfRect::new(153.125, 153.125, 256.25, 100.0)
        );
        assert_eq!(
            device.css_page_rect(rect),
            PdfRect::new(49.0, 49.0, 82.0, 32.0)
        );
        assert_eq!(device.css_page_operator(), "3.125 0 0 3.125 0 0 cm\n");
    }

    #[test]
    fn physical_edge_correction_leaves_the_interior_axis_untouched() {
        let transform = PageContentTransform::print(PdfVector::new(120.0, 78.0));
        let edges = transform.page_edge_contact(PdfRect::new(0.0, 20.0, 120.0, 30.0));
        assert!(edges.any());
        let device = transform.device_space().expect("print device space");
        let operator = device.layout_edge_correction_operator(edges);
        let values = operator
            .split_ascii_whitespace()
            .take(6)
            .map(|value| value.parse::<f64>().expect("numeric matrix value"))
            .collect::<Vec<_>>();
        assert_eq!(values[0], 1.0);
        assert_eq!(values[3], 1.0);
        assert_eq!(values[5], 0.0);
    }

    #[test]
    fn print_content_scale_is_anchored_at_the_page_top_left() {
        let scale = PrintContentScale::from_flow_width(252.0, 255.0);
        let transform =
            PageContentTransform::print(PdfVector::new(252.0, 72.0)).with_content_scale(scale);
        let scaled = transform.transform_rect(PdfRect::new(22.5, 29.25, 24.0, 12.0));

        assert!((scaled.left - 22.5 * 84.0 / 85.0).abs() < 0.000_1);
        assert!((scaled.bottom - 29.25 * 84.0 / 85.0 - 72.0 / 85.0).abs() < 0.000_1);
        assert!((scaled.width - 24.0 * 84.0 / 85.0).abs() < 0.000_1);
        assert!((scaled.height - 12.0 * 84.0 / 85.0).abs() < 0.000_1);
    }

    #[test]
    fn print_paint_snaps_absolute_box_edges_before_transforms() {
        let transform = PageContentTransform::print(PdfVector::new(150.0, 150.0));
        let authored = PdfRect::from_top(7.875, 142.125, 15.375, 15.375);

        assert_eq!(
            transform.snap_layout_box(authored),
            PdfRect::new(8.25, 126.75, 15.0, 15.0),
        );
    }
}
