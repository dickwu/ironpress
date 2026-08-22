use ironpress_core::HtmlConverter;

use crate::handles::{IronpressBuffer, IronpressError, OutputSlot, boundary};
use crate::input::IronpressBytes;
use crate::status::IronpressStatus;

/// Convert UTF-8 HTML with a default one-shot converter.
///
/// # Safety
///
/// Input bytes and output pointers must follow `ABI.md`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_html_to_pdf(
    html: IronpressBytes,
    out_pdf: *mut *mut IronpressBuffer,
    out_error: *mut *mut IronpressError,
) -> IronpressStatus {
    // SAFETY: Input bytes and output slots are validated before use.
    unsafe {
        boundary(out_error, || {
            let output = OutputSlot::required(out_pdf, "PDF")?;
            let html = html.parse_text("HTML")?;
            output.write(IronpressBuffer::new(HtmlConverter::new().convert(html)?));
            Ok(())
        })
    }
}

/// Convert UTF-8 Markdown with a default one-shot converter.
///
/// # Safety
///
/// Input bytes and output pointers must follow `ABI.md`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_markdown_to_pdf(
    markdown: IronpressBytes,
    out_pdf: *mut *mut IronpressBuffer,
    out_error: *mut *mut IronpressError,
) -> IronpressStatus {
    // SAFETY: Input bytes and output slots are validated before use.
    unsafe {
        boundary(out_error, || {
            let output = OutputSlot::required(out_pdf, "PDF")?;
            let markdown = markdown.parse_text("Markdown")?;
            output.write(IronpressBuffer::new(
                HtmlConverter::new().convert_markdown(markdown)?,
            ));
            Ok(())
        })
    }
}
