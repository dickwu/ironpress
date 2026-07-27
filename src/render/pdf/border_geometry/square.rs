use super::*;

/// One square border band between two edge insets.
///
/// Horizontal bands span the complete outer width. The vertical trapezoids
/// repaint only their corner triangles and therefore own the final diagonal
/// frontier. This is the browser PDF decomposition for opaque square 3D
/// borders; translucent paint must use exclusive side regions instead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::render::pdf) struct SquareBevelBandGeometry {
    outer: PdfRect,
    inner: PdfRect,
}

impl SquareBevelBandGeometry {
    pub(in crate::render::pdf) fn between(
        border_box: PdfRect,
        outer_inset: EdgeSizes,
        inner_inset: EdgeSizes,
    ) -> Self {
        Self {
            outer: border_box.inset(outer_inset),
            inner: border_box.inset(inner_inset),
        }
    }

    pub(in crate::render::pdf) fn top(self) -> PdfRect {
        PdfRect::new(
            self.outer.left,
            self.inner.top(),
            self.outer.width,
            self.outer.top() - self.inner.top(),
        )
    }

    pub(in crate::render::pdf) fn bottom(self) -> PdfRect {
        PdfRect::new(
            self.outer.left,
            self.outer.bottom,
            self.outer.width,
            self.inner.bottom - self.outer.bottom,
        )
    }

    pub(in crate::render::pdf) fn right(self) -> BorderSideRegion {
        BorderSideRegion {
            points: [
                PdfPoint::new(self.outer.right(), self.outer.top()),
                PdfPoint::new(self.outer.right(), self.outer.bottom),
                PdfPoint::new(self.inner.right(), self.inner.bottom),
                PdfPoint::new(self.inner.right(), self.inner.top()),
            ],
        }
    }

    pub(in crate::render::pdf) fn left(self) -> BorderSideRegion {
        BorderSideRegion {
            points: [
                PdfPoint::new(self.outer.left, self.outer.bottom),
                PdfPoint::new(self.outer.left, self.outer.top()),
                PdfPoint::new(self.inner.left, self.inner.top()),
                PdfPoint::new(self.inner.left, self.inner.bottom),
            ],
        }
    }
}
