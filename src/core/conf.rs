use core::ptr::NonNull;

use crate::ffi::{ngx_core_conf_t, ngx_module_t};

/// Raw access to core module main configuration slots.
///
/// This trait is implemented for NGINX-owned types such as `ngx_cycle_t` and `ngx_conf_t`
/// that carry configuration context pointers. It exposes the low-level lookup step that
/// retrieves a module's main configuration as an untyped pointer.
///
/// Most callers should not use this trait directly. Prefer `CoreModuleMainConf` to obtain a
/// typed reference for a specific core module.
pub trait CoreModuleConfExt {
    /// Get a non-null pointer to a core module's main configuration.
    ///
    /// # Safety
    /// Caller must ensure that type `T` matches the configuration type for the specified module.
    /// Supplying the wrong type will produce an invalid typed pointer.
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

/// Typed access to a core module's main configuration.
///
/// Implement this trait for a concrete core-style module to associate it with its main
/// configuration type and global `ngx_module_t`. The provided default methods build on top of
/// `CoreModuleConfExt` to turn the raw configuration slot lookup into typed references.
///
/// # Safety
/// Caller must ensure that type `CoreModuleMainConf::MainConf` matches the configuration type
/// for the specified module.
pub unsafe trait CoreModuleMainConf {
    /// Concrete type of this module's main configuration.
    type MainConf;

    /// Returns the global `ngx_module_t` describing this module.
    fn module() -> &'static ngx_module_t;

    /// Get a typed shared reference to this module's main configuration.
    fn main_conf(o: &impl CoreModuleConfExt) -> Option<&'static Self::MainConf> {
        unsafe { Some(o.core_main_conf_unchecked(Self::module())?.as_ref()) }
    }

    /// Get a typed mutable reference to this module's main configuration.
    fn main_conf_mut(o: &impl CoreModuleConfExt) -> Option<&'static mut Self::MainConf> {
        unsafe { Some(o.core_main_conf_unchecked(Self::module())?.as_mut()) }
    }
}

/// Auxiliary structure to access `ngx_core_module` configuration.
pub struct NgxCoreModule;

unsafe impl CoreModuleMainConf for NgxCoreModule {
    type MainConf = ngx_core_conf_t;

    fn module() -> &'static ngx_module_t {
        unsafe { &*core::ptr::addr_of!(nginx_sys::ngx_core_module) }
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    extern crate std;

    use alloc::boxed::Box;
    use core::ffi::c_void;
    use core::mem::MaybeUninit;

    use super::{CoreModuleConfExt, CoreModuleMainConf};
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
        let mut slots: [CoreConfSlot; 1] = [(&raw mut value).cast()];

        let mut cycle: ngx_cycle_t = unsafe { MaybeUninit::zeroed().assume_init() };
        cycle.conf_ctx = slots.as_mut_ptr();

        let mut conf: ngx_conf_t = unsafe { MaybeUninit::zeroed().assume_init() };
        conf.cycle = &raw mut cycle;

        let module = module_with_index(0);

        let got = unsafe { conf.core_main_conf_unchecked::<u32>(&module).map(|v| *v.as_ref()) };
        assert_eq!(got, Some(42));

        let got_mut =
            unsafe { conf.core_main_conf_unchecked::<u32>(&module).map(|mut v| v.as_mut()) };
        assert!(got_mut.is_some());
        if let Some(v) = got_mut {
            *v = 99;
        }
        assert_eq!(value, 99);
    }

    struct TestCoreModule;

    unsafe impl CoreModuleMainConf for TestCoreModule {
        type MainConf = u32;

        fn module() -> &'static ngx_module_t {
            Box::leak(Box::new(module_with_index(0)))
        }
    }

    #[test]
    fn main_conf_trait_accessors_return_typed_references() {
        let mut value: u32 = 42;
        let mut slots: [CoreConfSlot; 1] = [(&raw mut value).cast()];

        let mut cycle: ngx_cycle_t = unsafe { MaybeUninit::zeroed().assume_init() };
        cycle.conf_ctx = slots.as_mut_ptr();

        let mut conf: ngx_conf_t = unsafe { MaybeUninit::zeroed().assume_init() };
        conf.cycle = &raw mut cycle;

        assert_eq!(TestCoreModule::main_conf(&conf).copied(), Some(42));

        if let Some(v) = TestCoreModule::main_conf_mut(&conf) {
            *v = 99;
        }
        assert_eq!(value, 99);
    }
}
