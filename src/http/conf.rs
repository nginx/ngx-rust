use ::core::ptr::NonNull;

use crate::ffi::{
    ngx_http_conf_ctx_t, ngx_http_core_srv_conf_t, ngx_http_request_t,
    ngx_http_upstream_srv_conf_t, ngx_module_t,
};
use crate::http::HttpModule;

/// Utility trait for types containing HTTP module configuration
pub trait HttpModuleConfExt {
    /// Get a non-null reference to the main configuration structure for HTTP module
    ///
    /// # Safety
    /// Caller must ensure that type `T` matches the configuration type for the specified module.
    #[inline]
    unsafe fn http_main_conf_unchecked<T>(&self, _module: &ngx_module_t) -> Option<NonNull<T>> {
        None
    }

    /// Get a non-null reference to the server configuration structure for HTTP module
    ///
    /// # Safety
    /// Caller must ensure that type `T` matches the configuration type for the specified module.
    #[inline]
    unsafe fn http_server_conf_unchecked<T>(&self, _module: &ngx_module_t) -> Option<NonNull<T>> {
        None
    }

    /// Get a non-null reference to the location configuration structure for HTTP module
    ///
    /// Applies to a single `location`, `if` or `limit_except` block
    ///
    /// # Safety
    /// Caller must ensure that type `T` matches the configuration type for the specified module.
    #[inline]
    unsafe fn http_location_conf_unchecked<T>(&self, _module: &ngx_module_t) -> Option<NonNull<T>> {
        None
    }
}

impl HttpModuleConfExt for crate::ffi::ngx_http_conf_ctx_t {
    #[inline]
    unsafe fn http_main_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        NonNull::new(unsafe { *self.main_conf.add(module.ctx_index) }.cast())
    }

    #[inline]
    unsafe fn http_server_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        NonNull::new(unsafe { *self.srv_conf.add(module.ctx_index) }.cast())
    }

    #[inline]
    unsafe fn http_location_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        NonNull::new(unsafe { *self.loc_conf.add(module.ctx_index) }.cast())
    }
}

impl HttpModuleConfExt for crate::ffi::ngx_cycle_t {
    #[inline]
    unsafe fn http_main_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        let http_conf = unsafe {
            self.conf_ctx
                .add(nginx_sys::ngx_http_module.index)
                .as_ref()?
        };
        let conf_ctx = (*http_conf).cast::<ngx_http_conf_ctx_t>();
        unsafe { conf_ctx.as_ref()?.http_main_conf_unchecked(module) }
    }
}

impl HttpModuleConfExt for crate::ffi::ngx_conf_t {
    #[inline]
    unsafe fn http_main_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        let conf_ctx = self.ctx.cast::<ngx_http_conf_ctx_t>();
        unsafe { conf_ctx.as_ref()?.http_main_conf_unchecked(module) }
    }

    #[inline]
    unsafe fn http_server_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        let conf_ctx = self.ctx.cast::<ngx_http_conf_ctx_t>();
        unsafe { conf_ctx.as_ref()?.http_server_conf_unchecked(module) }
    }

    #[inline]
    unsafe fn http_location_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        let conf_ctx = self.ctx.cast::<ngx_http_conf_ctx_t>();
        unsafe { conf_ctx.as_ref()?.http_location_conf_unchecked(module) }
    }
}

impl HttpModuleConfExt for crate::ffi::ngx_http_connection_t {
    #[inline]
    unsafe fn http_main_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        unsafe { self.conf_ctx.as_ref()?.http_main_conf_unchecked(module) }
    }

    #[inline]
    unsafe fn http_server_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        unsafe { self.conf_ctx.as_ref()?.http_server_conf_unchecked(module) }
    }

    #[inline]
    unsafe fn http_location_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        unsafe { self.conf_ctx.as_ref()?.http_location_conf_unchecked(module) }
    }
}

impl HttpModuleConfExt for ngx_http_core_srv_conf_t {
    #[inline]
    unsafe fn http_main_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        unsafe { self.ctx.as_ref()?.http_main_conf_unchecked(module) }
    }

    #[inline]
    unsafe fn http_server_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        unsafe { self.ctx.as_ref()?.http_server_conf_unchecked(module) }
    }

    #[inline]
    unsafe fn http_location_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        unsafe { self.ctx.as_ref()?.http_location_conf_unchecked(module) }
    }
}

impl HttpModuleConfExt for ngx_http_request_t {
    #[inline]
    unsafe fn http_main_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        NonNull::new(unsafe { *self.main_conf.add(module.ctx_index) }.cast())
    }

    #[inline]
    unsafe fn http_server_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        NonNull::new(unsafe { *self.srv_conf.add(module.ctx_index) }.cast())
    }

    #[inline]
    unsafe fn http_location_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        NonNull::new(unsafe { *self.loc_conf.add(module.ctx_index) }.cast())
    }
}

impl HttpModuleConfExt for ngx_http_upstream_srv_conf_t {
    #[inline]
    unsafe fn http_server_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
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
/// Caller must ensure that type `HttpModuleMainConf::MainConf` matches the configuration type
/// for the specified module.
pub unsafe trait HttpModuleMainConf: HttpModule {
    /// Type for main module configuration
    type MainConf;
    /// Get reference to main module configuration
    fn main_conf(o: &impl HttpModuleConfExt) -> Option<&'static Self::MainConf> {
        unsafe { Some(o.http_main_conf_unchecked(Self::module())?.as_ref()) }
    }
    /// Get mutable reference to main module configuration
    fn main_conf_mut(o: &impl HttpModuleConfExt) -> Option<&'static mut Self::MainConf> {
        unsafe { Some(o.http_main_conf_unchecked(Self::module())?.as_mut()) }
    }
}

/// Trait to define and access server-specific module configuration
///
/// # Safety
/// Caller must ensure that type `HttpModuleServerConf::ServerConf` matches the configuration type
/// for the specified module.
pub unsafe trait HttpModuleServerConf: HttpModule {
    /// Type for server-specific module configuration
    type ServerConf;
    /// Get reference to server-specific module configuration
    fn server_conf(o: &impl HttpModuleConfExt) -> Option<&'static Self::ServerConf> {
        unsafe { Some(o.http_server_conf_unchecked(Self::module())?.as_ref()) }
    }
    /// Get mutable reference to server-specific module configuration
    fn server_conf_mut(o: &impl HttpModuleConfExt) -> Option<&'static mut Self::ServerConf> {
        unsafe { Some(o.http_server_conf_unchecked(Self::module())?.as_mut()) }
    }
}

/// Trait to define and access location-specific module configuration
///
/// Applies to a single `location`, `if` or `limit_except` block
///
/// # Safety
/// Caller must ensure that type `HttpModuleLocationConf::LocationConf` matches the configuration
/// type for the specified module.
pub unsafe trait HttpModuleLocationConf: HttpModule {
    /// Type for location-specific module configuration
    type LocationConf;
    /// Get reference to location-specific module configuration
    fn location_conf(o: &impl HttpModuleConfExt) -> Option<&'static Self::LocationConf> {
        unsafe { Some(o.http_location_conf_unchecked(Self::module())?.as_ref()) }
    }
    /// Get mutable reference to location-specific module configuration
    fn location_conf_mut(o: &impl HttpModuleConfExt) -> Option<&'static mut Self::LocationConf> {
        unsafe { Some(o.http_location_conf_unchecked(Self::module())?.as_mut()) }
    }
}

mod core {
    use crate::ffi::{
        ngx_http_core_loc_conf_t, ngx_http_core_main_conf_t, ngx_http_core_module,
        ngx_http_core_srv_conf_t,
    };

    /// Auxiliary structure to access `ngx_http_core_module` configuration.
    pub struct NgxHttpCoreModule;

    impl crate::http::HttpModule for NgxHttpCoreModule {
        fn module() -> &'static crate::ffi::ngx_module_t {
            unsafe { &*::core::ptr::addr_of!(ngx_http_core_module) }
        }
    }
    unsafe impl crate::http::HttpModuleMainConf for NgxHttpCoreModule {
        type MainConf = ngx_http_core_main_conf_t;
    }
    unsafe impl crate::http::HttpModuleServerConf for NgxHttpCoreModule {
        type ServerConf = ngx_http_core_srv_conf_t;
    }
    unsafe impl crate::http::HttpModuleLocationConf for NgxHttpCoreModule {
        type LocationConf = ngx_http_core_loc_conf_t;
    }
}

pub use core::NgxHttpCoreModule;

#[cfg(ngx_feature = "http_ssl")]
mod ssl {
    use crate::ffi::{ngx_http_ssl_module, ngx_http_ssl_srv_conf_t};

    /// Auxiliary structure to access `ngx_http_ssl_module` configuration.
    pub struct NgxHttpSslModule;

    impl crate::http::HttpModule for NgxHttpSslModule {
        fn module() -> &'static crate::ffi::ngx_module_t {
            unsafe { &*::core::ptr::addr_of!(ngx_http_ssl_module) }
        }
    }
    unsafe impl crate::http::HttpModuleServerConf for NgxHttpSslModule {
        type ServerConf = ngx_http_ssl_srv_conf_t;
    }
}
#[cfg(ngx_feature = "http_ssl")]
pub use ssl::NgxHttpSslModule;

mod upstream {
    use crate::ffi::{
        ngx_http_upstream_main_conf_t, ngx_http_upstream_module, ngx_http_upstream_srv_conf_t,
    };

    /// Auxiliary structure to access `ngx_http_upstream_module` configuration.
    pub struct NgxHttpUpstreamModule;

    impl crate::http::HttpModule for NgxHttpUpstreamModule {
        fn module() -> &'static crate::ffi::ngx_module_t {
            unsafe { &*::core::ptr::addr_of!(ngx_http_upstream_module) }
        }
    }
    unsafe impl crate::http::HttpModuleMainConf for NgxHttpUpstreamModule {
        type MainConf = ngx_http_upstream_main_conf_t;
    }
    unsafe impl crate::http::HttpModuleServerConf for NgxHttpUpstreamModule {
        type ServerConf = ngx_http_upstream_srv_conf_t;
    }
}

pub use upstream::NgxHttpUpstreamModule;

#[cfg(all(nginx1_25_1, ngx_feature = "http_v2"))]
mod http_v2 {
    use crate::ffi::{ngx_http_v2_module, ngx_http_v2_srv_conf_t};

    /// Auxiliary structure to access `ngx_http_v2_module` configuration.
    pub struct NgxHttpV2Module;

    impl crate::http::HttpModule for NgxHttpV2Module {
        fn module() -> &'static crate::ffi::ngx_module_t {
            unsafe { &*::core::ptr::addr_of!(ngx_http_v2_module) }
        }
    }
    unsafe impl crate::http::HttpModuleServerConf for NgxHttpV2Module {
        type ServerConf = ngx_http_v2_srv_conf_t;
    }
}
// ngx_http_v2_module was not exposed by default until aefd862a
#[cfg(all(nginx1_25_1, ngx_feature = "http_v2"))]
pub use http_v2::NgxHttpV2Module;

#[cfg(ngx_feature = "http_v3")]
mod http_v3 {
    use crate::ffi::{ngx_http_v3_module, ngx_http_v3_srv_conf_t};

    /// Auxiliary structure to access `ngx_http_v3_module` configuration.
    pub struct NgxHttpV3Module;

    impl crate::http::HttpModule for NgxHttpV3Module {
        fn module() -> &'static crate::ffi::ngx_module_t {
            unsafe { &*::core::ptr::addr_of!(ngx_http_v3_module) }
        }
    }
    unsafe impl crate::http::HttpModuleServerConf for NgxHttpV3Module {
        type ServerConf = ngx_http_v3_srv_conf_t;
    }
}

#[cfg(ngx_feature = "http_v3")]
pub use http_v3::NgxHttpV3Module;
