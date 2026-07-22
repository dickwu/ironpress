use crate::layout::elements::{BoxFragmentation, BoxTransform};
use crate::layout::engine::LayoutBorder;
use crate::render::svg_geometry::SvgViewportBox;
use crate::style::computed::{
    BackgroundClip, BackgroundOrigin, ShapeBox, TransformBox, TransformOrigin,
};
use crate::types::{CornerRadii, EdgeSizes};
use crate::util::{RasterDimensions, RasterTile};
use std::ops::{Add, Mul, Sub};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct PdfPoint {
    pub(super) x: f32,
    pub(super) y: f32,
}

impl PdfPoint {
    pub(super) const ORIGIN: Self = Self::new(0.0, 0.0);

    pub(super) const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub(super) fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

impl Add<PdfVector> for PdfPoint {
    type Output = Self;

    fn add(self, vector: PdfVector) -> Self::Output {
        Self::new(self.x + vector.x, self.y + vector.y)
    }
}

impl Sub for PdfPoint {
    type Output = PdfVector;

    fn sub(self, other: Self) -> Self::Output {
        PdfVector::new(self.x - other.x, self.y - other.y)
    }
}

impl Sub<PdfVector> for PdfPoint {
    type Output = Self;

    fn sub(self, vector: PdfVector) -> Self::Output {
        Self::new(self.x - vector.x, self.y - vector.y)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct PdfVector {
    pub(super) x: f32,
    pub(super) y: f32,
}

impl PdfVector {
    pub(super) const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub(super) fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    pub(super) fn is_positive(self) -> bool {
        self.is_finite() && self.x > 0.0 && self.y > 0.0
    }

    pub(super) fn component_quotient(self, divisor: Self) -> Option<Self> {
        divisor
            .is_positive()
            .then(|| Self::new(self.x / divisor.x, self.y / divisor.y))
            .filter(|quotient| quotient.is_finite())
    }

    pub(super) const fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y
    }
}

impl Add for PdfVector {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        Self::new(self.x + other.x, self.y + other.y)
    }
}

impl Sub for PdfVector {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        Self::new(self.x - other.x, self.y - other.y)
    }
}

impl Mul<f32> for PdfVector {
    type Output = Self;

    fn mul(self, scale: f32) -> Self::Output {
        Self::new(self.x * scale, self.y * scale)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PdfEllipse {
    pub(super) center: PdfPoint,
    pub(super) radii: PdfVector,
}

impl PdfEllipse {
    pub(super) const fn new(center: PdfPoint, radii: PdfVector) -> Self {
        Self { center, radii }
    }

    pub(super) const fn circle(center: PdfPoint, radius: f32) -> Self {
        Self::new(center, PdfVector::new(radius, radius))
    }

    /// Append four cubic Bezier arcs, starting at the right-hand vertex.
    pub(super) fn push_path(self, content: &mut String) {
        const K: f32 = 0.552_284_8;
        let PdfPoint { x, y } = self.center;
        let PdfVector { x: rx, y: ry } = self.radii;
        let control = self.radii * K;

        content.push_str(&format!("{} {y} m\n", x + rx));
        content.push_str(&format!(
            "{} {} {} {} {x} {} c\n",
            x + rx,
            y + control.y,
            x + control.x,
            y + ry,
            y + ry
        ));
        content.push_str(&format!(
            "{} {} {} {} {} {y} c\n",
            x - control.x,
            y + ry,
            x - rx,
            y + control.y,
            x - rx
        ));
        content.push_str(&format!(
            "{} {} {} {} {x} {} c\n",
            x - rx,
            y - control.y,
            x - control.x,
            y - ry,
            y - ry
        ));
        content.push_str(&format!(
            "{} {} {} {} {} {y} c\n",
            x + control.x,
            y - ry,
            x + rx,
            y - control.y,
            x + rx
        ));
    }
}

/// PDF affine matrix `[a b c d e f]`, grouped as two basis vectors and a
/// translation instead of six unrelated coefficients.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PdfMatrix {
    pub(super) x_axis: PdfVector,
    pub(super) y_axis: PdfVector,
    pub(super) translation: PdfPoint,
}

impl PdfMatrix {
    pub(super) const IDENTITY: Self = Self::new(
        PdfVector::new(1.0, 0.0),
        PdfVector::new(0.0, 1.0),
        PdfPoint::ORIGIN,
    );

    pub(super) const fn new(x_axis: PdfVector, y_axis: PdfVector, translation: PdfPoint) -> Self {
        Self {
            x_axis,
            y_axis,
            translation,
        }
    }

    pub(super) const fn scale(scale: PdfVector) -> Self {
        Self::new(
            PdfVector::new(scale.x, 0.0),
            PdfVector::new(0.0, scale.y),
            PdfPoint::ORIGIN,
        )
    }

    pub(super) const fn translate(translation: PdfPoint) -> Self {
        Self::new(
            PdfVector::new(1.0, 0.0),
            PdfVector::new(0.0, 1.0),
            translation,
        )
    }

    pub(super) const fn rotate_around(pivot: PdfPoint, sin: f32, cos: f32) -> Self {
        let one_minus_cos = 1.0 - cos;
        Self::new(
            PdfVector::new(cos, sin),
            PdfVector::new(-sin, cos),
            PdfPoint::new(
                sin * pivot.y + one_minus_cos * pivot.x,
                -sin * pivot.x + one_minus_cos * pivot.y,
            ),
        )
    }

    pub(super) const fn components(self) -> [f32; 6] {
        [
            self.x_axis.x,
            self.x_axis.y,
            self.y_axis.x,
            self.y_axis.y,
            self.translation.x,
            self.translation.y,
        ]
    }

    pub(super) fn cm_operator(self) -> String {
        let [a, b, c, d, e, f] = self.components();
        format!("{a} {b} {c} {d} {e} {f} cm\n")
    }

    pub(super) fn is_invertible(self) -> bool {
        self.x_axis.is_finite()
            && self.y_axis.is_finite()
            && self.translation.is_finite()
            && self.x_axis.x * self.y_axis.y - self.x_axis.y * self.y_axis.x != 0.0
    }

    pub(super) fn inverse(self) -> Option<Self> {
        if !self.is_invertible() {
            return None;
        }
        let [a, b, c, d, e, f] = self.components().map(f64::from);
        let determinant = a * d - b * c;
        Some(Self::new(
            PdfVector::new((d / determinant) as f32, (-b / determinant) as f32),
            PdfVector::new((-c / determinant) as f32, (a / determinant) as f32),
            PdfPoint::new(
                ((c * f - d * e) / determinant) as f32,
                ((b * e - a * f) / determinant) as f32,
            ),
        ))
    }

    const fn transform_vector(self, vector: PdfVector) -> PdfVector {
        PdfVector::new(
            self.x_axis.x * vector.x + self.y_axis.x * vector.y,
            self.x_axis.y * vector.x + self.y_axis.y * vector.y,
        )
    }

    pub(super) const fn transform_point(self, point: PdfPoint) -> PdfPoint {
        let transformed = self.transform_vector(PdfVector::new(point.x, point.y));
        PdfPoint::new(
            transformed.x + self.translation.x,
            transformed.y + self.translation.y,
        )
    }
}

impl Default for PdfMatrix {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mul for PdfMatrix {
    type Output = Self;

    /// Compose transforms so `(left * right)` applies `right` first.
    fn mul(self, right: Self) -> Self::Output {
        Self::new(
            self.transform_vector(right.x_axis),
            self.transform_vector(right.y_axis),
            self.transform_point(right.translation),
        )
    }
}

/// Axis-aligned rectangle in PDF page coordinates: x grows right and y grows
/// up from the bottom-left corner.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct PdfRect {
    pub(super) left: f32,
    pub(super) bottom: f32,
    pub(super) width: f32,
    pub(super) height: f32,
}

impl PdfRect {
    pub(super) const fn new(left: f32, bottom: f32, width: f32, height: f32) -> Self {
        Self {
            left,
            bottom,
            width,
            height,
        }
    }

    /// Construct from a top-left position without leaking top-down coordinates
    /// into the rest of the PDF renderer.
    pub(super) const fn from_top(left: f32, top: f32, width: f32, height: f32) -> Self {
        Self::new(left, top - height, width, height)
    }

    pub(super) const fn right(self) -> f32 {
        self.left + self.width
    }

    pub(super) const fn top(self) -> f32 {
        self.bottom + self.height
    }

    /// Resolve a CSS top-left transform origin into the PDF rectangle's
    /// bottom-left coordinate system.
    pub(super) fn css_transform_origin(self, origin: TransformOrigin) -> PdfPoint {
        let (x, y) = origin.resolve(self.width, self.height);
        PdfPoint::new(self.left + x, self.top() - y)
    }

    /// PDF function domain order: x-min, x-max, y-min, y-max.
    pub(super) const fn xy_domain(self) -> [f32; 4] {
        [self.left, self.right(), self.bottom, self.top()]
    }

    /// Map a top-down pixel tile onto this PDF rectangle without changing the
    /// requested full-surface sampling grid.
    pub(super) fn raster_tile(self, dimensions: RasterDimensions, tile: RasterTile) -> Self {
        let scale_x = self.width / dimensions.width as f32;
        let scale_y = self.height / dimensions.height as f32;
        Self::new(
            self.left + tile.x as f32 * scale_x,
            self.top() - (tile.y + tile.height) as f32 * scale_y,
            tile.width as f32 * scale_x,
            tile.height as f32 * scale_y,
        )
    }

    pub(super) fn is_empty(self) -> bool {
        ![self.left, self.bottom, self.width, self.height]
            .into_iter()
            .all(f32::is_finite)
            || self.width <= 0.0
            || self.height <= 0.0
    }

    pub(super) fn transformed_bounds(self, transform: PdfMatrix) -> Self {
        let points = [
            PdfPoint::new(self.left, self.bottom),
            PdfPoint::new(self.right(), self.bottom),
            PdfPoint::new(self.left, self.top()),
            PdfPoint::new(self.right(), self.top()),
        ]
        .map(|point| transform.transform_point(point));
        let left = points
            .iter()
            .map(|point| point.x)
            .fold(f32::INFINITY, f32::min);
        let right = points
            .iter()
            .map(|point| point.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let bottom = points
            .iter()
            .map(|point| point.y)
            .fold(f32::INFINITY, f32::min);
        let top = points
            .iter()
            .map(|point| point.y)
            .fold(f32::NEG_INFINITY, f32::max);
        Self::new(left, bottom, right - left, top - bottom)
    }

    pub(super) fn covers_with_margin(self, inner: Self, margin: f32) -> bool {
        self.left <= inner.left - margin
            && self.bottom <= inner.bottom - margin
            && self.right() >= inner.right() + margin
            && self.top() >= inner.top() + margin
    }

    pub(super) fn intersection(self, other: Self) -> Option<Self> {
        let left = self.left.max(other.left);
        let bottom = self.bottom.max(other.bottom);
        let right = self.right().min(other.right());
        let top = self.top().min(other.top());
        let intersection = Self::new(left, bottom, right - left, top - bottom);
        (!intersection.is_empty()).then_some(intersection)
    }

    pub(super) const fn translate(self, dx: f32, dy: f32) -> Self {
        Self::new(self.left + dx, self.bottom + dy, self.width, self.height)
    }

    /// Inset physical edges. The authored origin shift is retained even when
    /// the edges consume the entire rectangle; only the resulting extents are
    /// clamped.
    pub(super) fn inset(self, edges: EdgeSizes) -> Self {
        Self::new(
            self.left + edges.left,
            self.bottom + edges.bottom,
            (self.width - edges.horizontal()).max(0.0),
            (self.height - edges.vertical()).max(0.0),
        )
    }

    pub(super) const fn outset(self, edges: EdgeSizes) -> Self {
        Self::new(
            self.left - edges.left,
            self.bottom - edges.bottom,
            self.width + edges.horizontal(),
            self.height + edges.top + edges.bottom,
        )
    }

    pub(super) const fn outset_uniform(self, amount: f32) -> Self {
        self.outset(EdgeSizes::uniform(amount))
    }

    pub(super) fn rect_path(self) -> String {
        format!(
            "{} {} {} {} re\n",
            self.left, self.bottom, self.width, self.height
        )
    }

    pub(super) const fn rounded(self, radii: CornerRadii) -> RoundedRect {
        RoundedRect::new(self, radii)
    }
}

impl From<PdfRect> for SvgViewportBox {
    fn from(rect: PdfRect) -> Self {
        Self::new(rect.left, rect.bottom, rect.width, rect.height)
    }
}

/// One CSS box with its resolved physical edge sizes. Every derived rectangle
/// comes from the same border-box contract.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct BoxGeometry {
    pub(super) border_box: PdfRect,
    pub(super) border: EdgeSizes,
    pub(super) padding: EdgeSizes,
}

/// The concrete CSS transform reference box resolved from one laid-out box.
///
/// `transform-box` affects both the transform origin and every percentage in a
/// transform function. Keeping the reference rectangle intact until paint time
/// avoids lossy style-layer rewrites that cannot know the used border and
/// padding geometry yet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TransformReferenceGeometry {
    border_box: PdfRect,
    reference_box: PdfRect,
    origin: TransformOrigin,
}

impl TransformReferenceGeometry {
    /// Absolute transform pivot in PDF page coordinates.
    pub(super) fn pivot(self) -> PdfPoint {
        self.reference_box.css_transform_origin(self.origin)
    }

    /// Transform pivot in the border box's local, top-down CSS coordinates.
    pub(super) fn local_pivot(self) -> PdfVector {
        let pivot = self.pivot();
        PdfVector::new(
            pivot.x - self.border_box.left,
            self.border_box.top() - pivot.y,
        )
    }

    /// Dimensions against which transform percentages resolve.
    pub(super) const fn size(self) -> PdfVector {
        PdfVector::new(self.reference_box.width, self.reference_box.height)
    }

    pub(super) const fn border_box(self) -> PdfRect {
        self.border_box
    }

    pub(super) const fn z_origin(self) -> f32 {
        self.origin.z_length
    }
}

/// The two boxes involved in painting a possibly fragmented background.
/// Positioning uses the reassembled decoration box, while clipping always uses
/// the fragment that belongs to the current page.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct BackgroundFragmentGeometry {
    pub(super) positioning_box: PdfRect,
    pub(super) painting_box: RoundedRect,
}

/// The current fragment's paint box and its position in the reassembled box.
///
/// CSS Break 3 makes backgrounds, masks, and shape reference boxes derive from
/// the same composite dimensions for `box-decoration-break: slice`. Keeping
/// that source geometry paired with the current paint fragment prevents those
/// consumers from drifting into unrelated fragment-height calculations.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct FragmentPaintGeometry {
    painting: BoxGeometry,
    reassembled: BoxGeometry,
}

impl FragmentPaintGeometry {
    pub(super) const fn painting(self) -> BoxGeometry {
        self.painting
    }

    /// Composite positioning geometry with accumulated fragment progress.
    /// Background and mask images use this to remain continuous across slices.
    pub(super) const fn positioning(self) -> BoxGeometry {
        self.reassembled
    }

    /// Clip a sliced decoration at fragmentainer cuts while preserving paint
    /// overflow at the box's real edges.
    pub(super) fn decoration_clip(self, outsets: EdgeSizes) -> Option<PdfRect> {
        if self.painting == self.reassembled {
            return None;
        }
        const EDGE_EPSILON: f32 = 0.001;
        let painting = self.painting.border_box;
        let reference = self.reassembled.border_box;
        let has_real_top = (painting.top() - reference.top()).abs() <= EDGE_EPSILON;
        let has_real_bottom = (painting.bottom - reference.bottom).abs() <= EDGE_EPSILON;
        let top_outset = if has_real_top { outsets.top } else { 0.0 };
        let bottom_outset = if has_real_bottom { outsets.bottom } else { 0.0 };
        Some(PdfRect::new(
            painting.left - outsets.left,
            painting.bottom - bottom_outset,
            painting.width + outsets.horizontal(),
            painting.height + top_outset + bottom_outset,
        ))
    }

    /// Composite shape dimensions in the current fragment's effect space.
    ///
    /// CSS Break 3 §5.5 applies graphical effects per fragment, so a clip path
    /// starts at this fragment's origin. Its percentage reference dimensions
    /// still come from the whole box selected by §5.4, rather than the short
    /// fragment that happens to fit on this page.
    pub(super) fn shape_reference(self) -> BoxGeometry {
        BoxGeometry::new(
            PdfRect::from_top(
                self.painting.border_box.left,
                self.painting.border_box.top(),
                self.painting.border_box.width,
                self.reassembled.border_box.height,
            ),
            self.reassembled.border,
            self.reassembled.padding,
        )
    }

    pub(super) fn background(
        self,
        origin: BackgroundOrigin,
        clip: BackgroundClip,
        radii: CornerRadii,
    ) -> BackgroundFragmentGeometry {
        BackgroundFragmentGeometry {
            positioning_box: self.reassembled.background_origin_box(origin),
            painting_box: self.painting.background_clip_box(clip, radii),
        }
    }
}

impl BoxGeometry {
    pub(super) const fn new(border_box: PdfRect, border: EdgeSizes, padding: EdgeSizes) -> Self {
        Self {
            border_box,
            border,
            padding,
        }
    }

    pub(super) fn from_layout(
        border_box: PdfRect,
        border: &LayoutBorder,
        padding: EdgeSizes,
    ) -> Self {
        Self::new(border_box, border.widths(), padding)
    }

    pub(super) fn padding_box(self) -> PdfRect {
        self.border_box.inset(self.border)
    }

    pub(super) fn content_box(self) -> PdfRect {
        self.border_box.inset(self.border + self.padding)
    }

    /// Border box with the one globally normalized set of CSS corner radii.
    pub(super) fn rounded_border_box(self, radii: CornerRadii) -> RoundedRect {
        self.border_box
            .rounded(radii.fit_to(self.border_box.width, self.border_box.height))
    }

    /// Padding box whose curve is derived from the used outer border curve.
    pub(super) fn rounded_padding_box(self, radii: CornerRadii) -> RoundedRect {
        self.rounded_border_box(radii).inset(self.border)
    }

    pub(super) fn transform_reference(
        self,
        transform: &BoxTransform,
    ) -> TransformReferenceGeometry {
        let reference_box = match transform.reference_box {
            TransformBox::ContentBox | TransformBox::FillBox => self.content_box(),
            TransformBox::BorderBox | TransformBox::StrokeBox | TransformBox::ViewBox => {
                self.border_box
            }
        };
        TransformReferenceGeometry {
            border_box: self.border_box,
            reference_box,
            origin: transform.origin,
        }
    }

    pub(super) fn shape_box(self, kind: ShapeBox) -> PdfRect {
        match kind {
            ShapeBox::Border => self.border_box,
            ShapeBox::Padding => self.padding_box(),
            ShapeBox::Content => self.content_box(),
        }
    }

    pub(super) fn background_origin_box(self, origin: BackgroundOrigin) -> PdfRect {
        match origin {
            BackgroundOrigin::Border => self.border_box,
            BackgroundOrigin::Padding => self.padding_box(),
            BackgroundOrigin::Content => self.content_box(),
        }
    }

    pub(super) fn background_clip_box(
        self,
        clip: BackgroundClip,
        radii: CornerRadii,
    ) -> RoundedRect {
        let inset = match clip {
            BackgroundClip::Border | BackgroundClip::Text => EdgeSizes::ZERO,
            BackgroundClip::Padding => self.border,
            BackgroundClip::Content => self.border + self.padding,
        };
        self.rounded_border_box(radii).inset(inset)
    }

    pub(super) fn for_fragment(self, fragmentation: BoxFragmentation) -> FragmentPaintGeometry {
        let reassembled = fragmentation.reference_slice.map_or(self, |slice| {
            let edges = slice.edges();
            Self::new(
                PdfRect::from_top(
                    self.border_box.left,
                    self.border_box.top() + slice.block_offset(),
                    self.border_box.width,
                    slice.composite_block_size(),
                ),
                edges.border(),
                edges.padding(),
            )
        });
        FragmentPaintGeometry {
            painting: self,
            reassembled,
        }
    }
}

/// A rectangle and its resolved corner geometry. Keeping the two together
/// prevents callers from insetting one without the other.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct RoundedRect {
    pub(super) rect: PdfRect,
    pub(super) radii: CornerRadii,
}

impl RoundedRect {
    pub(super) const fn new(rect: PdfRect, radii: CornerRadii) -> Self {
        Self { rect, radii }
    }

    pub(super) fn inset(self, edges: EdgeSizes) -> Self {
        Self::new(self.rect.inset(edges), self.radii.inset(edges))
    }

    /// Approximate the complete rounded-rectangle path length.
    ///
    /// Dash and dot arrays on a closed CSS border must divide the whole path;
    /// otherwise the PDF dash iterator leaves a visibly compressed pair at its
    /// closing seam. Straight spans are exact and each elliptical quarter uses
    /// Ramanujan's second circumference approximation.
    pub(super) fn perimeter(self) -> f32 {
        let radii = self.radii.fit_to(self.rect.width, self.rect.height);
        let straight = (self.rect.width - radii.top_left.x - radii.top_right.x).max(0.0)
            + (self.rect.height - radii.top_right.y - radii.bottom_right.y).max(0.0)
            + (self.rect.width - radii.bottom_right.x - radii.bottom_left.x).max(0.0)
            + (self.rect.height - radii.bottom_left.y - radii.top_left.y).max(0.0);
        straight + radii.iter().map(quarter_ellipse_perimeter).sum::<f32>()
    }

    pub(super) fn path(self) -> Option<String> {
        let radii = self.radii.fit_to(self.rect.width, self.rect.height);
        if radii.is_zero() {
            return None;
        }
        Some(if let Some(radius) = radii.uniform_radius() {
            rounded_rect_path(self.rect, radius)
        } else {
            rounded_rect_path_per_corner(self.rect, radii)
        })
    }

    pub(super) fn path_or_rect(self) -> String {
        self.path().unwrap_or_else(|| self.rect.rect_path())
    }

    pub(super) fn push_clip(self, content: &mut String) {
        content.push_str(&self.clip_command());
    }

    pub(super) fn clip_command(self) -> String {
        let mut command = String::from("q\n");
        command.push_str(&self.path_or_rect());
        command.push_str("W n\n");
        command
    }

    pub(super) fn push_rounded_clip(self, content: &mut String) -> bool {
        if self.radii.is_zero() {
            return false;
        }
        self.push_clip(content);
        true
    }
}

fn quarter_ellipse_perimeter(radius: crate::types::CornerRadius) -> f32 {
    if radius.is_zero() {
        return 0.0;
    }
    let sum = radius.x + radius.y;
    let h = ((radius.x - radius.y) / sum).powi(2);
    std::f32::consts::PI * sum * (1.0 + 3.0 * h / (10.0 + (4.0 - 3.0 * h).sqrt())) / 4.0
}

fn rounded_rect_path_per_corner(rect: PdfRect, radii: CornerRadii) -> String {
    let radii = radii.fit_to(rect.width, rect.height);
    let kf = 0.552_284_8;
    let xl = rect.left;
    let xr = rect.right();
    let yt = rect.top();
    let yb = rect.bottom;
    let (tlx, tly) = (radii.top_left.x, radii.top_left.y);
    let (trx, try_) = (radii.top_right.x, radii.top_right.y);
    let (brx, bry) = (radii.bottom_right.x, radii.bottom_right.y);
    let (blx, bly) = (radii.bottom_left.x, radii.bottom_left.y);
    format!(
        "{a} {yt} m\n\
         {b} {yt} l {b2} {yt} {xr} {tr_y2} {xr} {tr_y} c\n\
         {xr} {br_y} l {xr} {br_y2} {br_x2} {yb} {br_x} {yb} c\n\
         {bl_x} {yb} l {bl_x2} {yb} {xl} {bl_y2} {xl} {bl_y} c\n\
         {xl} {tl_y} l {xl} {tl_y2} {tl_x2} {yt} {a} {yt} c\n\
         h\n",
        a = xl + tlx,
        b = xr - trx,
        b2 = xr - trx + trx * kf,
        tr_y = yt - try_,
        tr_y2 = yt - try_ + try_ * kf,
        br_y = yb + bry,
        br_y2 = yb + bry - bry * kf,
        br_x = xr - brx,
        br_x2 = xr - brx + brx * kf,
        bl_x = xl + blx,
        bl_x2 = xl + blx - blx * kf,
        bl_y = yb + bly,
        bl_y2 = yb + bly - bly * kf,
        tl_y = yt - tly,
        tl_y2 = yt - tly + tly * kf,
        tl_x2 = xl + tlx - tlx * kf,
    )
}

fn rounded_rect_path(rect: PdfRect, radius: f32) -> String {
    let radius = radius.min(rect.width / 2.0).min(rect.height / 2.0);
    let k = radius * 0.552_284_8;
    let x = rect.left;
    let y = rect.bottom;
    let width = rect.width;
    let height = rect.height;
    format!(
        "{x0} {y0} m\n\
         {x1} {y0} l {x2} {y0} {x3} {y3} {x3} {y4} c\n\
         {x3} {y5} l {x3} {y6} {x2} {y7} {x1} {y7} c\n\
         {x0} {y7} l {x8} {y7} {x9} {y6} {x9} {y5} c\n\
         {x9} {y4} l {x9} {y3} {x8} {y0} {x0} {y0} c\n\
         h\n",
        x0 = x + radius,
        x1 = x + width - radius,
        x2 = x + width - radius + k,
        x3 = x + width,
        x8 = x + radius - k,
        x9 = x,
        y0 = y + height,
        y3 = y + height - radius + k,
        y4 = y + height - radius,
        y5 = y + radius,
        y6 = y + radius - k,
        y7 = y,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CornerRadii, CornerRadius};

    #[test]
    fn pdf_rect_converts_top_coordinates_once() {
        let rect = PdfRect::from_top(10.0, 100.0, 30.0, 40.0);
        assert_eq!(rect, PdfRect::new(10.0, 60.0, 30.0, 40.0));
        assert_eq!(rect.right(), 40.0);
        assert_eq!(rect.top(), 100.0);
    }

    #[test]
    fn rounded_rect_perimeter_includes_straights_and_corner_arcs() {
        let square = PdfRect::new(0.0, 0.0, 100.0, 50.0).rounded(CornerRadii::ZERO);
        assert_eq!(square.perimeter(), 300.0);

        let rounded = PdfRect::new(0.0, 0.0, 100.0, 50.0).rounded(CornerRadii::circular(10.0));
        let expected = 220.0 + 20.0 * std::f32::consts::PI;
        assert!((rounded.perimeter() - expected).abs() < 0.001);
    }

    #[test]
    fn pdf_rect_resolves_css_transform_origin_from_the_top_edge() {
        let rect = PdfRect::new(10.0, 20.0, 30.0, 40.0);
        assert_eq!(
            rect.css_transform_origin(TransformOrigin {
                x_fraction: 0.5,
                y_fraction: 1.0,
                ..Default::default()
            }),
            PdfPoint::new(25.0, 20.0)
        );
    }

    #[test]
    fn pdf_rect_maps_top_down_raster_tiles_without_flipping_rows() {
        let rect = PdfRect::new(10.0, 20.0, 100.0, 100.0);
        let dimensions = RasterDimensions {
            width: 4,
            height: 4,
        };
        assert_eq!(
            rect.raster_tile(
                dimensions,
                RasterTile {
                    x: 1,
                    y: 0,
                    width: 2,
                    height: 2,
                },
            ),
            PdfRect::new(35.0, 70.0, 50.0, 50.0)
        );
        assert_eq!(
            rect.raster_tile(
                dimensions,
                RasterTile {
                    x: 1,
                    y: 2,
                    width: 2,
                    height: 2,
                },
            ),
            PdfRect::new(35.0, 20.0, 50.0, 50.0)
        );
    }

    #[test]
    fn pdf_rect_insets_asymmetric_physical_edges() {
        let rect =
            PdfRect::from_top(10.0, 100.0, 30.0, 40.0).inset(EdgeSizes::new(1.0, 2.0, 3.0, 4.0));
        assert_eq!(rect, PdfRect::new(14.0, 63.0, 24.0, 36.0));
    }

    #[test]
    fn oversized_insets_keep_the_authored_origin_shift() {
        let rect = PdfRect::new(10.0, 20.0, 3.0, 4.0).inset(EdgeSizes::new(8.0, 9.0, 7.0, 6.0));
        assert_eq!(rect, PdfRect::new(16.0, 27.0, 0.0, 0.0));
        assert!(rect.is_empty());
    }

    #[test]
    fn rectangle_coverage_uses_all_four_derived_edges() {
        let outer = PdfRect::new(10.0, 20.0, 30.0, 40.0);
        let inner = PdfRect::new(12.0, 22.0, 26.0, 36.0);
        assert!(outer.covers_with_margin(inner, 2.0));
        assert!(!outer.covers_with_margin(inner, 2.001));
    }

    #[test]
    fn rectangle_intersection_returns_only_shared_area() {
        let left = PdfRect::new(10.0, 20.0, 30.0, 40.0);
        let right = PdfRect::new(25.0, 5.0, 30.0, 30.0);
        assert_eq!(
            left.intersection(right),
            Some(PdfRect::new(25.0, 20.0, 15.0, 15.0))
        );
        assert_eq!(left.intersection(PdfRect::new(40.0, 20.0, 5.0, 5.0)), None);
    }

    #[test]
    fn box_geometry_derives_every_box_from_one_border_box() {
        let geometry = BoxGeometry::new(
            PdfRect::new(10.0, 20.0, 100.0, 80.0),
            EdgeSizes::new(1.0, 2.0, 3.0, 4.0),
            EdgeSizes::new(5.0, 6.0, 7.0, 8.0),
        );
        assert_eq!(geometry.padding_box(), PdfRect::new(14.0, 23.0, 94.0, 76.0));
        assert_eq!(geometry.content_box(), PdfRect::new(22.0, 30.0, 80.0, 64.0));
        assert_eq!(geometry.shape_box(ShapeBox::Border), geometry.border_box);
        assert_eq!(
            geometry.shape_box(ShapeBox::Padding),
            geometry.padding_box()
        );
        assert_eq!(
            geometry.shape_box(ShapeBox::Content),
            geometry.content_box()
        );
        assert_eq!(
            geometry.background_origin_box(BackgroundOrigin::Border),
            geometry.border_box
        );
        assert_eq!(
            geometry.background_origin_box(BackgroundOrigin::Padding),
            geometry.padding_box()
        );
        assert_eq!(
            geometry.background_origin_box(BackgroundOrigin::Content),
            geometry.content_box()
        );
    }

    #[test]
    fn content_box_transform_reference_retains_used_border_and_padding() {
        let geometry = BoxGeometry::new(
            PdfRect::new(10.0, 20.0, 100.0, 80.0),
            EdgeSizes::new(1.0, 2.0, 3.0, 4.0),
            EdgeSizes::new(5.0, 6.0, 7.0, 8.0),
        );
        let top_left = geometry.transform_reference(&BoxTransform {
            origin: TransformOrigin {
                x_fraction: 0.0,
                y_fraction: 0.0,
                ..Default::default()
            },
            reference_box: TransformBox::ContentBox,
            ..Default::default()
        });

        assert_eq!(top_left.size(), PdfVector::new(80.0, 64.0));
        assert_eq!(top_left.pivot(), PdfPoint::new(22.0, 94.0));
        assert_eq!(top_left.local_pivot(), PdfVector::new(12.0, 6.0));

        let center = geometry.transform_reference(&BoxTransform {
            reference_box: TransformBox::ContentBox,
            ..Default::default()
        });
        assert_eq!(center.pivot(), PdfPoint::new(62.0, 62.0));
        assert_eq!(center.local_pivot(), PdfVector::new(52.0, 38.0));
    }

    #[test]
    fn clip_rectangle_and_radii_share_the_same_asymmetric_inset() {
        let border = EdgeSizes::new(1.0, 2.0, 3.0, 4.0);
        let radii = CornerRadii::new(
            CornerRadius::new(10.0, 20.0),
            CornerRadius::new(30.0, 40.0),
            CornerRadius::new(50.0, 60.0),
            CornerRadius::new(70.0, 80.0),
        );
        let geometry = BoxGeometry::new(
            PdfRect::new(10.0, 20.0, 100.0, 100.0),
            border,
            EdgeSizes::ZERO,
        );
        let clip = geometry.background_clip_box(BackgroundClip::Padding, radii);
        assert_eq!(clip.rect, geometry.padding_box());
        assert_eq!(clip.radii, radii.fit_to(100.0, 100.0).inset(border));
        assert_eq!(
            geometry
                .background_clip_box(BackgroundClip::Text, radii)
                .rect,
            geometry.border_box
        );
    }
}
