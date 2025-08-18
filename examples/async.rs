use std::ffi::{c_char, c_void};
use std::time::Instant;

use ngx::async_::{sleep, spawn, Task};
use ngx::core;
use ngx::ffi::{
    ngx_array_push, ngx_buf_t, ngx_chain_t, ngx_command_t, ngx_conf_t, ngx_http_finalize_request,
    ngx_http_handler_pt, ngx_http_module_t, ngx_http_phases_NGX_HTTP_ACCESS_PHASE,
    ngx_http_read_client_request_body, ngx_http_request_t, ngx_int_t, ngx_module_t, ngx_str_t,
    ngx_uint_t, NGX_CONF_TAKE1, NGX_HTTP_LOC_CONF, NGX_HTTP_LOC_CONF_OFFSET, NGX_HTTP_MODULE,
    NGX_HTTP_SPECIAL_RESPONSE, NGX_LOG_EMERG,
};
use ngx::http::{self, HttpModule, MergeConfigError};
use ngx::http::{HttpModuleLocationConf, HttpModuleMainConf, NgxHttpCoreModule};
use ngx::{http_request_handler, ngx_conf_log_error, ngx_log_debug_http, ngx_string};

struct Module;

impl http::HttpModule for Module {
    fn module() -> &'static ngx_module_t {
        unsafe { &*std::ptr::addr_of!(ngx_http_async_module) }
    }

    unsafe extern "C" fn postconfiguration(cf: *mut ngx_conf_t) -> ngx_int_t {
        // SAFETY: this function is called with non-NULL cf always
        let cf = &mut *cf;
        let cmcf = NgxHttpCoreModule::main_conf_mut(cf).expect("http core main conf");

        let h = ngx_array_push(
            &mut cmcf.phases[ngx_http_phases_NGX_HTTP_ACCESS_PHASE as usize].handlers,
        ) as *mut ngx_http_handler_pt;
        if h.is_null() {
            return core::Status::NGX_ERROR.into();
        }
        // set an Access phase handler
        *h = Some(async_access_handler);
        core::Status::NGX_OK.into()
    }
}

#[derive(Debug, Default)]
struct ModuleConfig {
    enable: bool,
}

unsafe impl HttpModuleLocationConf for Module {
    type LocationConf = ModuleConfig;
}

static mut NGX_HTTP_ASYNC_COMMANDS: [ngx_command_t; 2] = [
    ngx_command_t {
        name: ngx_string!("async"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(ngx_http_async_commands_set_enable),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: std::ptr::null_mut(),
    },
    ngx_command_t::empty(),
];

static NGX_HTTP_ASYNC_MODULE_CTX: ngx_http_module_t = ngx_http_module_t {
    preconfiguration: Some(Module::preconfiguration),
    postconfiguration: Some(Module::postconfiguration),
    create_main_conf: None,
    init_main_conf: None,
    create_srv_conf: None,
    merge_srv_conf: None,
    create_loc_conf: Some(Module::create_loc_conf),
    merge_loc_conf: Some(Module::merge_loc_conf),
};

// Generate the `ngx_modules` table with exported modules.
// This feature is required to build a 'cdylib' dynamic module outside of the NGINX buildsystem.
#[cfg(feature = "export-modules")]
ngx::ngx_modules!(ngx_http_async_module);

#[used]
#[allow(non_upper_case_globals)]
#[cfg_attr(not(feature = "export-modules"), no_mangle)]
pub static mut ngx_http_async_module: ngx_module_t = ngx_module_t {
    ctx: std::ptr::addr_of!(NGX_HTTP_ASYNC_MODULE_CTX) as _,
    commands: unsafe { &NGX_HTTP_ASYNC_COMMANDS[0] as *const _ as *mut _ },
    type_: NGX_HTTP_MODULE as _,
    ..ngx_module_t::default()
};

impl http::Merge for ModuleConfig {
    fn merge(&mut self, prev: &ModuleConfig) -> Result<(), MergeConfigError> {
        if prev.enable {
            self.enable = true;
        };
        Ok(())
    }
}

extern "C" fn ngx_http_async_commands_set_enable(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    unsafe {
        let conf = &mut *(conf as *mut ModuleConfig);
        let args: &[ngx_str_t] = (*(*cf).args).as_slice();
        let val = match args[1].to_str() {
            Ok(s) => s,
            Err(_) => {
                ngx_conf_log_error!(NGX_LOG_EMERG, cf, "`async` argument is not utf-8 encoded");
                return ngx::core::NGX_CONF_ERROR;
            }
        };

        // set default value optionally
        conf.enable = false;

        if val.eq_ignore_ascii_case("on") {
            conf.enable = true;
        } else if val.eq_ignore_ascii_case("off") {
            conf.enable = false;
        }
    };

    ngx::core::NGX_CONF_OK
}

http_request_handler!(async_access_handler, |request: &mut http::Request| {
    let co = Module::location_conf(request).expect("module config is none");

    ngx_log_debug_http!(request, "async module enabled: {}", co.enable);

    if !co.enable {
        return core::Status::NGX_DECLINED;
    }

    if request
        .get_module_ctx::<Task<()>>(unsafe { &*std::ptr::addr_of!(ngx_http_async_module) })
        .is_some()
    {
        return core::Status::NGX_DONE;
    }

    let rc =
        unsafe { ngx_http_read_client_request_body(request.into(), Some(content_event_handler)) };
    if rc as u32 >= NGX_HTTP_SPECIAL_RESPONSE {
        return core::Status(rc);
    }

    core::Status::NGX_DONE
});

extern "C" fn content_event_handler(request: *mut ngx_http_request_t) {
    let task = spawn(async move {
        let start = Instant::now();
        sleep(std::time::Duration::from_secs(2)).await;

        let req = unsafe { http::Request::from_ngx_http_request(request) };
        req.add_header_out(
            "X-Async-Time",
            start.elapsed().as_millis().to_string().as_str(),
        );
        req.set_status(http::HTTPStatus::OK);
        req.send_header();
        let buf = req.pool().calloc(std::mem::size_of::<ngx_buf_t>()) as *mut ngx_buf_t;
        unsafe {
            (*buf).set_last_buf(if req.is_main() { 1 } else { 0 });
            (*buf).set_last_in_chain(1);
        }
        req.output_filter(&mut ngx_chain_t {
            buf,
            next: std::ptr::null_mut(),
        });

        unsafe {
            ngx::ffi::ngx_post_event(
                (*(*request).connection).write,
                std::ptr::addr_of_mut!(ngx::ffi::ngx_posted_events),
            );
        }
    });

    let req = unsafe { http::Request::from_ngx_http_request(request) };

    let ctx = req.pool().allocate::<Task<()>>(task);
    if ctx.is_null() {
        unsafe { ngx_http_finalize_request(request, core::Status::NGX_ERROR.into()) };
        return;
    }
    req.set_module_ctx(ctx.cast(), unsafe {
        &*std::ptr::addr_of!(ngx_http_async_module)
    });
    unsafe { (*request).write_event_handler = Some(write_event_handler) };
}

extern "C" fn write_event_handler(request: *mut ngx_http_request_t) {
    let req = unsafe { http::Request::from_ngx_http_request(request) };
    if let Some(task) =
        req.get_module_ctx::<Task<()>>(unsafe { &*std::ptr::addr_of!(ngx_http_async_module) })
    {
        if task.is_finished() {
            unsafe { ngx_http_finalize_request(request, core::Status::NGX_OK.into()) };
            return;
        }
    }

    let write_event =
        unsafe { (*(*request).connection).write.as_ref() }.expect("write event is not null");
    if write_event.timedout() != 0 {
        unsafe {
            ngx::ffi::ngx_connection_error(
                (*request).connection,
                ngx::ffi::NGX_ETIMEDOUT as i32,
                c"client timed out".as_ptr() as *mut _,
            )
        };
        return;
    }

    if unsafe { ngx::ffi::ngx_http_output_filter(request, std::ptr::null_mut()) }
        == ngx::ffi::NGX_ERROR as isize
    {
        // Client error
        return;
    }
    let clcf =
        NgxHttpCoreModule::location_conf(unsafe { request.as_ref().expect("request not null") })
            .expect("http core server conf");

    if unsafe {
        ngx::ffi::ngx_handle_write_event(std::ptr::from_ref(write_event) as *mut _, clcf.send_lowat)
    } != ngx::ffi::NGX_OK as isize
    {
        // Client error
        return;
    }

    if write_event.delayed() == 0 {
        if (write_event.active() != 0) && (write_event.ready() == 0) {
            unsafe {
                ngx::ffi::ngx_add_timer(
                    std::ptr::from_ref(write_event) as *mut _,
                    clcf.send_timeout,
                )
            }
        } else if write_event.timer_set() != 0 {
            unsafe { ngx::ffi::ngx_del_timer(std::ptr::from_ref(write_event) as *mut _) }
        }
    }
}
