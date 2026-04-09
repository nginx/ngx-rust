use core::fmt::Display;

use ngx::core::Status;
use ngx::http::subrequest::{SubRequestBuilder, SubRequestError};
use ngx::http::{
    HTTPStatus, HttpModule, HttpModuleLocationConf, HttpPhase, HttpRequestHandler,
    IntoHandlerStatus, Merge, MergeConfigError, Request, add_phase_handler,
};
use ngx::{ngx_log_debug_http, ngx_log_error};

use nginx_sys::{
    NGX_CONF_TAKE1, NGX_ERROR, NGX_HTTP_LOC_CONF, NGX_HTTP_LOC_CONF_OFFSET, NGX_LOG_ERR,
    ngx_command_t, ngx_conf_t, ngx_flag_t, ngx_http_complex_value_t, ngx_http_module_t,
    ngx_http_request_t, ngx_http_send_response, ngx_int_t, ngx_module_t, ngx_str_t, ngx_uint_t,
};

const NGX_CONF_UNSET_FLAG: ngx_flag_t = nginx_sys::NGX_CONF_UNSET as _;

struct SampleHandler;

enum SampleHandlerError {
    ContextAllocation,
    SubRequestCreation(SubRequestError),
    SubRequest(ngx_int_t),
    Response(ngx_int_t),
}

impl From<SubRequestError> for SampleHandlerError {
    fn from(e: SubRequestError) -> Self {
        SampleHandlerError::SubRequestCreation(e)
    }
}

impl Display for SampleHandlerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SampleHandlerError::ContextAllocation => {
                write!(f, "context allocation failed")
            }
            SampleHandlerError::SubRequestCreation(e) => {
                write!(f, "subrequest creation failed: {}", e)
            }
            SampleHandlerError::SubRequest(rc) => {
                write!(f, "subrequest failed with return code: {}", rc)
            }
            SampleHandlerError::Response(rc) => {
                write!(f, "response creation failed with return code: {}", rc)
            }
        }
    }
}

impl IntoHandlerStatus for SampleHandlerError {
    fn into_handler_status(self, r: &Request) -> ngx_int_t {
        ngx_log_error!(NGX_LOG_ERR, r.log(), "subrequest example: {self}");
        Status::NGX_ERROR.into()
    }
}

impl HttpRequestHandler for SampleHandler {
    const PHASE: HttpPhase = HttpPhase::Access;
    type Output = Result<Status, SampleHandlerError>;

    fn handler(request: &mut Request) -> Self::Output {
        let co = Module::location_conf(request).expect("module config is none");
        ngx_log_debug_http!(request, "subrequest module enabled: {}", co.enable);

        if co.enable != 1 {
            return Ok(Status::NGX_DECLINED);
        }

        let rptr: *mut ngx_http_request_t = request.as_mut();

        match SRCtx::get(request) {
            Some(ctx) => ctx.rc.map_or(
                // `ctx` has been created but not filled yet - subrequest is still in progress
                Ok(Status::NGX_AGAIN),
                // `ctx` has been created and filled - subrequest is completed
                |rc| {
                    let status = ctx.status.0;
                    let msg = format!("subrequest completed with HTTP status: {status}, rc: {rc}");
                    ngx_log_debug_http!(request, "{msg}");

                    if status >= nginx_sys::NGX_HTTP_SPECIAL_RESPONSE as _ {
                        Ok(Status::from(ctx.status))
                    } else if rc == nginx_sys::NGX_OK as _ && ctx.out.is_some() {
                        let outbuf = unsafe { &*ctx.out.unwrap().buf };
                        let mut ct = ctx.ct;
                        let mut cv: ngx_http_complex_value_t = unsafe { core::mem::zeroed() };
                        cv.value = ngx_str_t {
                            len: unsafe { outbuf.last.offset_from(outbuf.pos) } as _,
                            data: outbuf.pos as _,
                        };
                        let resp_rc = unsafe {
                            ngx_http_send_response(rptr, status, &raw mut ct, &raw mut cv)
                        };
                        if resp_rc == nginx_sys::NGX_OK as _ {
                            Ok(Status::from(ctx.status))
                        } else {
                            Err(SampleHandlerError::Response(resp_rc))
                        }
                    } else if rc == nginx_sys::NGX_OK as _ {
                        Ok(Status::from(ctx.status))
                    } else if let Ok(http_status) = HTTPStatus::try_from(rc) {
                        Ok(Status::from(http_status))
                    } else {
                        Err(SampleHandlerError::SubRequest(rc))
                    }
                },
            ),
            None => {
                if SRCtx::create(request).is_some() {
                    let uri: &str = co.uri.to_str().unwrap_or("/proxy");

                    SubRequestBuilder::new(request, uri)?
                        .args("arg1=val1&arg2=val2")?
                        .handler(sr_handler)
                        .in_memory()
                        .waited()
                        .build()?;

                    Ok(Status::NGX_AGAIN)
                } else {
                    Err(SampleHandlerError::ContextAllocation)
                }
            }
        }
    }
}

struct SRCtx<'r> {
    rc: Option<ngx_int_t>,
    status: HTTPStatus,
    out: Option<&'r nginx_sys::ngx_chain_t>,
    ct: ngx_str_t,
}

impl SRCtx<'_> {
    fn create(request: &mut Request) -> Option<&mut Self> {
        let ctx_ref = unsafe { request.pool().allocate_with_cleanup(Self::default).ok()?.as_mut() };
        request.set_module_ctx(ctx_ref as *mut _ as _, Module::module());
        Some(ctx_ref)
    }

    fn get(request: &Request) -> Option<&Self> {
        request.get_module_ctx::<Self>(Module::module())
    }

    fn get_mut(request: &mut Request) -> Option<&mut Self> {
        request.get_module_ctx_mut::<Self>(Module::module())
    }
}

impl Default for SRCtx<'_> {
    fn default() -> Self {
        Self { rc: None, status: HTTPStatus(NGX_ERROR as _), out: None, ct: ngx_str_t::empty() }
    }
}

fn sr_handler(r: &mut Request, mut rc: ngx_int_t) -> ngx_int_t {
    let newctx = SRCtx {
        rc: Some(rc),
        status: r.status(),
        // SAFETY: `r.as_ref().out` is valid as long as the main request is not finalized,
        // and the subrequest is always finalized before the main request.
        out: core::ptr::NonNull::new(r.as_ref().out).map(|out| unsafe { out.as_ref() }),
        ct: r.as_ref().headers_out.content_type,
    };
    if let Some(ctx) = SRCtx::get_mut(r.main_mut()) {
        *ctx = newctx;
    } else {
        ngx_log_error!(nginx_sys::NGX_LOG_ERR, r.log(), "subrequest: context not found");
        rc = NGX_ERROR as _;
    }
    rc
}

static NGX_HTTP_SUBREQUEST_MODULE_CTX: ngx_http_module_t = ngx_http_module_t {
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
ngx::ngx_modules!(ngx_http_subrequest_module);

#[used]
#[allow(non_upper_case_globals)]
#[cfg_attr(not(feature = "export-modules"), unsafe(no_mangle))]
pub static mut ngx_http_subrequest_module: ngx_module_t = ngx_module_t {
    ctx: &raw const NGX_HTTP_SUBREQUEST_MODULE_CTX as _,
    commands: unsafe { &raw mut NGX_HTTP_SUBREQUEST_COMMANDS[0] },
    type_: nginx_sys::NGX_HTTP_MODULE as _,
    ..ngx_module_t::default()
};

struct Module;

impl HttpModule for Module {
    fn module() -> &'static ngx_module_t {
        unsafe { &*::core::ptr::addr_of!(ngx_http_subrequest_module) }
    }

    unsafe extern "C" fn postconfiguration(cf: *mut ngx_conf_t) -> ngx_int_t {
        // SAFETY: this function is called with non-NULL cf always
        let cf = unsafe { &mut *cf };
        add_phase_handler::<SampleHandler>(cf)
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
        Self { enable: NGX_CONF_UNSET_FLAG, uri: ngx_str_t::empty() }
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
        if self.uri.data.is_null() {
            self.uri = prev.uri;
        }
        if self.enable == 1 && self.uri.data.is_null() {
            self.uri = ngx::ngx_string!("/proxy");
        }
        Ok(())
    }
}

unsafe impl HttpModuleLocationConf for Module {
    type LocationConf = ModuleConfig;
}

static mut NGX_HTTP_SUBREQUEST_COMMANDS: [ngx_command_t; 3] = [
    ngx_command_t {
        name: ngx::ngx_string!("subrequest"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(nginx_sys::ngx_conf_set_flag_slot),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: core::mem::offset_of!(ModuleConfig, enable),
        post: core::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx::ngx_string!("subrequest_uri"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(nginx_sys::ngx_conf_set_str_slot),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: core::mem::offset_of!(ModuleConfig, uri),
        post: core::ptr::null_mut(),
    },
    ngx_command_t::empty(),
];
