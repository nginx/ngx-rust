#![no_std]
use ::core::ffi::{c_char, c_void};
use ::core::{mem, ptr};

use nginx_sys::{
    ngx_command_t, ngx_conf_t, ngx_http_add_variable, ngx_http_compile_complex_value_t,
    ngx_http_complex_value_t, ngx_http_module_t, ngx_http_variable_t, ngx_int_t, ngx_module_t,
    ngx_parse_size, ngx_shared_memory_add, ngx_shm_zone_t, ngx_str_t, ngx_uint_t,
    ngx_variable_value_t, NGX_CONF_TAKE2, NGX_HTTP_MAIN_CONF, NGX_HTTP_MAIN_CONF_OFFSET,
    NGX_HTTP_MODULE, NGX_HTTP_VAR_CHANGEABLE, NGX_HTTP_VAR_NOCACHEABLE, NGX_LOG_EMERG,
};
use ngx::collections::RbTreeMap;
use ngx::core::{NgxStr, NgxString, Pool, SlabPool, Status, NGX_CONF_ERROR, NGX_CONF_OK};
use ngx::http::{HttpModule, HttpModuleMainConf, Request};
use ngx::{http_variable_get, http_variable_set, ngx_conf_log_error, ngx_log_debug, ngx_string};

struct HttpSharedDictModule;

impl HttpModule for HttpSharedDictModule {
    fn module() -> &'static ngx_module_t {
        unsafe { &*ptr::addr_of!(ngx_http_shared_dict_module) }
    }

    unsafe extern "C" fn preconfiguration(cf: *mut ngx_conf_t) -> ngx_int_t {
        for mut v in NGX_HTTP_SHARED_DICT_VARS {
            let var = ngx_http_add_variable(cf, &mut v.name, v.flags);
            if var.is_null() {
                return Status::NGX_ERROR.into();
            }
            (*var).get_handler = v.get_handler;
            (*var).set_handler = v.set_handler;
            (*var).data = v.data;
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
#[cfg_attr(not(feature = "export-modules"), no_mangle)]
pub static mut ngx_http_shared_dict_module: ngx_module_t = ngx_module_t {
    ctx: ptr::addr_of!(NGX_HTTP_SHARED_DICT_MODULE_CTX) as _,
    commands: unsafe { ptr::addr_of_mut!(NGX_HTTP_SHARED_DICT_COMMANDS[0]) },
    type_: NGX_HTTP_MODULE as _,
    ..ngx_module_t::default()
};

type SharedData = ngx::sync::RwLock<RbTreeMap<NgxString<SlabPool>, NgxString<SlabPool>, SlabPool>>;

#[derive(Debug)]
struct SharedDictMainConfig {
    shm_zone: *mut ngx_shm_zone_t,
}

impl Default for SharedDictMainConfig {
    fn default() -> Self {
        Self {
            shm_zone: ptr::null_mut(),
        }
    }
}

extern "C" fn ngx_http_shared_dict_add_zone(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    // SAFETY: configuration handlers always receive a valid `cf` pointer.
    let cf = unsafe { cf.as_mut().unwrap() };
    let smcf = unsafe {
        conf.cast::<SharedDictMainConfig>()
            .as_mut()
            .expect("shared dict main config")
    };

    // SAFETY:
    // - `cf.args` is guaranteed to be a pointer to an array with 3 elements (NGX_CONF_TAKE2).
    // - The pointers are well-aligned by construction method (`ngx_palloc`).
    debug_assert!(!cf.args.is_null() && unsafe { (*cf.args).nelts >= 3 });
    let args = unsafe { (*cf.args).as_slice_mut() };

    let name: ngx_str_t = args[1];
    let size = unsafe { ngx_parse_size(&mut args[2]) };
    if size == -1 {
        return NGX_CONF_ERROR;
    }

    smcf.shm_zone = unsafe {
        ngx_shared_memory_add(
            cf,
            ptr::addr_of!(name).cast_mut(),
            size as usize,
            ptr::addr_of_mut!(ngx_http_shared_dict_module).cast(),
        )
    };

    let Some(shm_zone) = (unsafe { smcf.shm_zone.as_mut() }) else {
        return NGX_CONF_ERROR;
    };

    shm_zone.init = Some(ngx_http_shared_dict_zone_init);
    shm_zone.data = ptr::from_mut(smcf).cast();

    NGX_CONF_OK
}

fn ngx_http_shared_dict_get_shared(shm_zone: &mut ngx_shm_zone_t) -> Option<&SharedData> {
    let mut alloc = unsafe { SlabPool::from_shm_zone(shm_zone) }?;

    if alloc.as_mut().data.is_null() {
        let shared: RbTreeMap<NgxString<SlabPool>, NgxString<SlabPool>, SlabPool> =
            RbTreeMap::try_new_in(alloc.clone()).ok()?;

        let shared = ngx::sync::RwLock::new(shared);

        alloc.as_mut().data = ngx::allocator::allocate(shared, &alloc)
            .ok()?
            .as_ptr()
            .cast();
    }

    unsafe { alloc.as_ref().data.cast::<SharedData>().as_ref() }
}

extern "C" fn ngx_http_shared_dict_zone_init(
    shm_zone: *mut ngx_shm_zone_t,
    _data: *mut c_void,
) -> ngx_int_t {
    let shm_zone = unsafe { &mut *shm_zone };

    ngx_http_shared_dict_get_shared(shm_zone)
        .map_or_else(|| Status::NGX_ERROR, |_| Status::NGX_OK)
        .into()
}

extern "C" fn ngx_http_shared_dict_add_variable(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    _conf: *mut c_void,
) -> *mut c_char {
    // SAFETY: configuration handlers always receive a valid `cf` pointer.
    let cf = unsafe { cf.as_mut().unwrap() };
    let pool = unsafe { Pool::from_ngx_pool(cf.pool) };

    let key = match pool.allocate_type_zeroed::<ngx_http_complex_value_t>() {
        Ok(p) => p.as_ptr(),
        Err(_) => return NGX_CONF_ERROR,
    };

    // SAFETY:
    // - `cf.args` is guaranteed to be a pointer to an array with 3 elements (NGX_CONF_TAKE2).
    // - The pointers are well-aligned by construction method (`ngx_palloc`).
    debug_assert!(!cf.args.is_null() && unsafe { (*cf.args).nelts >= 3 });
    let args = unsafe { (*cf.args).as_slice_mut() };

    let mut ccv: ngx_http_compile_complex_value_t = unsafe { mem::zeroed() };
    ccv.cf = cf;
    ccv.value = &mut args[1];
    ccv.complex_value = key;

    if unsafe { nginx_sys::ngx_http_compile_complex_value(&mut ccv) } != Status::NGX_OK.into() {
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
            &mut name,
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

http_variable_get!(
    ngx_http_shared_dict_get_variable,
    |r: &mut Request, v: &mut ngx_variable_value_t, data: usize| {
        let smcf = HttpSharedDictModule::main_conf_mut(r).expect("shared dict main config");

        let key = r.get_complex_value(&*(data as *mut ngx_http_complex_value_t))?;

        let shared = ngx_http_shared_dict_get_shared(unsafe { &mut *smcf.shm_zone })?;

        let value = shared
            .read()
            .get(key)
            .and_then(|x| unsafe { ngx_str_t::from_bytes(r.as_ref().pool, x.as_bytes()) });

        ngx_log_debug!(
            unsafe { (*r.connection()).log },
            "shared dict: get \"{}\" -> {:?} w:{} p:{}",
            key,
            value.as_ref().map(|x| unsafe { NgxStr::from_ngx_str(*x) }),
            unsafe { nginx_sys::ngx_worker },
            unsafe { nginx_sys::ngx_pid },
        );

        let Some(value) = value else {
            v.set_not_found(1);
            return None;
        };

        v.data = value.data;
        v.set_len(value.len as _);

        v.set_valid(1);
        v.set_no_cacheable(0);
        v.set_not_found(0);

        Some(Status::NGX_OK.into())
    }
);

http_variable_set!(
    ngx_http_shared_dict_set_variable,
    |r: &mut Request, v: &mut ngx_variable_value_t, data: usize| {
        let smcf = HttpSharedDictModule::main_conf_mut(r).expect("shared dict main config");

        let key = r.get_complex_value(&*(data as *mut ngx_http_complex_value_t))?;

        let shared = ngx_http_shared_dict_get_shared(unsafe { &mut *smcf.shm_zone })?;

        if r.method() == ngx::http::Method::DELETE {
            ngx_log_debug!(
                unsafe { (*r.connection()).log },
                "shared dict: delete \"{}\" w:{} p:{}",
                key,
                unsafe { nginx_sys::ngx_worker },
                unsafe { nginx_sys::ngx_pid },
            );

            let _ = shared.write().remove(key);
        } else {
            let alloc = unsafe { SlabPool::from_shm_zone(&*smcf.shm_zone).expect("slab pool") };

            let key = NgxString::try_from_bytes_in(key.as_bytes(), alloc.clone()).ok()?;

            let value = NgxString::try_from_bytes_in(v.as_bytes(), alloc.clone()).ok()?;

            ngx_log_debug!(
                unsafe { (*r.connection()).log },
                "shared dict: set \"{}\" -> \"{}\" w:{} p:{}",
                key,
                value,
                unsafe { nginx_sys::ngx_worker },
                unsafe { nginx_sys::ngx_pid },
            );

            let _ = shared.write().try_insert(key, value);
        }
        Some(())
    }
);

http_variable_get!(
    ngx_http_shared_dict_get_entries,
    |r: &mut Request, v: &mut ngx_variable_value_t, _data: usize| {
        use core::fmt::Write;

        let smcf = HttpSharedDictModule::main_conf_mut(r).expect("shared dict main config");

        ngx_log_debug!(
            unsafe { (*r.connection()).log },
            "shared dict: get all entries"
        );

        let shared = ngx_http_shared_dict_get_shared(unsafe { &mut *smcf.shm_zone })?;

        let mut str = NgxString::new_in(r.pool());
        {
            let dict = shared.read();

            let mut len: usize = 0;
            let mut values: usize = 0;

            for (key, value) in dict.iter() {
                len += key.len() + value.len() + b" = ; ".len();
                values += 1;
            }

            len += values.checked_ilog10().unwrap_or(0) as usize + b"0; ".len();

            str.try_reserve(len).ok()?;

            write!(str, "{values}; ").ok()?;

            for (key, value) in dict.iter() {
                write!(str, "{key} = {value}; ").ok()?;
            }
        }

        // The string is allocated on the `ngx_pool_t` and will be freed with the request.
        let (data, len, _, _) = str.into_raw_parts();

        v.data = data;
        v.set_len(len as _);

        v.set_valid(1);
        v.set_no_cacheable(1);
        v.set_not_found(0);

        Some(Status::NGX_OK.into())
    }
);

http_variable_set!(
    ngx_http_shared_dict_set_entries,
    |r: &mut Request, _v: &mut ngx_variable_value_t, _data: usize| {
        let smcf = HttpSharedDictModule::main_conf_mut(r).expect("shared dict main config");

        ngx_log_debug!(unsafe { (*r.connection()).log }, "shared dict: clear");

        let shared = ngx_http_shared_dict_get_shared(unsafe { &mut *smcf.shm_zone })?;

        let tree = RbTreeMap::try_new_in(shared.read().allocator().clone()).ok()?;

        // This would check both .clear() and the drop implementation
        *shared.write() = tree;
        // shared.write().clear()
        Some(())
    }
);
