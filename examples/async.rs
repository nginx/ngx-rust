use async_compat::Compat;
use futures::future::{self};
use futures_util::FutureExt;
use http_body_util::Empty;
use hyper::body::Bytes;
use hyper_util::rt::TokioIo;
use nginx_sys::{ngx_http_core_loc_conf_t, NGX_LOG_ERR};
use ngx::async_::resolver::Resolver;
use ngx::async_::{spawn, Task};
use ngx::core::{self, Pool, Status};
use ngx::ffi::{
    ngx_array_push, ngx_command_t, ngx_conf_t, ngx_connection_t, ngx_http_handler_pt,
    ngx_http_module_t, ngx_http_phases_NGX_HTTP_ACCESS_PHASE, ngx_int_t, ngx_module_t,
    ngx_post_event, ngx_posted_events, ngx_str_t, ngx_uint_t, NGX_CONF_TAKE1, NGX_HTTP_LOC_CONF,
    NGX_HTTP_LOC_CONF_OFFSET, NGX_HTTP_MODULE, NGX_LOG_EMERG,
};
use ngx::http::{self, HTTPStatus, HttpModule, MergeConfigError, Request};
use ngx::http::{HttpModuleLocationConf, HttpModuleMainConf, NgxHttpCoreModule};
use ngx::{
    http_request_handler, ngx_conf_log_error, ngx_log_debug_http, ngx_log_error, ngx_string,
};
use std::cell::RefCell;
use std::ffi::{c_char, c_void};
use std::future::Future;
use std::pin::Pin;
use std::ptr::{addr_of, addr_of_mut, NonNull};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::task::Poll;
use std::time::Instant;
use tokio::net::TcpStream;

struct Module;

impl http::HttpModule for Module {
    fn module() -> &'static ngx_module_t {
        unsafe { &*::core::ptr::addr_of!(ngx_http_async_module) }
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

fn yield_now() -> impl Future<Output = ()> {
    let mut yielded = false;
    future::poll_fn(move |cx| {
        if std::mem::replace(&mut yielded, true) {
            Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    })
}

async fn waste_yield() -> (String, String) {
    let start = Instant::now();

    for _ in 0..1000 {
        yield_now().await;
    }
    (
        "X-Waste-Yield-Time".to_string(),
        start.elapsed().as_millis().to_string(),
    )
}

async fn waste_sleep() -> (String, String) {
    let start = Instant::now();
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    (
        "X-Waste-Sleep-Time".to_string(),
        start.elapsed().as_millis().to_string(),
    )
}

async fn waste_ngx_sleep() -> (String, String) {
    let start = Instant::now();
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    (
        "X-Waste-Ngx-Sleep-Time".to_string(),
        start.elapsed().as_millis().to_string(),
    )
}

async fn resolve_something(
    clcf: &ngx_http_core_loc_conf_t,
    pool: &mut Pool,
    name: &str,
) -> (String, String) {
    let start = Instant::now();
    let resolver = Resolver::from_resolver(NonNull::new(clcf.resolver).expect("resolver"), 30000);

    let _resolution = resolver
        .resolve_name(unsafe { &ngx_str_t::from_str(pool.as_mut(), name) }, pool)
        .await
        .expect("resolution");

    (
        "X-Resolve-Time".to_string(),
        start.elapsed().as_millis().to_string(),
    )
}

async fn reqwest_something() -> (String, String) {
    let start = Instant::now();
    let _ = reqwest::get("https://example.com")
        .await
        .expect("response")
        .text()
        .await
        .expect("body");
    (
        "X-Reqwest-Time".to_string(),
        start.elapsed().as_millis().to_string(),
    )
}

async fn hyper_something() -> (String, String) {
    let start = Instant::now();
    // see https://hyper.rs/guides/1/client/basic/
    let url = "https://example.com".parse::<hyper::Uri>().expect("uri");
    let host = url.host().expect("uri has no host");
    let port = url.port_u16().unwrap_or(80);

    let address = format!("{}:{}", host, port);

    let stream = TcpStream::connect(address).await.expect("connect");

    let io = TokioIo::new(stream);

    // Create the Hyper client
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .expect("handshake");
    // Spawn a task to poll the connection, driving the HTTP state
    let http_task = spawn(async move {
        if let Err(err) = conn.await {
            println!("Connection failed: {:?}", err);
        }
    });
    let authority = url.authority().unwrap().clone();
    let req = hyper::Request::builder()
        .uri(url)
        .header(hyper::header::HOST, authority.as_str())
        .body(Empty::<Bytes>::new())
        .expect("body");
    let _ = sender.send_request(req).await.expect("response");

    http_task.cancel().await;

    (
        "X-Hyper-Time".to_string(),
        start.elapsed().as_millis().to_string(),
    )
}

async fn async_access(request: &mut Request) -> Status {
    let start = Instant::now();
    let clcf = NgxHttpCoreModule::location_conf(request).expect("http core loc conf");
    let mut pool = request.pool();

    // some examples for io and timers
    let futs: Vec<Pin<Box<dyn futures::Future<Output = (String, String)>>>> = vec![
        // ngx resolver
        Box::pin(resolve_something(clcf, &mut pool, "example.com")),
        // tokio sleep
        Box::pin(waste_sleep()),
        // ngx sleep
        Box::pin(waste_ngx_sleep()),
        // yield_now
        Box::pin(waste_yield()),
        // reqwest
        Box::pin(reqwest_something()),
        // hyper
        Box::pin(hyper_something()),
    ];
    for (header, value) in futures::future::join_all(futs).await {
        request.add_header_out(&header, &value);
    }
    request.add_header_out("X-Async-Time", &start.elapsed().as_millis().to_string());
    Status::NGX_OK
}

#[derive(Default)]
struct RequestCTX(RefCell<Option<Task<Status>>>);

http_request_handler!(async_access_handler, |request: &mut http::Request| {
    let co = Module::location_conf(request).expect("module config is none");

    ngx_log_debug_http!(request, "async module enabled: {}", co.enable);

    if !co.enable {
        return core::Status::NGX_DECLINED;
    }

    // Check if we were called *again*
    if let Some(RequestCTX(task)) =
        unsafe { request.get_module_ctx::<RequestCTX>(&*addr_of!(ngx_http_async_module)) }
    {
        let task = task.take().expect("Task");
        // task should be finished when re-entering the handler
        if !task.is_finished() {
            ngx_log_error!(NGX_LOG_ERR, request.log(), "Task not finished");
            return HTTPStatus::INTERNAL_SERVER_ERROR.into();
        }
        return task.now_or_never().expect("Task result");
    }

    // Request is no longer needed and can be converted to something movable to the async block
    let req = AtomicPtr::new(request.into());

    // Compat to provide a tokio runtime (without using the tokio scheduler)
    let task = spawn(Compat::new(async move {
        let req = unsafe { http::Request::from_ngx_http_request(req.load(Ordering::Relaxed)) };
        let result = async_access(req).await;

        let c: *mut ngx_connection_t = req.connection().cast();
        // trigger „write” event so nginx calls our handler again to finalize the request
        unsafe { ngx_post_event((*c).write, addr_of_mut!(ngx_posted_events)) };

        result
    }));

    let ctx = request
        .pool()
        .allocate(RequestCTX(RefCell::new(Some(task))));

    if ctx.is_null() {
        return Status::NGX_ERROR;
    }
    request.set_module_ctx(ctx.cast(), unsafe { &*addr_of!(ngx_http_async_module) });

    core::Status::NGX_AGAIN
});

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
