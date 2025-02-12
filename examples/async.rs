use std::ffi::{c_char, c_void};
use std::ptr::{addr_of, addr_of_mut};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use ngx::core;
use ngx::ffi::{
    ngx_array_push, ngx_command_t, ngx_conf_t, ngx_cycle, ngx_event_t, ngx_http_core_module, ngx_http_core_run_phases,
    ngx_http_handler_pt, ngx_http_module_t, ngx_http_phases_NGX_HTTP_ACCESS_PHASE, ngx_http_request_t, ngx_int_t,
    ngx_module_t, ngx_post_event, ngx_posted_events, ngx_str_t, ngx_uint_t, NGX_CONF_TAKE1, NGX_HTTP_LOC_CONF,
    NGX_HTTP_LOC_CONF_OFFSET, NGX_HTTP_MODULE,
};
use ngx::http::{self, HTTPModule, MergeConfigError};
use ngx::{http_request_handler, ngx_log_debug_http, ngx_string};
use tokio::runtime::Runtime;

struct Module;

impl http::HTTPModule for Module {
    type MainConf = ();
    type SrvConf = ();
    type LocConf = ModuleConfig;

    unsafe extern "C" fn postconfiguration(cf: *mut ngx_conf_t) -> ngx_int_t {
        let cmcf = http::ngx_http_conf_get_module_main_conf(cf, &*addr_of!(ngx_http_core_module));

        let h = ngx_array_push(&mut (*cmcf).phases[ngx_http_phases_NGX_HTTP_ACCESS_PHASE as usize].handlers)
            as *mut ngx_http_handler_pt;
        if h.is_null() {
            return core::Status::NGX_ERROR.into();
        }
        // set an Access phase handler
        *h = Some(async_access_handler);
        core::Status::NGX_OK.into()
    }
}

#[derive(Debug)]
struct ModuleConfig {
    enable: bool,
    rt: Runtime,
}

impl Default for ModuleConfig {
    fn default() -> Self {
        Self {
            enable: Default::default(),
            rt: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap(),
        }
    }
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
    create_main_conf: Some(Module::create_main_conf),
    init_main_conf: Some(Module::init_main_conf),
    create_srv_conf: Some(Module::create_srv_conf),
    merge_srv_conf: Some(Module::merge_srv_conf),
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
    let event = &mut (*event);
    let data = Arc::from_raw(event.data as *const EventData);
    let req = &mut (*(data.request as *const _ as *mut ngx_http_request_t));
    if data.done_flag.load(std::sync::atomic::Ordering::Relaxed) {
        (*req.main).set_count((*req.main).count() - 1);
        ngx_http_core_run_phases(req);
    } else {
        // this doesn't have have good performance but works as a simple thread-safe example and doesn't causes
        // segfault. The best method that provides both thread-safety and performance requires
        // an nginx patch.
        ngx_post_event(event, addr_of_mut!(ngx_posted_events));
    }
}

struct RequestCTX {
    event_data: Option<Arc<EventData>>,
}

struct EventData {
    done_flag: AtomicBool,
    request: *mut ngx_http_request_t,
}

unsafe impl Send for EventData {}
unsafe impl Sync for EventData {}

http_request_handler!(async_access_handler, |request: &mut http::Request| {
    let co = unsafe { request.get_module_loc_conf::<ModuleConfig>(&*addr_of!(ngx_http_async_module)) };
    let co = co.expect("module config is none");
    if !co.enable {
        return core::Status::NGX_DECLINED;
    }

    let event_data = unsafe {
        let ctx = request.get_inner().ctx.add(ngx_http_async_module.ctx_index);
        if (*ctx).is_null() {
            let ctx_data = &mut *(request.pool().alloc(std::mem::size_of::<RequestCTX>()) as *mut RequestCTX);
            ctx_data.event_data = Some(Arc::new(EventData {
                done_flag: AtomicBool::new(false),
                request: &request.get_inner() as *const _ as *mut _,
            }));
            *ctx = ctx_data as *const _ as _;
            ctx_data.event_data.as_ref().unwrap().clone()
        } else {
            let ctx = &*(ctx as *const RequestCTX);
            if ctx
                .event_data
                .as_ref()
                .unwrap()
                .done_flag
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                return core::Status::NGX_OK;
            } else {
                return core::Status::NGX_DONE;
            }
        }
    };

    event_data.done_flag.load(std::sync::atomic::Ordering::Relaxed);

    // create a posted event
    unsafe {
        let event = &mut *(request.pool().calloc(std::mem::size_of::<ngx_event_t>()) as *mut ngx_event_t);
        event.handler = Some(check_async_work_done);
        event.data = Arc::into_raw(event_data.clone()) as _;
        event.log = (*ngx_cycle).log;

        ngx_post_event(event, addr_of_mut!(ngx_posted_events));
    }

    ngx_log_debug_http!(request, "async module enabled: {}", co.enable);

    co.rt.spawn(async move {
        let start = Instant::now();
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let req = unsafe { http::Request::from_ngx_http_request(event_data.request) };
        // not really thread safe, we should apply all these operation in nginx thread
        // but this is just an example. proper way would be storing these headers in the request ctx
        // and apply them when we get back to the nginx thread.
        req.add_header_out("X-Async-Time", start.elapsed().as_millis().to_string().as_str());

        event_data.done_flag.store(true, std::sync::atomic::Ordering::Release);
        // there is a small issue here. If traffic is low we may get stuck behind a 300ms timer
        // in the nginx event loop. To workaround it we can notify the event loop using pthread_kill( nginx_thread, SIGIO )
        // to wake up the event loop. (or patch nginx and use the same trick as the thread pool)
    });

    unsafe {
        (*request.get_inner().main).set_count((*request.get_inner().main).count() + 1);
    }
    core::Status::NGX_DONE
});

extern "C" fn ngx_http_async_commands_set_enable(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    unsafe {
        let conf = &mut *(conf as *mut ModuleConfig);
        let args = (*(*cf).args).elts as *mut ngx_str_t;

        let val = (*args.add(1)).to_str();

        // set default value optionally
        conf.enable = false;

        if val.len() == 2 && val.eq_ignore_ascii_case("on") {
            conf.enable = true;
        } else if val.len() == 3 && val.eq_ignore_ascii_case("off") {
            conf.enable = false;
        }
    };

    std::ptr::null_mut()
}
