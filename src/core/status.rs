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
