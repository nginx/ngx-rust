//! Wrapper for the nginx slab pool allocator.
//!
//! See <https://nginx.org/en/docs/dev/development_guide.html#shared_memory>.
use core::alloc::Layout;
use core::cmp;
use core::ptr::{self, NonNull};

use nginx_sys::{
    ngx_shm_zone_t, ngx_shmtx_lock, ngx_shmtx_unlock, ngx_slab_alloc_locked, ngx_slab_free_locked,
    ngx_slab_pool_t,
};

use crate::allocator::{dangling_for_layout, AllocError, Allocator};

/// Non-owning wrapper for an [`ngx_slab_pool_t`] pointer, providing methods for working with
/// shared memory slab pools.
///
/// See <https://nginx.org/en/docs/dev/development_guide.html#shared_memory>.
#[derive(Clone, Debug)]
pub struct SlabPool(NonNull<ngx_slab_pool_t>);

unsafe impl Send for SlabPool {}
unsafe impl Sync for SlabPool {}

unsafe impl Allocator for SlabPool {
    #[inline]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.lock().allocate(layout)
    }

    #[inline]
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.lock().deallocate(ptr, layout)
    }

    #[inline]
    fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.lock().allocate_zeroed(layout)
    }

    #[inline]
    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        self.lock().grow(ptr, old_layout, new_layout)
    }

    #[inline]
    unsafe fn grow_zeroed(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        self.lock().grow_zeroed(ptr, old_layout, new_layout)
    }

    #[inline]
    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        self.lock().shrink(ptr, old_layout, new_layout)
    }
}

impl AsRef<ngx_slab_pool_t> for SlabPool {
    #[inline]
    fn as_ref(&self) -> &ngx_slab_pool_t {
        // SAFETY: this wrapper should be constructed with a valid pointer to ngx_slab_pool_t
        unsafe { self.0.as_ref() }
    }
}

impl AsMut<ngx_slab_pool_t> for SlabPool {
    #[inline]
    fn as_mut(&mut self) -> &mut ngx_slab_pool_t {
        // SAFETY: this wrapper should be constructed with a valid pointer to ngx_slab_pool_t
        unsafe { self.0.as_mut() }
    }
}

impl SlabPool {
    /// Creates a new `SlabPool` from an initialized shared zone.
    ///
    /// # Safety
    ///
    /// Shared zones are initialized and safe to use:
    ///  * between the zone init callback and configuration reload in the master process
    ///  * during the whole lifetime of a worker process.
    ///
    /// After the configuration reload (notably, in the cycle pool cleanup handlers), zone addresses
    /// in the old cycle may become unmapped.
    pub unsafe fn from_shm_zone(shm_zone: &ngx_shm_zone_t) -> Option<Self> {
        let ptr = NonNull::new(shm_zone.shm.addr)?.cast();
        Some(Self(ptr))
    }

    /// Locks the slab pool mutex.
    #[inline]
    pub fn lock(&self) -> LockedSlabPool {
        let shpool = self.0.as_ptr();
        unsafe { ngx_shmtx_lock(ptr::addr_of_mut!((*shpool).mutex)) };
        LockedSlabPool(self.0)
    }
}

/// Wrapper for a locked [`ngx_slab_pool_t`] pointer.
pub struct LockedSlabPool(NonNull<ngx_slab_pool_t>);

unsafe impl Allocator for LockedSlabPool {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        if layout.size() == 0 {
            return Ok(NonNull::slice_from_raw_parts(
                dangling_for_layout(&layout),
                layout.size(),
            ));
        }

        // Small slab allocations (size <= ngx_pagesize / 2) are always aligned to the size rounded
        // up to the nearest power of 2.
        // If the requested alignment exceeds size, we can guarantee the alignment by allocating
        // `align()` bytes.
        let size = cmp::max(layout.size(), layout.align());

        let ptr = unsafe { ngx_slab_alloc_locked(self.0.as_ptr(), size) };
        let ptr = NonNull::new(ptr.cast()).ok_or(AllocError)?;

        if ptr.align_offset(layout.align()) != 0 {
            unsafe { self.deallocate(ptr, layout) };
            return Err(AllocError);
        }

        Ok(NonNull::slice_from_raw_parts(ptr, layout.size()))
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        if layout.size() != 0 {
            ngx_slab_free_locked(self.0.as_ptr(), ptr.as_ptr().cast())
        }
    }
}

impl Drop for LockedSlabPool {
    fn drop(&mut self) {
        let shpool = unsafe { self.0.as_mut() };
        unsafe { ngx_shmtx_unlock(&mut shpool.mutex) }
    }
}
