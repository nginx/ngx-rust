use core::ffi::c_void;
use core::fmt::Display;
use core::ptr;

use alloc::string::{String, ToString};
use nginx_sys::{ngx_http_post_subrequest_t, ngx_http_request_t, ngx_int_t, ngx_str_t, ngx_uint_t};

use crate::{
    core::Pool,
    http::{IntoHandlerStatus, Request},
    ngx_log_debug_http,
};

#[cfg(feature = "async")]
pub use _async::*;

/// A builder for creating subrequests.
pub struct SubRequestBuilder {
    pool: Pool,
    uri: ngx_str_t,
    args: Option<ngx_str_t>,
    flags: ngx_uint_t,
}

/// An error type for subrequest operations.
#[derive(Debug)]
pub enum SubRequestError {
    /// Indicates that the subrequest allocation failed.
    RequestAllocFailed,
    /// Indicates that the post subrequest allocation failed.
    PostRequestAllocFailed,
    /// Indicates that the URI allocation failed.
    UriAllocFailed,
    /// Indicates that the arguments allocation failed.
    ArgsAllocFailed,
    /// Indicates that the subrequest creation failed.
    CreationFailed,
    /// Indicates that the subrequest modification failed.
    ModificationFailed(String),
}

impl Display for SubRequestError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SubRequestError::RequestAllocFailed => {
                write!(f, "subrequest: allocation failed")
            }
            SubRequestError::PostRequestAllocFailed => {
                write!(f, "subrequest: handler allocation failed")
            }
            SubRequestError::UriAllocFailed => {
                write!(f, "subrequest: URI allocation failed")
            }
            SubRequestError::ArgsAllocFailed => {
                write!(f, "subrequest: Arguments allocation failed")
            }
            SubRequestError::CreationFailed => {
                write!(f, "subrequest: creation failed")
            }
            SubRequestError::ModificationFailed(msg) => {
                write!(f, "subrequest: modification failed: {}", msg)
            }
        }
    }
}

impl SubRequestBuilder {
    /// Creates a new `SubRequestBuilder` with the specified URI.
    /// The URI is allocated from the provided pool. If the allocation fails, an error is returned.
    /// The Pool lifetime must be not shorter than the request which will be used
    /// to create the subrequest.
    pub fn new(pool: Pool, uri: &str) -> Result<Self, SubRequestError> {
        let uri = unsafe { ngx_str_t::from_bytes(pool.as_ptr(), uri.as_bytes()) }
            .ok_or(SubRequestError::UriAllocFailed)?;
        Ok(Self {
            pool,
            uri,
            args: None,
            flags: 0,
        })
    }

    /// Sets the arguments for the subrequest.
    pub fn args(mut self, args: &str) -> Result<Self, SubRequestError> {
        let args = unsafe { ngx_str_t::from_bytes(self.pool.as_ptr(), args.as_bytes()) }
            .ok_or(SubRequestError::ArgsAllocFailed)?;
        self.args = Some(args);
        Ok(self)
    }

    /// Sets the subrequest to be in-memory.
    pub fn in_memory(mut self) -> Self {
        self.flags |= nginx_sys::NGX_HTTP_SUBREQUEST_IN_MEMORY as ngx_uint_t;
        self
    }

    /// Sets the subrequest to be waited.
    /// It is supposed to provide some handler to handle the subrequest completion,
    /// otherwise it will be waited without any processing
    /// (see [`SubRequestBuilder::build_ext`] for details).
    pub fn waited(mut self) -> Self {
        self.flags |= nginx_sys::NGX_HTTP_SUBREQUEST_WAITED as ngx_uint_t;
        self
    }

    /// Sets the subrequest to be a background request.
    pub fn background(mut self) -> Self {
        self.flags |= nginx_sys::NGX_HTTP_SUBREQUEST_BACKGROUND as ngx_uint_t;
        self
    }

    /// Builds and initiates the subrequest.
    /// This method allows for an optional modifier function to modify the subrequest
    /// created by `ngx_http_subrequest()` before it is initiated,
    /// and an optional handler function to handle the subrequest's completion.
    pub fn build_ext<M, E, H, O>(
        mut self,
        request: &mut Request,
        modifier: Option<M>,
        handler: Option<H>,
    ) -> Result<(), SubRequestError>
    where
        M: FnOnce(&mut Request) -> Result<(), E>,
        E: Display,
        H: FnOnce(&mut Request, ngx_int_t) -> O,
        O: IntoHandlerStatus,
    {
        let sr_args_ptr = self
            .args
            .as_mut()
            .map_or(ptr::null_mut(), |args| args as *mut ngx_str_t);

        let psr_ptr: *mut ngx_http_post_subrequest_t = if handler.is_some() {
            let ctx = unsafe {
                self.pool
                    .allocate_with_cleanup(|| handler)
                    .ok_or(SubRequestError::RequestAllocFailed)
            }?;

            let psr = unsafe {
                self.pool
                    .allocate_with_cleanup(|| ngx_http_post_subrequest_t {
                        handler: Some(sr_handler::<H, O>),
                        data: ctx.as_ptr() as _,
                    })
                    .ok_or(SubRequestError::PostRequestAllocFailed)
            }?;
            psr.as_ptr() as _
        } else {
            ptr::null_mut()
        };

        let mut sr_ptr: *mut ngx_http_request_t = core::ptr::null_mut();

        let rc = unsafe {
            nginx_sys::ngx_http_subrequest(
                request.as_mut() as *mut _ as _,
                &raw mut self.uri,
                sr_args_ptr,
                &raw mut sr_ptr,
                psr_ptr,
                self.flags as ngx_uint_t,
            )
        };
        if rc != nginx_sys::NGX_OK as _ {
            return Err(SubRequestError::CreationFailed);
        }

        if let Some(modifier) = modifier {
            let sr = unsafe { Request::from_ngx_http_request(sr_ptr) };
            modifier(sr).map_err(|e| SubRequestError::ModificationFailed(e.to_string()))
        } else {
            Ok(())
        }
    }

    /// Builds and initiates the subrequest.
    /// This is a simplified version of `build_ext` that requires a handler
    /// and does not allow for subrequest modification.
    pub fn build<H, O>(self, request: &mut Request, handler: H) -> Result<(), SubRequestError>
    where
        H: FnOnce(&mut Request, ngx_int_t) -> O,
        O: IntoHandlerStatus,
    {
        self.build_ext(request, SR_MODIFIER_NO_OP, Some(handler))
    }
}

type SimpleSubRequestModifier = Option<fn(&mut Request) -> Result<(), core::convert::Infallible>>;
type SimpleSubRequestHandler = Option<fn(&mut Request, ngx_int_t) -> ngx_int_t>;
/// A no-op modifier function for subrequests.
pub const SR_MODIFIER_NO_OP: SimpleSubRequestModifier = None;
/// A no-op handler function for subrequests.
pub const SR_HANDLER_NO_OP: SimpleSubRequestHandler = None;

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
    if let Some(handler) = unsafe { &mut *(data as *mut Option<H>) }.take() {
        (handler)(request, rc).into_handler_status(request)
    } else {
        rc
    }
}

#[cfg(feature = "async")]
impl SubRequestBuilder {
    /// Builds and runs the subrequest asynchronously.
    pub async fn build_async<'r, M, E, AH>(
        mut self,
        request: &'r mut Request,
        modifier: Option<M>,
        handler: AH,
    ) -> Result<ngx_int_t, SubRequestError>
    where
        M: FnOnce(&mut Request) -> Result<(), E>,
        E: Display,
        AH: FnMut(&'r mut Request, ngx_int_t) -> ngx_int_t + Unpin,
    {
        let sr_args_ptr = self
            .args
            .as_mut()
            .map_or(ptr::null_mut(), |args| args as *mut ngx_str_t);

        let mut ctx = core::pin::pin!(AsyncSubRequest::<AH>::new(handler));

        let mut psr = core::pin::pin!(ngx_http_post_subrequest_t {
            handler: Some(AsyncSubRequest::<AH>::sr_handler),
            data: ctx.as_mut().get_mut() as *mut _ as _,
        });

        let mut sr_ptr: *mut ngx_http_request_t = core::ptr::null_mut();

        let rc = unsafe {
            nginx_sys::ngx_http_subrequest(
                request.as_mut() as *mut _ as _,
                &raw mut self.uri,
                sr_args_ptr,
                &raw mut sr_ptr,
                psr.as_mut().get_mut() as _,
                self.flags as ngx_uint_t,
            )
        };
        if rc != nginx_sys::NGX_OK as _ {
            return Err(SubRequestError::CreationFailed);
        }

        if let Some(modifier) = modifier {
            let sr = unsafe { Request::from_ngx_http_request(sr_ptr) };
            modifier(sr).map_err(|e| SubRequestError::ModificationFailed(e.to_string()))?;
        }

        ctx.await
    }
}

#[cfg(feature = "async")]
mod _async {

    use futures_util::task::noop_waker;

    use super::*;

    use core::pin::Pin;
    use core::task::Waker;

    use crate::ngx_log_debug_http;

    /// An asynchronous subrequest structure.
    pub struct AsyncSubRequest<'sr, H>
    where
        H: FnMut(&'sr mut Request, ngx_int_t) -> ngx_int_t + Unpin,
    {
        _phantom: core::marker::PhantomData<&'sr ()>,
        handler: Option<H>,
        waker: Waker,
        rc: Option<ngx_int_t>,
    }

    impl<'sr, H> AsyncSubRequest<'sr, H>
    where
        H: FnMut(&'sr mut Request, ngx_int_t) -> ngx_int_t + Unpin,
    {
        pub(super) fn new(handler: H) -> Self {
            Self {
                _phantom: core::marker::PhantomData,
                handler: Some(handler),
                waker: noop_waker(),
                rc: None,
            }
        }

        pub(super) extern "C" fn sr_handler(
            r: *mut ngx_http_request_t,
            data: *mut c_void,
            mut rc: ngx_int_t,
        ) -> ngx_int_t {
            let request = unsafe { Request::from_ngx_http_request(r) };
            ngx_log_debug_http!(
                request,
                "async subrequest done rc:{} s:{}",
                rc,
                request.get_status().0
            );

            let this = unsafe { &mut *(data as *mut Self) };
            this.rc = Some(rc);
            if let Some(mut handler) = this.handler.take() {
                rc = handler(request, rc);
            }
            this.waker.wake_by_ref();
            rc
        }
    }

    impl<'sr, H> core::future::Future for AsyncSubRequest<'sr, H>
    where
        H: FnMut(&'sr mut Request, ngx_int_t) -> ngx_int_t + Unpin,
    {
        type Output = Result<ngx_int_t, SubRequestError>;

        fn poll(
            mut self: Pin<&mut Self>,
            cx: &mut core::task::Context<'_>,
        ) -> core::task::Poll<Self::Output> {
            match self.rc {
                None => {
                    self.waker.clone_from(cx.waker());
                    core::task::Poll::Pending
                }
                Some(rc) => core::task::Poll::Ready(Ok(rc)),
            }
        }
    }
}
