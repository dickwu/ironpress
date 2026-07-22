use crate::style::computed::BorderStyle;
use crate::types::PhysicalSide;

/// Used width of either rule in a CSS `double` border.
///
/// CSS leaves the individual line/gap allocation implementation-defined. This
/// preserves the established browser-compatible integer-CSS-pixel allocation
/// and retains exact thirds for fractional widths.
pub(crate) fn double_rule_width(width: f32) -> f32 {
    let css_width = width / 0.75;
    let snapped_css_width = css_width.round();
    if snapped_css_width >= 3.0 && (css_width - snapped_css_width).abs() < 0.001 {
        ((snapped_css_width + 1.0) / 3.0).floor() * 0.75
    } else {
        width / 3.0
    }
    .min(width / 2.0)
}

const LIGHTENED_BLACK: f32 = 84.0 / 255.0;
const VALUE_STEP: f32 = 0.33;

fn to_legacy_3d_component(component: f32) -> f32 {
    (component.clamp(0.0, 1.0) * 255.999_98).floor() / 255.0
}

pub(crate) fn bevel_light_color((r, g, b): (f32, f32, f32)) -> (f32, f32, f32) {
    let value = r.max(g).max(b);
    if value == 0.0 {
        return (LIGHTENED_BLACK, LIGHTENED_BLACK, LIGHTENED_BLACK);
    }
    if r >= 150.0 / 255.0 || g >= 92.0 / 255.0 {
        return (r, g, b);
    }
    let multiplier = (value + VALUE_STEP).min(1.0) / value;
    (
        to_legacy_3d_component(r * multiplier),
        to_legacy_3d_component(g * multiplier),
        to_legacy_3d_component(b * multiplier),
    )
}

pub(crate) fn bevel_dark_color((r, g, b): (f32, f32, f32)) -> (f32, f32, f32) {
    let value = r.max(g).max(b);
    if value == 0.0 {
        return (0.0, 0.0, 0.0);
    }
    let multiplier = (value - VALUE_STEP).max(0.0) / value;
    (
        to_legacy_3d_component(r * multiplier),
        to_legacy_3d_component(g * multiplier),
        to_legacy_3d_component(b * multiplier),
    )
}

pub(crate) fn is_bevel_style(style: BorderStyle) -> bool {
    matches!(
        style,
        BorderStyle::Groove | BorderStyle::Ridge | BorderStyle::Inset | BorderStyle::Outset
    )
}

pub(crate) fn bevel_edge_color(
    style: BorderStyle,
    side: PhysicalSide,
    inner_band: bool,
    base: (f32, f32, f32),
) -> (f32, f32, f32) {
    let high_edge = matches!(side, PhysicalSide::Top | PhysicalSide::Left);
    let light_on_high_edge = match style {
        BorderStyle::Outset => true,
        BorderStyle::Inset => false,
        BorderStyle::Ridge => !inner_band,
        BorderStyle::Groove => inner_band,
        _ => return base,
    };
    if high_edge == light_on_high_edge {
        bevel_light_color(base)
    } else {
        bevel_dark_color(base)
    }
}
