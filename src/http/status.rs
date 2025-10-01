use core::{error, fmt};

use crate::core::{NgxResult, Status};
use crate::ffi::*;

/// Represents an HTTP status code.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HTTPStatus(pub ngx_uint_t);

/// A possible error value when converting a `HTTPStatus` from a `u16` or `&str`
///
/// This error indicates that the supplied input was not a valid number, was less
/// than 100, or was greater than 599.
#[derive(Debug)]
pub struct InvalidHTTPStatusCode {
    _priv: (),
}

impl InvalidHTTPStatusCode {
    fn new() -> InvalidHTTPStatusCode {
        InvalidHTTPStatusCode { _priv: () }
    }
}

impl fmt::Display for InvalidHTTPStatusCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("invalid status code")
    }
}

impl error::Error for InvalidHTTPStatusCode {}

impl From<HTTPStatus> for Status {
    fn from(val: HTTPStatus) -> Self {
        Status(val.0 as ngx_int_t)
    }
}

impl From<HTTPStatus> for NgxResult {
    fn from(val: HTTPStatus) -> Self {
        if (100..599).contains(&val.0) {
            Ok(val.0 as ngx_int_t)
        } else {
            Err(crate::core::NgxError {})
        }
    }
}

impl From<HTTPStatus> for ngx_uint_t {
    fn from(val: HTTPStatus) -> Self {
        val.0
    }
}

impl fmt::Debug for HTTPStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl HTTPStatus {
    /// Converts a u16 to a status code.
    #[inline]
    pub fn from_u16(src: u16) -> Result<HTTPStatus, InvalidHTTPStatusCode> {
        if !(100..600).contains(&src) {
            return Err(InvalidHTTPStatusCode::new());
        }

        Ok(HTTPStatus(src.into()))
    }

    /// Converts a &[u8] to a status code.
    pub fn from_bytes(src: &[u8]) -> Result<HTTPStatus, InvalidHTTPStatusCode> {
        if src.len() != 3 {
            return Err(InvalidHTTPStatusCode::new());
        }

        let a = src[0].wrapping_sub(b'0') as u16;
        let b = src[1].wrapping_sub(b'0') as u16;
        let c = src[2].wrapping_sub(b'0') as u16;

        if a == 0 || a > 5 || b > 9 || c > 9 {
            return Err(InvalidHTTPStatusCode::new());
        }

        let status = (a * 100) + (b * 10) + c;
        Ok(HTTPStatus(status.into()))
    }
}

macro_rules! http_status_codes {
    (
        $(
            $(#[$docs:meta])*
            ($num:expr, $konst:ident, $phrase:expr);
        )+
    ) => {
        impl HTTPStatus {
        $(
            $(#[$docs])*
            pub const $konst: HTTPStatus = HTTPStatus($num);
        )+

        }
    }
}

http_status_codes! {
    /// 100 CONTINUE
    (100, CONTINUE, "Continue");
    /// 101 SWITCHING_PROTOCOLS
    (101, SWITCHING_PROTOCOLS, "Switching Protocols");
    /// 102 PROCESSING
    (102, PROCESSING, "Processing");
    /// 200 OK
    (200, OK, "OK");
    /// 201 Created
    (201, CREATED, "Created");
    /// 202 Accepted
    (202, ACCEPTED, "Accepted");
    /// 204 No Content
    (204, NO_CONTENT, "No Content");
    /// 206 Partial Content
    (206, PARTIAL_CONTENT, "Partial Content");

    /// 300 SPECIAL_RESPONSE
    (300, SPECIAL_RESPONSE, "SPECIAL_RESPONSE");
    /// 301 Moved Permanently
    (301, MOVED_PERMANENTLY, "Moved Permanently");
    /// 302 Moved Temporarily
    (302, MOVED_TEMPORARILY, "Moved Temporarily");
    /// 303 See Other
    (303, SEE_OTHER, "See Other");
    /// 304 Not Modified
    (304, NOT_MODIFIED, "Not Modified");
    /// 307 Temporary Redirect
    (307, TEMPORARY_REDIRECT, "Temporary Redirect");
    /// 308 Permanent Redirect
    (308, PERMANENT_REDIRECT, "Permanent Redirect");

    /// 400 Bad Request
    (400, BAD_REQUEST, "Bad Request");
    /// 401 Unauthorized
    (401, UNAUTHORIZED, "Unauthorized");
    /// 403 Forbidden
    (403, FORBIDDEN, "Forbidden");
    /// 404 Not Found
    (404, NOT_FOUND, "Not Found");
    /// 405 Method Not Allowed
    (405, NOT_ALLOWED, "Method Not Allowed");
    /// 408 Request Time Out
    (408, REQUEST_TIME_OUT, "Request Time Out");
    /// 409 Conflict
    (409, CONFLICT, "Conflict");
    /// 411 Length Required
    (411, LENGTH_REQUIRED, "Length Required");
    /// 412 Precondition Failed
    (412, PRECONDITION_FAILED, "Precondition Failed");
    /// 413 Payload Too Large
    (413, REQUEST_ENTITY_TOO_LARGE, "Payload Too Large");
    /// 414 Request Uri Too Large
    (414, REQUEST_URI_TOO_LARGE, "Request Uri Too Large");
    /// 415 Unsupported Media Type
    (415, UNSUPPORTED_MEDIA_TYPE, "Unsupported Media Type");
    /// 416 Range Not Satisfiable
    (416, RANGE_NOT_SATISFIABLE, "Range Not Satisfiable");
    /// 421 Misdirected Request
    (421, MISDIRECTED_REQUEST, "Misdirected Request");
    /// 429 Too Many Requests
    (429, TOO_MANY_REQUESTS, "Too Many Requests");

    // /* Our own HTTP codes */
    // /* The special code to close connection without any response */
    /// 444 CLOSE
    (444, CLOSE, "CLOSE");

    /// 494 NGINX_CODES
    (494, NGINX_CODES, "NGINX_CODES");

    /// 494 REQUEST_HEADER_TOO_LARGE
    (494, REQUEST_HEADER_TOO_LARGE, "REQUEST_HEADER_TOO_LARGE");

    /// 495 NGX_HTTPS_CERT_ERROR
    (495, HTTPS_CERT_ERROR, "NGX_HTTPS_CERT_ERROR");
    /// 496 NGX_HTTPS_NO_CERT
    (496, HTTPS_NO_CERT, "NGX_HTTPS_NO_CERT");

    // /*
    //  * We use the special code for the plain HTTP requests that are sent to
    //  * HTTPS port to distinguish it from 4XX in an error page redirection
    //  */
    /// 497 TO_HTTPS
    (497, TO_HTTPS, "TO_HTTPS");

    /// 499 CLIENT_CLOSED_REQUEST
    (499, CLIENT_CLOSED_REQUEST, "CLIENT_CLOSED_REQUEST");

    /// 500 INTERNAL_SERVER_ERROR
    (500, INTERNAL_SERVER_ERROR, "INTERNAL_SERVER_ERROR");
    /// 501 NOT_IMPLEMENTED
    (501, NOT_IMPLEMENTED, "NOT_IMPLEMENTED");
    /// 502 BAD_GATEWAY
    (502, BAD_GATEWAY, "BAD_GATEWAY");
    /// 503 SERVICE_UNAVAILABLE
    (503, SERVICE_UNAVAILABLE, "SERVICE_UNAVAILABLE");
    /// 504 GATEWAY_TIME_OUT
    (504, GATEWAY_TIME_OUT, "GATEWAY_TIME_OUT");
    /// 505 VERSION_NOT_SUPPORTED
    (505, VERSION_NOT_SUPPORTED, "VERSION_NOT_SUPPORTED");
    /// 507 INSUFFICIENT_STORAGE
    (507, INSUFFICIENT_STORAGE, "INSUFFICIENT_STORAGE");
}
