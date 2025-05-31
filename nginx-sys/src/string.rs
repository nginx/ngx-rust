use core::fmt;
use core::ptr;
use core::slice;

use crate::bindings::{ngx_pool_t, ngx_str_t};
use crate::detail;

impl ngx_str_t {
    /// Returns the contents of this `ngx_str_t` as a byte slice.
    ///
    /// The returned slice will **not** contain the optional nul terminator that `ngx_str_t.data`
    /// may have.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        if self.is_empty() {
            &[]
        } else {
            // SAFETY: `ngx_str_t` with non-zero len must contain a valid correctly aligned pointer
            unsafe { slice::from_raw_parts(self.data, self.len) }
        }
    }

    /// Returns the contents of this `ngx_str_t` as a mutable byte slice.
    #[inline]
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        if self.is_empty() {
            &mut []
        } else {
            // SAFETY: `ngx_str_t` with non-zero len must contain a valid correctly aligned pointer
            unsafe { slice::from_raw_parts_mut(self.data, self.len) }
        }
    }

    /// Returns `true` if the string has a length of 0.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Convert the nginx string to a string slice (`&str`).
    ///
    /// # Panics
    /// This function panics if the `ngx_str_t` is not valid UTF-8.
    ///
    /// # Returns
    /// A string slice (`&str`) representing the nginx string.
    pub fn to_str(&self) -> &str {
        core::str::from_utf8(self.as_bytes()).unwrap()
    }

    /// Creates an empty `ngx_str_t` instance.
    ///
    /// This method replaces the `ngx_null_string` C macro.
    pub const fn empty() -> Self {
        ngx_str_t {
            len: 0,
            data: ptr::null_mut(),
        }
    }

    /// Create an `ngx_str_t` instance from a byte slice.
    ///
    /// # Safety
    ///
    /// The caller must provide a valid pointer to a memory pool.
    pub unsafe fn from_bytes(pool: *mut ngx_pool_t, src: &[u8]) -> Option<Self> {
        detail::bytes_to_uchar(pool, src).map(|data| Self {
            data,
            len: src.len(),
        })
    }

    /// Create an `ngx_str_t` instance from a string slice (`&str`).
    ///
    /// # Arguments
    ///
    /// * `pool` - A pointer to the nginx memory pool (`ngx_pool_t`).
    /// * `data` - The string slice from which to create the nginx string.
    ///
    /// # Safety
    /// This function is marked as unsafe because it accepts a raw pointer argument. There is no
    /// way to know if `pool` is pointing to valid memory. The caller must provide a valid pool to
    /// avoid indeterminate behavior.
    ///
    /// # Returns
    /// An `ngx_str_t` instance representing the given string slice.
    pub unsafe fn from_str(pool: *mut ngx_pool_t, data: &str) -> Self {
        ngx_str_t {
            data: detail::str_to_uchar(pool, data),
            len: data.len(),
        }
    }
}

impl AsRef<[u8]> for ngx_str_t {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl AsMut<[u8]> for ngx_str_t {
    fn as_mut(&mut self) -> &mut [u8] {
        self.as_bytes_mut()
    }
}

impl Default for ngx_str_t {
    fn default() -> Self {
        Self::empty()
    }
}

impl From<ngx_str_t> for &[u8] {
    fn from(s: ngx_str_t) -> Self {
        if s.len == 0 || s.data.is_null() {
            return Default::default();
        }
        unsafe { slice::from_raw_parts(s.data, s.len) }
    }
}

impl fmt::Display for ngx_str_t {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        detail::display_bytes(f, self.as_bytes())
    }
}

impl PartialEq for ngx_str_t {
    fn eq(&self, other: &Self) -> bool {
        PartialEq::eq(self.as_bytes(), other.as_bytes())
    }
}

impl Eq for ngx_str_t {}

impl PartialOrd<Self> for ngx_str_t {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ngx_str_t {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        Ord::cmp(self.as_bytes(), other.as_bytes())
    }
}

impl TryFrom<ngx_str_t> for &str {
    type Error = core::str::Utf8Error;

    fn try_from(s: ngx_str_t) -> Result<Self, Self::Error> {
        core::str::from_utf8(s.into())
    }
}
