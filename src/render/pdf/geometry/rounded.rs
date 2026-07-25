use super::PdfRect;
use crate::types::{CornerRadii, EdgeSizes};

/// A rectangle and its resolved corner geometry. Keeping the two together
/// prevents callers from insetting one without the other.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(in crate::render::pdf) struct RoundedRect {
    pub(in crate::render::pdf) rect: PdfRect,
    pub(in crate::render::pdf) radii: CornerRadii,
}

impl RoundedRect {
    pub(in crate::render::pdf) const fn new(rect: PdfRect, radii: CornerRadii) -> Self {
        Self { rect, radii }
    }

    pub(in crate::render::pdf) fn inset(self, edges: EdgeSizes) -> Self {
        Self::new(self.rect.inset(edges), self.radii.inset(edges))
    }

    pub(in crate::render::pdf) fn path(self) -> Option<String> {
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

    pub(in crate::render::pdf) fn path_or_rect(self) -> String {
        self.path().unwrap_or_else(|| self.rect.rect_path())
    }

    pub(in crate::render::pdf) fn push_clip(self, content: &mut String) {
        content.push_str(&self.clip_command());
    }

    pub(in crate::render::pdf) fn clip_command(self) -> String {
        let mut command = String::from("q\n");
        command.push_str(&self.path_or_rect());
        command.push_str("W n\n");
        command
    }

    pub(in crate::render::pdf) fn push_rounded_clip(self, content: &mut String) -> bool {
        if self.radii.is_zero() {
            return false;
        }
        self.push_clip(content);
        true
    }
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
