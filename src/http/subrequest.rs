use core::convert::Infallible;
use core::ffi::c_void;
use core::fmt::Display;
use core::ptr;

use nginx_sys::{ngx_http_post_subrequest_t, ngx_http_request_t, ngx_int_t, ngx_str_t, ngx_uint_t};

use crate::allocator::AllocError;
use crate::http::{IntoHandlerStatus, Request};
use crate::ngx_log_debug_http;

/// Default type for the subrequest initializer function
pub type SubRequestDefInit = fn(&mut Request) -> Result<(), Infallible>;
/// Default type for the subrequest post-completion handler function
pub type SubRequestDefHandler = fn(&mut Request, ngx_int_t) -> ngx_int_t;

/// A builder for creating and initiating HTTP subrequests.
///
/// `SubRequestBuilder` provides a fluent API for constructing nginx subrequests
/// ([`ngx_http_subrequest()`][nginx-dev-guide]). It handles URI and argument allocation from
/// the request pool, optional subrequest initialization, post-completion handlers, and
/// the various subrequest flags (`in_memory`, `waited`, `cloned`, `background`).
///
/// The builder is consumed by [`build`](Self::build), which creates the subrequest and
/// schedules it for processing. The caller typically returns `NGX_AGAIN` to suspend the
/// main request until the subrequest completes.
///
/// By default, the builder discards the parent request body in the subrequest (use
/// [`keep_body`](Self::keep_body) to preserve it) and initializes the subrequest's input
/// headers list with a capacity of 4 (use [`init_headers_in`](Self::init_headers_in) to
/// change or set to 0 to inherit the parent's headers without modification).
///
/// [nginx-dev-guide]: https://nginx.org/en/docs/dev/development_guide.html#http_subrequests
///
/// # Examples
///
/// A minimal subrequest with no handler:
///
/// ```no_run
/// # use ngx::http::subrequest::SubRequestBuilder;
/// # fn example(request: &mut ngx::http::Request) -> Result<(), Box<dyn std::error::Error>> {
/// SubRequestBuilder::new(request, "/proxy")?
///     .build()?;
/// # Ok(())
/// # }
/// ```
///
/// A fully configured subrequest with query arguments, an initializer that adds custom
/// headers, a post-subrequest handler, in-memory buffering, and the waited flag:
///
/// ```no_run
/// # use ngx::http::subrequest::SubRequestBuilder;
/// # use nginx_sys::ngx_int_t;
/// fn sr_handler(r: &mut ngx::http::Request, rc: ngx_int_t) -> ngx_int_t {
///     ngx::ngx_log_debug_http!(r, "subrequest completed with rc: {rc}");
///     rc
/// }
///
/// # fn example(request: &mut ngx::http::Request) -> Result<(), Box<dyn std::error::Error>> {
/// SubRequestBuilder::new(request, "/proxy")?
///     .args("arg1=val1&arg2=val2")?
///     .init(|sr| {
///         sr.add_header_in("X-SubRequest", "1").ok_or("cannot add header")
///     })
///     .handler(sr_handler)
///     .in_memory()
///     .waited()
///     .build()?;
/// # Ok(())
/// # }
/// ```
pub struct SubRequestBuilder<'r, I = SubRequestDefInit, H = SubRequestDefHandler> {
    request: &'r mut Request,
    uri: ngx_str_t,
    args: Option<ngx_str_t>,
    flags: ngx_uint_t,
    keep_body: bool,
    init_headers: ngx_uint_t,
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
    /// Indicates that the subrequest header initialization failed.
    HeaderInit,
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
            SubRequestError::HeaderInit => {
                write!(f, "subrequest: header initialization failed")
            }
            SubRequestError::Init(e) => {
                write!(f, "subrequest: initialization failed: {}", e)
            }
        }
    }
}

impl<E> core::error::Error for SubRequestError<E> where E: Display + core::fmt::Debug {}

impl<'r> SubRequestBuilder<'r> {
    /// Creates a new [`SubRequestBuilder`] with the specified URI.
    ///
    /// The URI string is copied into memory allocated from the request pool.
    /// Returns [`SubRequestError::Alloc`] if the pool allocation fails.
    pub fn new(request: &'r mut Request, uri: &str) -> Result<Self, SubRequestError> {
        let uri = unsafe { ngx_str_t::from_bytes(request.pool().as_ptr(), uri.as_bytes()) }
            .ok_or(SubRequestError::Alloc)?;
        Ok(Self {
            request,
            uri,
            args: None,
            flags: 0,
            keep_body: false,
            init_headers: 4,
            init: None,
            handler: None,
        })
    }
}

impl<'r, I, E, H, O> SubRequestBuilder<'r, I, H>
where
    I: FnOnce(&mut Request) -> Result<(), E>,
    H: FnOnce(&mut Request, ngx_int_t) -> O,
    O: IntoHandlerStatus,
{
    /// Sets the query string arguments for the subrequest.
    ///
    /// The arguments string (e.g. `"arg1=val1&arg2=val2"`) is copied into memory allocated
    /// from the request pool. Returns [`SubRequestError::Alloc`] if the allocation fails.
    pub fn args(mut self, args: &str) -> Result<Self, SubRequestError> {
        let args = unsafe { ngx_str_t::from_bytes(self.request.pool().as_ptr(), args.as_bytes()) }
            .ok_or(SubRequestError::Alloc)?;
        self.args = Some(args);
        Ok(self)
    }

    /// Sets an initializer function to modify the subrequest before it is initiated.
    ///
    /// The initializer runs after `ngx_http_subrequest()` creates the subrequest but before
    /// nginx begins processing it. Use this to set up headers, discard the request body,
    /// or perform other per-subrequest initialization.
    ///
    /// The function receives a mutable reference to the subrequest and must return
    /// `Result<(), E>`. If it returns an error, [`build`](Self::build) fails with
    /// [`SubRequestError::Init`].
    pub fn init<IT, ET>(self, init: IT) -> SubRequestBuilder<'r, IT, H>
    where
        IT: FnOnce(&mut Request) -> Result<(), ET>,
    {
        SubRequestBuilder::<IT, H> {
            request: self.request,
            uri: self.uri,
            args: self.args,
            flags: self.flags,
            keep_body: self.keep_body,
            init_headers: self.init_headers,
            init: Some(init),
            handler: self.handler,
        }
    }

    /// Sets a post-subrequest handler function.
    ///
    /// The handler is invoked by nginx when the subrequest completes. It receives a mutable
    /// reference to the subrequest and the completion result code (`ngx_int_t`). The handler
    /// is the place to inspect the subrequest response status and headers, read the buffered
    /// output (when combined with [`in_memory`](Self::in_memory)), and propagate results
    /// back to the main request via its module context.
    ///
    /// The handler is allocated from the request pool and wrapped in an
    /// `ngx_http_post_subrequest_t` callback. It is called exactly once.
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
            keep_body: self.keep_body,
            init_headers: self.init_headers,
            init: self.init,
            handler: Some(handler),
        }
    }

    /// Sets the subrequest to store its output in memory.
    ///
    /// When enabled, the response body is captured in the subrequest's `out` chain instead
    /// of being sent to the client connection. This is typically combined with a
    /// [post-subrequest handler](Self::handler) that reads the buffered response body.
    ///
    /// Corresponds to the `NGX_HTTP_SUBREQUEST_IN_MEMORY` flag.
    pub fn in_memory(mut self) -> Self {
        self.flags |= nginx_sys::NGX_HTTP_SUBREQUEST_IN_MEMORY as ngx_uint_t;
        self
    }

    /// Sets the subrequest to be waited.
    ///
    /// When enabled, the subrequest's `done` flag is set even if the subrequest is not
    /// active when it is finalized. This is typically combined with [`in_memory`](Self::in_memory)
    /// to ensure the main request resumes processing after the subrequest completes.
    ///
    /// Corresponds to the `NGX_HTTP_SUBREQUEST_WAITED` flag.
    pub fn waited(mut self) -> Self {
        self.flags |= nginx_sys::NGX_HTTP_SUBREQUEST_WAITED as ngx_uint_t;
        self
    }

    /// Sets the subrequest to be a clone of its parent.
    ///
    /// A cloned subrequest is started at the same location and proceeds from the same
    /// phase as the parent request, rather than being looked up by URI. This is useful
    /// when the subrequest must inherit the parent's location configuration.
    ///
    /// Corresponds to the `NGX_HTTP_SUBREQUEST_CLONE` flag.
    pub fn cloned(mut self) -> Self {
        self.flags |= nginx_sys::NGX_HTTP_SUBREQUEST_CLONE as ngx_uint_t;
        self
    }

    /// Sets the subrequest to run in the background.
    ///
    /// A background subrequest does not block any other subrequests or the main request,
    /// allowing them to proceed independently. However, the client connection is kept open
    /// until the background subrequest completes.
    ///
    /// Corresponds to the `NGX_HTTP_SUBREQUEST_BACKGROUND` flag.
    pub fn background(mut self) -> Self {
        self.flags |= nginx_sys::NGX_HTTP_SUBREQUEST_BACKGROUND as ngx_uint_t;
        self
    }

    /// Keep the request body in the subrequest.
    /// By default, the request body is discarded in the subrequest.
    pub fn keep_body(mut self) -> Self {
        self.keep_body = true;
        self
    }

    /// Sets the number of headers to initialize in the subrequest.
    /// By default, 4 headers are initialized, which is sufficient for most use cases.
    /// Setting this to 0 keeps all headers from the main request,
    /// but they cannot be modified in the subrequest.
    pub fn init_headers_in(mut self, count: ngx_uint_t) -> Self {
        self.init_headers = count;
        self
    }

    /// Builds and initiates the subrequest.
    ///
    /// This consumes the builder, allocates the post-subrequest callback (if a
    /// [`handler`](Self::handler) was set), calls `ngx_http_subrequest()` to create
    /// the subrequest, and then runs the [initializer](Self::init) (if set).
    ///
    /// # Errors
    ///
    /// Returns [`SubRequestError::Alloc`] if pool allocation for the handler or callback
    /// structure fails, [`SubRequestError::Create`] if `ngx_http_subrequest()` returns a
    /// non-`NGX_OK` status, or [`SubRequestError::Init`] if the initializer closure
    /// returns an error.
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

        if !self.keep_body {
            (unsafe { *sr_ptr }).request_body = ptr::null_mut();
        }

        let sr = unsafe { Request::from_ngx_http_request(sr_ptr) };

        if self.init_headers > 0 {
            sr.init_headers_in(self.init_headers).ok_or(SubRequestError::HeaderInit)?;
        }

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
