//! Browser print-to-page scaling for normal-flow layout overflow.
//!
//! Chromium reduces an overflowing document's normal-flow width to the page's
//! printable width before emitting its PDF content stream. This is distinct
//! from CSS overflow: positioned visual overflow remains clipped by the page
//! and must not change the scale of unrelated flow content.

use super::engine::{LayoutElement, Page};
use crate::types::PageSize;

/// A uniform scale applied to page content around the physical page's top-left
/// corner by the PDF renderer. It owns the print-fit decision so render paths
/// never infer a scale from individual paint operations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PrintContentScale(f32);

impl PrintContentScale {
    /// Keep normal-flow content within the physical page width without enlarging
    /// content that already fits. Invalid or empty geometry leaves it alone.
    pub(crate) fn from_flow_width(printable_width: f32, flow_right_edge: f32) -> Self {
        if !printable_width.is_finite()
            || !flow_right_edge.is_finite()
            || printable_width <= 0.0
            || flow_right_edge <= printable_width
        {
            return Self::default();
        }
        Self(printable_width / flow_right_edge)
    }

    pub(crate) const fn factor(self) -> f32 {
        self.0
    }

    pub(crate) const fn is_identity(self) -> bool {
        self.0 == 1.0
    }
}

impl Default for PrintContentScale {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Compute the per-page print scale after pagination, when page-specific
/// `@page` dimensions and margins are known.
pub(crate) fn assign_page_print_scales(pages: &mut [Page], default_page_size: PageSize) {
    for page in pages {
        let page_size = page.page_size_override.unwrap_or(default_page_size);
        let flow_right_edge = page
            .elements
            .iter()
            .filter_map(|(_, element)| normal_flow_right_edge(element.as_ref()))
            .fold(0.0_f32, f32::max);
        page.print_content_scale =
            PrintContentScale::from_flow_width(page_size.width, flow_right_edge);
    }
}

fn normal_flow_right_edge(element: &dyn LayoutElement) -> Option<f32> {
    element.inline_flow_extent()?.normal_flow_right_edge()
}

#[cfg(test)]
mod tests;
