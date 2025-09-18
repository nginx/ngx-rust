use core::alloc::Layout;
use core::ffi::c_void;
use core::mem;
use core::ptr::{self, NonNull};

use nginx_sys::{
    ngx_buf_t, ngx_create_temp_buf, ngx_palloc, ngx_pfree, ngx_pmemalign, ngx_pnalloc,
    ngx_pool_cleanup_add, ngx_pool_t, NGX_ALIGNMENT,
};

use crate::allocator::{dangling_for_layout, AllocError, Allocator};
use crate::core::buffer::{Buffer, MemoryBuffer, TemporaryBuffer};

/// Non-owning wrapper for an [`ngx_pool_t`] pointer, providing methods for working with memory pools.
///
/// See <https://nginx.org/en/docs/dev/development_guide.html#pool>
#[derive(Clone, Debug)]
#[repr(transparent)]
pub struct Pool(NonNull<ngx_pool_t>);

unsafe impl Allocator for Pool {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        // SAFETY:
        // * This wrapper should be constructed with a valid pointer to ngx_pool_t.
        // * The Pool type is !Send, thus we expect exclusive access for this call.
        // * Pointers are considered mutable unless obtained from an immutable reference.
        let ptr = if layout.size() == 0 {
            // We can guarantee alignment <= NGX_ALIGNMENT for allocations of size 0 made with
            // ngx_palloc_small. Any other cases are implementation-defined, and we can't tell which
            // one will be used internally.
            return Ok(NonNull::slice_from_raw_parts(
                dangling_for_layout(&layout),
                layout.size(),
            ));
        } else if layout.align() == 1 {
            unsafe { ngx_pnalloc(self.0.as_ptr(), layout.size()) }
        } else if layout.align() <= NGX_ALIGNMENT {
            unsafe { ngx_palloc(self.0.as_ptr(), layout.size()) }
        } else if cfg!(any(
            ngx_feature = "have_posix_memalign",
            ngx_feature = "have_memalign"
        )) {
            // ngx_pmemalign is always defined, but does not guarantee the requested alignment
            // unless memalign/posix_memalign exists.
            unsafe { ngx_pmemalign(self.0.as_ptr(), layout.size(), layout.align()) }
        } else {
            return Err(AllocError);
        };

        // Verify the alignment of the result
        debug_assert_eq!(ptr.align_offset(layout.align()), 0);

        let ptr = NonNull::new(ptr.cast()).ok_or(AllocError)?;
        Ok(NonNull::slice_from_raw_parts(ptr, layout.size()))
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        // ngx_pfree is noop for small allocations unless NGX_DEBUG_PALLOC is set.
        //
        // Note: there should be no cleanup handlers for the allocations made using this API.
        // Violating that could result in the following issues:
        //  - use-after-free on large allocation
        //  - multiple cleanup handlers attached to a dangling ptr (these are not unique)
        if layout.size() > 0 // 0 is dangling ptr
            && (layout.size() > self.as_ref().max || layout.align() > NGX_ALIGNMENT)
        {
            ngx_pfree(self.0.as_ptr(), ptr.as_ptr().cast());
        }
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        debug_assert!(
            new_layout.size() >= old_layout.size(),
            "`new_layout.size()` must be greater than or equal to `old_layout.size()`"
        );
        self.resize(ptr, old_layout, new_layout)
    }

    unsafe fn grow_zeroed(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        debug_assert!(
            new_layout.size() >= old_layout.size(),
            "`new_layout.size()` must be greater than or equal to `old_layout.size()`"
        );
        #[allow(clippy::manual_inspect)]
        self.resize(ptr, old_layout, new_layout).map(|new_ptr| {
            unsafe {
                new_ptr
                    .cast::<u8>()
                    .byte_add(old_layout.size())
                    .write_bytes(0, new_layout.size() - old_layout.size())
            };
            new_ptr
        })
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        debug_assert!(
            new_layout.size() <= old_layout.size(),
            "`new_layout.size()` must be smaller than or equal to `old_layout.size()`"
        );
        self.resize(ptr, old_layout, new_layout)
    }
}

impl AsRef<ngx_pool_t> for Pool {
    #[inline]
    fn as_ref(&self) -> &ngx_pool_t {
        // SAFETY: this wrapper should be constructed with a valid pointer to ngx_pool_t
        unsafe { self.0.as_ref() }
    }
}

impl AsMut<ngx_pool_t> for Pool {
    #[inline]
    fn as_mut(&mut self) -> &mut ngx_pool_t {
        // SAFETY: this wrapper should be constructed with a valid pointer to ngx_pool_t
        unsafe { self.0.as_mut() }
    }
}

impl Pool {
    /// Creates a new `Pool` from an `ngx_pool_t` pointer.
    ///
    /// # Safety
    /// The caller must ensure that a valid `ngx_pool_t` pointer is provided, pointing to valid
    /// memory and non-null. A null argument will cause an assertion failure and panic.
    pub unsafe fn from_ngx_pool(pool: *mut ngx_pool_t) -> Pool {
        debug_assert!(!pool.is_null());
        debug_assert!(pool.is_aligned());
        Pool(NonNull::new_unchecked(pool))
    }

    /// Expose the underlying `ngx_pool_t` pointer, for use with `ngx::ffi`
    /// functions.
    pub fn as_ptr(&self) -> *mut ngx_pool_t {
        self.0.as_ptr()
    }

    /// Creates a buffer of the specified size in the memory pool.
    ///
    /// Returns `Some(TemporaryBuffer)` if the buffer is successfully created, or `None` if
    /// allocation fails.
    pub fn create_buffer(&self, size: usize) -> Option<TemporaryBuffer> {
        let buf = unsafe { ngx_create_temp_buf(self.0.as_ptr(), size) };
        if buf.is_null() {
            return None;
        }

        Some(TemporaryBuffer::from_ngx_buf(buf))
    }

    /// Creates a buffer from a string in the memory pool.
    ///
    /// Returns `Some(TemporaryBuffer)` if the buffer is successfully created, or `None` if
    /// allocation fails.
    pub fn create_buffer_from_str(&self, str: &str) -> Option<TemporaryBuffer> {
        let mut buffer = self.create_buffer(str.len())?;
        unsafe {
            let buf = buffer.as_ngx_buf_mut();
            ptr::copy_nonoverlapping(str.as_ptr(), (*buf).pos, str.len());
            (*buf).last = (*buf).pos.add(str.len());
        }
        Some(buffer)
    }

    /// Creates a buffer from a static string in the memory pool.
    ///
    /// Returns `Some(MemoryBuffer)` if the buffer is successfully created, or `None` if allocation
    /// fails.
    pub fn create_buffer_from_static_str(&self, str: &'static str) -> Option<MemoryBuffer> {
        let buf = self.allocate(Layout::new::<ngx_buf_t>()).ok()?.as_ptr() as *mut ngx_buf_t;

        // We cast away const, but buffers with the memory flag are read-only
        let start = str.as_ptr() as *mut u8;
        let end = unsafe { start.add(str.len()) };

        unsafe {
            (*buf).start = start;
            (*buf).pos = start;
            (*buf).last = end;
            (*buf).end = end;
            (*buf).set_memory(1);
        }

        Some(MemoryBuffer::from_ngx_buf(buf))
    }

    /// Adds a cleanup handler for a value in the memory pool.
    ///
    /// Returns `Ok(())` if the cleanup handler is successfully added, or `Err(())` if the cleanup
    /// handler cannot be added.
    ///
    /// # Safety
    /// This function is marked as unsafe because it involves raw pointer manipulation.
    unsafe fn add_cleanup_for_value<T>(&self, value: *mut T) -> Result<(), ()> {
        let cln = ngx_pool_cleanup_add(self.0.as_ptr(), 0);
        if cln.is_null() {
            return Err(());
        }
        (*cln).handler = Some(cleanup_type::<T>);
        (*cln).data = value as *mut c_void;

        Ok(())
    }

    /// Allocates memory from the pool of the specified size.
    /// The resulting pointer is aligned to a platform word size.
    ///
    /// Returns a raw pointer to the allocated memory.
    pub fn alloc(&self, size: usize) -> *mut c_void {
        unsafe { ngx_palloc(self.0.as_ptr(), size) }
    }

    /// Allocates memory for a value of a specified type and adds a cleanup handler to the memory
    /// pool.
    ///
    /// Returns a typed pointer to the allocated memory if successful, or a null pointer if
    /// allocation or cleanup handler addition fails.
    pub fn alloc_with_cleanup<T>(&self, value: T) -> *mut T {
        unsafe {
            let p = self.alloc(mem::size_of::<T>()) as *mut T;
            ptr::write(p, value);
            if self.add_cleanup_for_value(p).is_err() {
                ptr::drop_in_place(p);
                return ptr::null_mut();
            };
            p
        }
    }

    /// Allocates unaligned memory from the pool of the specified size.
    ///
    /// Returns Result::Ok with a typed pointer to the allocated memory if successful,
    /// or Result::Err(AllocError) if allocation or cleanup handler addition fails.
    pub fn allocate_unaligned(&self, size: usize) -> Result<NonNull<[u8]>, AllocError> {
        self.allocate(unsafe { Layout::from_size_align_unchecked(size, 1) })
    }

    /// Allocates memory for a type from the pool.
    /// The resulting pointer is aligned to a platform word size.
    ///
    /// Returns Result::Ok with a typed pointer to the allocated memory if successful,
    /// or Result::Err(AllocError) if allocation fails.
    pub fn allocate_type<T>(&self) -> Result<NonNull<T>, AllocError> {
        self.allocate(Layout::new::<T>()).map(|ptr| ptr.cast())
    }

    /// Allocates zeroed memory for a type from the pool.
    /// The resulting pointer is aligned to a platform word size.
    ///
    /// Returns Result::Ok with a typed pointer to the allocated memory if successful,
    /// or Result::Err(AllocError) if allocation fails.
    pub fn allocate_type_zeroed<T>(&self) -> Result<NonNull<T>, AllocError> {
        self.allocate_zeroed(Layout::new::<T>())
            .map(|ptr| ptr.cast())
    }

    /// Resizes a memory allocation in place if possible.
    ///
    /// If resizing is requested for the last allocation in the pool, it may be
    /// possible to adjust pool data and avoid any real allocations.
    ///
    /// # Safety
    /// `ptr` must point to allocated address and `old_layout` must match the current layout
    /// of the allocation.
    #[inline(always)]
    unsafe fn resize(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        if ptr.byte_add(old_layout.size()).as_ptr() == self.as_ref().d.last
            && ptr.byte_add(new_layout.size()).as_ptr() <= self.as_ref().d.end
            && ptr.align_offset(new_layout.align()) == 0
        {
            let pool = self.0.as_ptr();
            unsafe { (*pool).d.last = ptr.byte_add(new_layout.size()).as_ptr() };
            Ok(NonNull::slice_from_raw_parts(ptr, new_layout.size()))
        } else {
            let size = core::cmp::min(old_layout.size(), new_layout.size());
            let new_ptr = <Self as Allocator>::allocate(self, new_layout)?;
            unsafe {
                ptr.copy_to_nonoverlapping(new_ptr.cast(), size);
                self.deallocate(ptr, old_layout);
            }
            Ok(new_ptr)
        }
    }
}

/// Cleanup handler for a specific type `T`.
///
/// This function is called when cleaning up a value of type `T` in an FFI context.
///
/// # Safety
/// This function is marked as unsafe due to the raw pointer manipulation and the assumption that
/// `data` is a valid pointer to `T`.
///
/// # Arguments
///
/// * `data` - A raw pointer to the value of type `T` to be cleaned up.
unsafe extern "C" fn cleanup_type<T>(data: *mut c_void) {
    ptr::drop_in_place(data as *mut T);
}
