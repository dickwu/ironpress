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
fn square_uniform_border_uses_the_canonical_centerline_stroke() {
    let mut content = String::new();
    let mut states = Vec::new();
    let mut counter = 0;

    assert!(paint_uniform_border(
        &mut content,
        RoundedRect::new(PdfRect::new(0.0, 0.0, 100.0, 50.0), CornerRadii::ZERO),
        solid(4.0, crate::types::Color::BLACK),
        PdfContentSpace::Points,
        &mut states,
        &mut counter,
    ));

    assert!(content.ends_with("S\n"));
    assert!(!content.contains("f*\n"));
}

#[test]
fn square_uniform_border_retains_page_css_serialization() {
    let mut content = String::new();
    let mut states = Vec::new();
    let mut counter = 0;
    let content_space =
        PdfContentSpace::page_css(PageContentTransform::print(PdfVector::new(150.0, 150.0)));

    assert!(paint_uniform_border(
        &mut content,
        RoundedRect::new(PdfRect::new(12.0, 12.0, 117.0, 123.0), CornerRadii::ZERO,),
        solid(0.75, crate::types::Color::BLACK),
        content_space,
        &mut states,
        &mut counter,
    ));

    assert!(content.starts_with("q\n"));
    assert!(content.contains("1 w\n"));
    assert!(content.ends_with("S\nQ\n"));
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
        PdfContentSpace::Points,
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
        PdfContentSpace::Points,
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
        PdfContentSpace::Points,
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
        PdfContentSpace::Points,
        &mut states,
        &mut counter,
    ));

    assert_eq!(content.lines().filter(|line| *line == "f*").count(), 2);
    assert!(!content.contains("S\n"));
}
