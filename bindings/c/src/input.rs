use std::slice;

use crate::status::{Failure, IRONPRESS_STATUS_INVALID_ARGUMENT, IRONPRESS_STATUS_INVALID_UTF8};

/// Borrowed bytes supplied by a foreign caller.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IronpressBytes {
    /// First byte, or null when `len` is zero.
    pub data: *const u8,
    /// Number of readable bytes starting at `data`.
    pub len: usize,
}

impl IronpressBytes {
    /// Parse the raw view into bytes valid for the duration of one ABI call.
    ///
    /// # Safety
    ///
    /// A non-null `data` must identify `len` readable bytes that remain alive
    /// and immutable for the returned borrow.
    pub(crate) unsafe fn parse<'a>(self, name: &str) -> Result<&'a [u8], Failure> {
        if self.len == 0 {
            return Ok(&[]);
        }
        if self.data.is_null() {
            return Err(Failure::new(
                IRONPRESS_STATUS_INVALID_ARGUMENT,
                format!("{name} has a null pointer with a non-zero length"),
            ));
        }
        // SAFETY: The foreign-call contract above requires a live readable range.
        Ok(unsafe { slice::from_raw_parts(self.data, self.len) })
    }

    /// Parse the raw view as UTF-8 at the foreign boundary.
    ///
    /// # Safety
    ///
    /// The pointer must satisfy [`Self::parse`].
    pub(crate) unsafe fn parse_text<'a>(self, name: &str) -> Result<&'a str, Failure> {
        // SAFETY: This method preserves the caller contract of `parse`.
        let bytes = unsafe { self.parse(name)? };
        std::str::from_utf8(bytes).map_err(|error| {
            Failure::new(
                IRONPRESS_STATUS_INVALID_UTF8,
                format!("{name} must be UTF-8: {error}"),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use super::*;
    use crate::status::{IRONPRESS_STATUS_INVALID_ARGUMENT, IRONPRESS_STATUS_INVALID_UTF8};

    #[test]
    fn null_pointer_represents_only_an_empty_input() {
        let empty = IronpressBytes {
            data: ptr::null(),
            len: 0,
        };
        let missing = IronpressBytes {
            data: ptr::null(),
            len: 1,
        };

        // SAFETY: The empty range never dereferences its null pointer.
        assert_eq!(unsafe { empty.parse("input") }.expect("empty input"), &[]);
        // SAFETY: The invalid range is rejected before its null pointer is read.
        let failure = unsafe { missing.parse("input") }.expect_err("missing range");
        assert_eq!(failure.status(), IRONPRESS_STATUS_INVALID_ARGUMENT);
    }

    #[test]
    fn text_is_parsed_as_utf8_at_the_boundary() {
        let invalid = [0xff];
        let input = IronpressBytes {
            data: invalid.as_ptr(),
            len: invalid.len(),
        };

        // SAFETY: `invalid` remains alive and immutable for this call.
        let failure = unsafe { input.parse_text("input") }.expect_err("invalid UTF-8");
        assert_eq!(failure.status(), IRONPRESS_STATUS_INVALID_UTF8);
    }
}
