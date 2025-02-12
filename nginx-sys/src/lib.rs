#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![no_std]

mod event;
mod queue;

use core::fmt;
use core::mem::offset_of;
use core::ptr::{self, copy_nonoverlapping};
use core::slice;

#[doc(hidden)]
mod bindings {
    #![allow(missing_docs)]
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]
    #![allow(dead_code)]
    #![allow(clippy::all)]
    #![allow(improper_ctypes)]
    #![allow(rustdoc::broken_intra_doc_links)]
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}
#[doc(no_inline)]
pub use bindings::*;

pub use event::*;
pub use queue::*;

/// The offset of the `main_conf` field in the `ngx_http_conf_ctx_t` struct.
///
/// This is used to access the main configuration context for an HTTP module.
pub const NGX_HTTP_MAIN_CONF_OFFSET: usize = offset_of!(ngx_http_conf_ctx_t, main_conf);

/// The offset of the `srv_conf` field in the `ngx_http_conf_ctx_t` struct.
///
/// This is used to access the server configuration context for an HTTP module.
pub const NGX_HTTP_SRV_CONF_OFFSET: usize = offset_of!(ngx_http_conf_ctx_t, srv_conf);

/// The offset of the `loc_conf` field in the `ngx_http_conf_ctx_t` struct.
///
/// This is used to access the location configuration context for an HTTP module.
pub const NGX_HTTP_LOC_CONF_OFFSET: usize = offset_of!(ngx_http_conf_ctx_t, loc_conf);

/// Convert a byte slice to a raw pointer (`*mut u_char`) allocated in the given nginx memory pool.
///
/// # Safety
///
/// The caller must provide a valid pointer to the memory pool.
pub unsafe fn bytes_to_uchar(pool: *mut ngx_pool_t, data: &[u8]) -> Option<*mut u_char> {
    let ptr: *mut u_char = ngx_pnalloc(pool, data.len()) as _;
    if ptr.is_null() {
        return None;
    }
    copy_nonoverlapping(data.as_ptr(), ptr, data.len());
    Some(ptr)
}

/// Convert a string slice (`&str`) to a raw pointer (`*mut u_char`) allocated in the given nginx memory pool.
///
/// # Arguments
///
/// * `pool` - A pointer to the nginx memory pool (`ngx_pool_t`).
/// * `data` - The string slice to convert to a raw pointer.
///
/// # Safety
/// This function is marked as unsafe because it involves raw pointer manipulation and direct memory allocation using `ngx_pnalloc`.
///
/// # Returns
/// A raw pointer (`*mut u_char`) to the allocated memory containing the converted string data.
///
/// # Example
/// ```rust,ignore
/// let pool: *mut ngx_pool_t = ...; // Obtain a pointer to the nginx memory pool
/// let data: &str = "example"; // The string to convert
/// let ptr = str_to_uchar(pool, data);
/// ```
pub unsafe fn str_to_uchar(pool: *mut ngx_pool_t, data: &str) -> *mut u_char {
    let ptr: *mut u_char = ngx_pnalloc(pool, data.len()) as _;
    debug_assert!(!ptr.is_null());
    copy_nonoverlapping(data.as_ptr(), ptr, data.len());
    ptr
}

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
    /// # Safety
    /// This function is marked as unsafe because it involves raw pointer manipulation.
    /// It assumes that the underlying `data` pointer is valid and points to a valid UTF-8 encoded string.
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
        bytes_to_uchar(pool, src).map(|data| Self { data, len: src.len() })
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
            data: str_to_uchar(pool, data),
            len: data.len(),
        }
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // The implementation is similar to an inlined `String::from_utf8_lossy`, with two
        // important differences:
        //
        //  - it writes directly to the Formatter instead of allocating a temporary String
        //  - invalid sequences are represented as escaped individual bytes
        for chunk in self.as_bytes().utf8_chunks() {
            f.write_str(chunk.valid())?;
            for byte in chunk.invalid() {
                f.write_str("\\x")?;
                fmt::LowerHex::fmt(byte, f)?;
            }
        }
        Ok(())
    }
}

impl TryFrom<ngx_str_t> for &str {
    type Error = core::str::Utf8Error;

    fn try_from(s: ngx_str_t) -> Result<Self, Self::Error> {
        core::str::from_utf8(s.into())
    }
}

impl ngx_command_t {
    /// Creates a new empty [`ngx_command_t`] instance.
    ///
    /// This method replaces the `ngx_null_command` C macro. This is typically used to terminate an
    /// array of configuration directives.
    ///
    /// [`ngx_command_t`]: https://nginx.org/en/docs/dev/development_guide.html#config_directives
    pub const fn empty() -> Self {
        Self {
            name: ngx_str_t::empty(),
            type_: 0,
            set: None,
            conf: 0,
            offset: 0,
            post: ptr::null_mut(),
        }
    }
}

impl ngx_module_t {
    /// Create a new `ngx_module_t` instance with default values.
    pub const fn default() -> Self {
        Self {
            ctx_index: ngx_uint_t::MAX,
            index: ngx_uint_t::MAX,
            name: ptr::null_mut(),
            spare0: 0,
            spare1: 0,
            version: nginx_version as ngx_uint_t,
            signature: NGX_RS_MODULE_SIGNATURE.as_ptr(),
            ctx: ptr::null_mut(),
            commands: ptr::null_mut(),
            type_: 0,
            init_master: None,
            init_module: None,
            init_process: None,
            init_thread: None,
            exit_thread: None,
            exit_process: None,
            exit_master: None,
            spare_hook0: 0,
            spare_hook1: 0,
            spare_hook2: 0,
            spare_hook3: 0,
            spare_hook4: 0,
            spare_hook5: 0,
            spare_hook6: 0,
            spare_hook7: 0,
        }
    }
}

/// Add a key-value pair to an nginx table entry (`ngx_table_elt_t`) in the given nginx memory pool.
///
/// # Arguments
///
/// * `table` - A pointer to the nginx table entry (`ngx_table_elt_t`) to modify.
/// * `pool` - A pointer to the nginx memory pool (`ngx_pool_t`) for memory allocation.
/// * `key` - The key string to add to the table entry.
/// * `value` - The value string to add to the table entry.
///
/// # Safety
/// This function is marked as unsafe because it involves raw pointer manipulation and direct memory allocation using `str_to_uchar`.
///
/// # Returns
/// An `Option<()>` representing the result of the operation. `Some(())` indicates success, while `None` indicates a null table pointer.
///
/// # Example
/// ```rust
/// # use nginx_sys::*;
/// # unsafe fn example(pool: *mut ngx_pool_t, headers: *mut ngx_list_t) {
/// // Obtain a pointer to the nginx table entry
/// let table: *mut ngx_table_elt_t = ngx_list_push(headers).cast();
/// assert!(!table.is_null());
/// let key: &str = "key"; // The key to add
/// let value: &str = "value"; // The value to add
/// let result = add_to_ngx_table(table, pool, key, value);
/// # }
/// ```
pub unsafe fn add_to_ngx_table(
    table: *mut ngx_table_elt_t,
    pool: *mut ngx_pool_t,
    key: impl AsRef<[u8]>,
    value: impl AsRef<[u8]>,
) -> Option<()> {
    if let Some(table) = table.as_mut() {
        let key = key.as_ref();
        table.key = ngx_str_t::from_bytes(pool, key)?;
        table.value = ngx_str_t::from_bytes(pool, value.as_ref())?;
        table.lowcase_key = ngx_pnalloc(pool, table.key.len).cast();
        if table.lowcase_key.is_null() {
            return None;
        }
        table.hash = ngx_hash_strlow(table.lowcase_key, table.key.data, table.key.len);
        return Some(());
    }
    None
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::string::ToString;

    use super::*;

    #[test]
    fn ngx_str_display() {
        let pairs: &[(&[u8], &str)] = &[
            (b"", ""),
            (b"Ferris the \xf0\x9f\xa6\x80", "Ferris the 🦀"),
            (b"\xF0\x90\x80", "\\xf0\\x90\\x80"),
            (b"\xF0\x90\x80Hello World", "\\xf0\\x90\\x80Hello World"),
            (b"Hello \xF0\x90\x80World", "Hello \\xf0\\x90\\x80World"),
            (b"Hello World\xF0\x90\x80", "Hello World\\xf0\\x90\\x80"),
        ];

        for (bytes, expected) in pairs {
            let str = ngx_str_t {
                data: bytes.as_ptr().cast_mut(),
                len: bytes.len(),
            };
            assert_eq!(str.to_string(), *expected);
        }
    }
}
