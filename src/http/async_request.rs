use core::ffi::c_void;
use core::fmt::Display;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};

use crate::core::Pool;
use crate::http::{HttpHandlerReturn, HttpModule, HttpPhase, HttpRequestHandler, Request};
use crate::log::ngx_cycle_log;
use crate::{async_ as ngx_async, ngx_log_debug_http, ngx_log_error};

use crate::ffi::{ngx_http_request_t, ngx_int_t, ngx_post_event, ngx_posted_events};

use alloc::string::String;
use allocator_api2::boxed::Box;
use nginx_sys::{ngx_http_post_subrequest_t, ngx_str_t, ngx_uint_t};
use pin_project_lite::*;

/// An asynchronous HTTP request handler trait.
pub trait AsyncHandler {
    /// The phase in which the handler will be executed.
    const PHASE: HttpPhase;
    /// The associated HTTP module type.
    type Module: HttpModule;
    /// The return type of the asynchronous worker function.
    type ReturnType: HttpHandlerReturn;
    /// The asynchronous worker function to be implemented.
    fn worker(request: &mut Request) -> impl Future<Output = Self::ReturnType>;
}

const fn async_phase(phase: HttpPhase) -> HttpPhase {
    assert!(
        !matches!(phase, HttpPhase::Content),
        "Content phase is not supported"
    );
    phase
}

/// An error type for asynchronous handler operations.
#[derive(Debug)]
pub enum AsyncHandlerError {
    /// Indicates that the context creation failed.
    ContextCreationFailed,
    /// Indicates that there is no async launcher available.
    NoAsyncLauncher,
    /// Indicates that the context deletion failed.
    ContextDeletionFailed,
}

impl Display for AsyncHandlerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AsyncHandlerError::ContextCreationFailed => {
                write!(f, "AsyncHandler: Context creation failed")
            }
            AsyncHandlerError::NoAsyncLauncher => {
                write!(f, "AsyncHandler: No async launcher available")
            }
            AsyncHandlerError::ContextDeletionFailed => {
                write!(f, "AsyncHandler: Context deletion failed")
            }
        }
    }
}

impl<AH> HttpRequestHandler for AH
where
    AH: AsyncHandler + 'static,
{
    const PHASE: HttpPhase = async_phase(AH::PHASE);
    type ReturnType = Result<ngx_int_t, AsyncHandlerError>;

    fn handler(request: &mut Request) -> Self::ReturnType {
        let mut pool = request.pool();
        let mut ctx = pool.get_unique_mut::<AsyncRequestContext>();
        #[allow(clippy::manual_inspect)]
        if ctx.is_none() {
            ctx = pool
                .allocate_unique(AsyncRequestContext::default())
                .map(|ctx| {
                    let request_ptr: *mut ngx_http_request_t = request.as_mut() as *mut _ as _;
                    ctx.launcher = Some(ngx_async::spawn(handler_future::<AH>(request_ptr)));
                    ctx
                })
        };

        let ctx = ctx.ok_or(AsyncHandlerError::ContextCreationFailed)?;

        if ctx.launcher.is_none() {
            Err(AsyncHandlerError::NoAsyncLauncher)
        } else if ctx.launcher.as_ref().unwrap().is_finished() {
            let rc = futures::executor::block_on(ctx.launcher.take().unwrap());
            ngx_log_debug_http!(request, "handler_wrapper: task joined; rc = {}", rc);
            pool.remove_unique::<AsyncRequestContext>()
                .ok_or(AsyncHandlerError::ContextDeletionFailed)?;
            Ok(rc)
        } else {
            ngx_log_debug_http!(request, "handler_wrapper: running");
            Ok(nginx_sys::NGX_AGAIN as _)
        }
    }
}

#[derive(Default)]
struct AsyncRequestContext {
    launcher: Option<async_task::Task<ngx_int_t>>,
}

pin_project! {
    struct HandlerFuture<Fut>
    where
        Fut: Future<Output = ngx_int_t>,
    {
        #[pin]
        worker_fut: Fut,
        request: *const ngx_http_request_t,
    }
}

fn handler_future<AH>(request: *mut ngx_http_request_t) -> impl Future<Output = ngx_int_t>
where
    AH: AsyncHandler,
{
    let fut = async move {
        let request = unsafe { Request::from_ngx_http_request(request) };
        AH::worker(request).await.into_ngx_int_t(request)
    };

    HandlerFuture::<_> {
        worker_fut: fut,
        request,
    }
}

impl<Fut> Future for HandlerFuture<Fut>
where
    Fut: Future<Output = ngx_int_t>,
{
    type Output = ngx_int_t;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let request = unsafe { Request::from_const_ngx_http_request(*this.request) };

        match this.worker_fut.poll(cx) {
            Poll::Pending => {
                ngx_log_debug_http!(request, "HandlerFuture: pending");
                Poll::Pending
            }
            Poll::Ready(rc) => {
                unsafe {
                    ngx_post_event(
                        (*request.connection()).write,
                        core::ptr::addr_of_mut!(ngx_posted_events),
                    )
                };
                ngx_log_debug_http!(request, "HandlerFuture: ready");
                Poll::Ready(rc)
            }
        }
    }
}

/// A builder for creating asynchronous subrequests.
#[derive(Default)]
pub struct AsyncSubRequestBuilder {
    uri: String,
    args: Option<String>,
    flags: ngx_uint_t,
}

/// An error type for asynchronous subrequest operations.
#[derive(Debug)]
pub enum AsyncSubRequestError {
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
}

impl Display for AsyncSubRequestError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AsyncSubRequestError::RequestAllocFailed => write!(f, "Subrequest allocation failed"),
            AsyncSubRequestError::PostRequestAllocFailed => {
                write!(f, "Post subrequest allocation failed")
            }
            AsyncSubRequestError::UriAllocFailed => write!(f, "URI allocation failed"),
            AsyncSubRequestError::ArgsAllocFailed => write!(f, "Arguments allocation failed"),
            AsyncSubRequestError::CreationFailed => write!(f, "Subrequest creation failed"),
        }
    }
}

impl AsyncSubRequestBuilder {
    /// Creates a new `AsyncSubRequestBuilder` with the specified URI.
    pub fn new<S: Into<String>>(uri: S) -> Self {
        Self {
            uri: uri.into(),
            ..Default::default()
        }
    }

    /// Sets the arguments for the subrequest.
    pub fn args<S: Into<String>>(mut self, args: S) -> Self {
        self.args = Some(args.into());
        self
    }

    /// Sets the subrequest to be in-memory.
    pub fn in_memory(mut self) -> Self {
        self.flags |= nginx_sys::NGX_HTTP_SUBREQUEST_IN_MEMORY as ngx_uint_t;
        self
    }

    /// Sets the subrequest to be waited.
    pub fn waited(mut self) -> Self {
        self.flags |= nginx_sys::NGX_HTTP_SUBREQUEST_WAITED as ngx_uint_t;
        self
    }

    /// Sets the subrequest to be a background request.
    pub fn background(mut self) -> Self {
        self.flags |= nginx_sys::NGX_HTTP_SUBREQUEST_BACKGROUND as ngx_uint_t;
        self
    }

    /// Builds and initiates the asynchronous subrequest.
    pub fn build<'r>(
        &self,
        request: &'r mut Request,
    ) -> Result<Pin<Box<AsyncSubRequest<'r>, Pool>>, AsyncSubRequestError> {
        let mut this = Box::<AsyncSubRequest, _>::try_new_in(Default::default(), request.pool())
            .map_err(|_| AsyncSubRequestError::RequestAllocFailed)?;

        let mut uri =
            unsafe { ngx_str_t::from_bytes(request.pool().as_ptr(), self.uri.as_bytes()) }
                .ok_or(AsyncSubRequestError::UriAllocFailed)?;

        let mut sr_args: ngx_str_t;
        let mut sr_args_ptr: *mut ngx_str_t = core::ptr::null_mut();

        if let Some(args) = &self.args {
            sr_args = unsafe { ngx_str_t::from_bytes(request.pool().as_ptr(), args.as_bytes()) }
                .ok_or(AsyncSubRequestError::ArgsAllocFailed)?;
            sr_args_ptr = &mut sr_args as *mut ngx_str_t;
        }

        let mut psr = Box::try_new_in(
            ngx_http_post_subrequest_t {
                handler: Some(AsyncSubRequest::sr_handler),
                data: core::ptr::null_mut(),
            },
            request.pool(),
        )
        .map_err(|_| AsyncSubRequestError::PostRequestAllocFailed)?;

        unsafe {
            let mut sr_ptr: *mut ngx_http_request_t = core::ptr::null_mut();
            let rc = nginx_sys::ngx_http_subrequest(
                request.as_mut() as *mut _ as _,
                &mut uri,
                sr_args_ptr,
                &mut sr_ptr,
                Box::as_mut_ptr(&mut psr),
                self.flags as ngx_uint_t,
            );

            if rc != nginx_sys::NGX_OK as _ {
                return Err(AsyncSubRequestError::CreationFailed);
            }

            this.sr = Some(Request::from_ngx_http_request(sr_ptr));
        }

        let this = Box::into_pin(this);

        psr.data = this.as_ref().get_ref() as *const _ as *mut c_void;

        Ok(this)
    }
}

/// An asynchronous subrequest structure.
#[derive(Default)]
pub struct AsyncSubRequest<'sr> {
    /// The subrequest reference.
    pub sr: Option<&'sr mut Request>,
    waker: Option<Waker>,
    rc: Option<ngx_int_t>,
}

impl<'sr> AsyncSubRequest<'sr> {
    extern "C" fn sr_handler(
        r: *mut ngx_http_request_t,
        data: *mut c_void,
        rc: ngx_int_t,
    ) -> ngx_int_t {
        let request = unsafe { Request::from_ngx_http_request(r) };
        ngx_log_debug_http!(request, "subrequest completed with rc = {}", rc);

        let this = unsafe { &mut *(data as *mut Self) };
        // ngx_log_debug_http!(request, "subrequest handler: at {:p} / {:p}", this, data);
        this.rc = Some(rc);
        if let Some(waker) = this.waker.take() {
            ngx_log_debug_http!(request, "subrequest completed; call waker");
            waker.wake();
        }
        rc
    }
}

impl<'sr> core::future::Future for AsyncSubRequest<'sr> {
    type Output = (ngx_int_t, Option<&'sr Request>);

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let this = self.get_mut();
        this.waker = Some(cx.waker().clone());

        if this.sr.is_none() {
            ngx_log_error!(
                nginx_sys::NGX_LOG_ERR,
                ngx_cycle_log().as_ptr(),
                "Subrequest not created"
            );
            return core::task::Poll::Ready((nginx_sys::NGX_ERROR as _, None));
        }

        if this.rc.is_none() {
            // ngx_log_debug_http!(request, "subrequest poll: pending because rc is none");
            return core::task::Poll::Pending;
        }

        // let request: &Request = unsafe { Request::from_ngx_http_request(this.sr.take().unwrap()) };
        let rc = this.rc.unwrap();
        // ngx_log_debug_http!(request, "subrequest poll: ready({rc})");
        core::task::Poll::Ready((rc, Some(this.sr.take().unwrap())))
    }
}
