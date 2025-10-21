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

/// HTTP phases in which a module can register handlers.
#[repr(u32)]
pub enum NgxHttpPhases {
    /// Post-read phase
    PostRead = crate::ngx_http_phases_NGX_HTTP_POST_READ_PHASE,
    /// Server rewrite phase
    ServerRewrite = crate::ngx_http_phases_NGX_HTTP_SERVER_REWRITE_PHASE,
    /// Find configuration phase
    FindConfig = crate::ngx_http_phases_NGX_HTTP_FIND_CONFIG_PHASE,
    /// Rewrite phase
    Rewrite = crate::ngx_http_phases_NGX_HTTP_REWRITE_PHASE,
    /// Post-rewrite phase
    PostRewrite = crate::ngx_http_phases_NGX_HTTP_POST_REWRITE_PHASE,
    /// Pre-access phase
    Preaccess = crate::ngx_http_phases_NGX_HTTP_PREACCESS_PHASE,
    /// Access phase
    Access = crate::ngx_http_phases_NGX_HTTP_ACCESS_PHASE,
    /// Post-access phase
    PostAccess = crate::ngx_http_phases_NGX_HTTP_POST_ACCESS_PHASE,
    /// Pre-content phase
    PreContent = crate::ngx_http_phases_NGX_HTTP_PRECONTENT_PHASE,
    /// Content phase
    Content = crate::ngx_http_phases_NGX_HTTP_CONTENT_PHASE,
    /// Log phase
    Log = crate::ngx_http_phases_NGX_HTTP_LOG_PHASE,
}
