use core::fmt::Display;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use crate::http::{HttpModule, HttpPhase, HttpRequestHandler, IntoHandlerStatus, Request};
use crate::{async_ as ngx_async, ngx_log_debug_http};

use crate::ffi::{ngx_http_request_t, ngx_int_t, ngx_post_event, ngx_posted_events};

use futures_util::FutureExt;
use pin_project_lite::*;

/// An asynchronous HTTP request handler trait.
pub trait AsyncHandler {
    /// The phase in which the handler will be executed.
    const PHASE: HttpPhase;
    /// The associated HTTP module type.
    type Module: HttpModule;
    /// The return type of the asynchronous worker function.
    type Output: IntoHandlerStatus;
    /// The asynchronous worker function to be implemented.
    fn worker(request: &mut Request) -> impl Future<Output = Self::Output>;
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
                write!(f, "async handler: Context creation failed")
            }
            AsyncHandlerError::NoAsyncLauncher => {
                write!(f, "async handler: No async launcher available")
            }
            AsyncHandlerError::ContextDeletionFailed => {
                write!(f, "async handler: Context deletion failed")
            }
        }
    }
}

#[derive(Default)]
struct AsyncRequestContext {
    launcher: Option<async_task::Task<ngx_int_t>>,
}

impl<AH> HttpRequestHandler for AH
where
    AH: AsyncHandler + 'static,
{
    const PHASE: HttpPhase = async_phase(AH::PHASE);
    type Output = Result<ngx_int_t, AsyncHandlerError>;

    fn handler(request: &mut Request) -> Self::Output {
        let mut pool = request.pool();

        let ctx = pool
            .get_or_add_unique(|| {
                let request_ptr: *mut ngx_http_request_t = request.as_mut() as *mut _ as _;
                AsyncRequestContext {
                    launcher: Some(ngx_async::spawn(handler_future::<AH>(request_ptr))),
                }
            })
            .ok_or(AsyncHandlerError::ContextCreationFailed)?;

        match &ctx.launcher {
            None => Err(AsyncHandlerError::NoAsyncLauncher),
            Some(launcher) if launcher.is_finished() => {
                // task is finished, so both expect() should not panic
                let task = ctx
                    .launcher
                    .take()
                    .expect("async handler: task should be present");
                let rc = task
                    .now_or_never()
                    .expect("async handler: task should be ready");
                ngx_log_debug_http!(request, "async handler: task joined; rc = {}", rc);
                pool.remove_unique::<AsyncRequestContext>()
                    .ok_or(AsyncHandlerError::ContextDeletionFailed)?;
                Ok(rc)
            }
            Some(_) => {
                ngx_log_debug_http!(request, "async handler: running");
                Ok(nginx_sys::NGX_AGAIN as _)
            }
        }
    }
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
        AH::worker(request).await.into_handler_status(request)
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
                ngx_log_debug_http!(request, "handler future: pending");
                Poll::Pending
            }
            Poll::Ready(rc) => {
                unsafe {
                    ngx_post_event(
                        (*request.connection()).write,
                        core::ptr::addr_of_mut!(ngx_posted_events),
                    )
                };
                ngx_log_debug_http!(request, "handler future: ready");
                Poll::Ready(rc)
            }
        }
    }
}
