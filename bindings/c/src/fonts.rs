use ironpress_core::{FontPack, FontPackKind};

use crate::handles::{IronpressConverter, IronpressError, boundary};
use crate::input::IronpressBytes;
use crate::status::{
    Failure, IRONPRESS_STATUS_INVALID_ARGUMENT, IRONPRESS_STATUS_INVALID_ENUM, IronpressStatus,
};

/// Japanese CJK fallback pack.
pub const IRONPRESS_FONT_PACK_CJK_JAPANESE: u32 = 1;
/// Korean CJK and Hangul fallback pack.
pub const IRONPRESS_FONT_PACK_CJK_KOREAN: u32 = 2;
/// Simplified Chinese CJK fallback pack.
pub const IRONPRESS_FONT_PACK_CJK_SIMPLIFIED_CHINESE: u32 = 3;
/// Traditional Chinese CJK fallback pack.
pub const IRONPRESS_FONT_PACK_CJK_TRADITIONAL_CHINESE: u32 = 4;
/// Monochrome outline emoji fallback pack.
pub const IRONPRESS_FONT_PACK_EMOJI: u32 = 5;

/// A font-pack role parsed from its stable ABI discriminant.
struct FontPackRole(FontPackKind);

impl FontPackRole {
    /// Parse one public font-pack constant.
    fn parse(value: u32) -> Result<Self, Failure> {
        let kind = match value {
            IRONPRESS_FONT_PACK_CJK_JAPANESE => FontPackKind::CjkJapanese,
            IRONPRESS_FONT_PACK_CJK_KOREAN => FontPackKind::CjkKorean,
            IRONPRESS_FONT_PACK_CJK_SIMPLIFIED_CHINESE => FontPackKind::CjkSimplifiedChinese,
            IRONPRESS_FONT_PACK_CJK_TRADITIONAL_CHINESE => FontPackKind::CjkTraditionalChinese,
            IRONPRESS_FONT_PACK_EMOJI => FontPackKind::Emoji,
            _ => {
                return Err(Failure::new(
                    IRONPRESS_STATUS_INVALID_ENUM,
                    format!("unknown font-pack value {value}"),
                ));
            }
        };
        Ok(Self(kind))
    }
}

/// Add or replace one custom TrueType font family.
///
/// # Safety
///
/// Handles, input bytes, and output pointers must follow `ABI.md`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_converter_add_font(
    converter: *mut IronpressConverter,
    family: IronpressBytes,
    font_data: IronpressBytes,
    out_error: *mut *mut IronpressError,
) -> IronpressStatus {
    // SAFETY: Raw handles, input bytes, and output slots are validated before use.
    unsafe {
        boundary(out_error, || {
            let converter = IronpressConverter::parse_mut(converter)?;
            let family = family.parse_text("font family")?;
            if family.is_empty() {
                return Err(Failure::new(
                    IRONPRESS_STATUS_INVALID_ARGUMENT,
                    "font family must not be empty",
                ));
            }
            let font_data = font_data.parse("font data")?;
            if font_data.is_empty() {
                return Err(Failure::new(
                    IRONPRESS_STATUS_INVALID_ARGUMENT,
                    "font data must not be empty",
                ));
            }
            let family = family.to_owned();
            let font_data = font_data.to_vec();
            converter.update(|current| current.add_font(&family, font_data));
            Ok(())
        })
    }
}

/// Parse and install one optional CJK or emoji fallback pack.
///
/// # Safety
///
/// Handles, input bytes, and output pointers must follow `ABI.md`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_converter_add_font_pack(
    converter: *mut IronpressConverter,
    kind: u32,
    font_data: IronpressBytes,
    out_error: *mut *mut IronpressError,
) -> IronpressStatus {
    // SAFETY: Raw handles, input bytes, and output slots are validated before use.
    unsafe {
        boundary(out_error, || {
            let converter = IronpressConverter::parse_mut(converter)?;
            let kind = FontPackRole::parse(kind)?.0;
            let font_data = font_data.parse("font-pack data")?.to_vec();
            let pack = FontPack::parse(kind, font_data)?;
            converter.update(|current| current.add_font_pack(pack));
            Ok(())
        })
    }
}
