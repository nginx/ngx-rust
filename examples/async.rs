use std::ffi::{c_char, c_void};
use std::ptr::{addr_of, addr_of_mut};
use std::slice;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use ngx::core;
use ngx::ffi::{
    ngx_array_push, ngx_command_t, ngx_conf_t, ngx_connection_t, ngx_event_t, ngx_http_handler_pt, ngx_http_module_t,
    ngx_http_phases_NGX_HTTP_ACCESS_PHASE, ngx_int_t, ngx_module_t, ngx_post_event, ngx_posted_events,
    ngx_posted_next_events, ngx_str_t, ngx_uint_t, NGX_CONF_TAKE1, NGX_HTTP_LOC_CONF, NGX_HTTP_LOC_CONF_OFFSET,
    NGX_HTTP_MODULE,
};
use ngx::http::{self, HttpModule, MergeConfigError};
use ngx::http::{HttpModuleLocationConf, HttpModuleMainConf, NgxHttpCoreModule};
use ngx::{http_request_handler, ngx_log_debug_http, ngx_string};
use tokio::runtime::Runtime;

struct Module;

impl http::HttpModule for Module {
    fn module() -> &'static ngx_module_t {
        unsafe { &*::core::ptr::addr_of!(ngx_http_async_module) }
    }

    unsafe extern "C" fn postconfiguration(cf: *mut ngx_conf_t) -> ngx_int_t {
        // SAFETY: this function is called with non-NULL cf always
        let cf = &mut *cf;
        let cmcf = NgxHttpCoreModule::main_conf_mut(cf).expect("http core main conf");

        let h = ngx_array_push(&mut cmcf.phases[ngx_http_phases_NGX_HTTP_ACCESS_PHASE as usize].handlers)
            as *mut ngx_http_handler_pt;
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

unsafe extern "C" fn check_async_work_done(event: *mut ngx_event_t) {
    let ctx = ngx::ngx_container_of!(event, RequestCTX, event);
    let c: *mut ngx_connection_t = (*event).data.cast();

    if (*ctx).done.load(Ordering::Relaxed) {
        // Triggering async_access_handler again
        ngx_post_event((*c).write, addr_of_mut!(ngx_posted_events));
    } else {
        // this doesn't have have good performance but works as a simple thread-safe example and doesn't causes
        // segfault. The best method that provides both thread-safety and performance requires
        // an nginx patch.
        ngx_post_event(event, addr_of_mut!(ngx_posted_next_events));
    }
}

struct RequestCTX {
    done: Arc<AtomicBool>,
    event: ngx_event_t,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl Default for RequestCTX {
    fn default() -> Self {
        Self {
            done: AtomicBool::new(false).into(),
            event: unsafe { std::mem::zeroed() },
            task: Default::default(),
        }
    }
}

impl Drop for RequestCTX {
    fn drop(&mut self) {
        if let Some(handle) = self.task.take() {
            handle.abort();
        }

        if self.event.posted() != 0 {
            unsafe { ngx::ffi::ngx_delete_posted_event(&mut self.event) };
        }
    }
}

http_request_handler!(async_access_handler, |request: &mut http::Request| {
    let co = Module::location_conf(request).expect("module config is none");

    ngx_log_debug_http!(request, "async module enabled: {}", co.enable);

    if !co.enable {
        return core::Status::NGX_DECLINED;
    }

    if let Some(ctx) = unsafe { request.get_module_ctx::<RequestCTX>(&*addr_of!(ngx_http_async_module)) } {
        if !ctx.done.load(Ordering::Relaxed) {
            return core::Status::NGX_AGAIN;
        }

        return core::Status::NGX_OK;
    }

    let ctx = request.pool().allocate(RequestCTX::default());
    if ctx.is_null() {
        return core::Status::NGX_ERROR;
    }
    request.set_module_ctx(ctx.cast(), unsafe { &*addr_of!(ngx_http_async_module) });

    let ctx = unsafe { &mut *ctx };
    ctx.event.handler = Some(check_async_work_done);
    ctx.event.data = request.connection().cast();
    ctx.event.log = unsafe { (*request.connection()).log };
    unsafe { ngx_post_event(&mut ctx.event, addr_of_mut!(ngx_posted_next_events)) };

    // Request is no longer needed and can be converted to something movable to the async block
    let req = AtomicPtr::new(request.into());
    let done_flag = ctx.done.clone();

    let rt = ngx_http_async_runtime();
    ctx.task = Some(rt.spawn(async move {
        let start = Instant::now();
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let req = unsafe { http::Request::from_ngx_http_request(req.load(Ordering::Relaxed)) };
        // not really thread safe, we should apply all these operation in nginx thread
        // but this is just an example. proper way would be storing these headers in the request ctx
        // and apply them when we get back to the nginx thread.
        req.add_header_out("X-Async-Time", start.elapsed().as_millis().to_string().as_str());

        done_flag.store(true, Ordering::Release);
        // there is a small issue here. If traffic is low we may get stuck behind a 300ms timer
        // in the nginx event loop. To workaround it we can notify the event loop using pthread_kill( nginx_thread, SIGIO )
        // to wake up the event loop. (or patch nginx and use the same trick as the thread pool)
    }));

    core::Status::NGX_AGAIN
});

extern "C" fn ngx_http_async_commands_set_enable(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    unsafe {
        let conf = &mut *(conf as *mut ModuleConfig);
        let args = slice::from_raw_parts((*(*cf).args).elts as *mut ngx_str_t, (*(*cf).args).nelts);
        let val = args[1].to_str();

        // set default value optionally
        conf.enable = false;

        if val.eq_ignore_ascii_case("on") {
            conf.enable = true;
        } else if val.eq_ignore_ascii_case("off") {
            conf.enable = false;
        }
    };

    std::ptr::null_mut()
}

fn ngx_http_async_runtime() -> &'static Runtime {
    // Should not be called from the master process
    assert_ne!(unsafe { ngx::ffi::ngx_process }, ngx::ffi::NGX_PROCESS_MASTER as _);

    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime init")
    })
}
