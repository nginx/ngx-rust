//! FFI bindings for unit tests that require linking with nginx library.

use nginx_sys::{ngx_cycle_t, ngx_int_t, ngx_str_t, u_char};

use core::sync::atomic::{AtomicBool, Ordering};

#[link(name = "nginx", kind = "static")]
extern "C" {
    /// Initialize the nginx library with the given path prefix.
    fn libngx_init(prefix: *mut u_char) -> *mut ngx_cycle_t;
    /// Clean up the nginx library instance.
    fn libngx_cleanup(cycle: *mut ngx_cycle_t);
    /// Create a new nginx cycle with the given configuration file.
    fn libngx_create_cycle(cycle: *mut ngx_cycle_t, conf: *mut ngx_str_t) -> ngx_int_t;
}

static NGINX_USED: AtomicBool = AtomicBool::new(false);

/// A wrapper around the nginx library instance.
pub struct LibNginx {
    cycle: *mut ngx_cycle_t,
}

impl LibNginx {
    fn lock() {
        while NGINX_USED
            .compare_exchange_weak(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {}
    }

    fn unlock() {
        NGINX_USED.store(false, Ordering::Release);
    }

    /// Initialize a new instance of the nginx library with the given path prefix.
    pub fn new(prefix: &str) -> Self {
        Self::lock();
        let cycle = unsafe { libngx_init(str_to_uchar(prefix)) };
        if cycle.is_null() {
            Self::unlock();
            panic!("Failed to initialize nginx library");
        }
        LibNginx { cycle }
    }

    /// Create a new instance of the nginx library with the given configuration file.
    pub fn from_conf(prefix: &str, conf: &str) -> Self {
        let instance = Self::new(prefix);
        let mut conf = unsafe { ngx_str_t::from_str((*instance.cycle).pool, conf) };
        let rc: ngx_int_t = unsafe { libngx_create_cycle(instance.cycle, &mut conf) };
        if rc != 0 {
            Self::unlock();
            panic!("Failed to create nginx cycle from config");
        }
        instance
    }
}

impl Drop for LibNginx {
    fn drop(&mut self) {
        unsafe { libngx_cleanup(self.cycle) };
        Self::unlock();
    }
}

fn str_to_uchar(prefix: &str) -> *mut u_char {
    let bytes = prefix.as_bytes();
    let mut u_chars = Vec::with_capacity(bytes.len() + 1);
    u_chars.extend_from_slice(bytes);
    u_chars.push(0); // Null-terminate
    u_chars.as_mut_ptr()
}
