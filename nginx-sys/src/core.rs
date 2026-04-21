use ::core::{mem, ptr};

pub use self::queue::*;
pub use self::rbtree::*;
use crate::bindings::{
    self, ngx_array_t, ngx_command_t, ngx_err_t, ngx_int_t, ngx_list_t, ngx_module_t, ngx_pool_t,
    ngx_str_t, ngx_time_t, ngx_uint_t, ngx_variable_value_t, time_t,
};

mod queue;
mod rbtree;
mod string;

impl ngx_array_t {
    /// Returns the contents of this array as a slice of `T`.
    ///
    /// # Safety
    ///
    /// The array must be a valid, initialized array containing elements of type T or compatible in
    /// layout with T (e.g. `#[repr(transparent)]` wrappers).
    pub unsafe fn as_slice<T>(&self) -> &[T] {
        debug_assert_eq!(
            mem::size_of::<T>(),
            self.size,
            "ngx_array_t::as_slice(): element size mismatch"
        );
        if self.nelts == 0 {
            &[]
        } else {
            // SAFETY: in a valid array, `elts` is a valid well-aligned pointer to at least `nelts`
            // elements of size `size`
            unsafe { ::core::slice::from_raw_parts(self.elts.cast(), self.nelts) }
        }
    }

    /// Returns the contents of this array as a mutable slice of `T`.
    ///
    /// # Safety
    ///
    /// The array must be a valid, initialized array containing elements of type T or compatible in
    /// layout with T (e.g. `#[repr(transparent)]` wrappers).
    pub unsafe fn as_slice_mut<T>(&mut self) -> &mut [T] {
        debug_assert_eq!(
            mem::size_of::<T>(),
            self.size,
            "ngx_array_t::as_slice_mut(): element size mismatch"
        );
        if self.nelts == 0 {
            &mut []
        } else {
            // SAFETY: in a valid array, `elts` is a valid well-aligned pointer to at least `nelts`
            // elements of size `size`
            unsafe { ::core::slice::from_raw_parts_mut(self.elts.cast(), self.nelts) }
        }
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
            version: bindings::nginx_version as ngx_uint_t,
            signature: bindings::NGX_RS_MODULE_SIGNATURE.as_ptr(),
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

impl ngx_variable_value_t {
    /// Returns the contents of this variable value as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        match self.len() {
            0 => &[],
            // SAFETY: data for non-empty value must be a valid well-aligned pointer.
            len => unsafe { ::core::slice::from_raw_parts(self.data, len as usize) },
        }
    }
}

impl AsRef<[u8]> for ngx_variable_value_t {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

/// Returns the error code of the last failed operation (`errno`).
#[inline]
pub fn ngx_errno() -> ngx_err_t {
    // SAFETY: GetLastError takes no arguments and reads a thread-local variable
    #[cfg(windows)]
    let err = unsafe { bindings::GetLastError() };

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
        bindings::SetLastError(err as _)
    }
    #[cfg(not(windows))]
    errno::set_errno(errno::Errno(err as _))
}

/// Returns the error code of the last failed sockets operation.
#[inline]
pub fn ngx_socket_errno() -> ngx_err_t {
    // SAFETY: WSAGetLastError takes no arguments and reads a thread-local variable
    #[cfg(windows)]
    let err = unsafe { bindings::WSAGetLastError() };

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
        bindings::WSASetLastError(err as _)
    }
    #[cfg(not(windows))]
    errno::set_errno(errno::Errno(err as _))
}

/// Returns a non cryptograhpically-secure pseudo-random integer.
#[inline]
pub fn ngx_random() -> ::core::ffi::c_long {
    #[cfg(windows)]
    unsafe {
        use bindings::rand;

        // Emulate random() as Microsoft CRT does not provide it.
        // rand() should be thread-safe in the multi-threaded CRT we link to, but will not be seeded
        // outside of the main thread.
        let x: u32 = ((rand() as u32) << 16) ^ ((rand() as u32) << 8) ^ (rand() as u32);
        (0x7fffffff & x) as _
    }
    #[cfg(not(windows))]
    unsafe {
        bindings::random()
    }
}

/// Causes the calling thread to relinquish the CPU.
#[inline]
pub fn ngx_sched_yield() {
    #[cfg(windows)]
    unsafe {
        bindings::SwitchToThread()
    };
    #[cfg(all(not(windows), ngx_feature = "have_sched_yield"))]
    unsafe {
        bindings::sched_yield()
    };
    #[cfg(not(any(windows, ngx_feature = "have_sched_yield")))]
    unsafe {
        bindings::usleep(1)
    }
}

/// Returns cached timestamp in seconds, updated at the start of the event loop iteration.
///
/// Can be stale when accessing from threads, see [bindings::ngx_time_update].
#[inline]
pub fn ngx_time() -> time_t {
    // SAFETY: ngx_cached_time is initialized before any module code can run
    unsafe { (*bindings::ngx_cached_time).sec }
}

/// Returns cached time, updated at the start of the event loop iteration.
///
/// Can be stale when accessing from threads, see [bindings::ngx_time_update].
/// A cached reference to the ngx_timeofday() result is guaranteed to remain unmodified for the next
/// NGX_TIME_SLOTS seconds.
#[inline]
pub fn ngx_timeofday() -> &'static ngx_time_t {
    // SAFETY: ngx_cached_time is initialized before any module code can run
    unsafe { &*bindings::ngx_cached_time }
}

/// Initialize a list, using a pool for the backing memory, with capacity to store the given number
/// of elements and element size.
///
/// # Safety
/// * `list` must be non-null
/// * `pool` must be a valid pool
#[inline]
pub unsafe fn ngx_list_init(
    list: *mut ngx_list_t,
    pool: *mut ngx_pool_t,
    n: ngx_uint_t,
    size: usize,
) -> ngx_int_t {
    unsafe {
        (*list).part.elts = bindings::ngx_palloc(pool, n * size);
        if (*list).part.elts.is_null() {
            return bindings::NGX_ERROR as ngx_int_t;
        }
        (*list).part.nelts = 0;
        (*list).part.next = ptr::null_mut();
        (*list).last = &raw mut (*list).part;
        (*list).size = size;
        (*list).nalloc = n;
        (*list).pool = pool;
        bindings::NGX_OK as ngx_int_t
    }
}
