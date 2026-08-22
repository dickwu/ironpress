use std::ffi::c_char;

use ironpress_core::{HtmlConverter, Margin, PageSize};

use crate::handles::{
    IronpressBuffer, IronpressConverter, IronpressError, OutputSlot, boundary, free_owned,
};
use crate::input::IronpressBytes;
use crate::status::{
    Failure, IRONPRESS_STATUS_INVALID_ARGUMENT, IRONPRESS_STATUS_INVALID_ENUM, IronpressStatus,
};

/// ABI generation implemented by this library.
pub const IRONPRESS_ABI_VERSION: u32 = 1;
/// Named ISO A4 page size.
pub const IRONPRESS_PAGE_SIZE_A4: u32 = 1;
/// Named US Letter page size.
pub const IRONPRESS_PAGE_SIZE_LETTER: u32 = 2;
/// Named US Legal page size.
pub const IRONPRESS_PAGE_SIZE_LEGAL: u32 = 3;

/// A named page size parsed from its stable ABI discriminant.
struct NamedPageSize(PageSize);

impl NamedPageSize {
    /// Parse one public page-size constant.
    fn parse(value: u32) -> Result<Self, Failure> {
        let page_size = match value {
            IRONPRESS_PAGE_SIZE_A4 => PageSize::A4,
            IRONPRESS_PAGE_SIZE_LETTER => PageSize::LETTER,
            IRONPRESS_PAGE_SIZE_LEGAL => PageSize::LEGAL,
            _ => {
                return Err(Failure::new(
                    IRONPRESS_STATUS_INVALID_ENUM,
                    format!("unknown page-size value {value}"),
                ));
            }
        };
        Ok(Self(page_size))
    }
}

/// Finite custom page dimensions accepted by the C contract.
struct PageDimensions(PageSize);

impl PageDimensions {
    /// Parse positive finite dimensions into a core page size.
    fn parse(width: f32, height: f32) -> Result<Self, Failure> {
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(Failure::new(
                IRONPRESS_STATUS_INVALID_ARGUMENT,
                "page width and height must be finite positive points",
            ));
        }
        Ok(Self(PageSize::new(width, height)))
    }
}

/// Finite physical margins accepted by the C contract.
struct PageMargins(Margin);

impl PageMargins {
    /// Parse four finite values in CSS clockwise order.
    fn parse(top: f32, right: f32, bottom: f32, left: f32) -> Result<Self, Failure> {
        if [top, right, bottom, left]
            .into_iter()
            .any(|value| !value.is_finite())
        {
            return Err(Failure::new(
                IRONPRESS_STATUS_INVALID_ARGUMENT,
                "page margins must contain finite point values",
            ));
        }
        Ok(Self(Margin::new(top, right, bottom, left)))
    }
}

/// Return the stable C ABI generation.
#[unsafe(no_mangle)]
pub const extern "C" fn ironpress_abi_version() -> u32 {
    IRONPRESS_ABI_VERSION
}

/// Return the null-terminated Ironpress package version.
#[unsafe(no_mangle)]
pub extern "C" fn ironpress_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast()
}

/// Allocate a default converter and transfer it into an empty output slot.
///
/// # Safety
///
/// Output pointers must follow the ownership and slot rules in `ABI.md`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_converter_new(
    out_converter: *mut *mut IronpressConverter,
    out_error: *mut *mut IronpressError,
) -> IronpressStatus {
    // SAFETY: Raw output slots are validated before they are written.
    unsafe {
        boundary(out_error, || {
            let output = OutputSlot::required(out_converter, "converter")?;
            output.write(IronpressConverter {
                converter: HtmlConverter::new(),
            });
            Ok(())
        })
    }
}

/// Release a converter and clear its owning handle.
///
/// # Safety
///
/// `converter` must identify the unique handle returned by this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_converter_free(
    converter: *mut *mut IronpressConverter,
) -> IronpressStatus {
    // SAFETY: The caller accepts the ownership contract above.
    unsafe { free_owned(converter) }
}

/// Configure one named page size.
///
/// # Safety
///
/// Handles and output pointers must follow `ABI.md`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_converter_set_page_size(
    converter: *mut IronpressConverter,
    page_size: u32,
    out_error: *mut *mut IronpressError,
) -> IronpressStatus {
    // SAFETY: Raw handles and output slots are validated before use.
    unsafe {
        boundary(out_error, || {
            let converter = IronpressConverter::parse_mut(converter)?;
            let page_size = NamedPageSize::parse(page_size)?.0;
            converter.update(|current| current.page_size(page_size));
            Ok(())
        })
    }
}

/// Configure a custom page size in points.
///
/// # Safety
///
/// Handles and output pointers must follow `ABI.md`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_converter_set_page_size_custom(
    converter: *mut IronpressConverter,
    width: f32,
    height: f32,
    out_error: *mut *mut IronpressError,
) -> IronpressStatus {
    // SAFETY: Raw handles and output slots are validated before use.
    unsafe {
        boundary(out_error, || {
            let converter = IronpressConverter::parse_mut(converter)?;
            let page_size = PageDimensions::parse(width, height)?.0;
            converter.update(|current| current.page_size(page_size));
            Ok(())
        })
    }
}

/// Configure physical page margins in top, right, bottom, left order.
///
/// # Safety
///
/// Handles and output pointers must follow `ABI.md`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_converter_set_margins(
    converter: *mut IronpressConverter,
    top: f32,
    right: f32,
    bottom: f32,
    left: f32,
    out_error: *mut *mut IronpressError,
) -> IronpressStatus {
    // SAFETY: Raw handles and output slots are validated before use.
    unsafe {
        boundary(out_error, || {
            let converter = IronpressConverter::parse_mut(converter)?;
            let margins = PageMargins::parse(top, right, bottom, left)?.0;
            converter.update(|current| current.margin(margins));
            Ok(())
        })
    }
}

/// Configure the plain-text page header.
///
/// # Safety
///
/// Handles, input bytes, and output pointers must follow `ABI.md`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_converter_set_header(
    converter: *mut IronpressConverter,
    header: IronpressBytes,
    out_error: *mut *mut IronpressError,
) -> IronpressStatus {
    // SAFETY: Raw handles, input bytes, and output slots are validated before use.
    unsafe {
        boundary(out_error, || {
            let converter = IronpressConverter::parse_mut(converter)?;
            let header = header.parse_text("header")?.to_owned();
            converter.update(|current| current.header(header));
            Ok(())
        })
    }
}

/// Configure the plain-text page footer.
///
/// # Safety
///
/// Handles, input bytes, and output pointers must follow `ABI.md`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_converter_set_footer(
    converter: *mut IronpressConverter,
    footer: IronpressBytes,
    out_error: *mut *mut IronpressError,
) -> IronpressStatus {
    // SAFETY: Raw handles, input bytes, and output slots are validated before use.
    unsafe {
        boundary(out_error, || {
            let converter = IronpressConverter::parse_mut(converter)?;
            let footer = footer.parse_text("footer")?.to_owned();
            converter.update(|current| current.footer(footer));
            Ok(())
        })
    }
}

/// Convert UTF-8 HTML through a configured converter.
///
/// # Safety
///
/// Handles, input bytes, and output pointers must follow `ABI.md`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_converter_convert_html(
    converter: *mut IronpressConverter,
    html: IronpressBytes,
    out_pdf: *mut *mut IronpressBuffer,
    out_error: *mut *mut IronpressError,
) -> IronpressStatus {
    // SAFETY: Raw handles, input bytes, and output slots are validated before use.
    unsafe {
        boundary(out_error, || {
            let output = OutputSlot::required(out_pdf, "PDF")?;
            let converter = IronpressConverter::parse_mut(converter)?;
            let html = html.parse_text("HTML")?;
            output.write(IronpressBuffer::new(converter.converter.convert(html)?));
            Ok(())
        })
    }
}

/// Convert UTF-8 Markdown through a configured converter.
///
/// # Safety
///
/// Handles, input bytes, and output pointers must follow `ABI.md`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_converter_convert_markdown(
    converter: *mut IronpressConverter,
    markdown: IronpressBytes,
    out_pdf: *mut *mut IronpressBuffer,
    out_error: *mut *mut IronpressError,
) -> IronpressStatus {
    // SAFETY: Raw handles, input bytes, and output slots are validated before use.
    unsafe {
        boundary(out_error, || {
            let output = OutputSlot::required(out_pdf, "PDF")?;
            let converter = IronpressConverter::parse_mut(converter)?;
            let markdown = markdown.parse_text("Markdown")?;
            output.write(IronpressBuffer::new(
                converter.converter.convert_markdown(markdown)?,
            ));
            Ok(())
        })
    }
}
