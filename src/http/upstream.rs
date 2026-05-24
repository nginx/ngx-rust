use core::ops::Deref;

use crate::core::NgxStr;
use crate::ffi::ngx_http_upstream_state_t;

/// Per-attempt state nginx records for each upstream contact a
/// request makes.  A request that succeeds on the first try has
/// exactly one entry; a request that exercises `proxy_next_upstream`
/// has one entry per attempt (e.g. failed peer, then successful
/// peer) in chronological order.
///
/// Yielded by [`crate::http::Request::upstream_states`].  Each
/// instance borrows from the underlying `ngx_http_request_t`'s pool
/// and is valid for the lifetime of the request.
///
/// Dereferences to [`ngx_http_upstream_state_t`] for the plain fields
/// (`status`, `response_time`, `connect_time`, `header_time`,
/// `queue_time`, `bytes_sent`, `bytes_received`, `response_length`).
///
/// See <https://nginx.org/en/docs/dev/development_guide.html#http_request>
/// for the per-attempt fields nginx tracks.
#[repr(transparent)]
pub struct UpstreamState(ngx_http_upstream_state_t);

impl UpstreamState {
    /// Peer address contacted for this attempt (typically `host:port`
    /// or `unix:/path`), or `None` when no peer was ever selected.
    ///
    /// `None` is the cache-HIT path where `r->upstream` exists (nginx
    /// uses the upstream framework to consult the cache) but no
    /// backend was contacted, plus init-time slots before peer
    /// selection in failure cases.
    pub fn peer(&self) -> Option<&NgxStr> {
        if self.0.peer.is_null() {
            return None;
        }
        // SAFETY: when non-null, `peer` points either to the peer name
        // in the upstream configuration, or, for upstreams in a zone
        // that can be modified at runtime, to a copy nginx makes in the
        // request pool (`ngx_http_upstream_connect`).  Both outlive the
        // request, and so `&self`.
        let s = unsafe { *self.0.peer };
        if s.len == 0 {
            return None;
        }
        Some(unsafe { NgxStr::from_ngx_str(s) })
    }
}

impl Deref for UpstreamState {
    type Target = ngx_http_upstream_state_t;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Define a static upstream peer initializer
///
/// Initializes the upstream 'get', 'free', and 'session' callbacks and gives the module writer an
/// opportunity to set custom data.
///
/// This macro will define the NGINX callback type:
/// `typedef ngx_int_t (*ngx_http_upstream_init_peer_pt)(ngx_http_request_t *r,
/// ngx_http_upstream_srv_conf_t *us)`, we keep this macro name in-sync with its underlying NGINX
/// type, this callback is required to initialize your peer.
///
/// Load Balancing: <https://nginx.org/en/docs/dev/development_guide.html#http_load_balancing>
#[macro_export]
macro_rules! http_upstream_init_peer_pt {
    ( $name: ident, $handler: expr ) => {
        extern "C" fn $name(
            r: *mut $crate::ffi::ngx_http_request_t,
            us: *mut $crate::ffi::ngx_http_upstream_srv_conf_t,
        ) -> $crate::ffi::ngx_int_t {
            let request = unsafe { $crate::http::Request::from_ngx_http_request(r) };
            let status: $crate::core::Status = $handler(request, us);
            status.0
        }
    };
}

#[cfg(test)]
mod tests {
    use core::mem::MaybeUninit;

    use super::*;
    use crate::ffi::ngx_str_t;

    #[test]
    fn peer_is_none_unless_a_name_was_recorded() {
        let mut state = UpstreamState(unsafe { MaybeUninit::zeroed().assume_init() });
        assert!(state.peer().is_none());

        // A non-null pointer to an empty name, as a cache hit leaves it.
        let mut empty = ngx_str_t { len: 0, data: core::ptr::null_mut() };
        state.0.peer = &raw mut empty;
        assert!(state.peer().is_none());

        let bytes = b"10.0.0.1:8080";
        let mut name = ngx_str_t { len: bytes.len(), data: bytes.as_ptr().cast_mut() };
        state.0.peer = &raw mut name;
        assert_eq!(state.peer().map(NgxStr::as_bytes), Some(&bytes[..]));
    }
}
