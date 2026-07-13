use core::ffi::c_void;
use core::fmt;
use core::ptr::NonNull;

use nginx_sys::{
    NGX_ERROR, NGX_OK, ngx_connection_t, ngx_int_t, ngx_log_t, ngx_module_t, ngx_str_t,
    ngx_stream_complex_value, ngx_stream_complex_value_t, ngx_stream_session_t,
    ngx_stream_upstream_t,
};

use crate::{
    core::{NgxStr, Status},
    stream::{StreamModuleMainConfExt, StreamModuleServerConfExt, StreamPhase},
};

/// Trait for converting handler return types into `ngx_int_t`.
/// Any desired error handling / logging logic can be implemented
/// in the `into_handler_status` method.
///
/// There are predefined implementations for `ngx_int_t`, [`Status`],
/// [`Option`] with value type implementing [`IntoHandlerStatus`].
pub trait IntoHandlerStatus
where
    Self: Sized,
{
    /// Convert the handler return type into an `ngx_int_t`.
    fn into_handler_status(self, _r: &Session) -> ngx_int_t;
}

impl<T> IntoHandlerStatus for Option<T>
where
    T: IntoHandlerStatus,
{
    #[inline]
    fn into_handler_status(self, r: &Session) -> ngx_int_t {
        self.map(|val| val.into_handler_status(r)).unwrap_or(NGX_ERROR as _)
    }
}

impl IntoHandlerStatus for ngx_int_t {
    #[inline]
    fn into_handler_status(self, _r: &Session) -> ngx_int_t {
        self
    }
}

impl IntoHandlerStatus for Status {
    #[inline]
    fn into_handler_status(self, _r: &Session) -> ngx_int_t {
        self.0
    }
}

/// Trait for static request handler.
pub trait StreamSessionHandler {
    /// The phase in which the handler is invoked.
    const PHASE: StreamPhase;
    /// The return type of the handler.
    type Output: IntoHandlerStatus;
    /// The handler function.
    fn handler(session: &mut Session) -> Self::Output;
    /// Handler name for logging purposes.
    /// [`core::any::type_name`] is used by default.
    fn name() -> &'static str {
        core::any::type_name::<Self>()
    }
}

/// The C-compatible handler wrapper function.
///
/// # Safety
///
/// The caller has provided a valid non-null pointer to an [`ngx_stream_session_t`].
pub(crate) unsafe extern "C" fn raw_handler<S>(s: *mut ngx_stream_session_t) -> ngx_int_t
where
    S: StreamSessionHandler,
{
    let s = unsafe { Session::from_ngx_stream_session(s) };
    S::handler(s).into_handler_status(s)
}

/// Wrapper struct for an [`ngx_stream_session_t`] pointer, providing methods for working with Stream
/// session.
#[repr(transparent)]
pub struct Session(ngx_stream_session_t);

impl<'a> From<&'a Session> for *const ngx_stream_session_t {
    fn from(session: &'a Session) -> Self {
        &raw const session.0
    }
}

impl<'a> From<&'a mut Session> for *mut ngx_stream_session_t {
    fn from(session: &'a mut Session) -> Self {
        &raw mut session.0
    }
}

impl AsRef<ngx_stream_session_t> for Session {
    fn as_ref(&self) -> &ngx_stream_session_t {
        &self.0
    }
}

impl AsMut<ngx_stream_session_t> for Session {
    fn as_mut(&mut self) -> &mut ngx_stream_session_t {
        &mut self.0
    }
}

impl Session {
    /// Create a [`Session`] from an [`ngx_stream_session_t`].
    ///
    /// # Safety
    ///
    /// The caller has provided a valid non-null pointer to a valid `ngx_stream_session_t`
    /// which shares the same representation as `Request`.
    pub unsafe fn from_ngx_stream_session<'a>(r: *mut ngx_stream_session_t) -> &'a mut Session {
        unsafe { &mut *r.cast::<Session>() }
    }

    /// Returns the result as an `Option` if it exists, otherwise `None`.
    ///
    /// The option wraps an ngx_stream_upstream_t instance, it will be none when the underlying NGINX
    /// request does not have a pointer to a [`ngx_stream_upstream_t`] upstream structure.
    pub fn upstream(&self) -> Option<*mut ngx_stream_upstream_t> {
        if self.0.upstream.is_null() {
            return None;
        }
        Some(self.0.upstream)
    }

    /// Pointer to a [`ngx_connection_t`] client connection object.
    ///
    /// [`ngx_connection_t`]: https://nginx.org/en/docs/dev/development_guide.html#connection
    pub fn connection(&self) -> *mut ngx_connection_t {
        self.0.connection
    }

    /// Pointer to a [`ngx_log_t`].
    ///
    /// [`ngx_log_t`]: https://nginx.org/en/docs/dev/development_guide.html#logging
    pub fn log(&self) -> *mut ngx_log_t {
        unsafe { (*self.connection()).log }
    }

    /// Get Module context pointer
    fn get_module_ctx_ptr(&self, module: &ngx_module_t) -> *mut c_void {
        unsafe { *self.0.ctx.add(module.ctx_index) }
    }

    /// Get Module context
    pub fn get_module_ctx<T>(&self, module: &ngx_module_t) -> Option<&T> {
        let ctx = self.get_module_ctx_ptr(module).cast::<T>();
        // SAFETY: ctx is either NULL or allocated with ngx_p(c)alloc and
        // explicitly initialized by the module
        unsafe { ctx.as_ref() }
    }

    /// Sets the value as the module's context.
    pub fn set_module_ctx(&self, value: *mut c_void, module: &ngx_module_t) {
        unsafe {
            *self.0.ctx.add(module.ctx_index) = value;
        };
    }

    /// Get the value of a [complex value].
    pub fn get_complex_value(&mut self, cv: &mut ngx_stream_complex_value_t) -> Option<&NgxStr> {
        let r = (self as *mut Session).cast();
        let val = cv as *mut ngx_stream_complex_value_t;
        // SAFETY: `ngx_stream_complex_value` does not mutate `r` or `val` and guarentees that
        // a valid Nginx string is stored in `value` if it successfully returns.
        unsafe {
            let mut value = ngx_str_t::default();
            if ngx_stream_complex_value(r, val, &raw mut value) != NGX_OK as ngx_int_t {
                return None;
            }
            Some(NgxStr::from_ngx_str(value))
        }
    }
}

impl StreamModuleMainConfExt for Session {
    #[inline]
    unsafe fn stream_main_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        unsafe {
            // SAFETY: main_conf[module.ctx_index] is either NULL or allocated with ngx_p(c)alloc
            // and explicitly initialized by the module
            NonNull::new((*self.0.main_conf.add(module.ctx_index)).cast())
        }
    }
}
impl StreamModuleServerConfExt for Session {
    #[inline]
    unsafe fn stream_server_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        unsafe {
            // SAFETY: srv_conf[module.ctx_index] is either NULL or allocated with ngx_p(c)alloc and
            // explicitly initialized by the module
            NonNull::new((*self.0.srv_conf.add(module.ctx_index)).cast())
        }
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session").field("session_", &self.0).finish()
    }
}
