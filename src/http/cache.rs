//! HTTP cache helpers.
//!
//! Only available on nginx builds where `--with-http_cache` (the
//! default for the stock distribution) is enabled.

use crate::core::NgxStr;
use crate::ffi::{
    NGX_HTTP_CACHE_BYPASS, NGX_HTTP_CACHE_EXPIRED, NGX_HTTP_CACHE_HIT, NGX_HTTP_CACHE_MISS,
    NGX_HTTP_CACHE_REVALIDATED, NGX_HTTP_CACHE_SCARCE, NGX_HTTP_CACHE_STALE,
    NGX_HTTP_CACHE_UPDATING, ngx_http_file_cache_t, ngx_uint_t,
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
}

/// A file cache declared with `proxy_cache_path`, `fastcgi_cache_path`
/// and the like.
///
/// nginx does not keep `max_size` in bytes: `ngx_http_file_cache_init`
/// divides it by the filesystem block size, so read off the struct it is
/// some thousands of times too small, with nothing to suggest anything is
/// wrong.  This type does not deref to the raw struct for that reason;
/// [`FileCache::max_size`] answers in bytes.
///
/// Obtained from [`crate::http::Request::file_cache`].
#[repr(transparent)]
pub struct FileCache(ngx_http_file_cache_t);

impl FileCache {
    /// Wraps a pointer to an initialized file cache.
    ///
    /// # Safety
    ///
    /// `ptr` must point to an `ngx_http_file_cache_t` that
    /// `ngx_http_file_cache_init` has set up, and that stays valid for
    /// `'a`.
    pub(crate) unsafe fn from_ptr<'a>(ptr: *mut ngx_http_file_cache_t) -> &'a Self {
        unsafe { &*ptr.cast::<Self>() }
    }

    /// The name of the `keys_zone`.
    pub fn name(&self) -> &NgxStr {
        // SAFETY: an initialized file cache always has its keys zone.
        unsafe { NgxStr::from_ngx_str((*self.0.shm_zone).shm.name) }
    }

    /// The configured `max_size`, in bytes.
    pub fn max_size(&self) -> u64 {
        self.0.max_size as u64 * self.0.bsize as u64
    }
}

#[cfg(test)]
mod tests {
    use core::mem::MaybeUninit;

    use super::*;

    #[test]
    fn from_raw_follows_nginx_numbering() {
        // nginx numbers the statuses from one: a default-numbered enum
        // would answer one less at every variant and still look like a
        // plausible status.
        for (raw, status) in [
            (NGX_HTTP_CACHE_MISS, CacheStatus::Miss),
            (NGX_HTTP_CACHE_BYPASS, CacheStatus::Bypass),
            (NGX_HTTP_CACHE_EXPIRED, CacheStatus::Expired),
            (NGX_HTTP_CACHE_STALE, CacheStatus::Stale),
            (NGX_HTTP_CACHE_UPDATING, CacheStatus::Updating),
            (NGX_HTTP_CACHE_REVALIDATED, CacheStatus::Revalidated),
            (NGX_HTTP_CACHE_HIT, CacheStatus::Hit),
            (NGX_HTTP_CACHE_SCARCE, CacheStatus::Scarce),
        ] {
            assert_eq!(CacheStatus::from_raw(raw as _), Some(status));
            assert_eq!(status as u32, raw);
        }

        // 0 is what nginx leaves when no cache lookup happened.
        assert_eq!(CacheStatus::from_raw(0), None);
        assert_eq!(CacheStatus::from_raw(9), None);
    }

    #[test]
    fn max_size_comes_back_in_bytes_not_blocks() {
        // What nginx holds after ngx_http_file_cache_init: 1 MiB as 256
        // blocks of 4 KiB.
        let mut raw: ngx_http_file_cache_t = unsafe { MaybeUninit::zeroed().assume_init() };
        raw.max_size = 256;
        raw.bsize = 4096;

        // SAFETY: `raw` outlives `cache`, and the test reads only the two
        // fields it sets above.
        let cache = unsafe { FileCache::from_ptr(&raw mut raw) };
        assert_eq!(cache.max_size(), 1024 * 1024);
    }
}
