//! Helpers around nginx's event-loop globals.

#[cfg(ngx_feature = "stat_stub")]
pub use connection_stats::{ConnectionStats, connection_stats};

#[cfg(ngx_feature = "stat_stub")]
mod connection_stats {
    use crate::ffi::{
        ngx_atomic_t, ngx_stat_accepted, ngx_stat_active, ngx_stat_handled, ngx_stat_reading,
        ngx_stat_requests, ngx_stat_waiting, ngx_stat_writing,
    };

    /// One snapshot of nginx's global connection-state atomics — the
    /// same values that the `ngx_http_stub_status_module` exposes.
    ///
    /// The seven atomics are read independently, so the snapshot is
    /// not consistent across counters; for monitoring use cases the
    /// drift between reads is sub-microsecond and irrelevant.
    #[derive(Clone, Copy, Debug, Default)]
    #[non_exhaustive]
    pub struct ConnectionStats {
        /// Connections currently in use.
        pub active: u64,
        /// Connections currently reading a request header.
        pub reading: u64,
        /// Connections currently writing a response.
        pub writing: u64,
        /// Connections currently idle in keep-alive.
        pub waiting: u64,
        /// Total connections accepted since process start.
        pub accepted: u64,
        /// Total connections handled since process start.
        pub handled: u64,
        /// Total requests served since process start.
        pub requests: u64,
    }

    /// Sample the global `ngx_stat_*` connection counters.
    ///
    /// This is the programmatic equivalent of fetching the
    /// `stub_status` response, suitable for plumbing into custom
    /// exporters (Prometheus, statsd, etc.) without having to scrape
    /// HTTP.  Available only on nginx builds where
    /// `ngx_http_stub_status_module` is compiled in.
    pub fn connection_stats() -> ConnectionStats {
        // SAFETY: the seven atomics are allocated and assigned by
        // nginx core in the master process before workers fork, and
        // are valid for the lifetime of the process.  Each pointer is
        // therefore stable to dereference once.
        unsafe {
            snapshot_from_ptrs(
                ngx_stat_active,
                ngx_stat_reading,
                ngx_stat_writing,
                ngx_stat_waiting,
                ngx_stat_accepted,
                ngx_stat_handled,
                ngx_stat_requests,
            )
        }
    }

    /// Read the seven atomics through the supplied pointers.  Exists
    /// as its own function so unit tests can exercise the field
    /// mapping with stack-allocated atomics, without depending on the
    /// global `ngx_stat_*` symbols being resolvable in the test
    /// binary.
    ///
    /// # Safety
    ///
    /// Every pointer must be valid for a non-volatile read of
    /// `ngx_atomic_t`.  The caller is responsible for upholding that
    /// (in production callers always pass nginx-owned globals, which
    /// satisfy the requirement by construction).
    #[allow(clippy::unnecessary_cast)] // `ngx_atomic_t` is `c_ulong`, which is 32-bit on some targets.
    unsafe fn snapshot_from_ptrs(
        active: *const ngx_atomic_t,
        reading: *const ngx_atomic_t,
        writing: *const ngx_atomic_t,
        waiting: *const ngx_atomic_t,
        accepted: *const ngx_atomic_t,
        handled: *const ngx_atomic_t,
        requests: *const ngx_atomic_t,
    ) -> ConnectionStats {
        unsafe {
            ConnectionStats {
                active: *active as u64,
                reading: *reading as u64,
                writing: *writing as u64,
                waiting: *waiting as u64,
                accepted: *accepted as u64,
                handled: *handled as u64,
                requests: *requests as u64,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn default_is_all_zero() {
            let s = ConnectionStats::default();
            assert_eq!(s.active, 0);
            assert_eq!(s.reading, 0);
            assert_eq!(s.writing, 0);
            assert_eq!(s.waiting, 0);
            assert_eq!(s.accepted, 0);
            assert_eq!(s.handled, 0);
            assert_eq!(s.requests, 0);
        }

        #[test]
        fn snapshot_from_ptrs_maps_each_field() {
            // Use distinct values so a field-to-field swap is caught.
            let active: ngx_atomic_t = 7;
            let reading: ngx_atomic_t = 1;
            let writing: ngx_atomic_t = 2;
            let waiting: ngx_atomic_t = 4;
            let accepted: ngx_atomic_t = 100;
            let handled: ngx_atomic_t = 99;
            let requests: ngx_atomic_t = 250;

            let s = unsafe {
                snapshot_from_ptrs(
                    &raw const active,
                    &raw const reading,
                    &raw const writing,
                    &raw const waiting,
                    &raw const accepted,
                    &raw const handled,
                    &raw const requests,
                )
            };

            assert_eq!(s.active, 7);
            assert_eq!(s.reading, 1);
            assert_eq!(s.writing, 2);
            assert_eq!(s.waiting, 4);
            assert_eq!(s.accepted, 100);
            assert_eq!(s.handled, 99);
            assert_eq!(s.requests, 250);
        }

        #[test]
        #[allow(clippy::unnecessary_cast)] // `ngx_atomic_t::MAX as u64` is a no-op on 64-bit targets.
        fn snapshot_from_ptrs_widens_to_u64_on_32bit_targets() {
            // Pick a value that does not fit in any signed integer
            // narrower than 64 bits.  Confirms that the `as u64`
            // widening cast does not truncate on platforms where
            // `ngx_atomic_t` is 32-bit (the cast becomes a no-op on
            // 64-bit targets).
            let value: ngx_atomic_t = ngx_atomic_t::MAX;
            let p = &raw const value;
            let s = unsafe { snapshot_from_ptrs(p, p, p, p, p, p, p) };
            assert_eq!(s.active, ngx_atomic_t::MAX as u64);
            assert_eq!(s.requests, ngx_atomic_t::MAX as u64);
        }
    }
}
