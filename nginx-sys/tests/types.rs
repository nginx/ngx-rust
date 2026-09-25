//! Check types for a limited set of randomly selected constants.
//!
//! These tests are inherently fragile and will break if we rename or remove
//! something in the nginx headers.  That is fine, integration test failure
//! cannot affect the projects using this crate.
//!
//! Verified with: 1.18.0..1.31
use core::ffi::c_int;
use core::mem;

use nginx_sys::*;

#[test]
fn core_constants() {
    let _err: &[ngx_int_t] =
        &[NGX_OK, NGX_ERROR, NGX_AGAIN, NGX_BUSY, NGX_DONE, NGX_DECLINED, NGX_ABORT];

    let _errno: &[ngx_err_t] = &[
        NGX_EPERM,
        NGX_ENOENT,
        NGX_ENOPATH,
        NGX_ENOMEM,
        NGX_EACCES,
        NGX_EAGAIN,
        //...
        ngx_errno(),
        ngx_socket_errno(),
    ];

    let _log_level: &[ngx_uint_t] = &[
        NGX_LOG_STDERR,
        NGX_LOG_EMERG,
        NGX_LOG_ALERT,
        NGX_LOG_CRIT,
        NGX_LOG_ERR,
        NGX_LOG_WARN,
        NGX_LOG_NOTICE,
        NGX_LOG_INFO,
        NGX_LOG_DEBUG,
        NGX_LOG_DEBUG_CORE
            | NGX_LOG_DEBUG_ALLOC
            | NGX_LOG_DEBUG_MUTEX
            | NGX_LOG_DEBUG_EVENT
            | NGX_LOG_DEBUG_HTTP
            | NGX_LOG_DEBUG_MAIL
            | NGX_LOG_DEBUG_STREAM,
        NGX_LOG_DEBUG_FIRST,
        NGX_LOG_DEBUG_LAST,
        NGX_LOG_DEBUG_CONNECTION,
        NGX_LOG_DEBUG_ALL,
    ];

    let _command_type: ngx_uint_t = NGX_CONF_NOARGS
        | NGX_CONF_TAKE1
        | NGX_CONF_TAKE2
        | NGX_CONF_TAKE3
        | NGX_CONF_TAKE4
        | NGX_CONF_TAKE5
        | NGX_CONF_TAKE6
        | NGX_CONF_TAKE7
        | NGX_CONF_TAKE12
        | NGX_CONF_TAKE13
        | NGX_CONF_TAKE23
        | NGX_CONF_TAKE123
        | NGX_CONF_TAKE1234
        | NGX_CONF_ARGS_NUMBER
        | NGX_CONF_BLOCK
        | NGX_CONF_FLAG
        | NGX_CONF_ANY
        | NGX_CONF_1MORE
        | NGX_CONF_2MORE
        | NGX_DIRECT_CONF
        | NGX_MAIN_CONF
        | NGX_ANY_CONF;

    let _module: &[ngx_uint_t] = &[NGX_CORE_MODULE, NGX_CONF_MODULE];

    let _: ngx_int_t = NGX_CONF_UNSET;
    let _: ngx_uint_t = NGX_CONF_UNSET_UINT;
    let _: usize = NGX_CONF_UNSET_SIZE;
    let _: ngx_msec_t = NGX_CONF_UNSET_MSEC;

    let _: *mut ngx_resolver_ctx_t = NGX_NO_RESOLVER;
    let _: &[ngx_uint_t] = &[
        NGX_RESOLVE_A,
        NGX_RESOLVE_CNAME,
        NGX_RESOLVE_PTR,
        NGX_RESOLVE_MX,
        NGX_RESOLVE_TXT,
        NGX_RESOLVE_AAAA,
        NGX_RESOLVE_SRV,
        NGX_RESOLVE_DNAME,
    ];
    let _: &[ngx_int_t] = &[
        NGX_RESOLVE_FORMERR,
        NGX_RESOLVE_SERVFAIL,
        NGX_RESOLVE_NXDOMAIN,
        NGX_RESOLVE_NOTIMP,
        NGX_RESOLVE_REFUSED,
        NGX_RESOLVE_TIMEDOUT,
    ];

    #[cfg(not(windows))]
    let _: ngx_fd_t = NGX_INVALID_FILE;
    let _: c_int = NGX_FILE_ERROR;
    let _: ngx_pid_t = NGX_INVALID_PID;

    let _: ngx_int_t = NGX_MAX_INT_T_VALUE;
    let _: i32 = NGX_MAX_INT32_VALUE;
    let _: off_t = NGX_MAX_OFF_T_VALUE;
    let _: usize = NGX_MAX_SIZE_T_VALUE;
    let _: time_t = NGX_MAX_TIME_T_VALUE;
    let _: u32 = NGX_MAX_UINT32_VALUE;

    assert_eq!(mem::size_of::<time_t>(), NGX_TIME_T_SIZE);
}

#[test]
#[cfg(all(feature = "http", ngx_feature = "http"))]
fn http_constants() {
    let _module: ngx_uint_t = NGX_HTTP_MODULE;
    let _conf: ngx_uint_t = NGX_HTTP_MAIN_CONF
        | NGX_HTTP_SRV_CONF
        | NGX_HTTP_LOC_CONF
        | NGX_HTTP_UPS_CONF
        | NGX_HTTP_SIF_CONF
        | NGX_HTTP_LIF_CONF
        | NGX_HTTP_LMT_CONF;
    let _conf_offset: usize =
        NGX_HTTP_MAIN_CONF_OFFSET + NGX_HTTP_SRV_CONF_OFFSET + NGX_HTTP_LOC_CONF_OFFSET;

    let _method: &[ngx_uint_t] = &[
        NGX_HTTP_UNKNOWN,
        NGX_HTTP_GET,
        NGX_HTTP_HEAD,
        NGX_HTTP_POST,
        NGX_HTTP_PUT,
        NGX_HTTP_DELETE,
        NGX_HTTP_MKCOL,
        NGX_HTTP_COPY,
        NGX_HTTP_MOVE,
        NGX_HTTP_OPTIONS,
        NGX_HTTP_PROPFIND,
        NGX_HTTP_PROPPATCH,
        NGX_HTTP_LOCK,
        NGX_HTTP_UNLOCK,
        NGX_HTTP_PATCH,
        NGX_HTTP_TRACE,
    ];

    let _status: &[ngx_int_t] = &[
        NGX_HTTP_CONTINUE,
        NGX_HTTP_SWITCHING_PROTOCOLS,
        NGX_HTTP_PROCESSING,
        NGX_HTTP_OK,
        NGX_HTTP_CREATED,
        NGX_HTTP_ACCEPTED,
        NGX_HTTP_NO_CONTENT,
        NGX_HTTP_PARTIAL_CONTENT,
        NGX_HTTP_SPECIAL_RESPONSE,
        NGX_HTTP_MOVED_PERMANENTLY,
        NGX_HTTP_MOVED_TEMPORARILY,
        NGX_HTTP_SEE_OTHER,
        NGX_HTTP_NOT_MODIFIED,
        NGX_HTTP_TEMPORARY_REDIRECT,
        NGX_HTTP_PERMANENT_REDIRECT,
        NGX_HTTP_BAD_REQUEST,
        NGX_HTTP_UNAUTHORIZED,
        NGX_HTTP_FORBIDDEN,
        NGX_HTTP_NOT_FOUND,
        NGX_HTTP_NOT_ALLOWED,
        NGX_HTTP_REQUEST_TIME_OUT,
        NGX_HTTP_CONFLICT,
        NGX_HTTP_LENGTH_REQUIRED,
        NGX_HTTP_PRECONDITION_FAILED,
        NGX_HTTP_REQUEST_ENTITY_TOO_LARGE,
        NGX_HTTP_REQUEST_URI_TOO_LARGE,
        NGX_HTTP_UNSUPPORTED_MEDIA_TYPE,
        NGX_HTTP_RANGE_NOT_SATISFIABLE,
        NGX_HTTP_MISDIRECTED_REQUEST,
        NGX_HTTP_TOO_MANY_REQUESTS,
        NGX_HTTP_CLOSE,
        NGX_HTTP_REQUEST_HEADER_TOO_LARGE,
        NGX_HTTPS_CERT_ERROR,
        NGX_HTTPS_NO_CERT,
        NGX_HTTP_TO_HTTPS,
        NGX_HTTP_CLIENT_CLOSED_REQUEST,
        NGX_HTTP_INTERNAL_SERVER_ERROR,
        NGX_HTTP_NOT_IMPLEMENTED,
        NGX_HTTP_BAD_GATEWAY,
        NGX_HTTP_SERVICE_UNAVAILABLE,
        NGX_HTTP_GATEWAY_TIME_OUT,
        NGX_HTTP_VERSION_NOT_SUPPORTED,
        NGX_HTTP_INSUFFICIENT_STORAGE,
    ];
}

#[test]
#[cfg(all(feature = "mail", ngx_feature = "mail"))]
fn mail_constants() {
    let _module: ngx_uint_t = NGX_MAIL_MODULE;
    let _conf: ngx_uint_t = NGX_MAIL_MAIN_CONF | NGX_MAIL_SRV_CONF;
    let _conf_offset: usize = NGX_MAIL_MAIN_CONF_OFFSET + NGX_MAIL_SRV_CONF_OFFSET;

    let _proto: &[core::ffi::c_uint] =
        &[NGX_MAIL_POP3_PROTOCOL, NGX_MAIL_IMAP_PROTOCOL, NGX_MAIL_SMTP_PROTOCOL];

    let _pop: ngx_uint_t = NGX_POP3_USER;
    let _imap: ngx_uint_t = NGX_IMAP_LOGIN;
    let _smtp: ngx_uint_t = NGX_SMTP_HELO;

    let _err: ngx_int_t = NGX_MAIL_PARSE_INVALID_COMMAND;

    let _auth: &[ngx_int_t] = &[
        NGX_MAIL_AUTH_PLAIN,
        NGX_MAIL_AUTH_LOGIN,
        NGX_MAIL_AUTH_LOGIN_USERNAME,
        NGX_MAIL_AUTH_APOP,
        NGX_MAIL_AUTH_CRAM_MD5,
        NGX_MAIL_AUTH_EXTERNAL,
        NGX_MAIL_AUTH_NONE,
    ];
    let _auth: ngx_uint_t = NGX_MAIL_AUTH_PLAIN_ENABLED
        | NGX_MAIL_AUTH_LOGIN_ENABLED
        | NGX_MAIL_AUTH_APOP_ENABLED
        | NGX_MAIL_AUTH_CRAM_MD5_ENABLED
        | NGX_MAIL_AUTH_EXTERNAL_ENABLED
        | NGX_MAIL_AUTH_NONE_ENABLED;
}

#[test]
#[cfg(all(feature = "stream", ngx_feature = "stream"))]
fn stream_constants() {
    let _module: ngx_uint_t = NGX_STREAM_MODULE;
    let _conf: ngx_uint_t = NGX_STREAM_MAIN_CONF | NGX_STREAM_SRV_CONF | NGX_STREAM_UPS_CONF;
    let _conf_offset: usize = NGX_STREAM_MAIN_CONF_OFFSET + NGX_STREAM_SRV_CONF_OFFSET;

    let _status: &[ngx_int_t] = &[
        NGX_STREAM_OK,
        NGX_STREAM_BAD_REQUEST,
        NGX_STREAM_FORBIDDEN,
        NGX_STREAM_INTERNAL_SERVER_ERROR,
        NGX_STREAM_BAD_GATEWAY,
        NGX_STREAM_SERVICE_UNAVAILABLE,
    ];
}
