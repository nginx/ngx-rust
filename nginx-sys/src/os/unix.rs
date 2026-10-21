/// Returns a non cryptograhpically-secure pseudo-random integer.
#[inline]
pub fn ngx_random() -> core::ffi::c_long {
    unsafe { crate::bindings::random() }
}

/// Causes the calling thread to relinquish the CPU.
#[inline]
pub fn ngx_sched_yield() {
    #[cfg(ngx_feature = "have_sched_yield")]
    unsafe {
        crate::bindings::sched_yield()
    };
    #[cfg(not(ngx_feature = "have_sched_yield"))]
    unsafe {
        crate::bindings::usleep(1)
    }
}
