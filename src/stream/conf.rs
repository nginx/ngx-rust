use ::core::ptr::NonNull;

use crate::{
    ffi::{
        ngx_conf_t, ngx_cycle_t, ngx_module_t, ngx_stream_conf_ctx_t, ngx_stream_core_srv_conf_t,
        ngx_stream_session_t, ngx_stream_upstream_srv_conf_t,
    },
    stream::StreamModule,
};

/// Utility trait for types containing Stream module main configuration
pub trait StreamModuleMainConfExt {
    /// Get a non-null reference to the main configuration structure for Stream module
    ///
    /// # Safety
    /// Caller must ensure that type `T` matches the configuration type for the specified module.
    #[inline]
    unsafe fn stream_main_conf_unchecked<T>(&self, _module: &ngx_module_t) -> Option<NonNull<T>> {
        None
    }
}

/// Utility trait for types containing Stream module server configuration
pub trait StreamModuleServerConfExt {
    /// Get a non-null reference to the server configuration structure for Stream module
    ///
    /// # Safety
    /// Caller must ensure that type `T` matches the configuration type for the specified module.
    #[inline]
    unsafe fn stream_server_conf_unchecked<T>(&self, _module: &ngx_module_t) -> Option<NonNull<T>> {
        None
    }
}

impl StreamModuleMainConfExt for ngx_stream_conf_ctx_t {
    #[inline]
    unsafe fn stream_main_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        NonNull::new(unsafe { *self.main_conf.add(module.ctx_index) }.cast())
    }
}
impl StreamModuleServerConfExt for ngx_stream_conf_ctx_t {
    #[inline]
    unsafe fn stream_server_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        NonNull::new(unsafe { *self.srv_conf.add(module.ctx_index) }.cast())
    }
}

impl StreamModuleMainConfExt for ngx_cycle_t {
    #[inline]
    unsafe fn stream_main_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        let stream_conf =
            unsafe { self.conf_ctx.add(nginx_sys::ngx_stream_module.index).as_ref()? };
        let conf_ctx = (*stream_conf).cast::<ngx_stream_conf_ctx_t>();
        unsafe { conf_ctx.as_ref()?.stream_main_conf_unchecked(module) }
    }
}

impl StreamModuleMainConfExt for ngx_conf_t {
    #[inline]
    unsafe fn stream_main_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        let conf_ctx = self.ctx.cast::<ngx_stream_conf_ctx_t>();
        unsafe { conf_ctx.as_ref()?.stream_main_conf_unchecked(module) }
    }
}
impl StreamModuleServerConfExt for ngx_conf_t {
    #[inline]
    unsafe fn stream_server_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        let conf_ctx = self.ctx.cast::<ngx_stream_conf_ctx_t>();
        unsafe {
            let conf_ctx = conf_ctx.as_ref()?;
            NonNull::new((*conf_ctx.srv_conf.add(module.ctx_index)).cast())
        }
    }
}

impl StreamModuleMainConfExt for ngx_stream_core_srv_conf_t {
    #[inline]
    unsafe fn stream_main_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        unsafe { self.ctx.as_ref()?.stream_main_conf_unchecked(module) }
    }
}
impl StreamModuleServerConfExt for ngx_stream_core_srv_conf_t {
    #[inline]
    unsafe fn stream_server_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        unsafe { self.ctx.as_ref()?.stream_server_conf_unchecked(module) }
    }
}

impl StreamModuleMainConfExt for ngx_stream_session_t {
    #[inline]
    unsafe fn stream_main_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        NonNull::new(unsafe { *self.main_conf.add(module.ctx_index) }.cast())
    }
}
impl StreamModuleServerConfExt for ngx_stream_session_t {
    #[inline]
    unsafe fn stream_server_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        NonNull::new(unsafe { *self.srv_conf.add(module.ctx_index) }.cast())
    }
}

impl StreamModuleServerConfExt for ngx_stream_upstream_srv_conf_t {
    #[inline]
    unsafe fn stream_server_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        let conf = self.srv_conf;
        if conf.is_null() {
            return None;
        }
        NonNull::new(unsafe { *conf.add(module.ctx_index) }.cast())
    }
}

/// Trait to define and access main module configuration
///
/// # Safety
/// Caller must ensure that type `StreamModuleMainConf::MainConf` matches the configuration type
/// for the specified module.
pub unsafe trait StreamModuleMainConf: StreamModule {
    /// Type for main module configuration
    type MainConf;
    /// Get reference to main module configuration
    fn main_conf(o: &impl StreamModuleMainConfExt) -> Option<&'static Self::MainConf> {
        unsafe { Some(o.stream_main_conf_unchecked(Self::module())?.as_ref()) }
    }
    /// Get mutable reference to main module configuration
    fn main_conf_mut(o: &impl StreamModuleMainConfExt) -> Option<&'static mut Self::MainConf> {
        unsafe { Some(o.stream_main_conf_unchecked(Self::module())?.as_mut()) }
    }
}

/// Trait to define and access server-specific module configuration
///
/// # Safety
/// Caller must ensure that type `StreamModuleServerConf::ServerConf` matches the configuration type
/// for the specified module.
pub unsafe trait StreamModuleServerConf: StreamModule {
    /// Type for server-specific module configuration
    type ServerConf;
    /// Get reference to server-specific module configuration
    fn server_conf(o: &impl StreamModuleServerConfExt) -> Option<&'static Self::ServerConf> {
        unsafe { Some(o.stream_server_conf_unchecked(Self::module())?.as_ref()) }
    }
    /// Get mutable reference to server-specific module configuration
    fn server_conf_mut(
        o: &impl StreamModuleServerConfExt,
    ) -> Option<&'static mut Self::ServerConf> {
        unsafe { Some(o.stream_server_conf_unchecked(Self::module())?.as_mut()) }
    }
}

mod core {
    use crate::stream::{
        StreamModule, StreamModuleMainConf, StreamModuleServerConf, StreamSessionHandler,
    };
    use crate::{
        allocator::AllocError,
        ffi::{ngx_stream_core_main_conf_t, ngx_stream_core_module, ngx_stream_core_srv_conf_t},
        ngx_conf_log_error,
    };

    /// Auxiliary structure to access `ngx_stream_core_module` configuration.
    pub struct NgxStreamCoreModule;

    impl StreamModule for NgxStreamCoreModule {
        fn module() -> &'static crate::ffi::ngx_module_t {
            unsafe { &*::core::ptr::addr_of!(ngx_stream_core_module) }
        }
    }
    unsafe impl StreamModuleMainConf for NgxStreamCoreModule {
        type MainConf = ngx_stream_core_main_conf_t;
    }
    unsafe impl StreamModuleServerConf for NgxStreamCoreModule {
        type ServerConf = ngx_stream_core_srv_conf_t;
    }

    /// Stream phases in which a module can register handlers.
    #[repr(usize)]
    pub enum StreamPhase {
        /// Post-accept phase
        PostAccept = crate::ffi::ngx_stream_phases_NGX_STREAM_POST_ACCEPT_PHASE as _,
        /// Pre-access phase
        Preaccess = crate::ffi::ngx_stream_phases_NGX_STREAM_PREACCESS_PHASE as _,
        /// Access phase
        Access = crate::ffi::ngx_stream_phases_NGX_STREAM_ACCESS_PHASE as _,
        /// Ssl phase
        Ssl = crate::ffi::ngx_stream_phases_NGX_STREAM_SSL_PHASE as _,
        /// Pre-read phase
        Preread = crate::ffi::ngx_stream_phases_NGX_STREAM_PREREAD_PHASE as _,
        /// Content phase
        Content = crate::ffi::ngx_stream_phases_NGX_STREAM_CONTENT_PHASE as _,
        /// Log phase
        Log = crate::ffi::ngx_stream_phases_NGX_STREAM_LOG_PHASE as _,
    }

    /// Register a request handler for a specified phase.
    /// This function must be called from the module's `postconfiguration()` function.
    pub fn add_phase_handler<S>(cf: &mut nginx_sys::ngx_conf_t) -> Result<(), AllocError>
    where
        S: StreamSessionHandler,
    {
        let cmcf = NgxStreamCoreModule::main_conf_mut(cf).expect("stream core main conf");
        let s: *mut nginx_sys::ngx_stream_handler_pt = unsafe {
            nginx_sys::ngx_array_push(&raw mut cmcf.phases[S::PHASE as usize].handlers).cast()
        };
        if s.is_null() {
            ngx_conf_log_error!(
                nginx_sys::NGX_LOG_EMERG,
                cf,
                "failed to register {} handler",
                S::name(),
            );
            return Err(AllocError);
        }
        // set an H::PHASE phase handler
        unsafe {
            *s = Some(crate::stream::raw_handler::<S>);
        }
        Ok(())
    }
}

pub use core::{NgxStreamCoreModule, StreamPhase, add_phase_handler};

#[cfg(ngx_feature = "stream_ssl")]
mod ssl {
    use crate::ffi::{ngx_stream_ssl_module, ngx_stream_ssl_srv_conf_t};

    use crate::stream::{StreamModule, StreamModuleServerConf};

    /// Auxiliary structure to access `ngx_stream_ssl_module` configuration.
    pub struct NgxStreamSslModule;

    impl StreamModule for NgxStreamSslModule {
        fn module() -> &'static crate::ffi::ngx_module_t {
            unsafe { &*::core::ptr::addr_of!(ngx_stream_ssl_module) }
        }
    }
    unsafe impl StreamModuleServerConf for NgxStreamSslModule {
        type ServerConf = ngx_stream_ssl_srv_conf_t;
    }
}
#[cfg(ngx_feature = "stream_ssl")]
pub use ssl::NgxStreamSslModule;

mod upstream {
    use super::{StreamModule, StreamModuleMainConf, StreamModuleServerConf};
    use crate::ffi::{
        ngx_stream_upstream_main_conf_t, ngx_stream_upstream_module, ngx_stream_upstream_srv_conf_t,
    };

    /// Auxiliary structure to access `ngx_stream_upstream_module` configuration.
    pub struct NgxStreamUpstreamModule;

    impl StreamModule for NgxStreamUpstreamModule {
        fn module() -> &'static crate::ffi::ngx_module_t {
            unsafe { &*::core::ptr::addr_of!(ngx_stream_upstream_module) }
        }
    }
    unsafe impl StreamModuleMainConf for NgxStreamUpstreamModule {
        type MainConf = ngx_stream_upstream_main_conf_t;
    }
    unsafe impl StreamModuleServerConf for NgxStreamUpstreamModule {
        type ServerConf = ngx_stream_upstream_srv_conf_t;
    }
}

pub use upstream::NgxStreamUpstreamModule;
