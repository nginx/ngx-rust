use core::fmt::Display;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use crate::core::type_storage::*;
use crate::http::{HttpHandlerReturn, HttpModule, HttpPhase, HttpRequestHandler, Request};
use crate::{async_ as ngx_async, ngx_log_debug_http};

use crate::ffi::{ngx_http_request_t, ngx_int_t, ngx_post_event, ngx_posted_events};

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
        let mut ctx = <AsyncRequestContext as TypeStorage>::get_mut(&mut pool);
        #[allow(clippy::manual_inspect)]
        if ctx.is_none() {
            ctx =
                <AsyncRequestContext as TypeStorage>::add(AsyncRequestContext::default(), &mut pool)
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
            <AsyncRequestContext as TypeStorage>::delete(&pool);
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
