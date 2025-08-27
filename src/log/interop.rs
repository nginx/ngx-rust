//! Interoperation with the [::log] crate's logging macros.
//!
//! An nginx module using ngx must run [`init`] on the main thread
//! in order for [::log] macros to log to the cycle logger.
//!
//! Logging from outside of the nginx main thread is not supported, because
//! Nginx does not provide any facilities for mutual exclusion of its logging
//! interfaces. If log is used from outside of the main thread, those will be
//! dropped, and the next use of log on main thread will attempt to log a
//! warning.
//!
//! ## Crate feature flags and logging levels
//!
//! The [::log] crate defines the logging levels, in ascending order, as:
//! `error`, `warn`, `info`, `debug`, and `trace`.
//!
//! Nginx defines logging levels as `ERROR`, `WARN`, `INFO`, and `DEBUG`. The
//! [::log] crate's `trace` level is mapped to Nginx's `DEBUG` level, and all
//! others are mapped according to their name.
//!
//! The maximum level logged is determined by this crate's feature flags:
//!
//! * `log` is a default feature. Iff nginx is configured `--with-debug`,
//!   `debug` is the maximum log level, otherwise `info` is the maximum level.
//! * `log-debug` implies `log`, and sets the maximum log level to `debug`,
//!   regardless of nginx's configuration.
//! * `log-trace` implies `log-debug`, and sets the maximum log level to
//!   `trace`, regardless of nginx's configuration.

use core::cell::Cell;
use core::ptr::NonNull;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::thread_local;

use crate::ffi::{ngx_log_t, ngx_uint_t, NGX_LOG_DEBUG_CORE};
use crate::log::{log_debug, log_error, ngx_cycle_log, write_fmt, DebugMask, LOG_BUFFER_SIZE};

static NGX_LOGGER: Logger = Logger;
static NGX_LOGGER_NONE_USED: AtomicBool = AtomicBool::new(false);
static NGX_LOGGER_NONE_REPORTED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static NGX_THREAD_LOGGER: Cell<Inner> = const { Cell::new(Inner::None) };
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Inner {
    None,
    Cycle,
    Specific(ngx_uint_t, NonNull<ngx_log_t>),
}

#[inline]
fn to_ngx_level(value: ::log::Level) -> ngx_uint_t {
    match value {
        ::log::Level::Error => nginx_sys::NGX_LOG_ERR as _,
        ::log::Level::Warn => nginx_sys::NGX_LOG_WARN as _,
        ::log::Level::Info => nginx_sys::NGX_LOG_INFO as _,
        ::log::Level::Debug => nginx_sys::NGX_LOG_DEBUG as _,
        ::log::Level::Trace => nginx_sys::NGX_LOG_DEBUG as _,
    }
}

/// Logger implementation for the [::log] facade
pub struct Logger;

pub(crate) struct LogScope(Inner);

impl Drop for LogScope {
    fn drop(&mut self) {
        NGX_THREAD_LOGGER.replace(self.0);
    }
}

/// Initializes nginx implementation for the [::log] facade.
pub fn init() {
    static INIT: OnceLock<&Logger> = OnceLock::new();

    INIT.get_or_init(|| {
        NGX_THREAD_LOGGER.set(Inner::Cycle);
        ::log::set_logger(&NGX_LOGGER).unwrap();
        if cfg!(feature = "log-trace") {
            ::log::set_max_level(::log::LevelFilter::Trace);
        } else if cfg!(ngx_feature = "debug") || cfg!(feature = "log-debug") {
            ::log::set_max_level(::log::LevelFilter::Debug);
        } else {
            ::log::set_max_level(::log::LevelFilter::Info);
        }
        &NGX_LOGGER
    });
}

impl Logger {
    pub(crate) fn enter<T>(target: DebugMask, log: NonNull<ngx_log_t>) -> LogScope
    where
        LogScope: From<T>,
    {
        init();
        let target: u32 = target.into();
        LogScope(NGX_THREAD_LOGGER.replace(Inner::Specific(target as ngx_uint_t, log)))
    }

    fn current(&self) -> Inner {
        NGX_THREAD_LOGGER.get()
    }
}

impl ::log::Log for Logger {
    fn enabled(&self, metadata: &::log::Metadata) -> bool {
        let (mask, log) = match self.current() {
            Inner::None => return false,
            Inner::Cycle => (NGX_LOG_DEBUG_CORE as _, ngx_cycle_log()),
            Inner::Specific(mask, ptr) => (mask, ptr),
        };

        let log_level = unsafe { log.as_ref().log_level };

        if metadata.level() < ::log::Level::Debug {
            to_ngx_level(metadata.level()) < log_level
        } else {
            log_level & mask != 0
        }
    }

    fn log(&self, record: &::log::Record) {
        if self.current() == Inner::None {
            NGX_LOGGER_NONE_USED.store(true, Ordering::Relaxed);
            return;
        }

        if !self.enabled(record.metadata()) {
            return;
        }

        let log = match self.current() {
            Inner::Cycle => ngx_cycle_log(),
            Inner::Specific(_, ptr) => ptr,
            Inner::None => unreachable!(),
        };

        let mut buf = [const { ::core::mem::MaybeUninit::<u8>::uninit() }; LOG_BUFFER_SIZE];
        let message = write_fmt(&mut buf, *record.args());

        if NGX_LOGGER_NONE_USED.load(Ordering::Relaxed)
            && !NGX_LOGGER_NONE_REPORTED.load(Ordering::Relaxed)
        {
            unsafe {
                log_error(
                    ::nginx_sys::NGX_LOG_WARN as _,
                    log.as_ptr(),
                    0,
                    "ngx::log::interop used off main thread, and messages were dropped".as_bytes(),
                )
            };
            NGX_LOGGER_NONE_REPORTED.store(true, Ordering::Relaxed);
        }

        if record.level() < ::log::Level::Debug {
            unsafe { log_error(to_ngx_level(record.level()), log.as_ptr(), 0, message) }
        } else {
            unsafe { log_debug(log.as_ptr(), 0, message) }
        }
    }

    fn flush(&self) {}
}

/// Runs a closure with [`::log`] output sent to a specific instance of the nginx logger.
#[inline(always)]
pub fn with_log<F, R>(target: DebugMask, log: NonNull<ngx_log_t>, func: F) -> R
where
    F: FnOnce() -> R,
{
    let _scope = Logger::enter(target, log);
    func()
}

#[cfg(feature = "async")]
mod async_ {
    use crate::log::{ngx_log_t, DebugMask};

    use core::future::Future;
    use core::pin::Pin;
    use core::ptr::NonNull;
    use core::task::{Context, Poll};

    /// Instrument a [`Future`] with [::log] output sent to a specific
    /// instance of the nginx logger.
    pub fn instrument_log<F>(target: DebugMask, log: NonNull<ngx_log_t>, fut: F) -> LogFut<F>
    where
        F: Future,
    {
        LogFut { target, log, fut }
    }
    pin_project_lite::pin_project! {
        /// Wrapper for a [`Future`] created by [`instrument_log`].
        pub struct LogFut<F> {
            target: DebugMask,
            log: NonNull<ngx_log_t>,
            #[pin]
            fut: F,
        }
    }
    impl<F: Future> Future for LogFut<F> {
        type Output = F::Output;
        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let target = self.target;
            let log = self.log;
            let this = self.project();
            super::with_log(target, log, || this.fut.poll(cx))
        }
    }
}
#[cfg(feature = "async")]
pub use self::async_::{instrument_log, LogFut};
