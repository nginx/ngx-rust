use crate::bindings::ngx_err_t;

#[cfg(unix)]
pub use self::unix::*;
#[cfg(windows)]
pub use self::win32::*;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod win32;

/// Returns the error code of the last failed operation (`errno`).
#[inline]
pub fn ngx_errno() -> ngx_err_t {
    errno::errno().0 as _
}

/// Sets the error code (`errno`).
#[inline]
pub fn ngx_set_errno(err: ngx_err_t) {
    errno::set_errno(errno::Errno(err as _))
}

/// Returns the error code of the last failed sockets operation.
#[inline]
pub fn ngx_socket_errno() -> ngx_err_t {
    // This will use GetLastError() instead of WSAGetLastError() on Windows.
    // However, the functions are equivalent on any supported Windows version.
    errno::errno().0 as _
}

/// Sets the error code of the sockets operation.
#[inline]
pub fn ngx_set_socket_errno(err: ngx_err_t) {
    // This will use SetLastError() instead of WSASetLastError() on Windows.
    // However, the functions are equivalent on any supported Windows version.
    errno::set_errno(errno::Errno(err as _))
}
