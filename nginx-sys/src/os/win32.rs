/// Returns a non cryptograhpically-secure pseudo-random integer.
#[inline]
pub fn ngx_random() -> core::ffi::c_long {
    use libc::rand;

    unsafe {
        // Emulate random() as Microsoft CRT does not provide it.
        // rand() should be thread-safe in the multi-threaded CRT we link to, but will not be seeded
        // outside of the main thread.
        let x: u32 = ((rand() as u32) << 16) ^ ((rand() as u32) << 8) ^ (rand() as u32);
        (0x7fffffff & x) as _
    }
}

/// Causes the calling thread to relinquish the CPU.
#[inline]
pub fn ngx_sched_yield() {
    unsafe { crate::bindings::SwitchToThread() };
}
