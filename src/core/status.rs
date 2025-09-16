use core::ffi::c_char;
use core::fmt;
use core::ptr;

use crate::ffi::*;

/// Status
///
/// Rust native wrapper for NGINX status codes.
#[derive(Ord, PartialOrd, Eq, PartialEq)]
pub struct Status(pub ngx_int_t);

impl Status {
    /// Is this Status equivalent to NGX_OK?
    pub fn is_ok(&self) -> bool {
        self == &Status::NGX_OK
    }
}

impl fmt::Debug for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl From<Status> for ngx_int_t {
    fn from(val: Status) -> Self {
        val.0
    }
}

macro_rules! ngx_codes {
    (
        $(
            $(#[$docs:meta])*
            ($konst:ident);
        )+
    ) => {
        impl Status {
        $(
            $(#[$docs])*
            pub const $konst: Status = Status($konst as ngx_int_t);
        )+

        }
    }
}

ngx_codes! {
    /// NGX_OK - Operation succeeded.
    (NGX_OK);
    /// NGX_ERROR - Operation failed.
    (NGX_ERROR);
    /// NGX_AGAIN - Operation incomplete; call the function again.
    (NGX_AGAIN);
    /// NGX_BUSY - Resource is not available.
    (NGX_BUSY);
    /// NGX_DONE - Operation complete or continued elsewhere. Also used as an alternative success code.
    (NGX_DONE);
    /// NGX_DECLINED - Operation rejected, for example, because it is disabled in the configuration.
    /// This is never an error.
    (NGX_DECLINED);
    /// NGX_ABORT - Function was aborted. Also used as an alternative error code.
    (NGX_ABORT);
}

/// An error occurred while parsing and validating configuration.
pub const NGX_CONF_ERROR: *mut c_char = ptr::null_mut::<c_char>().wrapping_offset(-1);
/// Configuration handler succeeded.
pub const NGX_CONF_OK: *mut c_char = ptr::null_mut();

/// Converts an NGINX status code to an Option type.
pub fn ngx_make_opt(code: ngx_int_t) -> Option<ngx_int_t> {
    if code != NGX_ERROR as ngx_int_t {
        Some(code)
    } else {
        None
    }
}

/// NGX_OK - Operation succeeded.
pub const NGX_O_OK: Option<ngx_int_t> = Some(NGX_OK as ngx_int_t);
/// NGX_ERROR - Operation failed.
pub const NGX_O_ERROR: Option<ngx_int_t> = None;
/// NGX_AGAIN - Operation incomplete; call the function again.
pub const NGX_O_AGAIN: Option<ngx_int_t> = Some(NGX_AGAIN as ngx_int_t);
/// NGX_BUSY - Resource is not available.
pub const NGX_O_BUSY: Option<ngx_int_t> = Some(NGX_BUSY as ngx_int_t);
/// NGX_DONE - Operation complete or continued elsewhere. Also used as an alternative success code.
pub const NGX_O_DONE: Option<ngx_int_t> = Some(NGX_DONE as ngx_int_t);
/// NGX_DECLINED - Operation rejected, for example, because it is disabled in the configuration.
/// This is never an error.
pub const NGX_O_DECLINED: Option<ngx_int_t> = Some(NGX_DECLINED as ngx_int_t);
/// NGX_ABORT - Function was aborted. Also used as an alternative error code.
pub const NGX_O_ABORT: Option<ngx_int_t> = Some(NGX_ABORT as ngx_int_t);
