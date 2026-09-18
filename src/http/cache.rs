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
/// `$upstream_cache_status` variable.  Variants line up with the
/// `NGX_HTTP_CACHE_*` constants nginx core publishes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CacheStatus {
    /// The request was a cache miss; nginx forwarded it to upstream
    /// and stored the response.
    Miss,
    /// The request bypassed the cache (e.g. `proxy_cache_bypass`).
    Bypass,
    /// The cached response had expired and was refreshed from
    /// upstream.
    Expired,
    /// A stale cached response was served (e.g. while upstream
    /// was unreachable).
    Stale,
    /// A stale cached response was served while the cache entry is
    /// being refreshed in the background.
    Updating,
    /// The cached response was revalidated against upstream and
    /// served from cache.
    Revalidated,
    /// The cached response was served directly without contacting
    /// upstream.
    Hit,
    /// `proxy_cache_min_uses` not yet reached; the response was not
    /// cached on this miss.
    Scarce,
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
    fn from_raw_rejects_no_cache_sentinel_and_unknown_values() {
        assert_eq!(CacheStatus::from_raw(0), None);
        assert_eq!(CacheStatus::from_raw(9), None);
        assert_eq!(CacheStatus::from_raw(ngx_uint_t::MAX), None);
    }
}
