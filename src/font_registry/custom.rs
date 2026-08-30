use std::collections::HashMap;

use crate::parser::ttf::{TtfFont, parse_ttf};

/// A normalized CSS family name used to register one caller-provided face.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CustomFontFamily(String);

impl CustomFontFamily {
    fn from_css_name(name: &str) -> Self {
        Self(name.to_ascii_lowercase())
    }

    fn into_string(self) -> String {
        self.0
    }
}

/// Parsed caller-provided fonts owned by one converter.
///
/// Parsing happens at registration, so every stored entry is ready for layout
/// and repeated conversions never need an ambient cache.
#[derive(Debug, Clone, Default)]
pub(crate) struct CustomFontCatalog {
    fonts: HashMap<CustomFontFamily, TtfFont>,
}

impl CustomFontCatalog {
    /// Replace one family. Invalid bytes remove the previous registration,
    /// preserving the existing fallback behavior at conversion time.
    pub(crate) fn replace(&mut self, name: &str, data: Vec<u8>) {
        let family = CustomFontFamily::from_css_name(name);
        self.fonts.remove(&family);
        if let Ok(font) = parse_ttf(data) {
            self.fonts.insert(family, font);
        }
    }

    /// Copy the converter-owned faces into one conversion's mutable registry.
    pub(crate) fn install_into(&self, fonts: &mut HashMap<String, TtfFont>) {
        fonts.extend(
            self.fonts
                .iter()
                .map(|(family, font)| (family.clone().into_string(), font.clone())),
        );
    }

    #[cfg(test)]
    pub(crate) fn contains_key(&self, name: &str) -> bool {
        self.fonts
            .contains_key(&CustomFontFamily::from_css_name(name))
    }
}
