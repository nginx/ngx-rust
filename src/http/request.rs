use core::error;
use core::ffi::c_void;
use core::fmt;
use core::ptr::NonNull;
use core::slice;
use core::str::FromStr;

use crate::core::*;
use crate::ffi::*;
use crate::http::HttpPhase;
use crate::http::status::*;

/// Define a static request handler.
///
/// Handlers are expected to take a single [`Request`] argument and return a [`Status`].
#[macro_export]
macro_rules! http_request_handler {
    ( $name: ident, $handler: expr ) => {
        extern "C" fn $name(r: *mut $crate::ffi::ngx_http_request_t) -> $crate::ffi::ngx_int_t {
            let request = unsafe { $crate::http::Request::from_ngx_http_request(r) };
            let status: $crate::core::Status = $handler(request);
            status.0
        }
    };
}

/// Define a static post subrequest handler.
///
/// Handlers are expected to take a single [`Request`] argument and return a [`Status`].
#[macro_export]
macro_rules! http_subrequest_handler {
    ( $name: ident, $handler: expr ) => {
        unsafe extern "C" fn $name(
            r: *mut $crate::ffi::ngx_http_request_t,
            data: *mut ::core::ffi::c_void,
            rc: $crate::ffi::ngx_int_t,
        ) -> $crate::ffi::ngx_int_t {
            $handler(r, data, rc)
        }
    };
}

/// Define a static variable setter.
///
/// The set handler allows setting the property referenced by the variable.
/// The set handler expects a [`Request`], [`mut ngx_variable_value_t`], and a [`usize`].
/// Variables: <https://nginx.org/en/docs/dev/development_guide.html#http_variables>
#[macro_export]
macro_rules! http_variable_set {
    ( $name: ident, $handler: expr ) => {
        unsafe extern "C" fn $name(
            r: *mut $crate::ffi::ngx_http_request_t,
            v: *mut $crate::ffi::ngx_variable_value_t,
            data: usize,
        ) {
            let request = unsafe { $crate::http::Request::from_ngx_http_request(r) };
            $handler(request, v, data);
        }
    };
}

/// Define a static variable evaluator.
///
/// The get handler is responsible for evaluating a variable in the context of a specific request.
/// Variable evaluators accept a [`Request`] input argument and two output
/// arguments: [`ngx_variable_value_t`] and [`usize`].
/// Variables: <https://nginx.org/en/docs/dev/development_guide.html#http_variables>
#[macro_export]
macro_rules! http_variable_get {
    ( $name: ident, $handler: expr ) => {
        unsafe extern "C" fn $name(
            r: *mut $crate::ffi::ngx_http_request_t,
            v: *mut $crate::ffi::ngx_variable_value_t,
            data: usize,
        ) -> $crate::ffi::ngx_int_t {
            let request = unsafe { $crate::http::Request::from_ngx_http_request(r) };
            let status: $crate::core::Status = $handler(request, v, data);
            status.0
        }
    };
}

/// Trait for converting handler return types into `ngx_int_t`.
/// Any desired error handling / logging logic can be implemented
/// in the `into_handler_status` method.
///
/// There are predefined implementations for `ngx_int_t`, [`Status`], [`HTTPStatus`],
/// [`Option`] with value type implementing [`IntoHandlerStatus`].
pub trait IntoHandlerStatus
where
    Self: Sized,
{
    /// Convert the handler return type into an `ngx_int_t`.
    fn into_handler_status(self, _r: &Request) -> ngx_int_t;
}

impl<T> IntoHandlerStatus for Option<T>
where
    T: IntoHandlerStatus,
{
    #[inline]
    fn into_handler_status(self, r: &Request) -> ngx_int_t {
        self.map(|val| val.into_handler_status(r)).unwrap_or(NGX_ERROR as _)
    }
}

impl IntoHandlerStatus for ngx_int_t {
    #[inline]
    fn into_handler_status(self, _r: &Request) -> ngx_int_t {
        self
    }
}

impl IntoHandlerStatus for Status {
    #[inline]
    fn into_handler_status(self, _r: &Request) -> ngx_int_t {
        self.0
    }
}

impl IntoHandlerStatus for HTTPStatus {
    #[inline]
    fn into_handler_status(self, _r: &Request) -> ngx_int_t {
        self.0 as _
    }
}

/// Trait for static request handler.
pub trait HttpRequestHandler {
    /// The phase in which the handler is invoked.
    const PHASE: HttpPhase;
    /// The return type of the handler.
    type Output: IntoHandlerStatus;
    /// The handler function.
    fn handler(request: &mut Request) -> Self::Output;
    /// Handler name for logging purposes.
    /// [`core::any::type_name`] is used by default.
    fn name() -> &'static str {
        core::any::type_name::<Self>()
    }
}

/// The C-compatible handler wrapper function.
///
/// # Safety
///
/// The caller has provided a valid non-null pointer to an [`ngx_http_request_t`].
pub(crate) unsafe extern "C" fn raw_handler<H>(r: *mut ngx_http_request_t) -> ngx_int_t
where
    H: HttpRequestHandler,
{
    let r = unsafe { Request::from_ngx_http_request(r) };
    H::handler(r).into_handler_status(r)
}

/// Wrapper struct for an [`ngx_http_request_t`] pointer, providing methods for working with HTTP
/// requests.
///
/// See <https://nginx.org/en/docs/dev/development_guide.html#http_request>
#[repr(transparent)]
pub struct Request(ngx_http_request_t);

impl<'a> From<&'a Request> for *const ngx_http_request_t {
    fn from(request: &'a Request) -> Self {
        &raw const request.0
    }
}

impl<'a> From<&'a mut Request> for *mut ngx_http_request_t {
    fn from(request: &'a mut Request) -> Self {
        &raw mut request.0
    }
}

impl AsRef<ngx_http_request_t> for Request {
    fn as_ref(&self) -> &ngx_http_request_t {
        &self.0
    }
}

impl AsMut<ngx_http_request_t> for Request {
    fn as_mut(&mut self) -> &mut ngx_http_request_t {
        &mut self.0
    }
}

impl Request {
    /// Create a [`Request`] from an [`ngx_http_request_t`].
    ///
    /// # Safety
    ///
    /// The caller has provided a valid non-null pointer to a valid `ngx_http_request_t`
    /// which shares the same representation as `Request`.
    pub unsafe fn from_ngx_http_request<'a>(r: *mut ngx_http_request_t) -> &'a mut Request {
        unsafe { &mut *r.cast::<Request>() }
    }

    /// Is this the main request (as opposed to a subrequest)?
    pub fn is_main(&self) -> bool {
        let main = self.0.main.cast();
        core::ptr::eq(self, main)
    }

    /// Request pool.
    pub fn pool(&self) -> Pool {
        // SAFETY: This request is allocated from `pool`, thus must be a valid pool.
        unsafe { Pool::from_ngx_pool(self.0.pool) }
    }

    /// Returns the result as an `Option` if it exists, otherwise `None`.
    ///
    /// The option wraps an ngx_http_upstream_t instance, it will be none when the underlying NGINX
    /// request does not have a pointer to a [`ngx_http_upstream_t`] upstream structure.
    ///
    /// [`ngx_http_upstream_t`] is best described in
    /// <https://nginx.org/en/docs/dev/development_guide.html#http_load_balancing>
    pub fn upstream(&self) -> Option<*mut ngx_http_upstream_t> {
        if self.0.upstream.is_null() {
            return None;
        }
        Some(self.0.upstream)
    }

    /// Cache lookup outcome for this request, mirroring
    /// `$upstream_cache_status`.  `None` when the request did not
    /// consult any cache (no `proxy_cache` / `fastcgi_cache` /
    /// configured, or the request was processed before nginx
    /// classified it).
    #[cfg(ngx_feature = "http_cache")]
    pub fn cache_status(&self) -> Option<crate::http::CacheStatus> {
        if self.0.upstream.is_null() {
            return None;
        }
        // SAFETY: `upstream` is non-null per the check above and is
        // populated by nginx core for the lifetime of the request.
        let status = unsafe { (*self.0.upstream).cache_status() };
        crate::http::CacheStatus::from_raw(status as ngx_uint_t)
    }

    /// Name of the `proxy_cache_path` / `fastcgi_cache_path`
    /// keys-zone consulted for this request, or `None` when no
    /// cache lookup happened.
    ///
    /// Useful as the `zone=` label for cache metrics: `nginx_vts`,
    /// `vts`, statsd exporters, etc.
    #[cfg(ngx_feature = "http_cache")]
    pub fn cache_zone_name(&self) -> Option<&NgxStr> {
        if self.0.cache.is_null() {
            return None;
        }
        // SAFETY: `cache` is non-null per the check above; the
        // chained pointers are populated by `ngx_http_file_cache_init`
        // in the master before workers fork and remain valid for the
        // lifetime of the process.
        unsafe {
            let file_cache = (*self.0.cache).file_cache;
            if file_cache.is_null() {
                return None;
            }
            let shm_zone = (*file_cache).shm_zone;
            if shm_zone.is_null() {
                return None;
            }
            Some(NgxStr::from_ngx_str((*shm_zone).shm.name))
        }
    }

    /// The `max_size` of the cache consulted for this request, in bytes,
    /// or `None` when no cache lookup happened.
    ///
    /// nginx does not keep this figure in bytes:
    /// `ngx_http_file_cache_init` divides the configured size by the
    /// filesystem block size, so that it can be compared against the
    /// running total directly.  Reading `max_size` off the struct
    /// therefore gives a number some thousands of times too small,
    /// with nothing to suggest anything is wrong.  Both this and
    /// [`Request::cache_zone_used_size`] undo that, and
    /// [`Request::cache_zone_block_size`] is there for callers that
    /// want the raw unit.
    #[cfg(ngx_feature = "http_cache")]
    pub fn cache_zone_max_size(&self) -> Option<u64> {
        let file_cache = self.file_cache()?;
        // SAFETY: an initialized file cache holds both fields.
        Some(unsafe { (*file_cache).max_size as u64 * (*file_cache).bsize as u64 })
    }

    /// What the cache consulted for this request currently holds on
    /// disk, in bytes, or `None` when no cache lookup happened.
    ///
    /// Kept in filesystem blocks for the reason given on
    /// [`Request::cache_zone_max_size`], and converted here for the
    /// same reason.
    #[cfg(ngx_feature = "http_cache")]
    pub fn cache_zone_used_size(&self) -> Option<u64> {
        let file_cache = self.file_cache()?;
        // SAFETY: an initialized file cache holds its shared state,
        // which the cache manager keeps up to date.
        unsafe {
            let sh = (*file_cache).sh;
            if sh.is_null() {
                return None;
            }
            Some((*sh).size as u64 * (*file_cache).bsize as u64)
        }
    }

    /// The filesystem block size the cache accounts in, or `None` when
    /// no cache lookup happened.
    #[cfg(ngx_feature = "http_cache")]
    pub fn cache_zone_block_size(&self) -> Option<usize> {
        let file_cache = self.file_cache()?;
        // SAFETY: an initialized file cache holds the field.
        Some(unsafe { (*file_cache).bsize })
    }

    /// The file cache behind this request, if it consulted one.
    #[cfg(ngx_feature = "http_cache")]
    fn file_cache(&self) -> Option<*mut crate::ffi::ngx_http_file_cache_t> {
        if self.0.cache.is_null() {
            return None;
        }
        // SAFETY: `cache` is non-null per the check above; the pointer
        // it holds is populated by `ngx_http_file_cache_init` in the
        // master before workers fork.
        let file_cache = unsafe { (*self.0.cache).file_cache };
        if file_cache.is_null() {
            return None;
        }
        Some(file_cache)
    }

    /// Pointer to a [`ngx_connection_t`] client connection object.
    ///
    /// [`ngx_connection_t`]: https://nginx.org/en/docs/dev/development_guide.html#connection
    pub fn connection(&self) -> *mut ngx_connection_t {
        self.0.connection
    }

    /// Pointer to a [`ngx_log_t`].
    ///
    /// [`ngx_log_t`]: https://nginx.org/en/docs/dev/development_guide.html#logging
    pub fn log(&self) -> *mut ngx_log_t {
        unsafe { (*self.connection()).log }
    }

    /// Get Module context pointer
    fn get_module_ctx_ptr(&self, module: &ngx_module_t) -> *mut c_void {
        unsafe { *self.0.ctx.add(module.ctx_index) }
    }

    /// Get Module context
    pub fn get_module_ctx<T>(&self, module: &ngx_module_t) -> Option<&T> {
        let ctx = self.get_module_ctx_ptr(module).cast::<T>();
        // SAFETY: ctx is either NULL or allocated with ngx_p(c)alloc and
        // explicitly initialized by the module
        unsafe { ctx.as_ref() }
    }

    /// Sets the value as the module's context.
    ///
    /// See <https://nginx.org/en/docs/dev/development_guide.html#http_request>
    pub fn set_module_ctx(&self, value: *mut c_void, module: &ngx_module_t) {
        unsafe {
            *self.0.ctx.add(module.ctx_index) = value;
        };
    }

    /// Get the value of a [complex value].
    ///
    /// [complex value]: https://nginx.org/en/docs/dev/development_guide.html#http_complex_values
    pub fn get_complex_value(&self, cv: &ngx_http_complex_value_t) -> Option<&NgxStr> {
        let r = (self as *const Request as *mut Request).cast();
        let val = cv as *const ngx_http_complex_value_t as *mut ngx_http_complex_value_t;
        // SAFETY: `ngx_http_complex_value` does not mutate `r` or `val` and guarentees that
        // a valid Nginx string is stored in `value` if it successfully returns.
        unsafe {
            let mut value = ngx_str_t::default();
            if ngx_http_complex_value(r, val, &raw mut value) != NGX_OK as ngx_int_t {
                return None;
            }
            Some(NgxStr::from_ngx_str(value))
        }
    }

    /// Discard (read and ignore) the [request body].
    ///
    /// [request body]: https://nginx.org/en/docs/dev/development_guide.html#http_request_body
    pub fn discard_request_body(&mut self) -> Status {
        unsafe { Status(ngx_http_discard_request_body(&raw mut self.0)) }
    }

    /// Client HTTP [User-Agent].
    ///
    /// [User-Agent]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/User-Agent
    pub fn user_agent(&self) -> Option<&NgxStr> {
        if !self.0.headers_in.user_agent.is_null() {
            unsafe { Some(NgxStr::from_ngx_str((*self.0.headers_in.user_agent).value)) }
        } else {
            None
        }
    }

    /// Set HTTP status of response.
    pub fn set_status(&mut self, status: HTTPStatus) {
        self.0.headers_out.status = status.into();
    }

    /// Add header to the `headers_in` object.
    ///
    /// See <https://nginx.org/en/docs/dev/development_guide.html#http_request>
    pub fn add_header_in(&mut self, key: &str, value: &str) -> Option<()> {
        let table: *mut ngx_table_elt_t =
            unsafe { ngx_list_push(&raw mut self.0.headers_in.headers).cast() };
        unsafe { add_to_ngx_table(table, self.0.pool, key, value) }
    }

    /// Add header to the `headers_out` object.
    ///
    /// See <https://nginx.org/en/docs/dev/development_guide.html#http_request>
    pub fn add_header_out(&mut self, key: &str, value: &str) -> Option<()> {
        let table: *mut ngx_table_elt_t =
            unsafe { ngx_list_push(&raw mut self.0.headers_out.headers).cast() };
        unsafe { add_to_ngx_table(table, self.0.pool, key, value) }
    }

    /// Set response body [Content-Length].
    ///
    /// [Content-Length]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Content-Length
    pub fn set_content_length_n(&mut self, n: usize) {
        self.0.headers_out.content_length_n = n as off_t;
    }

    /// Send the output header.
    ///
    /// Do not call this function until all output headers are set.
    pub fn send_header(&mut self) -> Status {
        unsafe { Status(ngx_http_send_header(&raw mut self.0)) }
    }

    /// Flag indicating that the output does not require a body.
    ///
    /// For example, this flag is used by `HTTP HEAD` requests.
    pub fn header_only(&self) -> bool {
        self.0.header_only() != 0
    }

    /// request method
    pub fn method(&self) -> Method {
        Method::from_ngx(self.0.method)
    }

    /// path part of request only
    pub fn path(&self) -> &NgxStr {
        unsafe { NgxStr::from_ngx_str(self.0.uri) }
    }

    /// full uri - containing path and args
    pub fn unparsed_uri(&self) -> &NgxStr {
        unsafe { NgxStr::from_ngx_str(self.0.unparsed_uri) }
    }

    /// Send the [response body].
    ///
    /// This function can be called multiple times.
    /// Set the `last_buf` flag in the last body buffer.
    ///
    /// [response body]: https://nginx.org/en/docs/dev/development_guide.html#http_request_body
    pub fn output_filter(&mut self, body: &mut ngx_chain_t) -> Status {
        unsafe { Status(ngx_http_output_filter(&raw mut self.0, body)) }
    }

    /// Perform internal redirect to a location
    pub fn internal_redirect(&self, location: &str) -> Status {
        assert!(!location.is_empty(), "uri location is empty");
        let uri_ptr = unsafe { &mut ngx_str_t::from_str(self.0.pool, location) as *mut _ };

        // FIXME: check status of ngx_http_named_location or ngx_http_internal_redirect
        if location.starts_with('@') {
            unsafe {
                ngx_http_named_location((self as *const Request as *mut Request).cast(), uri_ptr);
            }
        } else {
            unsafe {
                ngx_http_internal_redirect(
                    (self as *const Request as *mut Request).cast(),
                    uri_ptr,
                    core::ptr::null_mut(),
                );
            }
        }
        Status::NGX_DONE
    }

    /// Send a subrequest
    pub fn subrequest(
        &self,
        uri: &str,
        module: &ngx_module_t,
        post_callback: unsafe extern "C" fn(
            *mut ngx_http_request_t,
            *mut c_void,
            ngx_int_t,
        ) -> ngx_int_t,
    ) -> Status {
        let uri_ptr = unsafe { &mut ngx_str_t::from_str(self.0.pool, uri) as *mut _ };
        // -------------
        // allocate memory and set values for ngx_http_post_subrequest_t
        let sub_ptr = self.pool().alloc(core::mem::size_of::<ngx_http_post_subrequest_t>());

        // assert!(sub_ptr.is_null());
        let post_subreq =
            sub_ptr as *const ngx_http_post_subrequest_t as *mut ngx_http_post_subrequest_t;
        unsafe {
            (*post_subreq).handler = Some(post_callback);
            // WARN: safety! ensure that ctx is already set
            (*post_subreq).data = self.get_module_ctx_ptr(module);
        }
        // -------------

        let mut psr: *mut ngx_http_request_t = core::ptr::null_mut();
        let r = unsafe {
            ngx_http_subrequest(
                (self as *const Request as *mut Request).cast(),
                uri_ptr,
                core::ptr::null_mut(),
                &raw mut psr,
                sub_ptr as *mut _,
                NGX_HTTP_SUBREQUEST_WAITED as _,
            )
        };

        // previously call of ngx_http_subrequest() would ensure that the pointer is not null
        // anymore
        let sr = unsafe { &mut *psr };

        /*
         * allocate fake request body to avoid attempts to read it and to make
         * sure real body file (if already read) won't be closed by upstream
         */
        sr.request_body =
            self.pool().alloc(core::mem::size_of::<ngx_http_request_body_t>()) as *mut _;

        if sr.request_body.is_null() {
            return Status::NGX_ERROR;
        }
        sr.set_header_only(1 as _);
        Status(r)
    }

    /// Iterate over headers_in
    /// each header item is (&str, &str) (borrowed)
    pub fn headers_in_iterator(&self) -> NgxListIterator<'_> {
        unsafe { list_iterator(&self.0.headers_in.headers) }
    }

    /// Iterate over headers_out
    /// each header item is (&str, &str) (borrowed)
    pub fn headers_out_iterator(&self) -> NgxListIterator<'_> {
        unsafe { list_iterator(&self.0.headers_out.headers) }
    }
}

impl crate::http::HttpModuleConfExt for Request {
    #[inline]
    unsafe fn http_main_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        unsafe {
            // SAFETY: main_conf[module.ctx_index] is either NULL or allocated with ngx_p(c)alloc
            // and explicitly initialized by the module
            NonNull::new((*self.0.main_conf.add(module.ctx_index)).cast())
        }
    }

    #[inline]
    unsafe fn http_server_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        unsafe {
            // SAFETY: srv_conf[module.ctx_index] is either NULL or allocated with ngx_p(c)alloc and
            // explicitly initialized by the module
            NonNull::new((*self.0.srv_conf.add(module.ctx_index)).cast())
        }
    }

    #[inline]
    unsafe fn http_location_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        unsafe {
            // SAFETY: loc_conf[module.ctx_index] is either NULL or allocated with ngx_p(c)alloc and
            // explicitly initialized by the module
            NonNull::new((*self.0.loc_conf.add(module.ctx_index)).cast())
        }
    }
}

// trait OnSubRequestDone {

// }

impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Request").field("request_", &self.0).finish()
    }
}

/// Iterator for [`ngx_list_t`] types.
///
/// Implementes the core::iter::Iterator trait.
pub struct NgxListIterator<'a> {
    part: Option<ListPart<'a>>,
    i: ngx_uint_t,
}
struct ListPart<'a> {
    raw: &'a ngx_list_part_t,
    arr: &'a [ngx_table_elt_t],
}
impl<'a> From<&'a ngx_list_part_t> for ListPart<'a> {
    fn from(raw: &'a ngx_list_part_t) -> Self {
        let arr = if raw.nelts != 0 {
            unsafe { slice::from_raw_parts(raw.elts.cast(), raw.nelts) }
        } else {
            &[]
        };
        Self { raw, arr }
    }
}

/// Creates new HTTP header iterator
///
/// # Safety
///
/// The caller has provided a valid [`ngx_str_t`] which can be dereferenced validly.
pub unsafe fn list_iterator(list: &ngx_list_t) -> NgxListIterator<'_> {
    NgxListIterator { part: Some((&list.part).into()), i: 0 }
}

// iterator for ngx_list_t
impl<'a> Iterator for NgxListIterator<'a> {
    // TODO: try to use struct instead of &str pair
    // something like pub struct Header(ngx_table_elt_t);
    // then header would have key and value

    type Item = (&'a NgxStr, &'a NgxStr);

    fn next(&mut self) -> Option<Self::Item> {
        let part = self.part.as_mut()?;
        if self.i >= part.arr.len() {
            if let Some(next_part_raw) = unsafe { part.raw.next.as_ref() } {
                // loop back
                *part = next_part_raw.into();
                self.i = 0;
            } else {
                self.part = None;
                return None;
            }
        }
        let header = &part.arr[self.i];
        self.i += 1;
        unsafe { Some((NgxStr::from_ngx_str(header.key), NgxStr::from_ngx_str(header.value))) }
    }
}

/// A possible error value when converting `Method`
pub struct InvalidMethod {
    _priv: (),
}

/// Request method verb
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Method(MethodInner);

impl Method {
    /// UNKNOWN
    pub const UNKNOWN: Method = Method(MethodInner::Unknown);

    /// GET
    pub const GET: Method = Method(MethodInner::Get);

    /// HEAD
    pub const HEAD: Method = Method(MethodInner::Head);

    /// POST
    pub const POST: Method = Method(MethodInner::Post);

    /// PUT
    pub const PUT: Method = Method(MethodInner::Put);

    /// DELETE
    pub const DELETE: Method = Method(MethodInner::Delete);

    /// MKCOL
    pub const MKCOL: Method = Method(MethodInner::Mkcol);

    /// COPY
    pub const COPY: Method = Method(MethodInner::Copy);

    /// MOVE
    pub const MOVE: Method = Method(MethodInner::Move);

    /// OPTIONS
    pub const OPTIONS: Method = Method(MethodInner::Options);

    /// PROPFIND
    pub const PROPFIND: Method = Method(MethodInner::Propfind);

    /// PROPPATCH
    pub const PROPPATCH: Method = Method(MethodInner::Proppatch);

    /// LOCK
    pub const LOCK: Method = Method(MethodInner::Lock);

    /// UNLOCK
    pub const UNLOCK: Method = Method(MethodInner::Unlock);

    /// PATCH
    pub const PATCH: Method = Method(MethodInner::Patch);

    /// TRACE
    pub const TRACE: Method = Method(MethodInner::Trace);

    /// CONNECT
    pub const CONNECT: Method = Method(MethodInner::Connect);

    /// Convert a Method to a &str.
    #[inline]
    pub fn as_str(&self) -> &str {
        match self.0 {
            MethodInner::Unknown => "UNKNOWN",
            MethodInner::Get => "GET",
            MethodInner::Head => "HEAD",
            MethodInner::Post => "POST",
            MethodInner::Put => "PUT",
            MethodInner::Delete => "DELETE",
            MethodInner::Mkcol => "MKCOL",
            MethodInner::Copy => "COPY",
            MethodInner::Move => "MOVE",
            MethodInner::Options => "OPTIONS",
            MethodInner::Propfind => "PROPFIND",
            MethodInner::Proppatch => "PROPPATCH",
            MethodInner::Lock => "LOCK",
            MethodInner::Unlock => "UNLOCK",
            MethodInner::Patch => "PATCH",
            MethodInner::Trace => "TRACE",
            MethodInner::Connect => "CONNECT",
        }
    }

    fn from_bytes(_t: &[u8]) -> Result<Method, InvalidMethod> {
        todo!()
    }

    fn from_ngx(t: ngx_uint_t) -> Method {
        let t = t as _;
        match t {
            crate::ffi::NGX_HTTP_GET => Method(MethodInner::Get),
            crate::ffi::NGX_HTTP_HEAD => Method(MethodInner::Head),
            crate::ffi::NGX_HTTP_POST => Method(MethodInner::Post),
            crate::ffi::NGX_HTTP_PUT => Method(MethodInner::Put),
            crate::ffi::NGX_HTTP_DELETE => Method(MethodInner::Delete),
            crate::ffi::NGX_HTTP_MKCOL => Method(MethodInner::Mkcol),
            crate::ffi::NGX_HTTP_COPY => Method(MethodInner::Copy),
            crate::ffi::NGX_HTTP_MOVE => Method(MethodInner::Move),
            crate::ffi::NGX_HTTP_OPTIONS => Method(MethodInner::Options),
            crate::ffi::NGX_HTTP_PROPFIND => Method(MethodInner::Propfind),
            crate::ffi::NGX_HTTP_PROPPATCH => Method(MethodInner::Proppatch),
            crate::ffi::NGX_HTTP_LOCK => Method(MethodInner::Lock),
            crate::ffi::NGX_HTTP_UNLOCK => Method(MethodInner::Unlock),
            crate::ffi::NGX_HTTP_PATCH => Method(MethodInner::Patch),
            crate::ffi::NGX_HTTP_TRACE => Method(MethodInner::Trace),
            #[cfg(nginx1_21_1)]
            crate::ffi::NGX_HTTP_CONNECT => Method(MethodInner::Connect),
            _ => Method(MethodInner::Unknown),
        }
    }
}

impl AsRef<str> for Method {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<'a> PartialEq<&'a Method> for Method {
    #[inline]
    fn eq(&self, other: &&'a Method) -> bool {
        self == *other
    }
}

impl PartialEq<Method> for &Method {
    #[inline]
    fn eq(&self, other: &Method) -> bool {
        *self == other
    }
}

impl PartialEq<str> for Method {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.as_ref() == other
    }
}

impl PartialEq<Method> for str {
    #[inline]
    fn eq(&self, other: &Method) -> bool {
        self == other.as_ref()
    }
}

impl<'a> PartialEq<&'a str> for Method {
    #[inline]
    fn eq(&self, other: &&'a str) -> bool {
        self.as_ref() == *other
    }
}

impl PartialEq<Method> for &str {
    #[inline]
    fn eq(&self, other: &Method) -> bool {
        *self == other.as_ref()
    }
}

impl fmt::Debug for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl fmt::Display for Method {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str(self.as_ref())
    }
}

impl<'a> From<&'a Method> for Method {
    #[inline]
    fn from(t: &'a Method) -> Self {
        t.clone()
    }
}

impl<'a> TryFrom<&'a [u8]> for Method {
    type Error = InvalidMethod;

    #[inline]
    fn try_from(t: &'a [u8]) -> Result<Self, Self::Error> {
        Method::from_bytes(t)
    }
}

impl<'a> TryFrom<&'a str> for Method {
    type Error = InvalidMethod;

    #[inline]
    fn try_from(t: &'a str) -> Result<Self, Self::Error> {
        TryFrom::try_from(t.as_bytes())
    }
}

impl FromStr for Method {
    type Err = InvalidMethod;

    #[inline]
    fn from_str(t: &str) -> Result<Self, Self::Err> {
        TryFrom::try_from(t)
    }
}

impl InvalidMethod {
    #[allow(dead_code)]
    fn new() -> InvalidMethod {
        InvalidMethod { _priv: () }
    }
}

impl fmt::Debug for InvalidMethod {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("InvalidMethod")
            // skip _priv noise
            .finish()
    }
}

impl fmt::Display for InvalidMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid HTTP method")
    }
}

impl error::Error for InvalidMethod {}

#[derive(Clone, PartialEq, Eq, Hash)]
enum MethodInner {
    Unknown,
    Get,
    Head,
    Post,
    Put,
    Delete,
    Mkcol,
    Copy,
    Move,
    Options,
    Propfind,
    Proppatch,
    Lock,
    Unlock,
    Patch,
    Trace,
    Connect,
}

#[cfg(test)]
mod tests {
    use core::mem::MaybeUninit;

    use super::*;

    fn zeroed_request() -> ngx_http_request_t {
        // SAFETY: `ngx_http_request_t` is `#[repr(C)]` and tests only
        // read the fields they populate below.
        unsafe { MaybeUninit::zeroed().assume_init() }
    }

    fn request_from(r: &mut ngx_http_request_t) -> &mut Request {
        // SAFETY: `Request` is `#[repr(transparent)]` over `ngx_http_request_t`.
        unsafe { Request::from_ngx_http_request(r) }
    }

    #[cfg(ngx_feature = "http_cache")]
    mod cache {
        use super::*;
        use crate::ffi::{
            NGX_HTTP_CACHE_HIT, NGX_HTTP_CACHE_MISS, ngx_http_cache_t, ngx_http_file_cache_sh_t,
            ngx_http_file_cache_t, ngx_http_upstream_t, ngx_shm_zone_t, ngx_str_t,
        };
        use crate::http::CacheStatus;

        #[test]
        fn cache_status_none_when_upstream_null() {
            let mut r = zeroed_request();
            let req = request_from(&mut r);
            assert_eq!(req.cache_status(), None);
        }

        #[test]
        fn cache_status_none_when_field_is_zero_sentinel() {
            // `cache_status == 0` is the "no cache lookup" sentinel
            // nginx leaves on upstreams created for non-cached
            // requests.  Must surface as `None`, not a fake variant.
            let mut upstream: ngx_http_upstream_t = unsafe { MaybeUninit::zeroed().assume_init() };
            upstream.set_cache_status(0);
            let mut r = zeroed_request();
            r.upstream = &raw mut upstream;
            let req = request_from(&mut r);
            assert_eq!(req.cache_status(), None);
        }

        #[test]
        fn cache_status_maps_hit_and_miss() {
            let mut upstream: ngx_http_upstream_t = unsafe { MaybeUninit::zeroed().assume_init() };
            upstream.set_cache_status(NGX_HTTP_CACHE_HIT);
            let mut r = zeroed_request();
            r.upstream = &raw mut upstream;
            assert_eq!(request_from(&mut r).cache_status(), Some(CacheStatus::Hit));

            upstream.set_cache_status(NGX_HTTP_CACHE_MISS);
            assert_eq!(request_from(&mut r).cache_status(), Some(CacheStatus::Miss));
        }

        #[test]
        fn cache_zone_name_none_when_cache_null() {
            let mut r = zeroed_request();
            let req = request_from(&mut r);
            assert!(req.cache_zone_name().is_none());
        }

        #[test]
        fn cache_zone_name_none_when_file_cache_null() {
            let mut cache: ngx_http_cache_t = unsafe { MaybeUninit::zeroed().assume_init() };
            // file_cache stays null after zero-init.
            let mut r = zeroed_request();
            r.cache = &raw mut cache;
            assert!(request_from(&mut r).cache_zone_name().is_none());
        }

        #[test]
        fn cache_zone_name_none_when_shm_zone_null() {
            let mut file_cache: ngx_http_file_cache_t =
                unsafe { MaybeUninit::zeroed().assume_init() };
            // shm_zone stays null after zero-init.
            let mut cache: ngx_http_cache_t = unsafe { MaybeUninit::zeroed().assume_init() };
            cache.file_cache = &raw mut file_cache;
            let mut r = zeroed_request();
            r.cache = &raw mut cache;
            assert!(request_from(&mut r).cache_zone_name().is_none());
        }

        #[test]
        fn zone_sizes_none_when_no_cache_lookup() {
            let mut r = zeroed_request();
            let req = request_from(&mut r);
            assert_eq!(req.cache_zone_max_size(), None);
            assert_eq!(req.cache_zone_used_size(), None);
            assert_eq!(req.cache_zone_block_size(), None);
        }

        #[test]
        fn zone_sizes_come_back_in_bytes_not_blocks() {
            // What nginx holds: max_size and sh->size counted in
            // blocks of bsize, the division done once in
            // ngx_http_file_cache_init.  Reading either field
            // directly would answer 256 and 64 here.
            let mut sh: ngx_http_file_cache_sh_t = unsafe { MaybeUninit::zeroed().assume_init() };
            sh.size = 64;

            let mut file_cache: ngx_http_file_cache_t =
                unsafe { MaybeUninit::zeroed().assume_init() };
            file_cache.max_size = 256;
            file_cache.bsize = 4096;
            file_cache.sh = &raw mut sh;

            let mut cache: ngx_http_cache_t = unsafe { MaybeUninit::zeroed().assume_init() };
            cache.file_cache = &raw mut file_cache;

            let mut r = zeroed_request();
            r.cache = &raw mut cache;
            let req = request_from(&mut r);

            assert_eq!(req.cache_zone_max_size(), Some(1024 * 1024));
            assert_eq!(req.cache_zone_used_size(), Some(256 * 1024));
            assert_eq!(req.cache_zone_block_size(), Some(4096));
        }

        #[test]
        fn used_size_none_when_shared_state_null() {
            // sh stays null after zero-init: a cache the manager has
            // not brought up yet has no running total to report.
            let mut file_cache: ngx_http_file_cache_t =
                unsafe { MaybeUninit::zeroed().assume_init() };
            file_cache.bsize = 4096;

            let mut cache: ngx_http_cache_t = unsafe { MaybeUninit::zeroed().assume_init() };
            cache.file_cache = &raw mut file_cache;

            let mut r = zeroed_request();
            r.cache = &raw mut cache;
            assert_eq!(request_from(&mut r).cache_zone_used_size(), None);
        }

        #[test]
        fn cache_zone_name_returns_zone_name() {
            let bytes = b"my_cache";
            let mut shm_zone: ngx_shm_zone_t = unsafe { MaybeUninit::zeroed().assume_init() };
            shm_zone.shm.name = ngx_str_t { len: bytes.len(), data: bytes.as_ptr().cast_mut() };

            let mut file_cache: ngx_http_file_cache_t =
                unsafe { MaybeUninit::zeroed().assume_init() };
            file_cache.shm_zone = &raw mut shm_zone;

            let mut cache: ngx_http_cache_t = unsafe { MaybeUninit::zeroed().assume_init() };
            cache.file_cache = &raw mut file_cache;

            let mut r = zeroed_request();
            r.cache = &raw mut cache;

            let name = request_from(&mut r).cache_zone_name().expect("should resolve");
            assert_eq!(name.as_bytes(), bytes);
        }
    }
}
