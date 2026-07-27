use crate::layout::engine::{TextRun, TextRunMetadata};
use crate::style::computed::BoxShadow;

/// Coalesce adjacent runs exactly where layout and paint treat them as one
/// shaping buffer. Line wrapping, font subsetting, and every paint path consume
/// this same representation, so none can discover a contextual glyph or
/// advance after another subsystem has chosen a boundary.
pub(crate) fn coalesce_text_runs(runs: &[TextRun]) -> Vec<TextRun> {
    let mut coalesced = Vec::new();
    for run in runs {
        if run.inline_box.is_some() {
            coalesced.push(run.clone());
            continue;
        }
        if run.text.is_empty() {
            continue;
        }

        let can_coalesce = coalesced
            .last()
            .is_some_and(|previous| text_runs_share_shaping_buffer(previous, run));
        if can_coalesce {
            if let Some(previous) = coalesced.last_mut() {
                previous.text.push_str(&run.text);
                previous.metadata.boundary = run.metadata.boundary;
            }
        } else {
            coalesced.push(run.clone());
        }
    }
    coalesced
}

pub(crate) fn text_runs_share_shaping_buffer(previous: &TextRun, next: &TextRun) -> bool {
    previous.inline_box.is_none()
        && next.inline_box.is_none()
        && !previous.metadata.text_combine_upright.is_active()
        && !next.metadata.text_combine_upright.is_active()
        && previous.font_size == next.font_size
        && previous.bold == next.bold
        && previous.font_style == next.font_style
        && previous.color == next.color
        && previous.decorations == next.decorations
        && previous.link_url == next.link_url
        && previous.font_family == next.font_family
        && previous.font_synthesis == next.font_synthesis
        && previous.background_color == next.background_color
        && previous.padding == next.padding
        && previous.border_radii == next.border_radii
        && resolved_metric_matches(previous.line_height_factor, next.line_height_factor)
        && resolved_metric_matches(previous.line_height_basis, next.line_height_basis)
        && previous.font_variant_position == next.font_variant_position
        && previous.shaping == next.shaping
        && previous.vertical_align == next.vertical_align
        && shadows_match(&previous.text_shadow, &next.text_shadow)
        && metadata_matches(previous.metadata, next.metadata)
        // The bidi pass deliberately retained this visual-order boundary.
        && crate::bidi::has_rtl_chars(&previous.text) == crate::bidi::has_rtl_chars(&next.text)
}

fn resolved_metric_matches(previous: f32, next: f32) -> bool {
    previous == next || (previous.is_nan() && next.is_nan())
}

fn metadata_matches(previous: TextRunMetadata, next: TextRunMetadata) -> bool {
    previous.emphasis == next.emphasis
        && previous.spacing == next.spacing
        && previous.is_drop_cap == next.is_drop_cap
        // A boundary can become ordinary intra-run tracking only when it
        // carries the same spacing as the run and no separate pair-positioning
        // adjustment.
        && previous.boundary.can_be_absorbed_by(previous.spacing)
}

fn shadows_match(previous: &[BoxShadow], next: &[BoxShadow]) -> bool {
    previous.len() == next.len()
        && previous.iter().zip(next).all(|(previous, next)| {
            previous.offset_x == next.offset_x
                && previous.offset_y == next.offset_y
                && previous.blur == next.blur
                && previous.spread == next.spread
                && previous.color == next.color
                && previous.color_source == next.color_source
                && previous.inset == next.inset
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::computed::TextCombineUpright;

    fn split_word() -> Vec<TextRun> {
        "verification"
            .chars()
            .map(|character| TextRun {
                text: character.to_string(),
                ..Default::default()
            })
            .collect()
    }

    #[test]
    fn adjacent_character_tokens_form_one_shaping_buffer() {
        let coalesced = coalesce_text_runs(&split_word());

        assert_eq!(coalesced.len(), 1);
        assert_eq!(coalesced[0].text, "verification");
    }

    #[test]
    fn every_run_local_paint_boundary_prevents_coalescing() {
        let base = TextRun {
            text: "f".to_string(),
            ..Default::default()
        };
        let mut variants = Vec::new();

        let mut shaping = base.clone();
        shaping.text = "i".to_string();
        shaping.shaping.ligatures = false;
        variants.push(shaping);

        let mut spacing = base.clone();
        spacing.text = "i".to_string();
        spacing.metadata.spacing.letter = 1.0;
        variants.push(spacing);

        let mut shadow = base.clone();
        shadow.text = "i".to_string();
        shadow.text_shadow.push(BoxShadow {
            offset_x: 1.0,
            offset_y: 1.0,
            blur: 0.0,
            spread: 0.0,
            color: crate::types::Color::BLACK,
            color_source: crate::style::computed::ColorSource::Absolute,
            inset: false,
        });
        variants.push(shadow);

        let mut emphasis = base.clone();
        emphasis.text = "i".to_string();
        emphasis.metadata.emphasis.mark = true;
        variants.push(emphasis);

        let mut combine = base.clone();
        combine.text = "i".to_string();
        combine.metadata.text_combine_upright = TextCombineUpright::All;
        variants.push(combine);

        for variant in variants {
            assert_eq!(
                coalesce_text_runs(&[base.clone(), variant]).len(),
                2,
                "distinct run-local paint state must remain a shaping boundary"
            );
        }
    }
}
