//! HTTP cache helpers.
//!
//! Only available on nginx builds where `--with-http_cache` (the
//! default for the stock distribution) is enabled.

use crate::ffi::{
    NGX_HTTP_CACHE_BYPASS, NGX_HTTP_CACHE_EXPIRED, NGX_HTTP_CACHE_HIT, NGX_HTTP_CACHE_MISS,
    NGX_HTTP_CACHE_REVALIDATED, NGX_HTTP_CACHE_SCARCE, NGX_HTTP_CACHE_STALE,
    NGX_HTTP_CACHE_UPDATING, ngx_uint_t,
};

/// Outcome of nginx's cache lookup for a request, mirroring the
/// `$upstream_cache_status` variable.
///
/// The discriminants are the `NGX_HTTP_CACHE_*` constants nginx core
/// publishes, so a variant casts back to the value nginx reports.  A
/// module that keys its own storage by cache status needs that number
/// rather than the variant, and nginx numbers the statuses from one -
/// a default-numbered enum would answer one less at every variant, and
/// keep answering it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
#[non_exhaustive]
pub enum CacheStatus {
    /// The request was a cache miss; nginx forwarded it to upstream
    /// and stored the response.
    Miss = NGX_HTTP_CACHE_MISS,
    /// The request bypassed the cache (e.g. `proxy_cache_bypass`).
    Bypass = NGX_HTTP_CACHE_BYPASS,
    /// The cached response had expired and was refreshed from
    /// upstream.
    Expired = NGX_HTTP_CACHE_EXPIRED,
    /// A stale cached response was served (e.g. while upstream
    /// was unreachable).
    Stale = NGX_HTTP_CACHE_STALE,
    /// A stale cached response was served while the cache entry is
    /// being refreshed in the background.
    Updating = NGX_HTTP_CACHE_UPDATING,
    /// The cached response was revalidated against upstream and
    /// served from cache.
    Revalidated = NGX_HTTP_CACHE_REVALIDATED,
    /// The cached response was served directly without contacting
    /// upstream.
    Hit = NGX_HTTP_CACHE_HIT,
    /// `proxy_cache_min_uses` not yet reached; the response was not
    /// cached on this miss.
    Scarce = NGX_HTTP_CACHE_SCARCE,
}

impl CacheStatus {
    /// Convert the raw `r->upstream->cache_status` value reported by
    /// nginx into a typed variant.  Returns `None` for the
    /// "no cache lookup performed" sentinel (`0`) and for any value
    /// outside the documented range, so callers can distinguish
    /// "request had nothing to do with cache" from a known outcome.
    pub fn from_raw(raw: ngx_uint_t) -> Option<Self> {
        match raw as u32 {
            NGX_HTTP_CACHE_MISS => Some(Self::Miss),
            NGX_HTTP_CACHE_BYPASS => Some(Self::Bypass),
            NGX_HTTP_CACHE_EXPIRED => Some(Self::Expired),
            NGX_HTTP_CACHE_STALE => Some(Self::Stale),
            NGX_HTTP_CACHE_UPDATING => Some(Self::Updating),
            NGX_HTTP_CACHE_REVALIDATED => Some(Self::Revalidated),
            NGX_HTTP_CACHE_HIT => Some(Self::Hit),
            NGX_HTTP_CACHE_SCARCE => Some(Self::Scarce),
            _ => None,
        }
    }

    /// The value nginx reports for this status, the inverse of
    /// [`CacheStatus::from_raw`].  `#[non_exhaustive]` stops a caller
    /// writing this mapping itself without a catch-all arm that would
    /// swallow variants added later, so it belongs here.
    pub fn to_raw(self) -> ngx_uint_t {
        self as u32 as ngx_uint_t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_raw_maps_known_values() {
        assert_eq!(CacheStatus::from_raw(NGX_HTTP_CACHE_MISS as _), Some(CacheStatus::Miss));
        assert_eq!(CacheStatus::from_raw(NGX_HTTP_CACHE_BYPASS as _), Some(CacheStatus::Bypass));
        assert_eq!(CacheStatus::from_raw(NGX_HTTP_CACHE_EXPIRED as _), Some(CacheStatus::Expired));
        assert_eq!(CacheStatus::from_raw(NGX_HTTP_CACHE_STALE as _), Some(CacheStatus::Stale));
        assert_eq!(
            CacheStatus::from_raw(NGX_HTTP_CACHE_UPDATING as _),
            Some(CacheStatus::Updating)
        );
        assert_eq!(
            CacheStatus::from_raw(NGX_HTTP_CACHE_REVALIDATED as _),
            Some(CacheStatus::Revalidated)
        );
        assert_eq!(CacheStatus::from_raw(NGX_HTTP_CACHE_HIT as _), Some(CacheStatus::Hit));
        assert_eq!(CacheStatus::from_raw(NGX_HTTP_CACHE_SCARCE as _), Some(CacheStatus::Scarce));
    }

    #[test]
    fn to_raw_is_the_inverse_of_from_raw() {
        // The numbering is nginx's, not the declaration order: a
        // default-numbered enum would answer one less at every variant
        // and still look like a plausible status.
        for raw in [
            NGX_HTTP_CACHE_MISS,
            NGX_HTTP_CACHE_BYPASS,
            NGX_HTTP_CACHE_EXPIRED,
            NGX_HTTP_CACHE_STALE,
            NGX_HTTP_CACHE_UPDATING,
            NGX_HTTP_CACHE_REVALIDATED,
            NGX_HTTP_CACHE_HIT,
            NGX_HTTP_CACHE_SCARCE,
        ] {
            let status = CacheStatus::from_raw(raw as _).expect("known status");
            assert_eq!(status.to_raw(), raw as ngx_uint_t);
            assert_eq!(status as u32, raw);
        }
    }

    #[test]
    fn from_raw_rejects_no_cache_sentinel_and_unknown_values() {
        assert_eq!(CacheStatus::from_raw(0), None);
        assert_eq!(CacheStatus::from_raw(9), None);
        assert_eq!(CacheStatus::from_raw(ngx_uint_t::MAX), None);
    }
}
