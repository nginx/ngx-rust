use core::convert::Infallible;
use core::ffi::c_void;
use core::fmt::Display;
use core::ptr;

use nginx_sys::{ngx_http_post_subrequest_t, ngx_http_request_t, ngx_int_t, ngx_str_t, ngx_uint_t};

use crate::allocator::AllocError;
use crate::http::{IntoHandlerStatus, Request};
use crate::ngx_log_debug_http;

/// A builder for creating subrequests.
pub struct SubRequestBuilder<
    'r,
    I = fn(&mut Request) -> Result<(), Infallible>,
    H = fn(&mut Request, ngx_int_t) -> ngx_int_t,
> {
    request: &'r mut Request,
    uri: ngx_str_t,
    args: Option<ngx_str_t>,
    flags: ngx_uint_t,
    init: Option<I>,
    handler: Option<H>,
}

/// An error type for subrequest operations.
#[derive(Debug)]
pub enum SubRequestError<E = Infallible> {
    /// Indicates that the subrequest allocation failed.
    Alloc,
    /// Indicates that the subrequest creation failed.
    Create,
    /// Indicates that the subrequest initialization failed.
    Init(E),
}

impl<E> From<AllocError> for SubRequestError<E> {
    fn from(_: AllocError) -> Self {
        Self::Alloc
    }
}

impl<E> Display for SubRequestError<E>
where
    E: Display,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SubRequestError::Alloc => {
                write!(f, "subrequest: allocation failed")
            }
            SubRequestError::Create => {
                write!(f, "subrequest: creation failed")
            }
            SubRequestError::Init(e) => {
                write!(f, "subrequest: initialization failed: {}", e)
            }
        }
    }
}

impl<'r> SubRequestBuilder<'r> {
    /// Creates a new `SubRequestBuilder` with the specified URI.
    /// The URI is allocated from the pool associated with the request.
    /// If the allocation fails, an error is returned.
    pub fn new(request: &'r mut Request, uri: &str) -> Result<Self, SubRequestError> {
        let uri = unsafe { ngx_str_t::from_bytes(request.pool().as_ptr(), uri.as_bytes()) }
            .ok_or(SubRequestError::Alloc)?;
        Ok(Self { request, uri, args: None, flags: 0, init: None, handler: None })
    }
}

impl<'r, I, E, H, O> SubRequestBuilder<'r, I, H>
where
    I: FnOnce(&mut Request) -> Result<(), E>,
    H: FnOnce(&mut Request, ngx_int_t) -> O,
    O: IntoHandlerStatus,
{
    /// Sets the arguments for the subrequest.
    pub fn args(mut self, args: &str) -> Result<Self, SubRequestError> {
        let args = unsafe { ngx_str_t::from_bytes(self.request.pool().as_ptr(), args.as_bytes()) }
            .ok_or(SubRequestError::Alloc)?;
        self.args = Some(args);
        Ok(self)
    }

    /// Sets an optional initializer function to change the subrequest
    /// created by `ngx_http_subrequest()` before it is initiated
    pub fn init<IT, ET>(self, init: IT) -> SubRequestBuilder<'r, IT, H>
    where
        IT: FnOnce(&mut Request) -> Result<(), ET>,
    {
        SubRequestBuilder::<IT, H> {
            request: self.request,
            uri: self.uri,
            args: self.args,
            flags: self.flags,
            init: Some(init),
            handler: self.handler,
        }
    }

    /// Sets an optional handler function for the subrequest.
    pub fn handler<HT, OT>(self, handler: HT) -> SubRequestBuilder<'r, I, HT>
    where
        HT: FnOnce(&mut Request, ngx_int_t) -> OT,
        OT: IntoHandlerStatus,
    {
        SubRequestBuilder::<I, HT> {
            request: self.request,
            uri: self.uri,
            args: self.args,
            flags: self.flags,
            init: self.init,
            handler: Some(handler),
        }
    }

    /// Sets the subrequest to be in-memory.
    /// The subrequest output is stored in memory instead of being written to the client
    /// connection.
    pub fn in_memory(mut self) -> Self {
        self.flags |= nginx_sys::NGX_HTTP_SUBREQUEST_IN_MEMORY as ngx_uint_t;
        self
    }

    /// Sets the subrequest to be waited.
    /// The subrequest's `done` flag is set even if the subrequest is not active
    /// when it is finalized.
    pub fn waited(mut self) -> Self {
        self.flags |= nginx_sys::NGX_HTTP_SUBREQUEST_WAITED as ngx_uint_t;
        self
    }

    /// Sets the subrequest to be a clone of its parent.
    /// The subrequest is started at the same location and proceeds from the same phase
    /// as the parent request.
    pub fn cloned(mut self) -> Self {
        self.flags |= nginx_sys::NGX_HTTP_SUBREQUEST_CLONE as ngx_uint_t;
        self
    }

    /// Sets the subrequest to be a background request.
    /// The subrequest does not block any other subrequests or the main request. Client connection
    /// may not close until the subrequest is completed.
    pub fn background(mut self) -> Self {
        self.flags |= nginx_sys::NGX_HTTP_SUBREQUEST_BACKGROUND as ngx_uint_t;
        self
    }

    /// Builds and initiates the subrequest.
    pub fn build(mut self) -> Result<(), SubRequestError<E>> {
        let sr_args_ptr = self.args.as_mut().map_or(ptr::null_mut(), ptr::from_mut);

        let psr_ptr: *mut ngx_http_post_subrequest_t = if self.handler.is_some() {
            let pool = self.request.pool();

            let ctx = unsafe { pool.allocate_with_cleanup(|| self.handler) }?;

            let psr = unsafe {
                pool.allocate_with_cleanup(|| ngx_http_post_subrequest_t {
                    handler: Some(sr_handler::<H, O>),
                    data: ctx.as_ptr() as _,
                })
            }?;
            psr.as_ptr() as _
        } else {
            ptr::null_mut()
        };

        let mut sr_ptr: *mut ngx_http_request_t = core::ptr::null_mut();

        let rc = unsafe {
            nginx_sys::ngx_http_subrequest(
                self.request.as_mut() as *mut _ as _,
                &raw mut self.uri,
                sr_args_ptr,
                &raw mut sr_ptr,
                psr_ptr,
                self.flags as ngx_uint_t,
            )
        };
        if rc != nginx_sys::NGX_OK as _ {
            return Err(SubRequestError::Create);
        }

        let sr = unsafe { Request::from_ngx_http_request(sr_ptr) };
        if let Some(init) = self.init {
            init(sr).map_err(SubRequestError::Init)?;
        }
        Ok(())
    }
}

extern "C" fn sr_handler<H, O>(
    r: *mut ngx_http_request_t,
    data: *mut c_void,
    rc: ngx_int_t,
) -> ngx_int_t
where
    H: FnOnce(&mut Request, ngx_int_t) -> O,
    O: IntoHandlerStatus,
{
    let request = unsafe { Request::from_ngx_http_request(r) };
    ngx_log_debug_http!(request, "subrequest handler called with rc: {rc}");
    // SAFETY: `data` is a pointer to an `Option<H>` that is valid as long as the main request
    // is not finalized, and the subrequest is always finalized before the main request.
    if let Some(handler) = unsafe { &mut *(data as *mut Option<H>) }.take() {
        (handler)(request, rc).into_handler_status(request)
    } else {
        rc
    }
}
