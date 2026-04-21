#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![no_std]

pub mod detail;

mod core;
mod event;
#[cfg(all(feature = "http", ngx_feature = "http"))]
mod http;
#[cfg(all(feature = "mail", ngx_feature = "mail"))]
mod mail;
#[cfg(all(feature = "stream", ngx_feature = "stream"))]
mod stream;

#[doc(hidden)]
mod bindings {
    #![allow(clippy::all)]
    #![allow(dead_code)]
    #![allow(improper_ctypes)] // u128 in libc headers
    #![allow(missing_docs)]
    #![allow(nonstandard_style)]
    #![allow(rustdoc::broken_intra_doc_links)]
    #![allow(unknown_lints)] // unnecessary_transmutes before 1.88
    #![allow(unnecessary_transmutes)]
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}
#[doc(no_inline)]
pub use crate::bindings::*;
pub use crate::core::*;
pub use crate::event::*;
#[cfg(all(feature = "http", ngx_feature = "http"))]
pub use crate::http::*;
#[cfg(all(feature = "mail", ngx_feature = "mail"))]
pub use crate::mail::*;
#[cfg(all(feature = "stream", ngx_feature = "stream"))]
pub use crate::stream::*;

/// Default alignment for pool allocations.
pub const NGX_ALIGNMENT: usize = NGX_RS_ALIGNMENT;

// Check if the allocations made with ngx_palloc are properly aligned.
// If the check fails, objects allocated from `ngx_pool` can violate Rust pointer alignment
// requirements.
const _: () = assert!(::core::mem::align_of::<ngx_str_t>() <= NGX_ALIGNMENT);

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
/// This function is marked as unsafe because it involves raw pointer manipulation and direct memory
/// allocation using `str_to_uchar`.
///
/// # Returns
/// An `Option<()>` representing the result of the operation. `Some(())` indicates success, while
/// `None` indicates a null table pointer.
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
    if let Some(table) = unsafe { table.as_mut() } {
        let key = key.as_ref();
        table.key = unsafe { ngx_str_t::from_bytes(pool, key)? };
        table.value = unsafe { ngx_str_t::from_bytes(pool, value.as_ref())? };
        table.lowcase_key = unsafe { ngx_pnalloc(pool, table.key.len).cast() };
        if table.lowcase_key.is_null() {
            return None;
        }
        table.hash = unsafe { ngx_hash_strlow(table.lowcase_key, table.key.data, table.key.len) };
        return Some(());
    }
    None
}
