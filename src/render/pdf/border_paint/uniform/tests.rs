use super::*;

fn solid(width: f32, color: crate::types::Color) -> crate::layout::engine::LayoutBorderSide {
    crate::layout::engine::LayoutBorderSide {
        width,
        color,
        style: BorderStyle::Solid,
    }
}

fn double(width: f32, color: crate::types::Color) -> crate::layout::engine::LayoutBorderSide {
    crate::layout::engine::LayoutBorderSide {
        width,
        color,
        style: BorderStyle::Double,
    }
}

#[test]
fn square_single_color_fragment_uses_non_overlapping_rectangular_bands() {
    let color = crate::types::Color::from_srgb(0.2, 0.3, 0.4, 1.0);
    let border = crate::layout::engine::LayoutBorder {
        top: solid(1.0, color),
        right: solid(2.0, color),
        bottom: Default::default(),
        left: solid(3.0, color),
    };
    let square = SquareSolidBorder::from_layout(
        PdfRect::new(10.0, 20.0, 100.0, 50.0),
        &border,
        CornerRadii::ZERO,
    )
    .expect("single-color square border");

    assert_eq!(
        square.bands(),
        [
            PdfRect::new(10.0, 69.0, 100.0, 1.0),
            PdfRect::new(108.0, 20.0, 2.0, 49.0),
            PdfRect::new(10.0, 20.0, 100.0, 0.0),
            PdfRect::new(10.0, 20.0, 3.0, 49.0),
        ]
    );
}

#[test]
fn mixed_solid_colors_require_diagonal_corner_ownership() {
    let border = crate::layout::engine::LayoutBorder {
        top: solid(1.0, crate::types::Color::BLACK),
        right: solid(1.0, crate::types::Color::WHITE),
        bottom: Default::default(),
        left: solid(1.0, crate::types::Color::BLACK),
    };

    assert!(
        SquareSolidBorder::from_layout(
            PdfRect::new(0.0, 0.0, 100.0, 50.0),
            &border,
            CornerRadii::ZERO,
        )
        .is_none()
    );
}

#[test]
fn circular_uniform_border_uses_the_canonical_centerline_stroke() {
    let mut content = String::new();
    let mut states = Vec::new();
    let mut counter = 0;

    assert!(paint_uniform_border(
        &mut content,
        RoundedRect::new(
            PdfRect::new(0.0, 0.0, 100.0, 50.0),
            CornerRadii::circular(12.0),
        ),
        solid(4.0, crate::types::Color::BLACK),
        &mut states,
        &mut counter,
    ));

    assert!(content.ends_with("S\n"));
    assert!(!content.contains("f*\n"));
}

#[test]
fn elliptical_uniform_border_uses_the_css_border_ring() {
    let mut content = String::new();
    let mut states = Vec::new();
    let mut counter = 0;

    assert!(paint_uniform_border(
        &mut content,
        RoundedRect::new(
            PdfRect::new(0.0, 0.0, 100.0, 50.0),
            CornerRadii::uniform(crate::types::CornerRadius::new(18.0, 9.0)),
        ),
        solid(4.0, crate::types::Color::BLACK),
        &mut states,
        &mut counter,
    ));

    assert!(content.ends_with("f*\n"));
    assert!(!content.contains("S\n"));
}

#[test]
fn circular_uniform_double_border_uses_two_centerline_strokes() {
    let mut content = String::new();
    let mut states = Vec::new();
    let mut counter = 0;

    assert!(paint_uniform_border(
        &mut content,
        RoundedRect::new(
            PdfRect::new(0.0, 0.0, 100.0, 50.0),
            CornerRadii::circular(12.0),
        ),
        double(3.0, crate::types::Color::BLACK),
        &mut states,
        &mut counter,
    ));

    assert_eq!(content.lines().filter(|line| *line == "S").count(), 2);
    assert!(!content.contains("f*\n"));
}

#[test]
fn elliptical_uniform_double_border_keeps_exact_css_rings() {
    let mut content = String::new();
    let mut states = Vec::new();
    let mut counter = 0;

    assert!(paint_uniform_border(
        &mut content,
        RoundedRect::new(
            PdfRect::new(0.0, 0.0, 100.0, 50.0),
            CornerRadii::uniform(crate::types::CornerRadius::new(18.0, 9.0)),
        ),
        double(3.0, crate::types::Color::BLACK),
        &mut states,
        &mut counter,
    ));

    assert_eq!(content.lines().filter(|line| *line == "f*").count(), 2);
    assert!(!content.contains("S\n"));
}
