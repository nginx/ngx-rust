use crate::http::{HttpModule, Request};
use crate::ngx_log_debug_http;

/// A trait for managing request-specific context data.
pub trait RequestContext<Module: HttpModule>: Sized {
    /// Creates a new context and associates it with the given request.
    /// No check is performed to see if a context already exists.
    fn create<F>(request: &mut Request, f: F) -> Option<&mut Self>
    where
        F: FnOnce() -> Self,
    {
        let ctx_ref = unsafe { request.pool().allocate_with_cleanup(f)?.as_mut() };
        request.set_module_ctx(ctx_ref as *mut _ as _, Module::module());
        Some(ctx_ref)
    }

    /// Removes the context associated with the given request.
    fn remove(request: &mut Request) {
        if let Some(ctx_ptr) = request.get_module_ctx::<Self>(Module::module()) {
            unsafe { request.pool().remove(ctx_ptr as *const Self) };
            request.set_module_ctx(core::ptr::null_mut(), Module::module());
            ngx_log_debug_http!(request, "RequestContext removed from request");
        }
    }

    /// Retrieves an immutable reference to the context associated with the given request.
    fn get(request: &Request) -> Option<&Self> {
        request.get_module_ctx::<Self>(Module::module())
    }

    /// Retrieves a mutable reference to the context associated with the given request.
    fn get_mut(request: &mut Request) -> Option<&mut Self> {
        request.get_module_ctx_mut::<Self>(Module::module())
    }

    /// Checks if a context is associated with the given request.
    fn exists(request: &Request) -> bool {
        request.get_module_ctx::<Self>(Module::module()).is_some()
    }
}
