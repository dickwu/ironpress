use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use crate::status::{
    Failure, IRONPRESS_STATUS_INTERNAL, IRONPRESS_STATUS_INVALID_ARGUMENT,
    IRONPRESS_STATUS_INVALID_HANDLE, IRONPRESS_STATUS_OK, IRONPRESS_STATUS_OUTPUT_NOT_EMPTY,
    IronpressStatus,
};

/// Opaque owner of one configured Ironpress converter.
pub struct IronpressConverter {
    pub(crate) converter: ironpress_core::HtmlConverter,
}

impl IronpressConverter {
    /// Borrow a live converter handle for one read-only foreign call.
    ///
    /// # Safety
    ///
    /// A non-null pointer must come from `ironpress_converter_new`, remain
    /// alive for the borrow, and not be mutated concurrently.
    pub(crate) unsafe fn parse_ref<'a>(raw: *const Self) -> Result<&'a Self, Failure> {
        if raw.is_null() {
            return Err(Failure::new(
                IRONPRESS_STATUS_INVALID_HANDLE,
                "converter handle is null",
            ));
        }
        // SAFETY: The foreign-call contract above requires a live shared handle.
        Ok(unsafe { &*raw })
    }

    /// Borrow a live converter handle for one foreign call.
    ///
    /// # Safety
    ///
    /// A non-null pointer must come from `ironpress_converter_new`, remain
    /// exclusively accessible for the borrow, and not have been freed.
    pub(crate) unsafe fn parse_mut<'a>(raw: *mut Self) -> Result<&'a mut Self, Failure> {
        if raw.is_null() {
            return Err(Failure::new(
                IRONPRESS_STATUS_INVALID_HANDLE,
                "converter handle is null",
            ));
        }
        // SAFETY: The foreign-call contract above requires a live unique handle.
        Ok(unsafe { &mut *raw })
    }

    /// Replace the immutable builder value while preserving handle identity.
    pub(crate) fn update(
        &mut self,
        configure: impl FnOnce(ironpress_core::HtmlConverter) -> ironpress_core::HtmlConverter,
    ) {
        self.converter = configure(std::mem::take(&mut self.converter));
    }
}

/// Opaque owner of PDF bytes allocated by Ironpress.
pub struct IronpressBuffer {
    bytes: Vec<u8>,
}

impl IronpressBuffer {
    /// Own one completed PDF allocation.
    pub(crate) fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
}

/// Opaque owner of one categorized failure and its UTF-8 diagnostic.
pub struct IronpressError {
    status: IronpressStatus,
    message: String,
}

impl From<Failure> for IronpressError {
    fn from(failure: Failure) -> Self {
        let status = failure.status();
        Self {
            status,
            message: failure.into_message(),
        }
    }
}

/// A caller-owned pointer slot proven empty before an allocation is installed.
pub(crate) struct OutputSlot<'a, T> {
    target: &'a mut *mut T,
}

impl<'a, T> OutputSlot<'a, T> {
    /// Parse a required output slot once at the foreign boundary.
    ///
    /// # Safety
    ///
    /// `raw` must be null or point to a writable, exclusively borrowed `*mut T`.
    pub(crate) unsafe fn required(raw: *mut *mut T, name: &str) -> Result<Self, Failure> {
        if raw.is_null() {
            return Err(Failure::new(
                IRONPRESS_STATUS_INVALID_ARGUMENT,
                format!("{name} output slot is null"),
            ));
        }
        // SAFETY: The foreign-call contract above requires a writable unique slot.
        let target = unsafe { &mut *raw };
        if !target.is_null() {
            return Err(Failure::new(
                IRONPRESS_STATUS_OUTPUT_NOT_EMPTY,
                format!("{name} output slot must be initialized to null"),
            ));
        }
        Ok(Self { target })
    }

    /// Install one allocation and transfer its ownership to the caller.
    pub(crate) fn write(self, value: T) {
        *self.target = Box::into_raw(Box::new(value));
    }
}

/// Execute one operation without allowing Rust unwinding to cross the ABI.
///
/// # Safety
///
/// A non-null error slot must satisfy [`OutputSlot::required`].
pub(crate) unsafe fn boundary(
    out_error: *mut *mut IronpressError,
    operation: impl FnOnce() -> Result<(), Failure>,
) -> IronpressStatus {
    let error_slot = if out_error.is_null() {
        None
    } else {
        // SAFETY: The function inherits the slot contract documented above.
        match unsafe { OutputSlot::required(out_error, "error") } {
            Ok(slot) => Some(slot),
            Err(failure) => return failure.status(),
        }
    };

    let failure = match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(())) => return IRONPRESS_STATUS_OK,
        Ok(Err(failure)) => failure,
        Err(_) => Failure::new(
            IRONPRESS_STATUS_INTERNAL,
            "Ironpress caught an unexpected internal panic",
        ),
    };
    let status = failure.status();
    if let Some(slot) = error_slot {
        slot.write(failure.into());
    }
    status
}

/// Release an opaque owned handle and clear the caller's slot.
///
/// # Safety
///
/// `raw` must be null or point to a writable handle slot. A non-null handle in
/// that slot must be the unique owner returned by Ironpress.
pub(crate) unsafe fn free_owned<T>(raw: *mut *mut T) -> IronpressStatus {
    if raw.is_null() {
        return IRONPRESS_STATUS_INVALID_ARGUMENT;
    }
    let released = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: The foreign-call contract above requires a writable unique slot.
        let target = unsafe { &mut *raw };
        let owned = std::mem::replace(target, ptr::null_mut());
        if !owned.is_null() {
            // SAFETY: A non-null slot is the unique Box owner returned by Ironpress.
            drop(unsafe { Box::from_raw(owned) });
        }
    }));
    match released {
        Ok(()) => IRONPRESS_STATUS_OK,
        Err(_) => IRONPRESS_STATUS_INTERNAL,
    }
}

/// Return a borrowed pointer to the first PDF byte, or null for an invalid handle.
#[unsafe(no_mangle)]
pub extern "C" fn ironpress_buffer_data(buffer: *const IronpressBuffer) -> *const u8 {
    // SAFETY: `as_ref` only borrows a caller-provided handle when it is non-null.
    unsafe { buffer.as_ref() }.map_or(ptr::null(), |buffer| buffer.bytes.as_ptr())
}

/// Return the PDF byte length, or zero for an invalid handle.
#[unsafe(no_mangle)]
pub extern "C" fn ironpress_buffer_len(buffer: *const IronpressBuffer) -> usize {
    // SAFETY: `as_ref` only borrows a caller-provided handle when it is non-null.
    unsafe { buffer.as_ref() }.map_or(0, |buffer| buffer.bytes.len())
}

/// Release a PDF buffer and clear its owning handle.
///
/// # Safety
///
/// `buffer` must satisfy the ownership contract in [`free_owned`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_buffer_free(
    buffer: *mut *mut IronpressBuffer,
) -> IronpressStatus {
    // SAFETY: The caller accepts the ownership contract above.
    unsafe { free_owned(buffer) }
}

/// Return the status stored in an error handle.
#[unsafe(no_mangle)]
pub extern "C" fn ironpress_error_status(error: *const IronpressError) -> IronpressStatus {
    // SAFETY: `as_ref` only borrows a caller-provided handle when it is non-null.
    unsafe { error.as_ref() }.map_or(IRONPRESS_STATUS_INVALID_HANDLE, |error| error.status)
}

/// Return a borrowed pointer to the first error-message byte.
#[unsafe(no_mangle)]
pub extern "C" fn ironpress_error_message_data(error: *const IronpressError) -> *const u8 {
    // SAFETY: `as_ref` only borrows a caller-provided handle when it is non-null.
    unsafe { error.as_ref() }.map_or(ptr::null(), |error| error.message.as_ptr())
}

/// Return the UTF-8 error-message length, or zero for an invalid handle.
#[unsafe(no_mangle)]
pub extern "C" fn ironpress_error_message_len(error: *const IronpressError) -> usize {
    // SAFETY: `as_ref` only borrows a caller-provided handle when it is non-null.
    unsafe { error.as_ref() }.map_or(0, |error| error.message.len())
}

/// Release an error and clear its owning handle.
///
/// # Safety
///
/// `error` must satisfy the ownership contract in [`free_owned`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ironpress_error_free(error: *mut *mut IronpressError) -> IronpressStatus {
    // SAFETY: The caller accepts the ownership contract above.
    unsafe { free_owned(error) }
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use super::*;

    #[test]
    fn boundary_turns_a_panic_into_an_owned_internal_error() {
        let mut error = ptr::null_mut();

        // SAFETY: The test provides one initialized writable error slot.
        let status = unsafe {
            boundary(&mut error, || -> Result<(), Failure> {
                panic!("foreign boundary regression")
            })
        };

        assert_eq!(status, IRONPRESS_STATUS_INTERNAL);
        assert!(!error.is_null());
        assert_eq!(ironpress_error_status(error), IRONPRESS_STATUS_INTERNAL);
        assert!(ironpress_error_message_len(error) > 0);
        // SAFETY: `error` is the unique owner returned by `boundary`.
        assert_eq!(
            unsafe { ironpress_error_free(&mut error) },
            IRONPRESS_STATUS_OK
        );
        assert!(error.is_null());
    }

    #[test]
    fn opaque_owners_can_move_between_threads_while_idle() {
        fn assert_send<T: Send>() {}

        assert_send::<IronpressConverter>();
        assert_send::<IronpressBuffer>();
        assert_send::<IronpressError>();
    }
}
