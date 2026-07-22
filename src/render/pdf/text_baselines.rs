//! Shared horizontal text-baseline painting.
//!
//! CSS line layout retains its own flow cursor through PDF painting. Keeping
//! that fractional position intact prevents paint-time rounding from changing
//! a line's glyph coverage without changing its layout geometry.

use super::LineBoxMetrics;

/// Advances a text block's fractional flow position and resolves individual
/// horizontal baselines for PDF painting.
#[derive(Clone, Copy, Debug)]
pub(super) struct TextBaselineCursor {
    flow_y: f32,
}

impl TextBaselineCursor {
    pub(super) const fn new(content_top: f32) -> Self {
        Self {
            flow_y: content_top,
        }
    }

    /// Advance one line while preserving the fractional flow cursor.
    pub(super) fn next_raw(&mut self, metrics: LineBoxMetrics) -> f32 {
        self.flow_y -= metrics.half_leading + metrics.ascender;
        let baseline = self.flow_y;
        self.flow_y -= metrics.descender + metrics.half_leading;
        baseline
    }

    /// Advance one ordinary horizontal line without perturbing fractional layout
    /// geometry at paint time.
    pub(super) fn next_horizontal(&mut self, metrics: LineBoxMetrics) -> f32 {
        self.next_raw(metrics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_each_fractional_baseline_without_changing_line_advance() {
        let mut cursor = TextBaselineCursor::new(103.5);
        let metrics = LineBoxMetrics {
            ascender: 16.5,
            descender: 6.6,
            half_leading: 0.0,
        };

        assert_eq!(cursor.next_horizontal(metrics), 87.0);
        assert!((cursor.next_horizontal(metrics) - 63.9).abs() < 0.000_1);
        assert!((cursor.next_horizontal(metrics) - 40.8).abs() < 0.000_1);
    }
}
