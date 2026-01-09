use std::ffi::{c_char, c_void};

use ngx::http::{
    add_phase_handler, AsyncHandler, AsyncSubRequestBuilder, AsyncSubRequestError, HttpModule,
    HttpModuleLocationConf, HttpPhase, Merge, MergeConfigError, Request,
};
use ngx::{async_ as ngx_async, ngx_conf_log_error, ngx_log_debug_http, ngx_log_error};

use nginx_sys::{
    ngx_command_t, ngx_conf_t, ngx_http_complex_value_t, ngx_http_module_t, ngx_http_request_t,
    ngx_http_send_response, ngx_int_t, ngx_module_t, ngx_str_t, ngx_uint_t, NGX_CONF_TAKE1,
    NGX_HTTP_LOC_CONF, NGX_HTTP_LOC_CONF_OFFSET,
};

struct SampleAsyncHandler;

enum SampleAsyncHandlerError {
    SubrequestCreationFailed(AsyncSubRequestError),
    SubrequestFailed(ngx_int_t),
}

impl std::fmt::Display for SampleAsyncHandlerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SampleAsyncHandlerError::SubrequestCreationFailed(e) => {
                write!(f, "Subrequest creation failed: {}", e)
            }
            SampleAsyncHandlerError::SubrequestFailed(rc) => {
                write!(f, "Subrequest failed with return code: {}", rc)
            }
        }
    }
}

impl From<AsyncSubRequestError> for SampleAsyncHandlerError {
    fn from(err: AsyncSubRequestError) -> Self {
        SampleAsyncHandlerError::SubrequestCreationFailed(err)
    }
}

impl From<ngx_int_t> for SampleAsyncHandlerError {
    fn from(rc: ngx_int_t) -> Self {
        SampleAsyncHandlerError::SubrequestFailed(rc)
    }
}

impl AsyncHandler for SampleAsyncHandler {
    const PHASE: HttpPhase = HttpPhase::Access;
    type Module = Module;
    type ReturnType = Result<ngx_int_t, SampleAsyncHandlerError>;

    async fn worker(request: &mut Request) -> Self::ReturnType {
        ngx_log_debug_http!(request, "worker started");

        let co = Module::location_conf(request).expect("module config is none");
        ngx_log_debug_http!(request, "async_request module enabled: {}", co.enable);

        if !co.enable {
            return Ok(nginx_sys::NGX_DECLINED as _);
        }

        let log = request.log();
        let request_ptr: *mut ngx_http_request_t = request.as_mut();

        let fut = AsyncSubRequestBuilder::new("/proxy")
            //.args("arg1=val1&arg2=val2")
            .in_memory()
            .waited()
            .build(request)?;

        let subrc = fut.await;

        ngx_log_error!(nginx_sys::NGX_LOG_INFO, log, "Subrequest rc {}", subrc.0);

        if subrc.0 != nginx_sys::NGX_OK as _ || subrc.1.is_none() {
            return Err(SampleAsyncHandlerError::from(subrc.0));
        }

        let sr = subrc.1.unwrap();

        ngx_log_error!(
            nginx_sys::NGX_LOG_INFO,
            log,
            "Subrequest status: {:?}",
            sr.get_status()
        );

        ngx_async::sleep(core::time::Duration::from_secs(2)).await;

        let mut resp_len: usize = 0;

        let mut rc = nginx_sys::NGX_OK as ngx_int_t;

        if let Some(out) = sr.get_out() {
            if !out.buf.is_null() {
                let b = unsafe { &*out.buf };
                resp_len = unsafe { b.last.offset_from(b.pos) } as usize;

                let sr_ptr: *const ngx_http_request_t = sr.as_ref();

                let mut ct: ngx_str_t = (unsafe { *sr_ptr }).headers_out.content_type;

                let mut cv: ngx_http_complex_value_t = unsafe { core::mem::zeroed() };
                cv.value = ngx_str_t {
                    len: resp_len as _,
                    data: b.pos as _,
                };

                rc = unsafe {
                    ngx_http_send_response(request_ptr, sr.get_status().0, &mut ct, &mut cv)
                };

                if rc == nginx_sys::NGX_OK as _ {
                    rc = nginx_sys::NGX_HTTP_OK as _;
                }
            }
        }

        ngx_log_error!(
            nginx_sys::NGX_LOG_INFO,
            log,
            "Async handler after timeout; subrequest response length: {}",
            resp_len
        );

        Ok(rc)
    }
}

static NGX_HTTP_ASYNC_REQUEST_MODULE_CTX: ngx_http_module_t = ngx_http_module_t {
    preconfiguration: None,
    postconfiguration: Some(Module::postconfiguration),
    create_main_conf: None,
    init_main_conf: None,
    create_srv_conf: None,
    merge_srv_conf: None,
    create_loc_conf: Some(Module::create_loc_conf),
    merge_loc_conf: Some(Module::merge_loc_conf),
};

#[cfg(feature = "export-modules")]
ngx::ngx_modules!(ngx_http_async_request_module);

#[used]
#[allow(non_upper_case_globals)]
#[cfg_attr(not(feature = "export-modules"), no_mangle)]
pub static mut ngx_http_async_request_module: ngx_module_t = ngx_module_t {
    ctx: core::ptr::addr_of!(NGX_HTTP_ASYNC_REQUEST_MODULE_CTX) as _,
    commands: unsafe { &NGX_HTTP_ASYNC_REQUEST_COMMANDS[0] as *const _ as *mut _ },
    type_: nginx_sys::NGX_HTTP_MODULE as _,
    ..ngx_module_t::default()
};

struct Module;

impl HttpModule for Module {
    fn module() -> &'static ngx_module_t {
        unsafe { &*::core::ptr::addr_of!(ngx_http_async_request_module) }
    }

    unsafe extern "C" fn postconfiguration(cf: *mut ngx_conf_t) -> ngx_int_t {
        // SAFETY: this function is called with non-NULL cf always
        let cf = unsafe { &mut *cf };
        add_phase_handler::<SampleAsyncHandler>(cf)
            .map_or(nginx_sys::NGX_ERROR as _, |_| nginx_sys::NGX_OK as _)
    }
}

#[derive(Debug, Default)]
struct ModuleConfig {
    enable: bool,
}

unsafe impl HttpModuleLocationConf for Module {
    type LocationConf = ModuleConfig;
}

impl Merge for ModuleConfig {
    fn merge(&mut self, prev: &ModuleConfig) -> Result<(), MergeConfigError> {
        if prev.enable {
            self.enable = true;
        };
        Ok(())
    }
}

static mut NGX_HTTP_ASYNC_REQUEST_COMMANDS: [ngx_command_t; 2] = [
    ngx_command_t {
        name: ngx::ngx_string!("async_request"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(ngx_http_async_request_commands_set_enable),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: std::ptr::null_mut(),
    },
    ngx_command_t::empty(),
];

extern "C" fn ngx_http_async_request_commands_set_enable(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    unsafe {
        let conf = &mut *(conf as *mut ModuleConfig);
        let args: &[ngx_str_t] = (*(*cf).args).as_slice();

        conf.enable = match args[1].to_str() {
            Err(_) => false,
            Ok(s) if s.len() == 2 && s.eq_ignore_ascii_case("on") => true,
            Ok(s) if s.len() == 3 && s.eq_ignore_ascii_case("off") => false,
            _ => {
                ngx_conf_log_error!(
                    nginx_sys::NGX_LOG_EMERG,
                    cf,
                    "`async_request` argument must be 'on' or 'off'"
                );
                return ngx::core::NGX_CONF_ERROR;
            }
        };
    }

    ngx::core::NGX_CONF_OK
}
