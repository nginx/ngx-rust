use ::core::{
    ffi::{c_char, c_void},
    ptr,
};

use nginx_sys::{ngx_conf_t, ngx_int_t, ngx_module_t};

use crate::{
    core::{NGX_CONF_ERROR, Pool, Status},
    http::Merge,
    stream::{StreamModuleMainConf, StreamModuleServerConf},
};

/// The `StreamModule` trait provides the NGINX configuration stage interface.
///
/// These functions allocate structures, initialize them, and merge through the configuration
/// layers.
///
/// See <https://nginx.org/en/docs/dev/development_guide.html#adding_new_modules> for details.
pub trait StreamModule {
    /// Returns reference to a global variable of type [ngx_module_t] created for this module.
    fn module() -> &'static ngx_module_t;

    /// # Safety
    ///
    /// Callers should provide valid non-null `ngx_conf_t` arguments. Implementers must
    /// guard against null inputs or risk runtime errors.
    unsafe extern "C" fn preconfiguration(_cf: *mut ngx_conf_t) -> ngx_int_t {
        Status::NGX_OK.into()
    }

    /// # Safety
    ///
    /// Callers should provide valid non-null `ngx_conf_t` arguments. Implementers must
    /// guard against null inputs or risk runtime errors.
    unsafe extern "C" fn postconfiguration(_cf: *mut ngx_conf_t) -> ngx_int_t {
        Status::NGX_OK.into()
    }

    /// # Safety
    ///
    /// Callers should provide valid non-null `ngx_conf_t` arguments. Implementers must
    /// guard against null inputs or risk runtime errors.
    unsafe extern "C" fn create_main_conf(cf: *mut ngx_conf_t) -> *mut c_void
    where
        Self: StreamModuleMainConf,
        Self::MainConf: Default,
    {
        unsafe {
            let pool = Pool::from_ngx_pool((*cf).pool);
            pool.allocate::<Self::MainConf>(Default::default()) as *mut c_void
        }
    }

    /// # Safety
    ///
    /// Callers should provide valid non-null `ngx_conf_t` arguments. Implementers must
    /// guard against null inputs or risk runtime errors.
    unsafe extern "C" fn init_main_conf(_cf: *mut ngx_conf_t, _conf: *mut c_void) -> *mut c_char
    where
        Self: StreamModuleMainConf,
        Self::MainConf: Default,
    {
        ptr::null_mut()
    }

    /// # Safety
    ///
    /// Callers should provide valid non-null `ngx_conf_t` arguments. Implementers must
    /// guard against null inputs or risk runtime errors.
    unsafe extern "C" fn create_srv_conf(cf: *mut ngx_conf_t) -> *mut c_void
    where
        Self: StreamModuleServerConf,
        Self::ServerConf: Default,
    {
        unsafe {
            let pool = Pool::from_ngx_pool((*cf).pool);
            pool.allocate::<Self::ServerConf>(Default::default()) as *mut c_void
        }
    }

    /// # Safety
    ///
    /// Callers should provide valid non-null `ngx_conf_t` arguments. Implementers must
    /// guard against null inputs or risk runtime errors.
    unsafe extern "C" fn merge_srv_conf(
        _cf: *mut ngx_conf_t,
        prev: *mut c_void,
        conf: *mut c_void,
    ) -> *mut c_char
    where
        Self: StreamModuleServerConf,
        Self::ServerConf: Merge,
    {
        unsafe {
            let prev = &mut *(prev as *mut Self::ServerConf);
            let conf = &mut *(conf as *mut Self::ServerConf);
            match conf.merge(prev) {
                Ok(_) => ptr::null_mut(),
                Err(_) => NGX_CONF_ERROR as _,
            }
        }
    }
}
