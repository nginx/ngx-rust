use core::cell::Cell;

use bindgen::callbacks::{IntKind, ItemInfo, MacroParsingBehavior};

#[derive(Debug, Default)]
pub struct NginxCallbacks {
    off_t_is_macro: Cell<bool>,
}

impl bindgen::callbacks::ParseCallbacks for NginxCallbacks {
    fn will_parse_macro(&self, name: &str) -> MacroParsingBehavior {
        // Detect and rename (in item_name) libc types defined as a macro instead of alias.
        if name == "off_t" {
            self.off_t_is_macro.set(true);
        }

        MacroParsingBehavior::Default
    }

    fn int_macro(&self, name: &str, value: i64) -> Option<IntKind> {
        // Static rules
        for (pat, kind) in INT_MACRO_RULES.iter() {
            if name_matches(name, pat) {
                return Some(*kind);
            }
        }

        if name.starts_with("NGX_") {
            // Likely status codes (ngx_int_t).
            // If something matching is not a status code, add it to the static rules.
            if name.starts_with("NGX_HTTP_") && (100..600).contains(&value) {
                return Some(NGX_INT_T);
            }

            if name.starts_with("NGX_STREAM_") && (400..600).contains(&value) {
                return Some(NGX_INT_T);
            }

            // Fallback behavior for all unhandled NGX_ constants:
            // try to extend to ngx_int_t/ngx_uint_t, depending on the sign and value.

            // On 32-bit platforms, 64-bit constants (off_t, time_t) can exceed
            // ngx_(u)int_t limits. Let bindgen assign types for these.
            if *TARGET_BITS == 32 && (value < i32::MIN as i64 || value > u32::MAX as i64) {
                return None;
            }

            return Some(if value < 0 { NGX_INT_T } else { NGX_UINT_T });
        }

        None
    }

    fn item_name(&self, item_info: ItemInfo) -> Option<String> {
        match item_info.name {
            "__off_t" if self.off_t_is_macro.get() => Some("off_t".to_string()),
            _ => None,
        }
    }
}

/// Matches the name against a wildcard pattern with at most one '*'
fn name_matches(name: &str, pat: &str) -> bool {
    if let Some((prefix, suffix)) = pat.split_once('*') {
        name.starts_with(prefix) && name.ends_with(suffix)
    } else {
        pat == name
    }
}

static TARGET_BITS: std::sync::LazyLock<u32> = std::sync::LazyLock::new(|| {
    std::env::var("CARGO_CFG_TARGET_POINTER_WIDTH")
        .expect("always set for buildscripts")
        .parse::<u32>()
        .expect("an integer value")
});

const NGX_INT_T: IntKind = IntKind::Custom { name: "ngx_int_t", is_signed: true };
const NGX_UINT_T: IntKind = IntKind::Custom { name: "ngx_uint_t", is_signed: false };

/*
 * Type mappings for NGX_ macros from the public headers.
 * The list doesn't have to be exhaustive, but it should be good enough to call
 * common functions without type casts.
 *
 * Note that some definitions can be referenced in different contexts;
 * for example, HTTP status codes can be used both as ngx_int_t and ngx_uint_t.
 */
const INT_MACRO_RULES: &[(&str, IntKind)] = const {
    // ngx_err_t is unsigned on Windows, but it shouldn't matter as all the values are unsigned too.
    const NGX_ERR_T: IntKind = IntKind::Custom { name: "ngx_err_t", is_signed: true };
    const NGX_FD_T: IntKind = IntKind::Custom { name: "ngx_fd_t", is_signed: true };
    const NGX_MSEC_T: IntKind = IntKind::Custom { name: "ngx_msec_t", is_signed: false };
    const NGX_PID_T: IntKind = IntKind::Custom { name: "ngx_pid_t", is_signed: true };
    const OFF_T: IntKind = IntKind::Custom { name: "off_t", is_signed: true };
    const SIZE_T: IntKind = IntKind::Custom { name: "usize", is_signed: false };
    const SSIZE_T: IntKind = IntKind::Custom { name: "isize", is_signed: true };
    const TIME_T: IntKind = IntKind::Custom { name: "time_t", is_signed: true };

    &[
        ("nginx_version", NGX_UINT_T),
        ("NGX_OK", NGX_INT_T),
        ("NGX_AT_FDCWD", NGX_FD_T),
        ("NGX_FILE_ERROR", IntKind::Int),
        ("NGX_INVALID_FILE", NGX_FD_T),
        ("NGX_INVALID_PID", NGX_PID_T),
        ("NGX_MODULE_UNSET_INDEX", NGX_UINT_T),
        ("NGX_OPEN_FILE_DIRECTIO_OFF", OFF_T),
        ("NGX_TIMER_INFINITE", NGX_MSEC_T),
        // NGX_*_CONF, ngx_command_t.type
        ("NGX_*_CONF", NGX_UINT_T),
        // ngx_int_t event
        ("NGX_READ_EVENT", NGX_INT_T),
        ("NGX_VNODE_EVENT", NGX_INT_T),
        ("NGX_WRITE_EVENT", NGX_INT_T),
        // NGX_HAVE_, NGX_USE_, event flags
        ("NGX_*_EVENT", NGX_UINT_T),
        ("NGX_*_MODULE", NGX_UINT_T),
        // NGX_{READ,WRITE,RDWR}_SHUTDOWN
        ("NGX_*_SHUTDOWN", IntKind::Int),
        ("NGX_CONF_BLOCK_START", NGX_INT_T),
        ("NGX_CONF_BLOCK_DONE", NGX_INT_T),
        ("NGX_CONF_FILE_DONE", NGX_INT_T),
        ("NGX_CONF_UNSET_UINT", NGX_UINT_T),
        ("NGX_CONF_UNSET_SIZE", SIZE_T),
        ("NGX_CONF_UNSET_MSEC", NGX_MSEC_T),
        ("NGX_DISABLE_SYMLINKS_*", IntKind::UInt),
        ("NGX_ERROR", NGX_INT_T),
        // NGX_ESCAPE_* in ngx_string.h
        ("NGX_ESCAPE_*", NGX_UINT_T),
        // Likely error codes from ngx_errno.h.
        // Anything that is not should be added above.
        ("NGX_E*", NGX_ERR_T),
        // Http
        ("NGX_HTTP_*_BUFFERED", IntKind::UInt),
        ("NGX_HTTP_CACHE_*", IntKind::UInt),
        ("NGX_HTTP_COPY", NGX_UINT_T),
        ("NGX_HTTP_GZIP_PROXIED_*", NGX_UINT_T),
        ("NGX_HTTP_MAX_BLOCKED", NGX_UINT_T),
        ("NGX_HTTP_MOVE", NGX_UINT_T),
        ("NGX_HTTP_OPTIONS", NGX_UINT_T),
        ("NGX_HTTP_UPSTREAM_EARLY_HINTS", NGX_INT_T),
        ("NGX_HTTP_UPSTREAM_INVALID_HEADER", NGX_INT_T),
        ("NGX_HTTP_UPSTREAM_*", NGX_UINT_T),
        ("NGX_HTTP_V2_ENCODE_HUFF", IntKind::UChar),
        // Used as ngx_uint_t when passing to ngx_quic_finalize_connection
        // and as ngx_int_t for various h3 method return values.
        ("NGX_HTTP_V3_ERR_*", NGX_UINT_T),
        // NGX_HTTPS_NO_CERT, NGX_HTTPS_CERT_ERROR statuses
        ("NGX_HTTPS_*", NGX_INT_T),
        // Log
        ("NGX_LOG_*", NGX_UINT_T),
        // Mail
        ("NGX_MAIL_*_PROTOCOL", IntKind::UInt),
        ("NGX_MAIL_AUTH_*_ENABLED", NGX_UINT_T),
        ("NGX_MAIL_AUTH_*", NGX_INT_T),
        ("NGX_MAIL_PARSE_INVALID_COMMAND", NGX_INT_T),
        //
        ("NGX_MAX_INT_T_VALUE", NGX_INT_T),
        ("NGX_MAX_INT32_VALUE", IntKind::I32),
        ("NGX_MAX_OFF_T_VALUE", OFF_T),
        ("NGX_MAX_SIZE_T_VALUE", SIZE_T),
        ("NGX_MAX_TIME_T_VALUE", TIME_T),
        ("NGX_MAX_UINT32_VALUE", IntKind::U32),
        // Resolver
        ("NGX_RESOLVE_FORMERR", NGX_INT_T),
        ("NGX_RESOLVE_SERVFAIL", NGX_INT_T),
        ("NGX_RESOLVE_NXDOMAIN", NGX_INT_T),
        ("NGX_RESOLVE_NOTIMP", NGX_INT_T),
        ("NGX_RESOLVE_REFUSED", NGX_INT_T),
        ("NGX_RESOLVE_TIMEDOUT", NGX_INT_T),
        // SSL: ssize_t builtin_session_cache
        ("NGX_SSL_*_SCACHE", SSIZE_T),
        // Stream
        ("NGX_STREAM_*_BUFFERED", IntKind::UInt),
        ("NGX_STREAM_OK", NGX_INT_T),
        ("NGX_STREAM_UPSTREAM_*", NGX_UINT_T),
    ]
};
