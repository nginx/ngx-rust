use core::mem::offset_of;

use crate::bindings::ngx_http_conf_ctx_t;

/// The offset of the `main_conf` field in the `ngx_http_conf_ctx_t` struct.
///
/// This is used to access the main configuration context for an HTTP module.
pub const NGX_HTTP_MAIN_CONF_OFFSET: usize = offset_of!(ngx_http_conf_ctx_t, main_conf);

/// The offset of the `srv_conf` field in the `ngx_http_conf_ctx_t` struct.
///
/// This is used to access the server configuration context for an HTTP module.
pub const NGX_HTTP_SRV_CONF_OFFSET: usize = offset_of!(ngx_http_conf_ctx_t, srv_conf);

/// The offset of the `loc_conf` field in the `ngx_http_conf_ctx_t` struct.
///
/// This is used to access the location configuration context for an HTTP module.
pub const NGX_HTTP_LOC_CONF_OFFSET: usize = offset_of!(ngx_http_conf_ctx_t, loc_conf);
