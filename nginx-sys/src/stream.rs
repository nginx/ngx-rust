use core::mem::offset_of;

use crate::bindings::ngx_stream_conf_ctx_t;

/// The offset of the `main_conf` field in the `ngx_stream_conf_ctx_t` struct.
///
/// This is used to access the main configuration context for a STREAM module.
pub const NGX_STREAM_MAIN_CONF_OFFSET: usize = offset_of!(ngx_stream_conf_ctx_t, main_conf);

/// The offset of the `srv_conf` field in the `ngx_stream_conf_ctx_t` struct.
///
/// This is used to access the server configuration context for a STREAM module.
pub const NGX_STREAM_SRV_CONF_OFFSET: usize = offset_of!(ngx_stream_conf_ctx_t, srv_conf);
