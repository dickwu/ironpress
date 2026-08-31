//! CSS letter-spacing over a shaped SVG text run.

use crate::text::{ShapedGlyph, ShapedRun, is_default_ignorable};

/// Tracking applied to a shaped run per typographic character unit.
///
/// CSS Text 3 §8.2 inserts letter-spacing between typographic character units,
/// not between glyphs: a base letter together with its combining marks is one
/// unit, and invisible zero-width formatting characters receive no spacing at
/// all ("spacing must be added as if those characters did not exist"). The
/// shaper reports units as clusters, so the run is walked cluster by cluster
/// and the spacing is charged after the last glyph of each spaced unit. The
/// PDF `Tc` operator cannot express this because it spaces every glyph.
///
/// The trailing unit is spaced too, as in Chrome's SVG text layout, so the
/// tracked advance of a text chunk (the quantity `text-anchor` positions)
/// grows by one spacing per spaced unit.
pub(super) struct TrackedRun<'a> {
    shaped: &'a ShapedRun,
    /// Spacing charged after each glyph, indexed like `shaped.glyphs`.
    spacing_after: Vec<f32>,
}

impl<'a> TrackedRun<'a> {
    pub(super) fn new(shaped: &'a ShapedRun, letter_spacing: f32) -> Self {
        let glyphs = &shaped.glyphs;
        let mut spacing_after = vec![0.0; glyphs.len()];
        if letter_spacing != 0.0 {
            let unit_starts: Vec<usize> = glyphs
                .iter()
                .enumerate()
                .filter(|(_, glyph)| !glyph.unicode.is_empty())
                .map(|(index, _)| index)
                .collect();
            for (position, &start) in unit_starts.iter().enumerate() {
                let end = unit_starts
                    .get(position + 1)
                    .copied()
                    .unwrap_or(glyphs.len());
                if unit_is_spaced(&glyphs[start].unicode) {
                    spacing_after[end - 1] = letter_spacing;
                }
            }
        }
        Self {
            shaped,
            spacing_after,
        }
    }

    pub(super) fn glyphs(&self) -> &[ShapedGlyph] {
        &self.shaped.glyphs
    }

    /// The tracking charged after the glyph at `index`.
    pub(super) fn spacing_after(&self, index: usize) -> f32 {
        self.spacing_after.get(index).copied().unwrap_or(0.0)
    }

    /// The run's advance including tracking.
    pub(super) fn advance(&self) -> f32 {
        self.shaped.width + self.spacing_after.iter().sum::<f32>()
    }
}

/// A unit is spaced unless every character in it is default-ignorable.
fn unit_is_spaced(unicode: &[u16]) -> bool {
    char::decode_utf16(unicode.iter().copied())
        .any(|decoded| decoded.is_ok_and(|ch| !is_default_ignorable(ch)))
}

#[cfg(test)]
mod tests {
    use super::TrackedRun;
    use crate::text::{ShapedGlyph, ShapedRun};

    fn glyph(advance: f32, cluster_text: &str) -> ShapedGlyph {
        ShapedGlyph {
            glyph_id: 1,
            x_advance: advance,
            y_advance: 0.0,
            x_offset: 0.0,
            y_offset: 0.0,
            unicode: cluster_text.encode_utf16().collect(),
        }
    }

    fn run(glyphs: Vec<ShapedGlyph>) -> ShapedRun {
        let width = glyphs.iter().map(|glyph| glyph.x_advance).sum();
        ShapedRun { glyphs, width }
    }

    #[test]
    fn every_visible_unit_is_spaced_including_the_last() {
        let shaped = run(vec![glyph(10.0, "A"), glyph(10.0, "B"), glyph(10.0, "C")]);
        let tracked = TrackedRun::new(&shaped, 2.0);
        assert_eq!(
            (0..3).map(|i| tracked.spacing_after(i)).collect::<Vec<_>>(),
            vec![2.0, 2.0, 2.0]
        );
        assert_eq!(tracked.advance(), 36.0);
    }

    #[test]
    fn combining_marks_share_their_base_letter_unit() {
        // A base letter followed by a mark glyph continuing the same cluster.
        let shaped = run(vec![
            glyph(10.0, "e\u{301}"),
            glyph(0.0, ""),
            glyph(10.0, "x"),
        ]);
        let tracked = TrackedRun::new(&shaped, 3.0);
        assert_eq!(
            (0..3).map(|i| tracked.spacing_after(i)).collect::<Vec<_>>(),
            vec![0.0, 3.0, 3.0]
        );
    }

    #[test]
    fn zero_width_formatting_characters_receive_no_spacing() {
        let shaped = run(vec![
            glyph(10.0, "A"),
            glyph(0.0, "\u{200B}"),
            glyph(10.0, "B"),
            glyph(0.0, "\u{200D}"),
        ]);
        let tracked = TrackedRun::new(&shaped, 4.0);
        assert_eq!(
            (0..4).map(|i| tracked.spacing_after(i)).collect::<Vec<_>>(),
            vec![4.0, 0.0, 4.0, 0.0]
        );
        let plain = run(vec![glyph(10.0, "A"), glyph(10.0, "B")]);
        assert_eq!(tracked.advance(), TrackedRun::new(&plain, 4.0).advance());
    }

    #[test]
    fn normal_tracking_leaves_the_shaped_advance_alone() {
        let shaped = run(vec![glyph(10.0, "A"), glyph(12.0, "B")]);
        let tracked = TrackedRun::new(&shaped, 0.0);
        assert_eq!(tracked.advance(), 22.0);
        assert_eq!(tracked.spacing_after(1), 0.0);
        assert_eq!(tracked.spacing_after(7), 0.0);
    }
}
