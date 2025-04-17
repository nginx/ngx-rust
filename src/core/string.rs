use core::str::{self, Utf8Error};

#[cfg(all(not(feature = "std"), feature = "alloc"))]
use alloc::{borrow::Cow, string::String};
#[cfg(feature = "std")]
use std::{borrow::Cow, string::String};

use crate::ffi::{ngx_str_t, u_char};

/// Static string initializer for [`ngx_str_t`].
///
/// The resulting byte string is always nul-terminated (just like a C string).
///
/// [`ngx_str_t`]: https://nginx.org/en/docs/dev/development_guide.html#string_overview
#[macro_export]
macro_rules! ngx_string {
    ($s:expr) => {{
        $crate::ffi::ngx_str_t {
            len: $s.len() as _,
            data: concat!($s, "\0").as_ptr() as *mut u8,
        }
    }};
}

/// Representation of a borrowed [Nginx string].
///
/// [Nginx string]: https://nginx.org/en/docs/dev/development_guide.html#string_overview
pub struct NgxStr([u_char]);

impl NgxStr {
    /// Create an [`NgxStr`] from an [`ngx_str_t`].
    ///
    /// [`ngx_str_t`]: https://nginx.org/en/docs/dev/development_guide.html#string_overview
    ///
    /// # Safety
    ///
    /// The caller has provided a valid `ngx_str_t` with a `data` pointer that points
    /// to range of bytes of at least `len` bytes, whose content remains valid and doesn't
    /// change for the lifetime of the returned `NgxStr`.
    pub unsafe fn from_ngx_str<'a>(str: ngx_str_t) -> &'a NgxStr {
        str.as_bytes().into()
    }

    /// Access the [`NgxStr`] as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Yields a `&str` slice if the [`NgxStr`] contains valid UTF-8.
    pub fn to_str(&self) -> Result<&str, Utf8Error> {
        str::from_utf8(self.as_bytes())
    }

    /// Converts an [`NgxStr`] into a [`Cow<str>`], replacing invalid UTF-8 sequences.
    ///
    /// See [`String::from_utf8_lossy`].
    #[cfg(feature = "alloc")]
    pub fn to_string_lossy(&self) -> Cow<str> {
        String::from_utf8_lossy(self.as_bytes())
    }

    /// Returns `true` if the [`NgxStr`] is empty, otherwise `false`.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&[u8]> for &NgxStr {
    fn from(bytes: &[u8]) -> Self {
        // SAFETY: An `NgxStr` is identical to a `[u8]` slice, given `u_char` is an alias for `u8`.
        unsafe { &*(bytes as *const [u8] as *const NgxStr) }
    }
}

impl From<&str> for &NgxStr {
    fn from(s: &str) -> Self {
        s.as_bytes().into()
    }
}

impl AsRef<[u8]> for NgxStr {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl Default for &NgxStr {
    fn default() -> Self {
        // SAFETY: The empty `ngx_str_t` is always a valid Nginx string.
        unsafe { NgxStr::from_ngx_str(ngx_str_t::default()) }
    }
}
