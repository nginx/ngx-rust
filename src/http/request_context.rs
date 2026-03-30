use core::ffi::c_void;
use core::marker::PhantomData;

use allocator_api2::alloc::AllocError;
use nginx_sys::ngx_http_request_t;

use crate::{
    core::Pool,
    http::{HttpModule, Request},
};

/// Request wrapper with context support.
pub struct RequestWithContext<M, T>(ngx_http_request_t, PhantomData<(M, T)>)
where
    M: HttpModule,
    T: Sized;

impl<M, T> RequestWithContext<M, T>
where
    M: HttpModule,
    T: Sized,
{
    /// Creates a new `RequestWithContext` from a raw pointer to `ngx_http_request_t`.
    ///
    /// # Safety
    ///
    /// The caller has provided a valid non-null pointer to a valid `ngx_http_request_t`
    /// which shares the same representation as `Request`.
    pub unsafe fn from_ngx_http_request<'a>(r: *mut ngx_http_request_t) -> &'a mut Self {
        unsafe { &mut *r.cast::<Self>() }
    }

    /// Creates a new `RequestWithContext` from a `Request`.
    pub fn from_request(r: &mut Request) -> (&mut Self, &mut Request) {
        let rptr: *mut ngx_http_request_t = r.into();
        unsafe { (&mut *rptr.cast::<Self>(), r) }
    }

    /// Get Module context pointer
    #[inline]
    fn get_module_ctx_ptr(&self) -> *mut c_void {
        unsafe { *self.0.ctx.add(M::module().ctx_index) }
    }

    /// Check if module context exists for a specific module.
    pub fn exists(&self) -> bool {
        !self.get_module_ctx_ptr().is_null()
    }

    /// Get module context for a specific module,
    /// returning `None` if the context is not set.
    pub fn get(&self) -> Option<&T> {
        let ctx_ptr = self.get_module_ctx_ptr();
        if ctx_ptr.is_null() {
            None
        } else {
            Some(unsafe { &*ctx_ptr.cast::<T>() })
        }
    }

    /// Set module context for a specific module, returning a mutable reference to the context
    /// or an `AllocError` if allocation fails.
    pub fn set(&mut self, value: T) -> Result<&mut T, AllocError> {
        let pool = unsafe { Pool::from_ngx_pool(self.0.pool) };
        let ctx_ptr = pool.allocate::<T>(value) as *mut c_void;
        if ctx_ptr.is_null() {
            return Err(AllocError);
        }
        unsafe {
            *self.0.ctx.add(M::module().ctx_index) = ctx_ptr;
        }
        Ok(unsafe { &mut *ctx_ptr.cast::<T>() })
    }

    /// Modify the module context for a specific module using a provided closure,
    /// returning a reference to the modified context
    pub fn modify(&mut self, f: impl FnOnce(&mut T, &Request)) -> Option<&T> {
        let ctx_ptr = self.get_module_ctx_ptr();
        if ctx_ptr.is_null() {
            None
        } else {
            let ctx_ref = unsafe { &mut *ctx_ptr.cast::<T>() };
            let req = unsafe { Request::from_ngx_http_request(&raw mut self.0) };
            f(ctx_ref, req);
            Some(ctx_ref as &T)
        }
    }
}
