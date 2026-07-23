//! CSS value resolution for calc(), var(), and new unit types (%, em, rem, vw, vh).
use std::collections::{HashMap, HashSet};

use crate::parser::css::{CssValue, MathUnitContext};

#[cfg(test)]
const DEFAULT_FONT_SIZE: f32 = 12.0;
#[cfg(test)]
const DEFAULT_PAGE_WIDTH: f32 = 595.28;
#[cfg(test)]
const DEFAULT_PAGE_HEIGHT: f32 = 841.89;

#[derive(Debug, Clone, Copy)]
pub struct LengthResolutionContext {
    /// The containing-block dimension against which percentages resolve.
    pub percentage_basis: f32,
    /// Every non-percentage unit basis used by CSS math.
    pub units: MathUnitContext,
}

impl LengthResolutionContext {
    pub const fn new(percentage_basis: f32, units: MathUnitContext) -> Self {
        Self {
            percentage_basis,
            units,
        }
    }

    pub const fn with_percentage_basis(self, percentage_basis: f32) -> Self {
        Self {
            percentage_basis,
            ..self
        }
    }

    #[cfg(test)]
    pub const fn pdf_defaults(parent_width: f32) -> Self {
        Self::new(
            parent_width,
            MathUnitContext::from_font_and_viewport(
                DEFAULT_FONT_SIZE,
                DEFAULT_FONT_SIZE,
                DEFAULT_PAGE_WIDTH,
                DEFAULT_PAGE_HEIGHT,
            ),
        )
    }

    #[cfg(test)]
    pub const fn pdf_with_font_sizes(
        parent_width: f32,
        font_size: f32,
        root_font_size: f32,
    ) -> Self {
        Self::new(
            parent_width,
            MathUnitContext::from_font_and_viewport(
                font_size,
                root_font_size,
                DEFAULT_PAGE_WIDTH,
                DEFAULT_PAGE_HEIGHT,
            ),
        )
    }
}

impl From<&crate::layout::engine::LayoutContext> for LengthResolutionContext {
    fn from(ctx: &crate::layout::engine::LayoutContext) -> Self {
        Self {
            percentage_basis: ctx.parent.content_width,
            units: MathUnitContext::from_font_and_viewport(
                ctx.parent.font_size,
                ctx.root_font_size,
                ctx.viewport.width,
                ctx.viewport.height,
            ),
        }
    }
}

/// Resolve a CssValue to absolute length in points using a caller-provided
/// `font_size` basis for em units.
pub fn resolve_length_value_in_context(
    val: &CssValue,
    ctx: LengthResolutionContext,
    custom_properties: &HashMap<String, String>,
) -> Option<f32> {
    match val {
        CssValue::Length(v) => Some(*v),
        CssValue::Em(v) => Some(*v * ctx.units.font.em),
        CssValue::Number(v) if *v == 0.0 => Some(0.0),
        CssValue::Percentage(v) => Some(ctx.percentage_basis * v / 100.0),
        // ex/ch resolve against the element's own font metrics. The resolution
        // context does not carry the font, so use the css-values-4 fallbacks
        // (x-height ~= 0.5em, '0' advance ~= 0.5em). The `font-size` property —
        // where the metric matters most and the font *is* known — resolves these
        // with real metrics in `apply_style_map`.
        CssValue::Ex(v) => Some(*v * ctx.units.font.ex),
        CssValue::Ch(v) => Some(*v * ctx.units.font.ch),
        CssValue::Rem(v) => Some(*v * ctx.units.font.rem),
        CssValue::Vw(v) => Some(ctx.units.viewport.width * v / 100.0),
        CssValue::Vh(v) => Some(ctx.units.viewport.height * v / 100.0),
        CssValue::Vmin(v) => {
            Some(ctx.units.viewport.width.min(ctx.units.viewport.height) * v / 100.0)
        }
        CssValue::Vmax(v) => {
            Some(ctx.units.viewport.width.max(ctx.units.viewport.height) * v / 100.0)
        }
        CssValue::Math(expression) => expression.resolve(ctx.units, ctx.percentage_basis),
        CssValue::Var(name, fallback) => {
            let raw = resolve_var_to_string(name, fallback.as_deref(), custom_properties)?;
            let parsed = crate::parser::css::parse_inline_style(&format!("_x: {raw}"));
            parsed
                .get("_x")
                .and_then(|inner| resolve_length_value_in_context(inner, ctx, custom_properties))
        }
        _ => None,
    }
}

/// Resolve a CssValue to absolute length in points.
#[cfg(test)]
pub fn resolve_length_value(
    val: &CssValue,
    parent_width: f32,
    root_font_size: f32,
    page_width: f32,
    page_height: f32,
    custom_properties: &HashMap<String, String>,
) -> Option<f32> {
    resolve_length_value_in_context(
        val,
        LengthResolutionContext::new(
            parent_width,
            MathUnitContext::from_font_and_viewport(
                DEFAULT_FONT_SIZE,
                root_font_size,
                page_width,
                page_height,
            ),
        ),
        custom_properties,
    )
}

/// Try to resolve a CssValue to an absolute length using defaults.
#[cfg(test)]
pub fn try_resolve_to_length(
    val: &CssValue,
    custom_properties: &HashMap<String, String>,
    parent_width_hint: f32,
) -> Option<f32> {
    resolve_length_value_in_context(
        val,
        LengthResolutionContext::pdf_defaults(parent_width_hint),
        custom_properties,
    )
}

/// Try to resolve a CssValue to an absolute length using a caller-provided
/// `font_size` basis for em units.
pub fn try_resolve_to_length_in_context(
    val: &CssValue,
    custom_properties: &HashMap<String, String>,
    ctx: LengthResolutionContext,
) -> Option<f32> {
    resolve_length_value_in_context(val, ctx, custom_properties)
}

/// Try to resolve a CssValue to an absolute length using a caller-provided
/// `font_size` basis for em units.
#[cfg(test)]
pub fn try_resolve_to_length_with_font_size(
    val: &CssValue,
    custom_properties: &HashMap<String, String>,
    parent_width_hint: f32,
    font_size: f32,
    root_font_size: f32,
) -> Option<f32> {
    try_resolve_to_length_in_context(
        val,
        custom_properties,
        LengthResolutionContext::pdf_with_font_sizes(parent_width_hint, font_size, root_font_size),
    )
}

fn parse_var_reference(raw: &str) -> Option<(&str, Option<&str>)> {
    let inner = raw.trim().strip_prefix("var(")?.strip_suffix(')')?.trim();
    let (name, fallback) = match inner.split_once(',') {
        Some((name, fallback)) => (name.trim(), Some(fallback.trim())),
        None => (inner, None),
    };

    name.starts_with("--").then_some((name, fallback))
}

struct VarResolver<'a> {
    custom_properties: &'a HashMap<String, String>,
    visiting: HashSet<String>,
}

impl<'a> VarResolver<'a> {
    fn new(custom_properties: &'a HashMap<String, String>) -> Self {
        Self {
            custom_properties,
            visiting: HashSet::new(),
        }
    }

    fn resolve_var(&mut self, name: &str, fallback: Option<&str>) -> Option<String> {
        if self.visiting.contains(name) {
            return fallback.and_then(|value| self.resolve_raw(value));
        }

        let resolved = self
            .custom_properties
            .get(name)
            .map(String::as_str)
            .and_then(|raw| {
                self.visiting.insert(name.to_string());
                let result = self.resolve_raw(raw);
                self.visiting.remove(name);
                result
            });

        resolved.or_else(|| fallback.and_then(|value| self.resolve_raw(value)))
    }

    fn resolve_raw(&mut self, raw: &str) -> Option<String> {
        if let Some((name, fallback)) = parse_var_reference(raw) {
            self.resolve_var(name, fallback)
        } else {
            Some(raw.trim().to_string())
        }
    }
}

/// Resolve a var() name to its final value string, following nested aliases.
pub fn resolve_var_to_string(
    name: &str,
    fallback: Option<&str>,
    custom_properties: &HashMap<String, String>,
) -> Option<String> {
    VarResolver::new(custom_properties).resolve_var(name, fallback)
}

/// Try to resolve a CssValue::Var to a color.
pub fn try_resolve_var_to_color(
    val: &CssValue,
    custom_properties: &HashMap<String, String>,
) -> Option<crate::parser::css::SpecifiedColor> {
    if let CssValue::Var(name, fallback) = val {
        let raw = resolve_var_to_string(name, fallback.as_deref(), custom_properties)?;
        match crate::parser::css::parse_color(&raw) {
            Some(CssValue::Color(color)) => Some(color),
            _ => None,
        }
    } else {
        None
    }
}

/// Try to resolve a CssValue::Var to a keyword string.
pub fn try_resolve_var_to_keyword(
    val: &CssValue,
    custom_properties: &HashMap<String, String>,
) -> Option<String> {
    if let CssValue::Var(name, fallback) = val {
        resolve_var_to_string(name, fallback.as_deref(), custom_properties)
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
