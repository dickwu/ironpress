use crate::parser::dom::ElementNode;
use crate::types::Color;
use std::cell::RefCell;
use std::collections::HashMap;

const BACKGROUND_LAYER_SOURCES: &str = "background-layer-sources";
const BACKGROUND_LAYER_RECORD_SEP: char = '\x1f';
const BACKGROUND_LAYER_FIELD_SEP: char = '\x1e';

thread_local! {
    static BACKGROUND_LAYER_CAPTURE: RefCell<Vec<(String, String)>> = const { RefCell::new(Vec::new()) };
}

/// Context for evaluating CSS media queries against the target page.
#[derive(Debug, Clone, Copy)]
pub struct MediaContext {
    /// Page width in points.
    pub width: f32,
    /// Page height in points.
    pub height: f32,
}

/// Per-ancestor context for nth-child matching in descendant selectors.
#[derive(Debug, Clone)]
pub struct AncestorInfo<'a> {
    /// The ancestor element.
    pub element: &'a ElementNode,
    /// Zero-based index of this ancestor among its parent's children.
    pub child_index: usize,
    /// Total number of children in this ancestor's parent.
    pub sibling_count: usize,
    /// Preceding sibling elements for this ancestor within its parent.
    pub preceding_siblings: Vec<(String, Vec<String>)>,
    /// Following sibling elements for this ancestor within its parent.
    pub following_siblings: Vec<(String, Vec<String>)>,
    /// Whether this ancestor has no element children / non-whitespace text.
    pub is_empty: bool,
}

/// Context for advanced CSS selector matching.
#[derive(Debug, Clone, Default)]
pub struct SelectorContext<'a> {
    /// Ancestor elements from root to direct parent (outermost first).
    pub ancestors: Vec<AncestorInfo<'a>>,
    /// Zero-based index of this element among its parent's element children.
    pub child_index: usize,
    /// Total number of element children in the parent.
    pub sibling_count: usize,
    /// Preceding sibling elements (tag name, class list) in document order.
    pub preceding_siblings: Vec<(String, Vec<String>)>,
    /// Following sibling elements (tag name, class list) in document order.
    /// Needed for `:last-of-type`, `:only-of-type`, `:nth-last-of-type`, and
    /// `:has(~ ...)`/`:has(+ ...)` relational matching. Defaults to empty in
    /// layout paths that don't track forward siblings.
    pub following_siblings: Vec<(String, Vec<String>)>,
    /// Whether this element has no element children and no non-whitespace text
    /// (drives `:empty`). Defaults to `false` where not tracked.
    pub is_empty: bool,
}

/// An operator in a calc() expression.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CalcOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// A token in a calc() expression.
#[derive(Debug, Clone)]
pub enum CalcToken {
    /// Absolute length in points.
    Length(f32),
    /// Percentage value (0-100).
    Percent(f32),
    /// Value in em units.
    Em(f32),
    /// Value in rem units.
    Rem(f32),
    /// Value in vw units.
    Vw(f32),
    /// Value in vh units.
    Vh(f32),
    /// Value in vmin units (1% of the smaller viewport axis).
    Vmin(f32),
    /// Value in vmax units (1% of the larger viewport axis).
    Vmax(f32),
    /// An operator.
    Op(CalcOp),
}

/// Parsed CSS property value.
#[derive(Debug, Clone)]
pub enum CssValue {
    Length(f32),
    Color(Color),
    Keyword(String),
    Number(f32),
    /// Percentage value (0-100 range, e.g. 50% stored as 50.0).
    Percentage(f32),
    /// `ex` unit (css-values-4 §6.1.1): a multiple of the resolved font's
    /// x-height. Stored as the raw coefficient (e.g. `4ex` -> `Ex(4.0)`),
    /// resolved against the font metrics downstream.
    Ex(f32),
    /// `ch` unit (css-values-4 §6.1.1): a multiple of the advance of the `'0'`
    /// glyph in the resolved font. Stored as the raw coefficient.
    Ch(f32),
    /// Rem value (relative to root font-size).
    Rem(f32),
    /// Viewport-width percentage.
    Vw(f32),
    /// Viewport-height percentage.
    Vh(f32),
    /// Percentage of the smaller viewport axis (css-values-4 §6.1.2.2).
    Vmin(f32),
    /// Percentage of the larger viewport axis (css-values-4 §6.1.2.2).
    Vmax(f32),
    /// A calc() expression as a list of tokens.
    Calc(Vec<CalcToken>),
    /// A clamp(min, preferred, max) expression. Each operand is itself a
    /// length-like value (length, percentage, calc, …) resolved lazily so the
    /// percentage basis is known. Resolves to `max(min, min(preferred, max))`.
    Clamp(Box<CssValue>, Box<CssValue>, Box<CssValue>),
    /// A var() reference: (variable_name, optional_fallback).
    Var(String, Option<String>),
}

/// A map of CSS property names to values.
#[derive(Debug, Clone, Default)]
pub struct StyleMap {
    pub properties: HashMap<String, CssValue>,
    pub important: HashMap<String, bool>,
}

impl StyleMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, key: &str, value: CssValue) {
        self.set_with_importance(key, value, false);
    }

    pub fn set_with_importance(&mut self, key: &str, value: CssValue, is_important: bool) {
        if self.is_important(key) && !is_important {
            return;
        }
        capture_background_layer_source(key, &value);
        self.properties.insert(key.to_string(), value);
        self.important.insert(key.to_string(), is_important);
        if key == "font"
            && let Some(CssValue::Keyword(font)) = self.properties.get(key)
            && let Some(family) = font_shorthand_family(font)
        {
            self.properties
                .insert("font-family".to_string(), CssValue::Keyword(family));
            self.important
                .insert("font-family".to_string(), is_important);
        }
        if key == "background-layer-slots"
            && let Some(CssValue::Keyword(slots)) = self.properties.get(key)
            && let Some(sources) = captured_background_layer_sources(slots)
        {
            self.properties.insert(
                BACKGROUND_LAYER_SOURCES.to_string(),
                CssValue::Keyword(sources),
            );
            self.important
                .insert(BACKGROUND_LAYER_SOURCES.to_string(), is_important);
        }
    }

    pub fn get(&self, key: &str) -> Option<&CssValue> {
        self.properties.get(key)
    }

    pub fn remove(&mut self, key: &str) {
        self.properties.remove(key);
        self.important.remove(key);
        if matches!(
            key,
            "background-image"
                | "background-svg"
                | "background-gradient"
                | "background-radial-gradient"
                | "background-conic-gradient"
                | "background-layer-slots"
        ) {
            self.properties.remove(BACKGROUND_LAYER_SOURCES);
            self.important.remove(BACKGROUND_LAYER_SOURCES);
        }
    }

    pub fn is_important(&self, key: &str) -> bool {
        self.important.get(key).copied().unwrap_or(false)
    }

    #[allow(dead_code)]
    pub fn merge(&mut self, other: &StyleMap) {
        for (key, value) in &other.properties {
            let is_important = other.is_important(key);
            if self.is_important(key) && !is_important {
                continue;
            }
            self.properties.insert(key.clone(), value.clone());
            self.important.insert(key.clone(), is_important);
            if key == "font"
                && let Some(CssValue::Keyword(font)) = self.properties.get(key)
                && let Some(family) = font_shorthand_family(font)
            {
                self.properties
                    .insert("font-family".to_string(), CssValue::Keyword(family));
                self.important
                    .insert("font-family".to_string(), is_important);
            }
        }
    }
}

fn capture_background_layer_source(key: &str, value: &CssValue) {
    let kind = match key {
        "background-image"
        | "background-svg"
        | "background-gradient"
        | "background-radial-gradient"
        | "background-conic-gradient" => key,
        _ => return,
    };
    let CssValue::Keyword(raw) = value else {
        return;
    };
    BACKGROUND_LAYER_CAPTURE.with(|capture| {
        capture
            .borrow_mut()
            .push((kind.to_string(), raw.to_string()));
    });
}

fn captured_background_layer_sources(slots: &str) -> Option<String> {
    let layer_count = slots
        .split(',')
        .filter(|slot| !slot.trim().is_empty())
        .count();
    if layer_count <= 1 {
        return None;
    }
    BACKGROUND_LAYER_CAPTURE.with(|capture| {
        let capture = capture.borrow();
        if capture.len() < layer_count {
            return None;
        }
        let start = capture.len() - layer_count;
        Some(
            capture[start..]
                .iter()
                .map(|(kind, raw)| format!("{kind}{BACKGROUND_LAYER_FIELD_SEP}{raw}"))
                .collect::<Vec<_>>()
                .join(&BACKGROUND_LAYER_RECORD_SEP.to_string()),
        )
    })
}

fn font_shorthand_family(value: &str) -> Option<String> {
    let mut saw_size = false;
    let mut skip_line_height = false;
    let mut family = Vec::new();

    for token in value.split_whitespace() {
        if !saw_size {
            let size = token.split_once('/').map_or(token, |(size, _)| size);
            if css_font_size_token(size) {
                saw_size = true;
                skip_line_height = token.ends_with('/');
            }
            continue;
        }
        if token == "/" {
            skip_line_height = true;
            continue;
        }
        if skip_line_height {
            skip_line_height = false;
            continue;
        }
        family.push(token);
    }

    let family = family.join(" ");
    (!family.trim().is_empty()).then_some(family)
}

fn css_font_size_token(token: &str) -> bool {
    let token = token.trim();
    token
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_digit() || ch == '.')
        || matches!(
            token.to_ascii_lowercase().as_str(),
            "xx-small"
                | "x-small"
                | "small"
                | "medium"
                | "large"
                | "x-large"
                | "xx-large"
                | "xxx-large"
                | "smaller"
                | "larger"
        )
}

/// Pseudo-element type for `::before`, `::after`, and `::marker`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PseudoElement {
    Before,
    After,
    /// The list-item marker box (`::marker`). Only a limited set of properties
    /// apply (color, font, content); see `compute_pseudo_element_style`.
    Marker,
    /// The first formatted line of a block container (`::first-line`).
    /// Restyles the runs that land on the first wrapped line. Per
    /// css-pseudo-4 §2.1 only a restricted property subset applies.
    FirstLine,
    /// The first typographic letter unit (plus associated leading punctuation)
    /// of the first formatted line (`::first-letter`). Per css-pseudo-4 §2.2
    /// a restricted property subset applies; enables drop-cap styling.
    FirstLetter,
}

/// A CSS rule: a selector and its declarations.
#[derive(Debug, Clone)]
pub struct CssRule {
    pub selector: String,
    pub declarations: StyleMap,
    /// If this rule targets a `::before` or `::after` pseudo-element.
    pub pseudo_element: Option<PseudoElement>,
}

struct CounterStyleRuleRegistry {
    rules: Vec<CssRule>,
    collecting: bool,
}

thread_local! {
    static COUNTER_STYLE_RULES: RefCell<CounterStyleRuleRegistry> = const {
        RefCell::new(CounterStyleRuleRegistry { rules: Vec::new(), collecting: false })
    };
}

impl CssRule {
    pub(crate) fn begin_counter_style_stylesheet_scan() {
        COUNTER_STYLE_RULES.with(|registry| {
            let mut registry = registry.borrow_mut();
            if !registry.collecting {
                registry.rules.clear();
                registry.collecting = true;
            }
        });
    }

    pub(crate) fn register_counter_style_rules(rules: impl IntoIterator<Item = CssRule>) {
        COUNTER_STYLE_RULES.with(|registry| {
            registry.borrow_mut().rules.extend(rules);
        });
    }

    pub(crate) fn finish_counter_style_stylesheet_scan() {
        COUNTER_STYLE_RULES.with(|registry| {
            registry.borrow_mut().collecting = false;
        });
    }

    pub(crate) fn registered_counter_style_declarations(name: &str) -> Option<StyleMap> {
        let selector = format!("@counter-style {}", name.to_ascii_lowercase());
        COUNTER_STYLE_RULES.with(|registry| {
            registry
                .borrow()
                .rules
                .iter()
                .rev()
                .find(|rule| rule.selector.trim().to_ascii_lowercase() == selector)
                .map(|rule| rule.declarations.clone())
        })
    }
}

/// A source entry from an `@font-face src:` descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontFaceSource {
    /// `url(...)` source.
    Url(String),
    /// `local(...)` source.
    Local(String),
}

/// A parsed `unicode-range` interval from an `@font-face` descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnicodeRange {
    /// Inclusive first Unicode codepoint.
    pub start: u32,
    /// Inclusive last Unicode codepoint.
    pub end: u32,
}

impl UnicodeRange {
    /// Whether this interval contains `ch`.
    pub const fn contains(self, ch: char) -> bool {
        let codepoint = ch as u32;
        self.start <= codepoint && codepoint <= self.end
    }
}

/// A parsed `@font-face` rule with font-family name, source list, and descriptors.
#[derive(Debug, Clone)]
pub struct FontFaceRule {
    /// The font-family name declared in the rule.
    pub font_family: String,
    /// The ordered source list from the `src:` descriptor.
    pub sources: Vec<FontFaceSource>,
    /// Whether the face descriptor declares a bold weight.
    pub font_weight_bold: bool,
    /// Whether the face descriptor declares italic/oblique style.
    pub font_style_italic: bool,
    /// CSS Fonts `size-adjust` descriptor as a multiplier (`normal` = 1.0).
    pub size_adjust: f32,
    /// The `unicode-range` intervals. Empty means the default full Unicode range.
    pub unicode_ranges: Vec<UnicodeRange>,
}

impl FontFaceRule {
    /// Iterate source entries as `(is_local, value)`, preserving source-list order.
    pub fn source_entries(&self) -> impl Iterator<Item = (bool, &str)> {
        self.sources.iter().map(|source| match source {
            FontFaceSource::Local(name) => (true, name.as_str()),
            FontFaceSource::Url(path) => (false, path.as_str()),
        })
    }

    /// Iterate `local(...)` source names.
    pub fn local_source_names(&self) -> impl Iterator<Item = &str> {
        self.source_entries()
            .filter_map(|(is_local, value)| is_local.then_some(value))
    }
}

/// A parsed `@import` rule with the local file path.
#[derive(Debug, Clone)]
pub struct ImportRule {
    /// The local file path to import.
    pub path: String,
}

/// The selector of an `@page` rule — the text between `@page` and `{`
/// (CSS Paged Media 3 §3 "Page selectors and the page context").
///
/// `@page { }` (no selector) is [`PageSelector::None`] and applies to every
/// page; the pseudo-class / named variants override per page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PageSelector {
    /// `@page { }` — the default rule, applies to all pages.
    #[default]
    None,
    /// `@page :first { }` — the first page of the document.
    First,
    /// `@page :left { }` — verso (left) pages.
    Left,
    /// `@page :right { }` — recto (right) pages.
    Right,
    /// `@page :blank { }` — intentionally-blank pages.
    Blank,
    /// `@page <name> { }` — a named page targeted by the `page` property.
    Named(String),
}

/// The position of a page-margin box inside the `@page` context
/// (CSS Paged Media 3 §5 "Page-margin boxes"). The 16 boxes are arranged
/// around the page border: a top and bottom row (corners + left/center/right),
/// and left/right side columns (top/middle/bottom).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarginBoxPosition {
    TopLeftCorner,
    TopLeft,
    TopCenter,
    TopRight,
    TopRightCorner,
    BottomLeftCorner,
    BottomLeft,
    BottomCenter,
    BottomRight,
    BottomRightCorner,
    LeftTop,
    LeftMiddle,
    LeftBottom,
    RightTop,
    RightMiddle,
    RightBottom,
}

/// Which horizontal band (top vs bottom margin area) a margin box renders in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarginBoxBand {
    Top,
    Bottom,
}

/// Horizontal alignment of a margin box within its band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarginBoxAlign {
    Left,
    Center,
    Right,
}

impl MarginBoxPosition {
    /// Map an `@<ident>` margin-box at-rule name to its position.
    pub fn from_at_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "top-left-corner" => Some(Self::TopLeftCorner),
            "top-left" => Some(Self::TopLeft),
            "top-center" => Some(Self::TopCenter),
            "top-right" => Some(Self::TopRight),
            "top-right-corner" => Some(Self::TopRightCorner),
            "bottom-left-corner" => Some(Self::BottomLeftCorner),
            "bottom-left" => Some(Self::BottomLeft),
            "bottom-center" => Some(Self::BottomCenter),
            "bottom-right" => Some(Self::BottomRight),
            "bottom-right-corner" => Some(Self::BottomRightCorner),
            "left-top" => Some(Self::LeftTop),
            "left-middle" => Some(Self::LeftMiddle),
            "left-bottom" => Some(Self::LeftBottom),
            "right-top" => Some(Self::RightTop),
            "right-middle" => Some(Self::RightMiddle),
            "right-bottom" => Some(Self::RightBottom),
            _ => None,
        }
    }

    /// The horizontal band (top/bottom margin area) this box paints in, if it
    /// is a top- or bottom-row box. Side boxes (`left-*`/`right-*`) return
    /// `None` and are not rendered as running headers/footers.
    pub fn band(self) -> Option<MarginBoxBand> {
        match self {
            Self::TopLeftCorner
            | Self::TopLeft
            | Self::TopCenter
            | Self::TopRight
            | Self::TopRightCorner => Some(MarginBoxBand::Top),
            Self::BottomLeftCorner
            | Self::BottomLeft
            | Self::BottomCenter
            | Self::BottomRight
            | Self::BottomRightCorner => Some(MarginBoxBand::Bottom),
            _ => None,
        }
    }

    /// The horizontal alignment of this box within its band.
    pub fn align(self) -> MarginBoxAlign {
        match self {
            Self::TopLeftCorner | Self::TopLeft | Self::BottomLeftCorner | Self::BottomLeft => {
                MarginBoxAlign::Left
            }
            Self::TopCenter | Self::BottomCenter => MarginBoxAlign::Center,
            _ => MarginBoxAlign::Right,
        }
    }
}

/// A token in a margin-box `content` value (CSS Paged Media 3 §5.3). The
/// `content` of a running header/footer is a concatenation of string literals
/// and the page counters `counter(page)` / `counter(pages)`, e.g.
/// `content: "Page " counter(page) " of " counter(pages)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarginContentToken {
    /// A quoted string literal.
    Literal(String),
    /// `counter(page)` — resolved to the 1-based current page index.
    PageNumber,
    /// `counter(pages)` — resolved to the total page count.
    PageCount,
    /// `element(name)` — resolved to a captured `position: running(name)` box.
    Element(String),
    /// `string(name, page-policy)` — resolved from `string-set` captures.
    NamedString(String, Option<String>),
}

/// A parsed page-margin box (CSS Paged Media 3 §5): its position and the
/// resolved `content` token list rendered on every page.
#[derive(Debug, Clone)]
pub struct MarginBox {
    /// The box position within the page margin area.
    pub position: MarginBoxPosition,
    /// The page selector that owns this margin box.
    pub selector: PageSelector,
    pub page_counter_reset: Option<i32>,
    pub page_counter_increment: Option<i32>,
    pub color: Option<Color>,
    pub background_color: Option<Color>,
    pub font_size: Option<f32>,
    /// The `content` value parsed into a token list (literals + counters).
    pub content: Vec<MarginContentToken>,
}

/// Declarations from the GCPM `@footnote` area rule that affect pagination and
/// painting of the footnote area.
#[derive(Debug, Clone, Copy, Default)]
pub struct FootnoteAreaStyle {
    pub max_height: Option<f32>,
    pub padding_top: f32,
    pub border_top_width: f32,
    pub border_top_color: Option<Color>,
}

/// A parsed `@page` rule with page size and margin overrides.
#[derive(Debug, Clone, Default)]
pub struct PageRule {
    /// The page selector (`:first`/`:left`/`:right`/`:blank`/name) classified
    /// from the text between `@page` and `{`. [`PageSelector::None`] for an
    /// unselected `@page { }` rule that applies to every page.
    pub selector: PageSelector,
    /// Page width in points (if specified).
    pub width: Option<f32>,
    /// Page height in points (if specified).
    pub height: Option<f32>,
    /// Top margin in points (if specified).
    pub margin_top: Option<f32>,
    /// Right margin in points (if specified).
    pub margin_right: Option<f32>,
    /// Bottom margin in points (if specified).
    pub margin_bottom: Option<f32>,
    /// Left margin in points (if specified).
    pub margin_left: Option<f32>,
    /// `counter-reset: page <n>` in the page context.
    pub page_counter_reset: Option<i32>,
    /// `counter-increment: page <n>` in the page context.
    pub page_counter_increment: Option<i32>,
    /// `@footnote { max-height: ... }` from CSS GCPM.
    pub footnote_max_height: Option<f32>,
    /// `@footnote { padding-top: ... }` from CSS GCPM.
    pub footnote_padding_top: Option<f32>,
    /// `@footnote { border-top-width: ... }` from CSS GCPM.
    pub footnote_border_top_width: Option<f32>,
    /// `@footnote { border-top-color: ... }` from CSS GCPM.
    pub footnote_border_top_color: Option<Color>,
    /// The raw declaration block of the `@page` rule (the text between `{` and
    /// `}`), retained verbatim so a CSS-aware parser can later extract the
    /// `@page` background (CSS Paged Media 3 §3.1 bleed-area background). Kept
    /// raw — rather than pre-split on `;` like size/margin — so data-URI values
    /// containing `;` (e.g. `;base64,`) survive intact.
    pub raw_declarations: Option<String>,
    /// Parsed page-margin boxes (CSS Paged Media 3 §5) — the `@top-center`,
    /// `@bottom-center`, etc. at-rules nested in this `@page` block, used for
    /// running headers/footers and page numbering.
    pub margin_boxes: Vec<MarginBox>,
}
