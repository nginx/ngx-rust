use crate::core::*;
use crate::ffi::*;
use crate::http::flags::SubrequestFlags;
use crate::http::status::*;
use crate::ngx_null_string;
use std::fmt;
use std::os::raw::c_void;

use std::error::Error;
use std::str::FromStr;

/// Define a static request handler.
///
/// Handlers are expected to take a single [`Request`] argument and return a [`Status`].
#[macro_export]
macro_rules! http_request_handler {
    ( $name: ident, $handler: expr ) => {
        #[no_mangle]
        extern "C" fn $name(r: *mut ngx_http_request_t) -> ngx_int_t {
            let status: Status = $handler(unsafe { &mut $crate::http::Request::from_ngx_http_request(r) });
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
        #[no_mangle]
        unsafe extern "C" fn $name(r: *mut ngx_http_request_t, data: *mut c_void, rc: ngx_int_t) -> ngx_int_t {
            $handler(r, data, rc)
        }
    };
}

/// Define a static variable setter.
///
/// The set handler allows setting the property referenced by the variable.
/// The set handler expects a [`Request`], [`mut ngx_variable_valut_t`], and a [`usize`].
/// Variables: <https://nginx.org/en/docs/dev/development_guide.html#http_variables>
#[macro_export]
macro_rules! http_variable_set {
    ( $name: ident, $handler: expr ) => {
        #[no_mangle]
        unsafe extern "C" fn $name(r: *mut ngx_http_request_t, v: *mut ngx_variable_value_t, data: usize) {
            $handler(
                unsafe { &mut $crate::http::Request::from_ngx_http_request(r) },
                v,
                data,
            );
        }
    };
}

/// Define a static variable evaluator.
///
/// The get handler is responsible for evaluating a variable in the context of a specific request.
/// Variable evaluators accept a [`Request`] input argument and two output
/// arguments: [`ngx_http_variable_valut_t`] and [`usize`].
/// Variables: <https://nginx.org/en/docs/dev/development_guide.html#http_variables>
#[macro_export]
macro_rules! http_variable_get {
    ( $name: ident, $handler: expr ) => {
        #[no_mangle]
        unsafe extern "C" fn $name(r: *mut ngx_http_request_t, v: *mut ngx_variable_value_t, data: usize) -> ngx_int_t {
            let status: Status = $handler(
                unsafe { &mut $crate::http::Request::from_ngx_http_request(r) },
                v,
                data,
            );
            status.0
        }
    };
}

/// Wrapper struct for an `ngx_http_request_t` pointer, , providing methods for working with HTTP requests.
#[repr(transparent)]
pub struct Request(ngx_http_request_t);

impl Request {
    /// Create a [`Request`] from an [`ngx_http_request_t`].
    ///
    /// [`ngx_http_request_t`]: https://nginx.org/en/docs/dev/development_guide.html#http_request
    ///
    /// # Safety
    ///
    /// The caller has provided a valid non-null pointer to a valid `ngx_http_request_t`
    /// which shares the same representation as `Request`.
    pub unsafe fn from_ngx_http_request<'a>(r: *mut ngx_http_request_t) -> &'a mut Request {
        &mut *r.cast::<Request>()
    }

    /// Is this the main request (as opposed to a subrequest)?
    pub fn is_main(&self) -> bool {
        let main = self.0.main.cast();
        std::ptr::eq(self, main)
    }

    /// Request pool.
    pub fn pool(&self) -> Pool {
        // SAFETY: This request is allocated from `pool`, thus must be a valid pool.
        unsafe { Pool::from_ngx_pool(self.0.pool) }
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

    /// Module location configuration.
    fn get_module_loc_conf_ptr(&self, module: &ngx_module_t) -> *mut c_void {
        unsafe { *self.0.loc_conf.add(module.ctx_index) }
    }

    /// Module location configuration.
    pub fn get_module_loc_conf<T>(&self, module: &ngx_module_t) -> Option<&T> {
        let lc_prt = self.get_module_loc_conf_ptr(module) as *mut T;
        if lc_prt.is_null() {
            return None;
        }
        let lc = unsafe { &*lc_prt };
        Some(lc)
    }

    /// Get Module context pointer
    fn get_module_ctx_ptr(&self, module: &ngx_module_t) -> *mut c_void {
        unsafe { *self.0.ctx.add(module.ctx_index) }
    }

    /// Get Module context
    pub fn get_module_ctx<T>(&self, module: &ngx_module_t) -> Option<&T> {
        let cf = self.get_module_ctx_ptr(module) as *mut T;

        if cf.is_null() {
            return None;
        }
        let co = unsafe { &*cf };
        Some(co)
    }

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
            let mut value = ngx_null_string!();
            if ngx_http_complex_value(r, val, &mut value) != NGX_OK as ngx_int_t {
                return None;
            }
            Some(NgxStr::from_ngx_str(value))
        }
    }

    /// Discard (read and ignore) the [request body].
    ///
    /// [request body]: https://nginx.org/en/docs/dev/development_guide.html#http_request_body
    pub fn discard_request_body(&mut self) -> Status {
        unsafe { Status(ngx_http_discard_request_body(&mut self.0)) }
    }

    /// Client HTTP [User-Agent].
    ///
    /// [User-Agent]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/User-Agent
    pub fn user_agent(&self) -> &NgxStr {
        unsafe { NgxStr::from_ngx_str((*self.0.headers_in.user_agent).value) }
    }

    /// Set HTTP status of response.
    pub fn set_status(&mut self, status: HTTPStatus) {
        self.0.headers_out.status = status.into();
    }

    /// Get HTTP status of response.
    pub fn get_status(&self) -> HTTPStatus {
        HTTPStatus(self.0.headers_out.status)
    }

    /// Add one to the request's current cycle count.
    pub fn increment_cycle_count(&mut self) {
        self.0.set_count(self.0.count() + 1);
    }

    /// Add header key and value to the input headers object.
    ///
    /// See https://nginx.org/en/docs/dev/development_guide.html#http_request `headers_in`.
    pub fn add_header_in(&mut self, key: &str, value: &str) -> Option<()> {
        let table: *mut ngx_table_elt_t = unsafe { ngx_list_push(&mut self.0.headers_in.headers) as _ };
        unsafe { add_to_ngx_table(table, self.0.pool, key, value) }
    }

    /// Add header key and value to the output headers object.
    ///
    /// See https://nginx.org/en/docs/dev/development_guide.html#http_request `headers_out`.
    pub fn add_header_out(&mut self, key: &str, value: &str) -> Option<()> {
        let table: *mut ngx_table_elt_t = unsafe { ngx_list_push(&mut self.0.headers_out.headers) as _ };
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
        unsafe { Status(ngx_http_send_header(&mut self.0)) }
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
        unsafe { Status(ngx_http_output_filter(&mut self.0, body)) }
    }

    /// Utility method to perform an internal redirect without args (query parameters) or named
    /// location.
    /// For full control methods see `ngx_internal_redirect` and `ngx_named_location`.
    ///
    /// # Safety
    ///
    /// This method invokes unsafe methods.
    pub unsafe fn internal_redirect(&self, location: &str) -> Status {
        if location.starts_with('@') {
            self.ngx_named_location(location)
        } else {
            self.ngx_internal_redirect(location, "")
        }
    }

    /// Invoke ngx_internal_redirect to perform an internal redirect to a location.
    ///
    /// # Safety
    ///
    /// This method calls into unsafe functions on the stack and dereferences raw pointers to
    /// interface with NGINX API primitives.
    pub unsafe fn ngx_internal_redirect(&self, location: &str, args: &str) -> Status {
        assert!(!location.is_empty(), "uri location is empty");
        let uri_ptr = &mut ngx_str_t::from_str(self.0.pool, location) as *mut _;
        let args_ptr = if !args.is_empty() {
            &mut ngx_str_t::from_str(self.0.pool, location) as *mut _
        } else {
            std::ptr::null_mut()
        };

        Status(ngx_http_internal_redirect(
            (self as *const Request as *mut Request).cast(),
            uri_ptr,
            args_ptr,
        ))
    }

    /// Invoke ngx_named_location to perform an internal redirect to a named location.
    ///
    /// # Safety
    ///
    /// This method calls into unsafe functions on the stack and dereferences raw pointers to
    /// interface with NGINX API primitives.
    pub unsafe fn ngx_named_location(&self, location: &str) -> Status {
        assert!(!location.is_empty(), "uri location is empty");
        assert!(location.starts_with('@'), "named location must start with @");
        let uri_ptr = &mut ngx_str_t::from_str(self.0.pool, location) as *mut _;

        Status(ngx_http_named_location(
            (self as *const Request as *mut Request).cast(),
            uri_ptr,
        ))
    }

    /// How many subrequests are available to make in this request,
    /// will return NGX_HTTP_MAX_SUBREQUESTS for a parent request.
    pub fn subrequests_available(&self) -> u32 {
        // 1 is subtracted because this function was caught returning 1 extra
        // The return value should be (50, 0), with the parent request returning
        // NGX_HTTP_MAX_SUBREQUESTS.
        // See http://nginx.org/en/docs/dev/development_guide.html#http_subrequests
        // for more information
        self.0.subrequests() - 1
    }

    /// Send a subrequest
    pub fn subrequest(
        &self,
        uri: &str,
        args: &str,
        flags: SubrequestFlags,
        module: &ngx_module_t,
        data: Option<*mut c_void>,
        post_callback: unsafe extern "C" fn(*mut ngx_http_request_t, *mut c_void, ngx_int_t) -> ngx_int_t,
    ) -> Status {
        let uri_ptr = unsafe { &mut ngx_str_t::from_str(self.0.pool, uri) as *mut _ };
        let args_ptr = unsafe { &mut ngx_str_t::from_str(self.0.pool, args) as *mut _ };
        // -------------
        // allocate memory and set values for ngx_http_post_subrequest_t
        let sub_ptr = self.pool().alloc(std::mem::size_of::<ngx_http_post_subrequest_t>());

        // assert!(sub_ptr.is_null());
        let post_subreq = sub_ptr as *const ngx_http_post_subrequest_t as *mut ngx_http_post_subrequest_t;
        unsafe {
            (*post_subreq).handler = Some(post_callback);
            if let Some(datum) = data {
                (*post_subreq).data = datum;
            } else {
                // WARN: safety! ensure that ctx is already set
                (*post_subreq).data = self.get_module_ctx_ptr(module);
            }
        }
        // -------------

        let mut psr: *mut ngx_http_request_t = std::ptr::null_mut();
        let r = unsafe {
            ngx_http_subrequest(
                (self as *const Request as *mut Request).cast(),
                uri_ptr,
                args_ptr,
                &mut psr as *mut _,
                sub_ptr as *mut _,
                flags.into(),
            )
        };

        Status(r)
    }

    /// Iterate over headers_in
    /// each header item is (String, String) (copied)
    pub fn headers_in_iterator(&self) -> NgxListIterator {
        unsafe { list_iterator(&self.0.headers_in.headers) }
    }

    /// Iterate over headers_out
    /// each header item is (String, String) (copied)
    pub fn headers_out_iterator(&self) -> NgxListIterator {
        unsafe { list_iterator(&self.0.headers_out.headers) }
    }
}

// trait OnSubRequestDone {

// }

impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Request").field("request_", &self.0).finish()
    }
}

pub struct NgxListIterator {
    done: bool,
    part: *const ngx_list_part_t,
    h: *const ngx_table_elt_t,
    i: ngx_uint_t,
}

// create new http request iterator
/// # Safety
///
/// The caller has provided a valid `ngx_str_t` which can be dereferenced validly.
pub unsafe fn list_iterator(list: *const ngx_list_t) -> NgxListIterator {
    let part: *const ngx_list_part_t = &(*list).part;

    NgxListIterator {
        done: false,
        part,
        h: (*part).elts as *const ngx_table_elt_t,
        i: 0,
    }
}

// iterator for ngx_list_t
impl Iterator for NgxListIterator {
    // type Item = (&str,&str);
    // TODO: try to use str instead of string
    // something like pub struct Header(ngx_table_elt_t);
    // then header would have key and value

    type Item = (String, String);

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            if self.done {
                None
            } else {
                if self.i >= (*self.part).nelts {
                    if (*self.part).next.is_null() {
                        self.done = true;
                        return None;
                    }

                    // loop back
                    self.part = (*self.part).next;
                    self.h = (*self.part).elts as *mut ngx_table_elt_t;
                    self.i = 0;
                }

                let header: *const ngx_table_elt_t = self.h.add(self.i);
                let header_name: ngx_str_t = (*header).key;
                let header_value: ngx_str_t = (*header).value;
                self.i += 1;
                Some((header_name.to_string(), header_value.to_string()))
            }
        }
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
            NGX_HTTP_GET => Method(MethodInner::Get),
            NGX_HTTP_HEAD => Method(MethodInner::Head),
            NGX_HTTP_POST => Method(MethodInner::Post),
            NGX_HTTP_PUT => Method(MethodInner::Put),
            NGX_HTTP_DELETE => Method(MethodInner::Delete),
            NGX_HTTP_MKCOL => Method(MethodInner::Mkcol),
            NGX_HTTP_COPY => Method(MethodInner::Copy),
            NGX_HTTP_MOVE => Method(MethodInner::Move),
            NGX_HTTP_OPTIONS => Method(MethodInner::Options),
            NGX_HTTP_PROPFIND => Method(MethodInner::Propfind),
            NGX_HTTP_PROPPATCH => Method(MethodInner::Proppatch),
            NGX_HTTP_LOCK => Method(MethodInner::Lock),
            NGX_HTTP_UNLOCK => Method(MethodInner::Unlock),
            NGX_HTTP_PATCH => Method(MethodInner::Patch),
            NGX_HTTP_TRACE => Method(MethodInner::Trace),
            NGX_HTTP_CONNECT => Method(MethodInner::Connect),
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

impl<'a> PartialEq<Method> for &'a Method {
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

impl<'a> PartialEq<Method> for &'a str {
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

impl Error for InvalidMethod {}

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
