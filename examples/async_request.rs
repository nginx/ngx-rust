use ngx::http::{
    AsyncHandler, HTTPStatus, HttpModule, HttpModuleLocationConf, HttpPhase, Merge,
    MergeConfigError, Request, add_phase_handler,
};

use ngx::http::subrequest::{SR_MODIFIER_NO_OP, SubRequestBuilder, SubRequestError};
use ngx::{async_ as ngx_async, ngx_log_debug_http, ngx_log_error};

use nginx_sys::{
    NGX_CONF_TAKE1, NGX_HTTP_LOC_CONF, NGX_HTTP_LOC_CONF_OFFSET, ngx_command_t, ngx_conf_t,
    ngx_flag_t, ngx_http_complex_value_t, ngx_http_module_t, ngx_http_request_t,
    ngx_http_send_response, ngx_int_t, ngx_module_t, ngx_str_t, ngx_uint_t,
};

const NGX_CONF_UNSET_FLAG: ngx_flag_t = nginx_sys::NGX_CONF_UNSET as _;

struct SampleAsyncHandler;

enum SampleAsyncHandlerError {
    SubRequestCreationFailed(SubRequestError),
    SubRequestFailed(ngx_int_t),
    NoSubRequestReturned,
}

impl core::fmt::Display for SampleAsyncHandlerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SampleAsyncHandlerError::SubRequestCreationFailed(e) => {
                write!(f, "subrequest creation failed: {}", e)
            }
            SampleAsyncHandlerError::SubRequestFailed(rc) => {
                write!(f, "subrequest failed with return code: {}", rc)
            }
            SampleAsyncHandlerError::NoSubRequestReturned => {
                write!(f, "subrequest did not return a request reference")
            }
        }
    }
}

impl From<SubRequestError> for SampleAsyncHandlerError {
    fn from(err: SubRequestError) -> Self {
        SampleAsyncHandlerError::SubRequestCreationFailed(err)
    }
}

impl From<ngx_int_t> for SampleAsyncHandlerError {
    fn from(rc: ngx_int_t) -> Self {
        SampleAsyncHandlerError::SubRequestFailed(rc)
    }
}

impl AsyncHandler for SampleAsyncHandler {
    const PHASE: HttpPhase = HttpPhase::Access;
    type Module = Module;
    type Output = Result<ngx_int_t, SampleAsyncHandlerError>;

    async fn worker(request: &mut Request) -> Self::Output {
        ngx_log_debug_http!(request, "worker started");

        let co = Module::location_conf(request).expect("module config is none");
        ngx_log_debug_http!(request, "async_request module enabled: {}", co.enable);

        if co.enable != 1 {
            return Ok(nginx_sys::NGX_DECLINED as _);
        }

        let log = request.log();
        let request_ptr: *mut ngx_http_request_t = request.as_mut();
        let uri: &str = if co.uri.is_empty() {
            "/proxy"
        } else {
            co.uri.to_str().unwrap_or("/proxy")
        };

        let mut sr: Option<&Request> = None;

        let subrc = SubRequestBuilder::new(request.pool(), uri)?
            .args("arg1=val1&arg2=val2")?
            .in_memory()
            .waited()
            .build_async(request, SR_MODIFIER_NO_OP, |r, rc| {
                sr = Some(r);
                rc
            })
            .await?;

        ngx_log_error!(nginx_sys::NGX_LOG_INFO, log, "subrequest rc:{}", subrc);

        if subrc != nginx_sys::NGX_OK as _ {
            return HTTPStatus::try_from(subrc)
                .map(Into::into)
                .map_err(|_| SampleAsyncHandlerError::from(subrc));
        }

        if sr.is_none() {
            return Err(SampleAsyncHandlerError::NoSubRequestReturned);
        }

        let sr = sr.unwrap();

        ngx_log_error!(
            nginx_sys::NGX_LOG_INFO,
            log,
            "Subrequest status: {:?}",
            sr.get_status()
        );

        ngx_async::sleep(core::time::Duration::from_millis(100)).await;

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
                    ngx_http_send_response(request_ptr, sr.get_status().0, &raw mut ct, &raw mut cv)
                };

                if rc == nginx_sys::NGX_OK as _ {
                    rc = nginx_sys::NGX_HTTP_OK as _;
                }
            }
        }

        ngx_log_error!(
            nginx_sys::NGX_LOG_INFO,
            log,
            "async handler after timeout; subrequest response length: {}",
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
#[cfg_attr(not(feature = "export-modules"), unsafe(no_mangle))]
pub static mut ngx_http_async_request_module: ngx_module_t = ngx_module_t {
    ctx: &raw const NGX_HTTP_ASYNC_REQUEST_MODULE_CTX as _,
    commands: unsafe { &raw mut NGX_HTTP_ASYNC_REQUEST_COMMANDS[0] },
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

#[derive(Debug)]
struct ModuleConfig {
    enable: ngx_flag_t,
    uri: ngx_str_t,
}

impl Default for ModuleConfig {
    fn default() -> Self {
        Self {
            enable: NGX_CONF_UNSET_FLAG,
            uri: ngx_str_t::empty(),
        }
    }
}

impl Merge for ModuleConfig {
    fn merge(&mut self, prev: &ModuleConfig) -> Result<(), MergeConfigError> {
        if self.enable == NGX_CONF_UNSET_FLAG {
            if prev.enable != NGX_CONF_UNSET_FLAG {
                self.enable = prev.enable;
            } else {
                self.enable = 0;
            }
        }
        if self.uri.len == 0 {
            self.uri = prev.uri;
        }
        Ok(())
    }
}

unsafe impl HttpModuleLocationConf for Module {
    type LocationConf = ModuleConfig;
}

static mut NGX_HTTP_ASYNC_REQUEST_COMMANDS: [ngx_command_t; 3] = [
    ngx_command_t {
        name: ngx::ngx_string!("async_request"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(nginx_sys::ngx_conf_set_flag_slot),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: core::mem::offset_of!(ModuleConfig, enable),
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx::ngx_string!("async_uri"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(nginx_sys::ngx_conf_set_str_slot),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: core::mem::offset_of!(ModuleConfig, uri),
        post: core::ptr::null_mut(),
    },
    ngx_command_t::empty(),
];
