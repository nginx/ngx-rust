#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![no_std]

pub mod detail;
mod event;
#[cfg(ngx_feature = "http")]
mod http;
mod queue;
mod rbtree;
#[cfg(ngx_feature = "stream")]
mod stream;
mod string;

use core::ptr;

#[doc(hidden)]
mod bindings {
    #![allow(unknown_lints)] // unnecessary_transmutes
    #![allow(missing_docs)]
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]
    #![allow(dead_code)]
    #![allow(clippy::all)]
    #![allow(improper_ctypes)]
    #![allow(rustdoc::broken_intra_doc_links)]
    #![allow(unnecessary_transmutes)]
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}
#[doc(no_inline)]
pub use bindings::*;
pub use event::*;
#[cfg(ngx_feature = "http")]
pub use http::*;
pub use queue::*;
pub use rbtree::*;
#[cfg(ngx_feature = "stream")]
pub use stream::*;

/// Default alignment for pool allocations.
pub const NGX_ALIGNMENT: usize = NGX_RS_ALIGNMENT;

// Check if the allocations made with ngx_palloc are properly aligned.
// If the check fails, objects allocated from `ngx_pool` can violate Rust pointer alignment
// requirements.
const _: () = assert!(core::mem::align_of::<ngx_str_t>() <= NGX_ALIGNMENT);

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

/// Returns the error code of the last failed operation (`errno`).
#[inline]
pub fn ngx_errno() -> ngx_err_t {
    // SAFETY: GetLastError takes no arguments and reads a thread-local variable
    #[cfg(windows)]
    let err = unsafe { GetLastError() };

    #[cfg(not(windows))]
    let err = errno::errno().0;

    err as ngx_err_t
}

/// Sets the error code (`errno`).
#[inline]
pub fn ngx_set_errno(err: ngx_err_t) {
    #[cfg(windows)]
    // SAFETY: SetLastError takes one argument by value and updates a thread-local variable
    unsafe {
        SetLastError(err as _)
    }
    #[cfg(not(windows))]
    errno::set_errno(errno::Errno(err as _))
}

/// Returns the error code of the last failed sockets operation.
#[inline]
pub fn ngx_socket_errno() -> ngx_err_t {
    // SAFETY: WSAGetLastError takes no arguments and reads a thread-local variable
    #[cfg(windows)]
    let err = unsafe { WSAGetLastError() };

    #[cfg(not(windows))]
    let err = errno::errno().0;

    err as ngx_err_t
}

/// Sets the error code of the sockets operation.
#[inline]
pub fn ngx_set_socket_errno(err: ngx_err_t) {
    #[cfg(windows)]
    // SAFETY: WSaSetLastError takes one argument by value and updates a thread-local variable
    unsafe {
        WSASetLastError(err as _)
    }
    #[cfg(not(windows))]
    errno::set_errno(errno::Errno(err as _))
}

/// Returns a non cryptograhpically-secure pseudo-random integer.
#[inline]
pub fn ngx_random() -> core::ffi::c_long {
    #[cfg(windows)]
    unsafe {
        // Emulate random() as Microsoft CRT does not provide it.
        // rand() should be thread-safe in the multi-threaded CRT we link to, but will not be seeded
        // outside of the main thread.
        let x: u32 = ((rand() as u32) << 16) ^ ((rand() as u32) << 8) ^ (rand() as u32);
        (0x7fffffff & x) as _
    }
    #[cfg(not(windows))]
    unsafe {
        random()
    }
}

/// Causes the calling thread to relinquish the CPU.
#[inline]
pub fn ngx_sched_yield() {
    #[cfg(windows)]
    unsafe {
        SwitchToThread()
    };
    #[cfg(all(not(windows), ngx_feature = "have_sched_yield"))]
    unsafe {
        sched_yield()
    };
    #[cfg(not(any(windows, ngx_feature = "have_sched_yield")))]
    unsafe {
        usleep(1)
    }
}

/// Returns cached timestamp in seconds, updated at the start of the event loop iteration.
///
/// Can be stale when accessing from threads, see [ngx_time_update].
#[inline]
pub fn ngx_time() -> time_t {
    // SAFETY: ngx_cached_time is initialized before any module code can run
    unsafe { (*ngx_cached_time).sec }
}

/// Returns cached time, updated at the start of the event loop iteration.
///
/// Can be stale when accessing from threads, see [ngx_time_update].
/// A cached reference to the ngx_timeofday() result is guaranteed to remain unmodified for the next
/// NGX_TIME_SLOTS seconds.
#[inline]
pub fn ngx_timeofday() -> &'static ngx_time_t {
    // SAFETY: ngx_cached_time is initialized before any module code can run
    unsafe { &*ngx_cached_time }
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
