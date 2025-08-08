use core::cmp;
use core::fmt::{self, Write};
use core::mem::MaybeUninit;
use core::ptr::NonNull;

use crate::ffi::{self, ngx_err_t, ngx_log_t, ngx_uint_t, NGX_MAX_ERROR_STR};

/// This constant is set to `true` if NGINX is compiled with debug logging (`--with-debug`).
pub const DEBUG: bool = cfg!(ngx_feature = "debug");

/// Size of the static buffer used to format log messages.
///
/// Approximates the remaining space in `u_char[NGX_MAX_ERROR_STR]` after writing the standard
/// prefix
pub const LOG_BUFFER_SIZE: usize =
    NGX_MAX_ERROR_STR as usize - b"1970/01/01 00:00:00 [info] 1#1: ".len();

/// Obtains a pointer to the global (cycle) log object.
///
/// The returned pointer is tied to the current cycle lifetime, and will be invalidated by a
/// configuration reload in the master process or in a single-process mode. If you plan to store it,
/// make sure that your storage is also tied to the cycle lifetime (e.g. module configuration or
/// connection/request data).
///
/// The function may panic if you call it before the main() in nginx creates an initial cycle.
#[inline(always)]
pub fn ngx_cycle_log() -> NonNull<ngx_log_t> {
    NonNull::new(unsafe { (*nginx_sys::ngx_cycle).log }).expect("global logger")
}

/// Utility function to provide typed checking of the mask's field state.
#[inline(always)]
pub fn check_mask(mask: DebugMask, log_level: usize) -> bool {
    let mask_bits: u32 = mask.into();
    if log_level & mask_bits as usize == 0 {
        return false;
    }
    true
}

/// Format args into a provided buffer
// May produce incomplete UTF-8 sequences. But any writes to `ngx_log_t` already can be truncated,
// so nothing we can do here.
#[inline]
pub fn write_fmt<'a>(buf: &'a mut [MaybeUninit<u8>], args: fmt::Arguments<'_>) -> &'a [u8] {
    if let Some(str) = args.as_str() {
        str.as_bytes()
    } else {
        let mut buf = LogBuf::from(buf);
        // nothing we can or want to do on errors
        let _ = buf.write_fmt(args);
        buf.filled()
    }
}

/// Writes the provided buffer to the nginx logger at a specified level.
///
/// # Safety
/// Requires a valid log pointer.
#[inline]
pub unsafe fn log_error(level: ngx_uint_t, log: *mut ngx_log_t, err: ngx_err_t, buf: &[u8]) {
    unsafe {
        #[cfg(ngx_feature = "have_variadic_macros")]
        ffi::ngx_log_error_core(level, log, err, c"%*s".as_ptr(), buf.len(), buf.as_ptr());
        #[cfg(not(ngx_feature = "have_variadic_macros"))]
        ffi::ngx_log_error(level, log, err, c"%*s".as_ptr(), buf.len(), buf.as_ptr());
    }
}

/// Writes the provided buffer to the nginx logger at the debug level.
///
/// # Safety
/// Requires a valid log pointer.
#[inline]
pub unsafe fn log_debug(log: *mut ngx_log_t, err: ngx_err_t, buf: &[u8]) {
    unsafe {
        #[cfg(ngx_feature = "have_variadic_macros")]
        ffi::ngx_log_error_core(
            ffi::NGX_LOG_DEBUG as _,
            log,
            err,
            c"%*s".as_ptr(),
            buf.len(),
            buf.as_ptr(),
        );
        #[cfg(not(ngx_feature = "have_variadic_macros"))]
        ffi::ngx_log_debug_core(log, err, c"%*s".as_ptr(), buf.len(), buf.as_ptr());
    }
}

/// Write to logger at a specified level.
///
/// See [Logging](https://nginx.org/en/docs/dev/development_guide.html#logging)
/// for available log levels.
#[macro_export]
macro_rules! ngx_log_error {
    ( $level:expr, $log:expr, $($arg:tt)+ ) => {
        let log = $log;
        let level = $level as $crate::ffi::ngx_uint_t;
        if level < unsafe { (*log).log_level } {
            let mut buf =
                [const { ::core::mem::MaybeUninit::<u8>::uninit() }; $crate::log::LOG_BUFFER_SIZE];
            let message = $crate::log::write_fmt(&mut buf, format_args!($($arg)+));
            unsafe { $crate::log::log_error(level, log, 0, message) };
        }
    }
}

/// Write to logger with the context of currently processed configuration file.
#[macro_export]
macro_rules! ngx_conf_log_error {
    ( $level:expr, $cf:expr, $($arg:tt)+ ) => {
        let cf: *mut $crate::ffi::ngx_conf_t = $cf;
        let level = $level as $crate::ffi::ngx_uint_t;
        if level < unsafe { (*(*cf).log).log_level } {
            let mut buf =
                [const { ::core::mem::MaybeUninit::<u8>::uninit() }; $crate::log::LOG_BUFFER_SIZE];
            let message = $crate::log::write_fmt(&mut buf, format_args!($($arg)+));
            unsafe {
                $crate::ffi::ngx_conf_log_error(
                    level,
                    cf,
                    0,
                    c"%*s".as_ptr(),
                    message.len(),
                    message.as_ptr()
                );
            }
        }
    }
}

/// Write to logger at debug level.
#[macro_export]
macro_rules! ngx_log_debug {
    ( mask: $mask:expr, $log:expr, $($arg:tt)+ ) => {
        let log = $log;
        if $crate::log::DEBUG && $crate::log::check_mask($mask, unsafe { (*log).log_level }) {
            let mut buf =
                [const { ::core::mem::MaybeUninit::<u8>::uninit() }; $crate::log::LOG_BUFFER_SIZE];
            let message = $crate::log::write_fmt(&mut buf, format_args!($($arg)+));
            unsafe { $crate::log::log_debug(log, 0, message) };
        }
    };
    ( $log:expr, $($arg:tt)+ ) => {
        $crate::ngx_log_debug!(mask: $crate::log::DebugMask::All, $log, $($arg)+);
    }
}

/// Log to request connection log at level [`NGX_LOG_DEBUG_HTTP`].
///
/// [`NGX_LOG_DEBUG_HTTP`]: https://nginx.org/en/docs/dev/development_guide.html#logging
#[macro_export]
macro_rules! ngx_log_debug_http {
    ( $request:expr, $($arg:tt)+ ) => {
        let log = unsafe { (*$request.connection()).log };
        $crate::ngx_log_debug!(mask: $crate::log::DebugMask::Http, log, $($arg)+);
    }
}

/// Log with requested debug mask.
///
/// **NOTE:** This macro supports [`DebugMask::Http`] (`NGX_LOG_DEBUG_HTTP`), however, if you have
/// access to a Request object via an http handler it can be more convenient and readable to use
/// the [`ngx_log_debug_http`] macro instead.
///
/// See <https://nginx.org/en/docs/dev/development_guide.html#logging> for details and available
/// masks.
#[macro_export]
macro_rules! ngx_log_debug_mask {
    ( DebugMask::Core, $log:expr, $($arg:tt)+ ) => {
        $crate::ngx_log_debug!(mask: $crate::log::DebugMask::Core, $log, $($arg)+);
    };
    ( DebugMask::Alloc, $log:expr, $($arg:tt)+ ) => {
        $crate::ngx_log_debug!(mask: $crate::log::DebugMask::Alloc, $log, $($arg)+);
    };
    ( DebugMask::Mutex, $log:expr, $($arg:tt)+ ) => {
        $crate::ngx_log_debug!(mask: $crate::log::DebugMask::Mutex, $log, $($arg)+);
    };
    ( DebugMask::Event, $log:expr, $($arg:tt)+ ) => {
        $crate::ngx_log_debug!(mask: $crate::log::DebugMask::Event, $log, $($arg)+);
    };
    ( DebugMask::Http, $log:expr, $($arg:tt)+ ) => {
        $crate::ngx_log_debug!(mask: $crate::log::DebugMask::Http, $log, $($arg)+);
    };
    ( DebugMask::Mail, $log:expr, $($arg:tt)+ ) => {
        $crate::ngx_log_debug!(mask: $crate::log::DebugMask::Mail, $log, $($arg)+);
    };
    ( DebugMask::Stream, $log:expr, $($arg:tt)+ ) => {
        $crate::ngx_log_debug!(mask: $crate::log::DebugMask::Stream, $log, $($arg)+);
    };
}

/// Debug masks for use with [`ngx_log_debug_mask`], these represent the only accepted values for
/// the mask.
#[derive(Debug)]
pub enum DebugMask {
    /// Aligns to the NGX_LOG_DEBUG_CORE mask.
    Core,
    /// Aligns to the NGX_LOG_DEBUG_ALLOC mask.
    Alloc,
    /// Aligns to the NGX_LOG_DEBUG_MUTEX mask.
    Mutex,
    /// Aligns to the NGX_LOG_DEBUG_EVENT mask.
    Event,
    /// Aligns to the NGX_LOG_DEBUG_HTTP mask.
    Http,
    /// Aligns to the NGX_LOG_DEBUG_MAIL mask.
    Mail,
    /// Aligns to the NGX_LOG_DEBUG_STREAM mask.
    Stream,
    /// Aligns to the NGX_LOG_DEBUG_ALL mask.
    All,
}

impl TryFrom<u32> for DebugMask {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            crate::ffi::NGX_LOG_DEBUG_CORE => Ok(DebugMask::Core),
            crate::ffi::NGX_LOG_DEBUG_ALLOC => Ok(DebugMask::Alloc),
            crate::ffi::NGX_LOG_DEBUG_MUTEX => Ok(DebugMask::Mutex),
            crate::ffi::NGX_LOG_DEBUG_EVENT => Ok(DebugMask::Event),
            crate::ffi::NGX_LOG_DEBUG_HTTP => Ok(DebugMask::Http),
            crate::ffi::NGX_LOG_DEBUG_MAIL => Ok(DebugMask::Mail),
            crate::ffi::NGX_LOG_DEBUG_STREAM => Ok(DebugMask::Stream),
            crate::ffi::NGX_LOG_DEBUG_ALL => Ok(DebugMask::All),
            _ => Err(0),
        }
    }
}

impl From<DebugMask> for u32 {
    fn from(value: DebugMask) -> Self {
        match value {
            DebugMask::Core => crate::ffi::NGX_LOG_DEBUG_CORE,
            DebugMask::Alloc => crate::ffi::NGX_LOG_DEBUG_ALLOC,
            DebugMask::Mutex => crate::ffi::NGX_LOG_DEBUG_MUTEX,
            DebugMask::Event => crate::ffi::NGX_LOG_DEBUG_EVENT,
            DebugMask::Http => crate::ffi::NGX_LOG_DEBUG_HTTP,
            DebugMask::Mail => crate::ffi::NGX_LOG_DEBUG_MAIL,
            DebugMask::Stream => crate::ffi::NGX_LOG_DEBUG_STREAM,
            DebugMask::All => crate::ffi::NGX_LOG_DEBUG_ALL,
        }
    }
}

/// Minimal subset of unstable core::io::{BorrowedBuf,BorrowedCursor}
struct LogBuf<'data> {
    buf: &'data mut [MaybeUninit<u8>],
    filled: usize,
}

impl<'data> LogBuf<'data> {
    pub fn filled(&self) -> &'data [u8] {
        // SAFETY: valid bytes have been written to self.buf[..self.filled]
        unsafe {
            let buf = self.buf.get_unchecked(..self.filled);
            // inlined MaybeUninit::slice_assume_init_ref
            &*(buf as *const [MaybeUninit<u8>] as *const [u8])
        }
    }

    pub fn append(&mut self, buf: &[u8]) -> &mut Self {
        let n = cmp::min(self.buf.len() - self.filled, buf.len());
        unsafe {
            // SAFETY: The source buf has at least n bytes
            let src = buf.get_unchecked(..n);
            // SAFETY: &[u8] and &[MaybeUninit<u8>] have the same layout
            let src: &[MaybeUninit<u8>] = core::mem::transmute(src);
            // SAFETY: self.buf has at least n bytes available after self.filled
            self.buf
                .get_unchecked_mut(self.filled..self.filled + n)
                .copy_from_slice(src);
        }
        self.filled += n;
        self
    }
}

impl<'data> From<&'data mut [MaybeUninit<u8>]> for LogBuf<'data> {
    fn from(buf: &'data mut [MaybeUninit<u8>]) -> Self {
        Self { buf, filled: 0 }
    }
}

impl fmt::Write for LogBuf<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.append(s.as_bytes());
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_mask_lower_bound() {
        assert!(<DebugMask as Into<u32>>::into(DebugMask::Core) == crate::ffi::NGX_LOG_DEBUG_FIRST);
    }
    #[test]
    fn test_mask_upper_bound() {
        assert!(
            <DebugMask as Into<u32>>::into(DebugMask::Stream) == crate::ffi::NGX_LOG_DEBUG_LAST
        );
    }
    #[test]
    fn test_check_mask() {
        struct MockLog {
            log_level: usize,
        }
        let mock = MockLog { log_level: 16 };

        let mut r = check_mask(DebugMask::Core, mock.log_level);
        assert!(r);

        r = check_mask(DebugMask::Alloc, mock.log_level);
        assert!(!r);
    }

    #[test]
    fn log_buffer() {
        use core::str;

        let mut buf = [const { MaybeUninit::<u8>::uninit() }; 32];
        let mut buf = LogBuf::from(&mut buf[..]);
        let words = ["Hello", "World"];

        // normal write
        write!(&mut buf, "{} {}!", words[0], words[1]).unwrap();
        assert_eq!(str::from_utf8(buf.filled()), Ok("Hello World!"));

        // overflow results in truncated output
        write!(&mut buf, " This is a test, {}", usize::MAX).unwrap();
        assert_eq!(
            str::from_utf8(buf.filled()),
            Ok("Hello World! This is a test, 184")
        );

        // and any following writes are still safe
        write!(&mut buf, "test").unwrap();
        assert_eq!(
            str::from_utf8(buf.filled()),
            Ok("Hello World! This is a test, 184")
        );
    }
}
