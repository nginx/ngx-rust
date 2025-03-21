use crate::ffi::*;

use crate::http::HttpModule;

/// Utility trait for types containing HTTP module configuration
pub trait HttpModuleConfExt {
    /// Get a reference to the main configuration structure for HTTP module
    ///
    /// # Safety
    /// Caller must ensure that type `T` matches the configuration type for the specified module.
    unsafe fn http_main_conf_unchecked<T>(&self, _module: &ngx_module_t) -> Option<&'static T> {
        None
    }

    /// Get a mutable reference to the main configuration structure for HTTP module
    ///
    /// # Safety
    /// Caller must ensure that type `T` matches the configuration type for the specified module.
    unsafe fn http_main_conf_mut_unchecked<T>(&self, _module: &ngx_module_t) -> Option<&'static mut T> {
        None
    }

    /// Get a reference to the server configuration structure for HTTP module
    ///
    /// # Safety
    /// Caller must ensure that type `T` matches the configuration type for the specified module.
    unsafe fn http_server_conf_unchecked<T>(&self, _module: &ngx_module_t) -> Option<&'static T> {
        None
    }

    /// Get a mutable reference to the server configuration structure for HTTP module
    ///
    /// # Safety
    /// Caller must ensure that type `T` matches the configuration type for the specified module.
    unsafe fn http_server_conf_mut_unchecked<T>(&self, _module: &ngx_module_t) -> Option<&'static mut T> {
        None
    }

    /// Get a reference to the location configuration structure for HTTP module
    ///
    /// # Safety
    /// Caller must ensure that type `T` matches the configuration type for the specified module.
    unsafe fn http_location_conf_unchecked<T>(&self, _module: &ngx_module_t) -> Option<&'static T> {
        None
    }

    /// Get a mutable reference to the location configuration structure for HTTP module
    ///
    /// # Safety
    /// Caller must ensure that type `T` matches the configuration type for the specified module.
    unsafe fn http_location_conf_mut_unchecked<T>(&self, _module: &ngx_module_t) -> Option<&'static mut T> {
        None
    }
}

impl HttpModuleConfExt for crate::ffi::ngx_cycle_t {
    unsafe fn http_main_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<&'static T> {
        let http_conf = self.conf_ctx.add(nginx_sys::ngx_http_module.index).as_ref()?;
        let conf_ctx = (*http_conf).cast::<ngx_http_conf_ctx_t>();
        let conf_ctx = conf_ctx.as_ref()?;
        (*conf_ctx.main_conf.add(module.ctx_index)).cast::<T>().as_ref()
    }
    unsafe fn http_main_conf_mut_unchecked<T>(&self, module: &ngx_module_t) -> Option<&'static mut T> {
        let http_conf = self.conf_ctx.add(nginx_sys::ngx_http_module.index).as_ref()?;
        let conf_ctx = (*http_conf).cast::<ngx_http_conf_ctx_t>();
        let conf_ctx = conf_ctx.as_ref()?;
        (*conf_ctx.main_conf.add(module.ctx_index)).cast::<T>().as_mut()
    }
}

impl HttpModuleConfExt for crate::ffi::ngx_conf_t {
    unsafe fn http_main_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<&'static T> {
        let conf_ctx = self.ctx.cast::<ngx_http_conf_ctx_t>();
        let conf_ctx = conf_ctx.as_ref()?;
        (*conf_ctx.main_conf.add(module.ctx_index)).cast::<T>().as_ref()
    }
    unsafe fn http_main_conf_mut_unchecked<T>(&self, module: &ngx_module_t) -> Option<&'static mut T> {
        let conf_ctx = self.ctx.cast::<ngx_http_conf_ctx_t>();
        let conf_ctx = conf_ctx.as_ref()?;
        (*conf_ctx.main_conf.add(module.ctx_index)).cast::<T>().as_mut()
    }
    unsafe fn http_server_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<&'static T> {
        let conf_ctx = self.ctx.cast::<ngx_http_conf_ctx_t>();
        let conf_ctx = conf_ctx.as_ref()?;
        (*conf_ctx.srv_conf.add(module.ctx_index)).cast::<T>().as_ref()
    }
    unsafe fn http_server_conf_mut_unchecked<T>(&self, module: &ngx_module_t) -> Option<&'static mut T> {
        let conf_ctx = self.ctx.cast::<ngx_http_conf_ctx_t>();
        let conf_ctx = conf_ctx.as_ref()?;
        (*conf_ctx.srv_conf.add(module.ctx_index)).cast::<T>().as_mut()
    }
    unsafe fn http_location_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<&'static T> {
        let conf_ctx = self.ctx.cast::<ngx_http_conf_ctx_t>();
        let conf_ctx = conf_ctx.as_ref()?;
        (*conf_ctx.loc_conf.add(module.ctx_index)).cast::<T>().as_ref()
    }
    unsafe fn http_location_conf_mut_unchecked<T>(&self, module: &ngx_module_t) -> Option<&'static mut T> {
        let conf_ctx = self.ctx.cast::<ngx_http_conf_ctx_t>();
        let conf_ctx = conf_ctx.as_ref()?;
        (*conf_ctx.loc_conf.add(module.ctx_index)).cast::<T>().as_mut()
    }
}

impl HttpModuleConfExt for ngx_http_upstream_srv_conf_t {
    unsafe fn http_server_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<&'static T> {
        let conf = self.srv_conf;
        if conf.is_null() {
            return None;
        }
        (*conf.add(module.ctx_index)).cast::<T>().as_ref()
    }
    unsafe fn http_server_conf_mut_unchecked<T>(&self, module: &ngx_module_t) -> Option<&'static mut T> {
        let conf = self.srv_conf;
        if conf.is_null() {
            return None;
        }
        (*conf.add(module.ctx_index)).cast::<T>().as_mut()
    }
}

/// Trait to define and access main module configuration
pub trait HttpModuleMainConf: HttpModule {
    /// Type for main module configuration
    type MainConf;
    /// Get reference to main module configuration
    fn main_conf(o: &impl HttpModuleConfExt) -> Option<&'static Self::MainConf> {
        unsafe { o.http_main_conf_unchecked(Self::module()) }
    }
    /// Get mutable reference to main module configuration
    fn main_conf_mut(o: &impl HttpModuleConfExt) -> Option<&'static mut Self::MainConf> {
        unsafe { o.http_main_conf_mut_unchecked(Self::module()) }
    }
}

/// Trait to define and access server-specific module configuration
pub trait HttpModuleServerConf: HttpModule {
    /// Type for server-specific module configuration
    type ServerConf;
    /// Get reference to server-level module configuration
    fn server_conf(o: &impl HttpModuleConfExt) -> Option<&'static Self::ServerConf> {
        unsafe { o.http_server_conf_unchecked(Self::module()) }
    }
    /// Get mutable reference to server-specific module configuration
    fn server_conf_mut(o: &impl HttpModuleConfExt) -> Option<&'static mut Self::ServerConf> {
        unsafe { o.http_server_conf_mut_unchecked(Self::module()) }
    }
}

/// Trait to define and access location-specific module configuration
pub trait HttpModuleLocationConf: HttpModule {
    /// Type for location-specific module configuration
    type LocationConf;
    /// Get reference to location-specific module configuration
    fn location_conf(o: &impl HttpModuleConfExt) -> Option<&'static Self::LocationConf> {
        unsafe { o.http_location_conf_unchecked(Self::module()) }
    }
    /// Get mutable reference to location-level module configuration
    fn location_conf_mut(o: &impl HttpModuleConfExt) -> Option<&'static mut Self::LocationConf> {
        unsafe { o.http_location_conf_mut_unchecked(Self::module()) }
    }
}

mod core {
    use crate::ffi::{
        ngx_http_core_loc_conf_t, ngx_http_core_main_conf_t, ngx_http_core_module, ngx_http_core_srv_conf_t,
    };

    /// Auxiliary structure to access core module configuration
    pub struct NgxHttpCoreModule;

    impl crate::http::HttpModule for NgxHttpCoreModule {
        fn module() -> &'static crate::ffi::ngx_module_t {
            unsafe { &*::core::ptr::addr_of!(ngx_http_core_module) }
        }
    }
    impl crate::http::HttpModuleMainConf for NgxHttpCoreModule {
        type MainConf = ngx_http_core_main_conf_t;
    }
    impl crate::http::HttpModuleServerConf for NgxHttpCoreModule {
        type ServerConf = ngx_http_core_srv_conf_t;
    }
    impl crate::http::HttpModuleLocationConf for NgxHttpCoreModule {
        type LocationConf = ngx_http_core_loc_conf_t;
    }
}

pub use core::NgxHttpCoreModule;

#[cfg(ngx_feature = "http_ssl")]
mod ssl {
    use crate::ffi::{ngx_http_ssl_module, ngx_http_ssl_srv_conf_t};

    /// Auxiliary structure to access SSL module configuration
    pub struct NgxHttpSSLModule;

    impl crate::http::HttpModule for NgxHttpSSLModule {
        fn module() -> &'static crate::ffi::ngx_module_t {
            unsafe { &*::core::ptr::addr_of!(ngx_http_ssl_module) }
        }
    }
    impl crate::http::HttpModuleServerConf for NgxHttpSSLModule {
        type ServerConf = ngx_http_ssl_srv_conf_t;
    }
}
#[cfg(ngx_feature = "http_ssl")]
pub use ssl::NgxHttpSSLModule;

mod upstream {
    use crate::ffi::{ngx_http_upstream_main_conf_t, ngx_http_upstream_module, ngx_http_upstream_srv_conf_t};

    /// Auxiliary structure to access upstream module configuration
    pub struct NgxHttpUpstreamModule;

    impl crate::http::HttpModule for NgxHttpUpstreamModule {
        fn module() -> &'static crate::ffi::ngx_module_t {
            unsafe { &*::core::ptr::addr_of!(ngx_http_upstream_module) }
        }
    }
    impl crate::http::HttpModuleMainConf for NgxHttpUpstreamModule {
        type MainConf = ngx_http_upstream_main_conf_t;
    }
    impl crate::http::HttpModuleServerConf for NgxHttpUpstreamModule {
        type ServerConf = ngx_http_upstream_srv_conf_t;
    }
}

pub use upstream::NgxHttpUpstreamModule;

#[cfg(ngx_feature = "http_v2")]
mod http_v2 {
    use crate::ffi::{ngx_http_v2_module, ngx_http_v2_srv_conf_t};

    /// Auxiliary structure to access HTTP V2 module configuration
    pub struct NgxHttpV2Module;

    impl crate::http::HttpModule for NgxHttpV2Module {
        fn module() -> &'static crate::ffi::ngx_module_t {
            unsafe { &*::core::ptr::addr_of!(ngx_http_v2_module) }
        }
    }
    impl crate::http::HttpModuleServerConf for NgxHttpV2Module {
        type ServerConf = ngx_http_v2_srv_conf_t;
    }
}

#[cfg(ngx_feature = "http_v2")]
pub use http_v2::NgxHttpV2Module;

#[cfg(ngx_feature = "http_v3")]
mod http_v3 {
    use crate::ffi::{ngx_http_v3_module, ngx_http_v3_srv_conf_t};

    /// Auxiliary structure to access HTTP V3 module configuration
    pub struct NgxHttpV3Module;

    impl crate::http::HttpModule for NgxHttpV3Module {
        fn module() -> &'static crate::ffi::ngx_module_t {
            unsafe { &*::core::ptr::addr_of!(ngx_http_v3_module) }
        }
    }
    impl crate::http::HttpModuleServerConf for NgxHttpV3Module {
        type ServerConf = ngx_http_v3_srv_conf_t;
    }
}

#[cfg(ngx_feature = "http_v3")]
pub use http_v3::NgxHttpV3Module;
