use core::ffi::c_void;
use core::mem::MaybeUninit;
use core::ptr;

use ngx::core::{CoreModuleConfExt, core_main_conf, core_main_conf_mut};
use ngx::ffi::{ngx_conf_t, ngx_core_conf_t, ngx_cycle_t, ngx_module_t};

type CoreConfSlot = *mut *mut *mut c_void;
type CoreConfCtx = *mut CoreConfSlot;

fn module_with_index(index: usize) -> ngx_module_t {
    let mut module = ngx_module_t::default();
    module.index = index;
    module
}

// Manual baseline matching the raw NGINX traversal: cycle.conf_ctx[module.index].
unsafe fn manual_core_main_conf_ptr(
    cycle: &ngx_cycle_t,
    module: &ngx_module_t,
) -> *mut ngx_core_conf_t {
    if cycle.conf_ctx.is_null() {
        return ptr::null_mut();
    }

    let slot = unsafe { *cycle.conf_ctx.add(module.index) };
    slot.cast()
}

#[test]
// Validates that cycle-based helper access resolves the exact same slot pointer
// as manual top-level conf_ctx indexing.
fn cycle_core_conf_matches_manual_traversal() {
    let mut core_conf: ngx_core_conf_t = unsafe { MaybeUninit::zeroed().assume_init() };
    let mut slots: [CoreConfSlot; 2] = [ptr::null_mut(); 2];
    slots[1] = (&raw mut core_conf).cast();

    let mut cycle: ngx_cycle_t = unsafe { MaybeUninit::zeroed().assume_init() };
    let conf_ctx: CoreConfCtx = slots.as_mut_ptr();
    cycle.conf_ctx = conf_ctx;

    let module = module_with_index(1);
    let from_manual = unsafe { manual_core_main_conf_ptr(&cycle, &module) };

    let from_helper = unsafe { cycle.core_main_conf_unchecked::<ngx_core_conf_t>(&module) }
        .map(|p| p.as_ptr())
        .unwrap();
    let from_generic = core_main_conf::<ngx_core_conf_t>(&cycle, &module)
        .map(|r| r as *const ngx_core_conf_t)
        .unwrap();
    let from_mut = core_main_conf_mut::<ngx_core_conf_t>(&cycle, &module)
        .map(|r| r as *mut ngx_core_conf_t)
        .unwrap();

    assert_eq!(from_helper, from_manual);
    assert_eq!(from_generic, from_manual);
    assert_eq!(from_mut, from_manual);
}

#[test]
// Validates that parser-time ngx_conf_t access delegates through cf.cycle and
// still resolves the same core config slot as manual traversal.
fn conf_core_conf_matches_manual_traversal() {
    let mut core_conf: ngx_core_conf_t = unsafe { MaybeUninit::zeroed().assume_init() };
    let mut slots: [CoreConfSlot; 1] = [(&raw mut core_conf).cast()];

    let mut cycle: ngx_cycle_t = unsafe { MaybeUninit::zeroed().assume_init() };
    cycle.conf_ctx = slots.as_mut_ptr();

    let mut conf: ngx_conf_t = unsafe { MaybeUninit::zeroed().assume_init() };
    conf.cycle = &raw mut cycle;

    let module = module_with_index(0);
    let from_manual = unsafe { manual_core_main_conf_ptr(&cycle, &module) };

    let from_helper = unsafe { conf.core_main_conf_unchecked::<ngx_core_conf_t>(&module) }
        .map(|p| p.as_ptr())
        .unwrap();
    let from_generic = core_main_conf::<ngx_core_conf_t>(&conf, &module)
        .map(|r| r as *const ngx_core_conf_t)
        .unwrap();
    let from_mut = core_main_conf_mut::<ngx_core_conf_t>(&conf, &module)
        .map(|r| r as *mut ngx_core_conf_t)
        .unwrap();

    assert_eq!(from_helper, from_manual);
    assert_eq!(from_generic, from_manual);
    assert_eq!(from_mut, from_manual);
}
