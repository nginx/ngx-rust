use core::alloc::Layout;
use core::ffi::c_void;
use core::mem;
use core::ptr::{self, NonNull};

use nginx_sys::{
    NGX_ALIGNMENT, ngx_buf_t, ngx_create_temp_buf, ngx_palloc, ngx_pcalloc, ngx_pfree,
    ngx_pmemalign, ngx_pnalloc, ngx_pool_cleanup_add, ngx_pool_cleanup_t, ngx_pool_t,
};

use crate::allocator::{AllocError, Allocator, dangling_for_layout};
use crate::core::buffer::{Buffer, MemoryBuffer, TemporaryBuffer};

/// Non-owning wrapper for an [`ngx_pool_t`] pointer, providing methods for working with memory
/// pools.
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
            return Ok(NonNull::slice_from_raw_parts(dangling_for_layout(&layout), layout.size()));
        } else if layout.align() == 1 {
            unsafe { ngx_pnalloc(self.0.as_ptr(), layout.size()) }
        } else if layout.align() <= NGX_ALIGNMENT {
            unsafe { ngx_palloc(self.0.as_ptr(), layout.size()) }
        } else if cfg!(any(ngx_feature = "have_posix_memalign", ngx_feature = "have_memalign")) {
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
            unsafe { ngx_pfree(self.0.as_ptr(), ptr.as_ptr().cast()) };
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
        unsafe { self.resize(ptr, old_layout, new_layout) }
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
        unsafe {
            #[allow(clippy::manual_inspect)]
            self.resize(ptr, old_layout, new_layout).map(|new_ptr| {
                new_ptr
                    .cast::<u8>()
                    .byte_add(old_layout.size())
                    .write_bytes(0, new_layout.size() - old_layout.size());
                new_ptr
            })
        }
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
        unsafe { self.resize(ptr, old_layout, new_layout) }
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
        unsafe {
            debug_assert!(!pool.is_null());
            debug_assert!(pool.is_aligned());
            Pool(NonNull::new_unchecked(pool))
        }
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
        let buf = self.calloc_type::<ngx_buf_t>();
        if buf.is_null() {
            return None;
        }

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

    /// Allocates memory for a value and adds a cleanup handler to the memory pool.
    ///
    /// The value is created by calling the provided closure `f`. If allocation fails,
    /// the closure is not called.
    ///
    /// Returns `Ok(NonNull<T>)` if the allocation and cleanup handler addition are successful,
    /// or `Err(AllocError)` if allocation fails.
    ///
    /// # Safety
    /// The returned pointer must not outlive the pool, must not be freed manually
    /// (as it has a cleanup handler), and must not be accessed after the pool is destroyed.
    /// In case of types with non-standard alignment, the returned pointer may not be properly
    /// aligned.
    pub unsafe fn allocate_with_cleanup<T, F: FnOnce() -> T>(
        &self,
        f: F,
    ) -> Result<NonNull<T>, AllocError> {
        let cln = unsafe {
            ngx_pool_cleanup_add(self.0.as_ptr(), mem::size_of::<T>()).as_mut().ok_or(AllocError)?
        };
        // 'data' may be NULL only if `T` is zero-sized. In that case, no real value is stored,
        // so we can just use the cleanup structure itself as a placeholder.
        // Note that zero-sized `T` may implement `Drop`, and this implementation will be
        // called at cleanup time.
        if cln.data.is_null() {
            cln.data = ptr::from_mut(cln).cast();
        }
        cln.handler = Some(cleanup_type::<T>);
        // `data` points to the memory allocated for the value by `ngx_pool_cleanup_add()`
        unsafe { ptr::write(cln.data as *mut T, f()) };
        NonNull::new(cln.data.cast()).ok_or(AllocError)
    }

    /// Allocates memory from the pool of the specified size.
    /// The resulting pointer is aligned to a platform word size.
    ///
    /// Returns a raw pointer to the allocated memory.
    pub fn alloc(&self, size: usize) -> *mut c_void {
        unsafe { ngx_palloc(self.0.as_ptr(), size) }
    }

    /// Allocates memory for a type from the pool.
    /// The resulting pointer is aligned to a platform word size.
    ///
    /// Returns a typed pointer to the allocated memory.
    pub fn alloc_type<T: Copy>(&self) -> *mut T {
        self.alloc(mem::size_of::<T>()) as *mut T
    }

    /// Allocates zeroed memory from the pool of the specified size.
    /// The resulting pointer is aligned to a platform word size.
    ///
    /// Returns a raw pointer to the allocated memory.
    pub fn calloc(&self, size: usize) -> *mut c_void {
        unsafe { ngx_pcalloc(self.0.as_ptr(), size) }
    }

    /// Allocates zeroed memory for a type from the pool.
    /// The resulting pointer is aligned to a platform word size.
    ///
    /// Returns a typed pointer to the allocated memory.
    pub fn calloc_type<T: Copy>(&self) -> *mut T {
        self.calloc(mem::size_of::<T>()) as *mut T
    }

    /// Allocates unaligned memory from the pool of the specified size.
    ///
    /// Returns a raw pointer to the allocated memory.
    pub fn alloc_unaligned(&self, size: usize) -> *mut c_void {
        unsafe { ngx_pnalloc(self.0.as_ptr(), size) }
    }

    /// Allocates unaligned memory for a type from the pool.
    ///
    /// Returns a typed pointer to the allocated memory.
    pub fn alloc_type_unaligned<T: Copy>(&self) -> *mut T {
        self.alloc_unaligned(mem::size_of::<T>()) as *mut T
    }

    /// Allocates memory for a value of a specified type and adds a cleanup handler to the memory
    /// pool.
    ///
    /// Returns a typed pointer to the allocated memory if successful, or a null pointer if
    /// allocation or cleanup handler addition fails.
    pub fn allocate<T>(&self, value: T) -> *mut T {
        unsafe {
            match self.allocate_with_cleanup(|| value) {
                Err(_) => ptr::null_mut(),
                Ok(mut ptr) => ptr.as_mut(),
            }
        }
    }

    /// Runs the cleanup handler for a value and removes it.
    ///
    /// Returns `Some(())` if the value was successfully removed,
    /// or `None` if the value was not found.
    ///
    /// # Safety
    /// The caller must ensure that `value` is a valid pointer to a value that has an
    /// associated cleanup handler in the pool.
    pub unsafe fn remove<T>(&mut self, value: *const T) -> Option<()> {
        // Comparing function pointers is generally unreliable, but in this specific
        // case we can assume that the same function pointer was used when adding the cleanup
        // handler.
        #[allow(unpredictable_function_pointer_comparisons)]
        self.remove_cleanup_if(|cln| {
            cln.handler == Some(cleanup_type::<T>) && core::ptr::addr_eq(cln.data, value)
        })
        .map(|cln| {
            unsafe { cln.handler.unwrap()(cln.data) };
        })
    }

    /// Internal method to find and remove a cleanup handler matching a predicate.
    ///
    /// Returns `Some(&ngx_pool_cleanup_t)` if a matching handler is found and removed,
    /// or `None` if not found.
    fn remove_cleanup_if(
        &mut self,
        predicate: impl Fn(&ngx_pool_cleanup_t) -> bool,
    ) -> Option<&ngx_pool_cleanup_t> {
        let mut top = ngx_pool_cleanup_t {
            handler: None,
            data: core::ptr::null_mut(),
            next: unsafe { self.0.as_mut().cleanup },
        };

        let mut prev = &mut top;
        let top_ptr = prev as *const _;

        while let Some(cln) = unsafe { prev.next.as_mut() } {
            if predicate(cln) {
                if core::ptr::eq(prev, top_ptr) {
                    unsafe { self.0.as_mut().cleanup = cln.next };
                } else {
                    prev.next = cln.next;
                }
                return Some(cln);
            }
            prev = cln;
        }

        None
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
        if unsafe {
            ptr.byte_add(old_layout.size()).as_ptr() == self.as_ref().d.last
                && ptr.byte_add(new_layout.size()).as_ptr() <= self.as_ref().d.end
                && ptr.align_offset(new_layout.align()) == 0
        } {
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
/// This function is used as a type-specific cleanup handler for the `Pool`.
/// The handler is added to the pool's cleanup chain when allocating a value of type `T`
/// with a cleanup. It is called when the pool is destroyed or when the value is removed,
/// and it drops the value of type `T`.
///
/// # Safety
/// `data` should be a valid, writable, and properly aligned pointer to `T`.
unsafe extern "C" fn cleanup_type<T>(data: *mut c_void) {
    unsafe {
        ptr::drop_in_place(data as *mut T);
    }
}
