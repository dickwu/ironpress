use ironpress_core::IronpressError;

/// Machine-readable result returned by every fallible C operation.
pub type IronpressStatus = i32;

/// The operation completed successfully.
pub const IRONPRESS_STATUS_OK: IronpressStatus = 0;
/// A pointer, length, number, or output slot violates the function contract.
pub const IRONPRESS_STATUS_INVALID_ARGUMENT: IronpressStatus = 1;
/// Text input is not valid UTF-8.
pub const IRONPRESS_STATUS_INVALID_UTF8: IronpressStatus = 2;
/// A fixed-width integer does not identify a documented ABI value.
pub const IRONPRESS_STATUS_INVALID_ENUM: IronpressStatus = 3;
/// A required opaque handle is null.
pub const IRONPRESS_STATUS_INVALID_HANDLE: IronpressStatus = 4;
/// An output slot was not initialized to null by the caller.
pub const IRONPRESS_STATUS_OUTPUT_NOT_EMPTY: IronpressStatus = 5;
/// The HTML parser rejected the document.
pub const IRONPRESS_STATUS_PARSE: IronpressStatus = 10;
/// The CSS parser rejected the document.
pub const IRONPRESS_STATUS_CSS: IronpressStatus = 11;
/// The layout engine could not lay out the document.
pub const IRONPRESS_STATUS_LAYOUT: IronpressStatus = 12;
/// The PDF renderer could not produce the document.
pub const IRONPRESS_STATUS_RENDER: IronpressStatus = 13;
/// A font or font pack could not be parsed or embedded.
pub const IRONPRESS_STATUS_FONT: IronpressStatus = 14;
/// A filesystem operation failed.
pub const IRONPRESS_STATUS_IO: IronpressStatus = 15;
/// The security policy rejected the document.
pub const IRONPRESS_STATUS_SECURITY: IronpressStatus = 16;
/// Ironpress caught an unexpected internal panic at the foreign boundary.
pub const IRONPRESS_STATUS_INTERNAL: IronpressStatus = 255;

/// One categorized failure ready to cross the C boundary.
#[derive(Debug)]
pub(crate) struct Failure {
    status: IronpressStatus,
    message: String,
}

impl Failure {
    /// Create a failure from its stable category and diagnostic.
    pub(crate) fn new(status: IronpressStatus, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    /// Return the stable machine-readable category.
    pub(crate) const fn status(&self) -> IronpressStatus {
        self.status
    }

    /// Consume the failure and return its diagnostic.
    pub(crate) fn into_message(self) -> String {
        self.message
    }
}

impl From<IronpressError> for Failure {
    fn from(error: IronpressError) -> Self {
        let status = match &error {
            IronpressError::ParseError(_) => IRONPRESS_STATUS_PARSE,
            IronpressError::CssError(_) => IRONPRESS_STATUS_CSS,
            IronpressError::LayoutError(_) => IRONPRESS_STATUS_LAYOUT,
            IronpressError::RenderError(_) => IRONPRESS_STATUS_RENDER,
            IronpressError::FontError(_) => IRONPRESS_STATUS_FONT,
            IronpressError::IoError(_) => IRONPRESS_STATUS_IO,
            IronpressError::SecurityError(_) => IRONPRESS_STATUS_SECURITY,
        };
        Self::new(status, error.to_string())
    }
}

impl From<ironpress_core::FontPackError> for Failure {
    fn from(error: ironpress_core::FontPackError) -> Self {
        Self::new(IRONPRESS_STATUS_FONT, error.to_string())
    }
}
