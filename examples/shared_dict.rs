#![no_std]
use core::ffi::{c_char, c_void};
use core::mem;
use core::ptr::{self, NonNull};

use nginx_sys::{
    NGX_CONF_TAKE2, NGX_HTTP_DELETE, NGX_HTTP_MAIN_CONF, NGX_HTTP_MAIN_CONF_OFFSET,
    NGX_HTTP_MODULE, NGX_HTTP_VAR_CHANGEABLE, NGX_HTTP_VAR_NOCACHEABLE, NGX_LOG_EMERG,
    ngx_command_t, ngx_conf_t, ngx_http_add_variable, ngx_http_compile_complex_value_t,
    ngx_http_complex_value, ngx_http_complex_value_t, ngx_http_module_t, ngx_http_request_t,
    ngx_http_variable_t, ngx_http_variable_value_t, ngx_int_t, ngx_module_t, ngx_parse_size,
    ngx_str_t, ngx_uint_t,
};
use ngx::allocator::AllocError;
use ngx::collections::RbTreeMap;
use ngx::core::{
    NGX_CONF_ERROR, NGX_CONF_OK, NgxStr, NgxString, Pool, SharedZone, SharedZoneData, SlabPool,
    Status,
};
use ngx::http::{HttpModule, HttpModuleMainConf};
use ngx::{ngx_conf_log_error, ngx_log_debug, ngx_string};

struct HttpSharedDictModule;

impl HttpModule for HttpSharedDictModule {
    fn module() -> &'static ngx_module_t {
        unsafe { &*ptr::addr_of!(ngx_http_shared_dict_module) }
    }

    unsafe extern "C" fn preconfiguration(cf: *mut ngx_conf_t) -> ngx_int_t {
        for mut v in unsafe { NGX_HTTP_SHARED_DICT_VARS } {
            let var = NonNull::new(unsafe { ngx_http_add_variable(cf, &raw mut v.name, v.flags) });
            if var.is_none() {
                return Status::NGX_ERROR.into();
            }
            let var = unsafe { var.unwrap().as_mut() };
            var.get_handler = v.get_handler;
            var.set_handler = v.set_handler;
            var.data = v.data;
        }
        Status::NGX_OK.into()
    }
}

unsafe impl HttpModuleMainConf for HttpSharedDictModule {
    type MainConf = SharedDictMainConfig;
}

static mut NGX_HTTP_SHARED_DICT_COMMANDS: [ngx_command_t; 3] = [
    ngx_command_t {
        name: ngx_string!("shared_dict_zone"),
        type_: (NGX_HTTP_MAIN_CONF | NGX_CONF_TAKE2) as ngx_uint_t,
        set: Some(ngx_http_shared_dict_add_zone),
        conf: NGX_HTTP_MAIN_CONF_OFFSET,
        offset: 0,
        post: ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("shared_dict"),
        type_: (NGX_HTTP_MAIN_CONF | NGX_CONF_TAKE2) as ngx_uint_t,
        set: Some(ngx_http_shared_dict_add_variable),
        conf: NGX_HTTP_MAIN_CONF_OFFSET,
        offset: 0,
        post: ptr::null_mut(),
    },
    ngx_command_t::empty(),
];

static mut NGX_HTTP_SHARED_DICT_VARS: [ngx_http_variable_t; 1] = [ngx_http_variable_t {
    name: ngx_string!("shared_dict_entries"),
    set_handler: Some(ngx_http_shared_dict_set_entries),
    get_handler: Some(ngx_http_shared_dict_get_entries),
    data: 0,
    flags: (NGX_HTTP_VAR_CHANGEABLE | NGX_HTTP_VAR_NOCACHEABLE) as ngx_uint_t,
    index: 0,
}];

static NGX_HTTP_SHARED_DICT_MODULE_CTX: ngx_http_module_t = ngx_http_module_t {
    preconfiguration: Some(HttpSharedDictModule::preconfiguration),
    postconfiguration: None,
    create_main_conf: Some(HttpSharedDictModule::create_main_conf),
    init_main_conf: None,
    create_srv_conf: None,
    merge_srv_conf: None,
    create_loc_conf: None,
    merge_loc_conf: None,
};

// Generate the `ngx_modules` table with exported modules.
// This feature is required to build a 'cdylib' dynamic module outside of the NGINX buildsystem.
#[cfg(feature = "export-modules")]
ngx::ngx_modules!(ngx_http_shared_dict_module);

#[used]
#[allow(non_upper_case_globals)]
#[cfg_attr(not(feature = "export-modules"), unsafe(no_mangle))]
pub static mut ngx_http_shared_dict_module: ngx_module_t = ngx_module_t {
    ctx: &raw const NGX_HTTP_SHARED_DICT_MODULE_CTX as _,
    commands: unsafe { &raw mut NGX_HTTP_SHARED_DICT_COMMANDS[0] },
    type_: NGX_HTTP_MODULE as _,
    ..ngx_module_t::default()
};

/// Contents of the shared memory zone.
struct SharedDict(ngx::sync::RwLock<RbTreeMap<NgxString<SlabPool>, NgxString<SlabPool>, SlabPool>>);

impl SharedZoneData for SharedDict {
    fn new_in(alloc: SlabPool) -> Result<Self, AllocError> {
        Ok(Self(ngx::sync::RwLock::new(RbTreeMap::try_new_in(alloc)?)))
    }
}

#[derive(Debug, Default)]
struct SharedDictMainConfig {
    shm_zone: Option<SharedZone<SharedDict>>,
}

extern "C" fn ngx_http_shared_dict_add_zone(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    // SAFETY: configuration handlers always receive a valid `cf` pointer.
    let cf = unsafe { cf.as_mut().unwrap() };
    let smcf =
        unsafe { conf.cast::<SharedDictMainConfig>().as_mut().expect("shared dict main config") };

    // SAFETY:
    // - `cf.args` is guaranteed to be a pointer to an array with 3 elements (NGX_CONF_TAKE2).
    // - The pointers are well-aligned by construction method (`ngx_palloc`).
    debug_assert!(!cf.args.is_null() && unsafe { (*cf.args).nelts >= 3 });
    let args = unsafe { (*cf.args).as_slice_mut() };

    // SAFETY: the directive arguments are valid nginx strings owned by the configuration pool.
    let name = unsafe { NgxStr::from_ngx_str(args[1]) };
    let size = unsafe { ngx_parse_size(&raw mut args[2]) };
    if size == -1 {
        return NGX_CONF_ERROR;
    }

    match SharedZone::add(cf, name, size as usize, HttpSharedDictModule::module()) {
        Ok(zone) => {
            smcf.shm_zone = Some(zone);
            NGX_CONF_OK
        }
        Err(_) => NGX_CONF_ERROR,
    }
}

extern "C" fn ngx_http_shared_dict_add_variable(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    _conf: *mut c_void,
) -> *mut c_char {
    // SAFETY: configuration handlers always receive a valid `cf` pointer.
    let cf = unsafe { cf.as_mut().unwrap() };
    let pool = unsafe { Pool::from_ngx_pool(cf.pool) };

    let key = pool.calloc_type::<ngx_http_complex_value_t>();
    if key.is_null() {
        return NGX_CONF_ERROR;
    }

    // SAFETY:
    // - `cf.args` is guaranteed to be a pointer to an array with 3 elements (NGX_CONF_TAKE2).
    // - The pointers are well-aligned by construction method (`ngx_palloc`).
    debug_assert!(!cf.args.is_null() && unsafe { (*cf.args).nelts >= 3 });
    let args = unsafe { (*cf.args).as_slice_mut() };

    let mut ccv: ngx_http_compile_complex_value_t = unsafe { mem::zeroed() };
    ccv.cf = cf;
    ccv.value = &raw mut args[1];
    ccv.complex_value = key;

    if unsafe { nginx_sys::ngx_http_compile_complex_value(&raw mut ccv) } != Status::NGX_OK.into() {
        return NGX_CONF_ERROR;
    }

    let mut name = args[2];

    if name.as_bytes()[0] != b'$' {
        ngx_conf_log_error!(NGX_LOG_EMERG, cf, "invalid variable name \"{name}\"");
        return NGX_CONF_ERROR;
    }

    name.data = unsafe { name.data.add(1) };
    name.len -= 1;

    let var = unsafe {
        ngx_http_add_variable(
            cf,
            &raw mut name,
            (NGX_HTTP_VAR_CHANGEABLE | NGX_HTTP_VAR_NOCACHEABLE) as ngx_uint_t,
        )
    };
    if var.is_null() {
        return NGX_CONF_ERROR;
    }

    unsafe {
        (*var).get_handler = Some(ngx_http_shared_dict_get_variable);
        (*var).set_handler = Some(ngx_http_shared_dict_set_variable);
        (*var).data = key as usize;
    }

    NGX_CONF_OK
}

extern "C" fn ngx_http_shared_dict_get_variable(
    r: *mut ngx_http_request_t,
    v: *mut ngx_http_variable_value_t,
    data: usize,
) -> ngx_int_t {
    let r = unsafe { &mut *r };
    let v = unsafe { &mut *v };
    let smcf = HttpSharedDictModule::main_conf_mut(r).expect("shared dict main config");

    let mut key = ngx_str_t::empty();
    if unsafe { ngx_http_complex_value(r, data as _, &raw mut key) } != Status::NGX_OK.into() {
        return Status::NGX_ERROR.into();
    }

    let key = unsafe { NgxStr::from_ngx_str(key) };

    let Some(shared) = smcf.shm_zone.as_ref().and_then(SharedZone::get) else {
        return Status::NGX_ERROR.into();
    };

    let value = shared
        .0
        .read()
        .get(key)
        .and_then(|x| unsafe { ngx_str_t::from_bytes(r.pool, x.as_bytes()) });

    ngx_log_debug!(
        unsafe { (*r.connection).log },
        "shared dict: get \"{}\" -> {:?} w:{} p:{}",
        key,
        value.as_ref().map(|x| unsafe { NgxStr::from_ngx_str(*x) }),
        unsafe { nginx_sys::ngx_worker },
        unsafe { nginx_sys::ngx_pid },
    );

    let Some(value) = value else {
        v.set_not_found(1);
        return Status::NGX_ERROR.into();
    };

    v.data = value.data;
    v.set_len(value.len as _);

    v.set_valid(1);
    v.set_no_cacheable(0);
    v.set_not_found(0);

    Status::NGX_OK.into()
}

extern "C" fn ngx_http_shared_dict_set_variable(
    r: *mut ngx_http_request_t,
    v: *mut ngx_http_variable_value_t,
    data: usize,
) {
    let r = unsafe { &mut *r };
    let v = unsafe { &mut *v };
    let smcf = HttpSharedDictModule::main_conf_mut(r).expect("shared dict main config");
    let mut key = ngx_str_t::empty();

    if unsafe { ngx_http_complex_value(r, data as _, &raw mut key) } != Status::NGX_OK.into() {
        return;
    }

    let Some(shared) = smcf.shm_zone.as_ref().and_then(SharedZone::get) else {
        return;
    };

    if r.method == NGX_HTTP_DELETE as _ {
        let key = unsafe { NgxStr::from_ngx_str(key) };

        ngx_log_debug!(
            unsafe { (*r.connection).log },
            "shared dict: delete \"{}\" w:{} p:{}",
            key,
            unsafe { nginx_sys::ngx_worker },
            unsafe { nginx_sys::ngx_pid },
        );

        let _ = shared.0.write().remove(key);
    } else {
        let Some(alloc) = smcf.shm_zone.as_ref().and_then(SharedZone::slab_pool) else {
            return;
        };

        let Ok(key) = NgxString::try_from_bytes_in(key.as_bytes(), alloc.clone()) else {
            return;
        };

        let Ok(value) = NgxString::try_from_bytes_in(v.as_bytes(), alloc.clone()) else {
            return;
        };

        ngx_log_debug!(
            unsafe { (*r.connection).log },
            "shared dict: set \"{}\" -> \"{}\" w:{} p:{}",
            key,
            value,
            unsafe { nginx_sys::ngx_worker },
            unsafe { nginx_sys::ngx_pid },
        );

        let _ = shared.0.write().try_insert(key, value);
    }
}

extern "C" fn ngx_http_shared_dict_get_entries(
    r: *mut ngx_http_request_t,
    v: *mut ngx_http_variable_value_t,
    _data: usize,
) -> ngx_int_t {
    use core::fmt::Write;

    let r = unsafe { &mut *r };
    let v = unsafe { &mut *v };
    let pool = unsafe { Pool::from_ngx_pool(r.pool) };
    let smcf = HttpSharedDictModule::main_conf_mut(r).expect("shared dict main config");

    ngx_log_debug!(unsafe { (*r.connection).log }, "shared dict: get all entries");

    let Some(shared) = smcf.shm_zone.as_ref().and_then(SharedZone::get) else {
        return Status::NGX_ERROR.into();
    };

    let mut str = NgxString::new_in(pool);
    {
        let dict = shared.0.read();

        let mut len: usize = 0;
        let mut values: usize = 0;

        for (key, value) in dict.iter() {
            len += key.len() + value.len() + b" = ; ".len();
            values += 1;
        }

        len += values.checked_ilog10().unwrap_or(0) as usize + b"0; ".len();

        if str.try_reserve(len).is_err() {
            return Status::NGX_ERROR.into();
        }

        if write!(str, "{values}; ").is_err() {
            return Status::NGX_ERROR.into();
        }

        for (key, value) in dict.iter() {
            if write!(str, "{key} = {value}; ").is_err() {
                return Status::NGX_ERROR.into();
            }
        }
    }

    // The string is allocated on the `ngx_pool_t` and will be freed with the request.
    let (data, len, _, _) = str.into_raw_parts();

    v.data = data;
    v.set_len(len as _);

    v.set_valid(1);
    v.set_no_cacheable(1);
    v.set_not_found(0);

    Status::NGX_OK.into()
}

extern "C" fn ngx_http_shared_dict_set_entries(
    r: *mut ngx_http_request_t,
    _v: *mut ngx_http_variable_value_t,
    _data: usize,
) {
    let r = unsafe { &mut *r };
    let smcf = HttpSharedDictModule::main_conf_mut(r).expect("shared dict main config");

    ngx_log_debug!(unsafe { (*r.connection).log }, "shared dict: clear");

    let Some(shared) = smcf.shm_zone.as_ref().and_then(SharedZone::get) else {
        return;
    };

    let Ok(tree) = RbTreeMap::try_new_in(shared.0.read().allocator().clone()) else {
        return;
    };

    // This would check both .clear() and the drop implementation
    *shared.0.write() = tree;
    // shared.0.write().clear()
}
