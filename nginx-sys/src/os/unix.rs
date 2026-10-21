/// Returns a non cryptograhpically-secure pseudo-random integer.
#[inline]
pub fn ngx_random() -> core::ffi::c_long {
    // random() is not available in ::libc
    unsafe { crate::bindings::random() }
}

/// Causes the calling thread to relinquish the CPU.
#[inline]
pub fn ngx_sched_yield() {
    unsafe { libc::sched_yield() };
}
