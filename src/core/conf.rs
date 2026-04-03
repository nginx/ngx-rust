use core::ptr::NonNull;

use crate::ffi::{ngx_core_conf_t, ngx_module_t};

/// Utility trait for types containing core module main configuration.
pub trait CoreModuleConfExt {
    /// Get a non-null reference to the main configuration structure for a core module.
    ///
    /// # Safety
    /// Caller must ensure that type `T` matches the configuration type for the specified module.
    #[inline]
    unsafe fn core_main_conf_unchecked<T>(&self, _module: &ngx_module_t) -> Option<NonNull<T>> {
        None
    }
}

impl CoreModuleConfExt for crate::ffi::ngx_cycle_t {
    #[inline]
    unsafe fn core_main_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        let conf_ctx = NonNull::new(self.conf_ctx)?;
        let conf = unsafe { *conf_ctx.as_ptr().add(module.index) };
        NonNull::new(conf.cast())
    }
}

impl CoreModuleConfExt for crate::ffi::ngx_conf_t {
    #[inline]
    unsafe fn core_main_conf_unchecked<T>(&self, module: &ngx_module_t) -> Option<NonNull<T>> {
        unsafe { self.cycle.as_ref()?.core_main_conf_unchecked(module) }
    }
}

/// Get a typed reference to the main configuration structure for a core module.
pub fn core_main_conf<T>(o: &impl CoreModuleConfExt, module: &ngx_module_t) -> Option<&'static T> {
    unsafe { Some(o.core_main_conf_unchecked(module)?.as_ref()) }
}

/// Get a typed mutable reference to the main configuration structure for a core module.
pub fn core_main_conf_mut<T>(
    o: &impl CoreModuleConfExt,
    module: &ngx_module_t,
) -> Option<&'static mut T> {
    unsafe { Some(o.core_main_conf_unchecked(module)?.as_mut()) }
}

/// Auxiliary structure to access `ngx_core_module` configuration.
pub struct NgxCoreModule;

impl NgxCoreModule {
    /// Returns a reference to the global `ngx_core_module`.
    pub fn module() -> &'static ngx_module_t {
        unsafe { &*core::ptr::addr_of!(nginx_sys::ngx_core_module) }
    }

    /// Get a typed reference to `ngx_core_module` main configuration.
    pub fn main_conf(o: &impl CoreModuleConfExt) -> Option<&'static ngx_core_conf_t> {
        core_main_conf(o, Self::module())
    }

    /// Get a typed mutable reference to `ngx_core_module` main configuration.
    pub fn main_conf_mut(o: &impl CoreModuleConfExt) -> Option<&'static mut ngx_core_conf_t> {
        core_main_conf_mut(o, Self::module())
    }
}

#[cfg(test)]
mod tests {
    use core::ffi::c_void;
    use core::mem::MaybeUninit;

    use super::{CoreModuleConfExt, core_main_conf, core_main_conf_mut};
    use crate::ffi::{ngx_conf_t, ngx_cycle_t, ngx_module_t};

    type CoreConfSlot = *mut *mut *mut c_void;

    fn module_with_index(index: usize) -> ngx_module_t {
        let mut module = ngx_module_t::default();
        module.index = index;
        module
    }

    #[test]
    fn null_conf_ctx_returns_none() {
        let cycle: ngx_cycle_t = unsafe { MaybeUninit::zeroed().assume_init() };
        let module = module_with_index(0);
        assert!(unsafe { cycle.core_main_conf_unchecked::<u32>(&module) }.is_none());
    }

    #[test]
    fn missing_module_slot_returns_none() {
        let mut slots: [CoreConfSlot; 2] = [core::ptr::null_mut(); 2];
        let mut cycle: ngx_cycle_t = unsafe { MaybeUninit::zeroed().assume_init() };
        cycle.conf_ctx = slots.as_mut_ptr();

        let module = module_with_index(1);
        assert!(unsafe { cycle.core_main_conf_unchecked::<u32>(&module) }.is_none());
    }

    #[test]
    fn populated_slot_returns_typed_reference() {
        let mut value: u32 = 42;
        let mut slots: [CoreConfSlot; 1] = [(&mut value as *mut u32).cast()];

        let mut cycle: ngx_cycle_t = unsafe { MaybeUninit::zeroed().assume_init() };
        cycle.conf_ctx = slots.as_mut_ptr();

        let mut conf: ngx_conf_t = unsafe { MaybeUninit::zeroed().assume_init() };
        conf.cycle = &mut cycle;

        let module = module_with_index(0);

        let got = core_main_conf::<u32>(&conf, &module).copied();
        assert_eq!(got, Some(42));

        let got_mut = core_main_conf_mut::<u32>(&conf, &module);
        assert!(got_mut.is_some());
        if let Some(v) = got_mut {
            *v = 99;
        }
        assert_eq!(value, 99);
    }
}
