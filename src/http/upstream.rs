use crate::core::NgxStr;
use crate::ffi::{ngx_http_upstream_state_t, ngx_msec_t, off_t};

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
        // SAFETY: `peer` points to an `ngx_str_t` owned by the request
        // pool when non-null; it lives at least as long as `&self`.
        let s = unsafe { *self.0.peer };
        if s.len == 0 {
            return None;
        }
        Some(unsafe { NgxStr::from_ngx_str(s) })
    }

    /// HTTP status returned by the peer.  Zero when the attempt
    /// failed before any response was received (e.g. connect error,
    /// timeout).
    pub fn status(&self) -> u16 {
        self.0.status as u16
    }

    /// Total time (in milliseconds) spent on this attempt, from
    /// connect through the last byte of the response.
    pub fn response_time(&self) -> ngx_msec_t {
        self.0.response_time
    }

    /// Time (in milliseconds) the connect() syscall took.  Useful
    /// for distinguishing connection setup latency from full
    /// response latency.
    pub fn connect_time(&self) -> ngx_msec_t {
        self.0.connect_time
    }

    /// Time (in milliseconds) waiting for the peer to start sending
    /// the response header.
    pub fn header_time(&self) -> ngx_msec_t {
        self.0.header_time
    }

    /// Time (in milliseconds) the request spent queued before nginx
    /// dispatched it to this peer.
    pub fn queue_time(&self) -> ngx_msec_t {
        self.0.queue_time
    }

    /// Bytes sent to the peer (the proxied request body).
    pub fn bytes_sent(&self) -> off_t {
        self.0.bytes_sent
    }

    /// Bytes received from the peer (the response, including
    /// headers).
    pub fn bytes_received(&self) -> off_t {
        self.0.bytes_received
    }

    /// Length of the response body advertised by the peer.
    pub fn response_length(&self) -> off_t {
        self.0.response_length
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

    /// Build a zero-initialised `UpstreamState` for tests.  Real
    /// nginx allocates these from the request pool; here we just
    /// need the bytes to be zero so each accessor sees a defined
    /// starting point.
    fn zeroed_state() -> UpstreamState {
        // SAFETY: `ngx_http_upstream_state_t` is `#[repr(C)]` and
        // every field is a scalar or raw pointer for which all-zero
        // bytes is a valid (and defined) value.
        unsafe { MaybeUninit::zeroed().assume_init() }
    }

    #[test]
    fn peer_none_when_pointer_null() {
        let state = zeroed_state();
        assert!(state.peer().is_none());
    }

    #[test]
    fn peer_none_when_str_empty() {
        // The pointer is non-null but the `ngx_str_t` it targets
        // is the zero-length sentinel cache lookups leave behind.
        let empty: ngx_str_t = unsafe { MaybeUninit::zeroed().assume_init() };
        let mut state = zeroed_state();
        state.0.peer = (&raw const empty).cast_mut();
        assert!(state.peer().is_none());
    }

    #[test]
    fn peer_some_when_populated() {
        let bytes = b"10.0.0.1:8080";
        let mut peer = ngx_str_t { len: bytes.len(), data: bytes.as_ptr().cast_mut() };
        let mut state = zeroed_state();
        state.0.peer = &raw mut peer;

        let got = state.peer().expect("peer should resolve");
        assert_eq!(got.as_bytes(), bytes);
    }

    #[test]
    fn accessors_read_underlying_fields() {
        let mut state = zeroed_state();
        state.0.status = 502;
        state.0.response_time = 75;
        state.0.connect_time = 5;
        state.0.header_time = 10;
        state.0.queue_time = 1;
        state.0.bytes_sent = 1024;
        state.0.bytes_received = 4096;
        state.0.response_length = 2048;

        assert_eq!(state.status(), 502);
        assert_eq!(state.response_time(), 75);
        assert_eq!(state.connect_time(), 5);
        assert_eq!(state.header_time(), 10);
        assert_eq!(state.queue_time(), 1);
        assert_eq!(state.bytes_sent(), 1024);
        assert_eq!(state.bytes_received(), 4096);
        assert_eq!(state.response_length(), 2048);
    }

    #[test]
    fn upstream_state_size_matches_underlying_struct() {
        // Guards the slice cast in `Request::upstream_states`: the
        // `#[repr(transparent)]` newtype must have the same layout
        // as the raw nginx struct so the slice elements line up.
        assert_eq!(
            core::mem::size_of::<UpstreamState>(),
            core::mem::size_of::<ngx_http_upstream_state_t>(),
        );
        assert_eq!(
            core::mem::align_of::<UpstreamState>(),
            core::mem::align_of::<ngx_http_upstream_state_t>(),
        );
    }
}
