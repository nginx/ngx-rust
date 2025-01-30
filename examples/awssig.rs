use std::ffi::{c_char, c_void};
use std::ptr::addr_of;

use http::HeaderMap;
use ngx::core;
use ngx::core::prelude::*;
use ngx::ffi::{ngx_array_push, ngx_http_phases_NGX_HTTP_PRECONTENT_PHASE};
use ngx::http::prelude::*;
use ngx::http::*;
use ngx::{http_request_handler, ngx_log_debug_http, ngx_null_command, ngx_string};

struct Module;

impl HTTPModule for Module {
    type MainConf = ();
    type SrvConf = ();
    type LocConf = ModuleConfig;

    unsafe extern "C" fn postconfiguration(cf: *mut ngx_conf_t) -> ngx_int_t {
        let cmcf = ngx_http_conf_get_module_main_conf(cf, &*addr_of!(ngx_http_core_module));

        let h = ngx_array_push(&mut (*cmcf).phases[ngx_http_phases_NGX_HTTP_PRECONTENT_PHASE as usize].handlers)
            as *mut ngx_http_handler_pt;
        if h.is_null() {
            return core::Status::NGX_ERROR.into();
        }
        // set an phase handler
        *h = Some(awssigv4_header_handler);
        core::Status::NGX_OK.into()
    }
}

#[derive(Debug, Default)]
struct ModuleConfig {
    enable: bool,
    access_key: String,
    secret_key: String,
    s3_bucket: String,
    s3_endpoint: String,
}

static mut NGX_HTTP_AWSSIGV4_COMMANDS: [ngx_command_t; 6] = [
    ngx_command_t {
        name: ngx_string!("awssigv4"),
        type_: (NGX_HTTP_LOC_CONF | NGX_HTTP_SRV_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(ngx_http_awssigv4_commands_set_enable),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: std::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("awssigv4_access_key"),
        type_: (NGX_HTTP_LOC_CONF | NGX_HTTP_SRV_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(ngx_http_awssigv4_commands_set_access_key),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: std::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("awssigv4_secret_key"),
        type_: (NGX_HTTP_LOC_CONF | NGX_HTTP_SRV_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(ngx_http_awssigv4_commands_set_secret_key),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: std::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("awssigv4_s3_bucket"),
        type_: (NGX_HTTP_LOC_CONF | NGX_HTTP_SRV_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(ngx_http_awssigv4_commands_set_s3_bucket),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: std::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("awssigv4_s3_endpoint"),
        type_: (NGX_HTTP_LOC_CONF | NGX_HTTP_SRV_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(ngx_http_awssigv4_commands_set_s3_endpoint),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: std::ptr::null_mut(),
    },
    ngx_null_command!(),
];

static NGX_HTTP_AWSSIGV4_MODULE_CTX: ngx_http_module_t = ngx_http_module_t {
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
ngx::ngx_modules!(ngx_http_awssigv4_module);

#[used]
#[allow(non_upper_case_globals)]
#[cfg_attr(not(feature = "export-modules"), no_mangle)]
pub static mut ngx_http_awssigv4_module: ngx_module_t = ngx_module_t {
    ctx: std::ptr::addr_of!(NGX_HTTP_AWSSIGV4_MODULE_CTX) as _,
    commands: unsafe { &NGX_HTTP_AWSSIGV4_COMMANDS[0] as *const _ as *mut _ },
    type_: NGX_HTTP_MODULE as _,
    ..ngx_module_t::default()
};

impl Merge for ModuleConfig {
    fn merge(&mut self, prev: &ModuleConfig) -> Result<(), MergeConfigError> {
        if prev.enable {
            self.enable = true;
        };

        if self.access_key.is_empty() {
            self.access_key = String::from(if !prev.access_key.is_empty() {
                &prev.access_key
            } else {
                ""
            });
        }
        if self.enable && self.access_key.is_empty() {
            return Err(MergeConfigError::NoValue);
        }

        if self.secret_key.is_empty() {
            self.secret_key = String::from(if !prev.secret_key.is_empty() {
                &prev.secret_key
            } else {
                ""
            });
        }
        if self.enable && self.secret_key.is_empty() {
            return Err(MergeConfigError::NoValue);
        }

        if self.s3_bucket.is_empty() {
            self.s3_bucket = String::from(if !prev.s3_bucket.is_empty() {
                &prev.s3_bucket
            } else {
                ""
            });
        }
        if self.enable && self.s3_bucket.is_empty() {
            return Err(MergeConfigError::NoValue);
        }

        if self.s3_endpoint.is_empty() {
            self.s3_endpoint = String::from(if !prev.s3_endpoint.is_empty() {
                &prev.s3_endpoint
            } else {
                "s3.amazonaws.com"
            });
        }
        Ok(())
    }
}

extern "C" fn ngx_http_awssigv4_commands_set_enable(
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

extern "C" fn ngx_http_awssigv4_commands_set_access_key(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    unsafe {
        let conf = &mut *(conf as *mut ModuleConfig);
        let args = (*(*cf).args).elts as *mut ngx_str_t;
        conf.access_key = (*args.add(1)).to_string();
    };

    std::ptr::null_mut()
}

extern "C" fn ngx_http_awssigv4_commands_set_secret_key(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    unsafe {
        let conf = &mut *(conf as *mut ModuleConfig);
        let args = (*(*cf).args).elts as *mut ngx_str_t;
        conf.secret_key = (*args.add(1)).to_string();
    };

    std::ptr::null_mut()
}

extern "C" fn ngx_http_awssigv4_commands_set_s3_bucket(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    unsafe {
        let conf = &mut *(conf as *mut ModuleConfig);
        let args = (*(*cf).args).elts as *mut ngx_str_t;
        conf.s3_bucket = (*args.add(1)).to_string();
        if conf.s3_bucket.len() == 1 {
            println!("Validation failed");
            return ngx::core::NGX_CONF_ERROR as _;
        }
    };
    std::ptr::null_mut()
}

extern "C" fn ngx_http_awssigv4_commands_set_s3_endpoint(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    unsafe {
        let conf = &mut *(conf as *mut ModuleConfig);
        let args = (*(*cf).args).elts as *mut ngx_str_t;
        conf.s3_endpoint = (*args.add(1)).to_string();
    };

    std::ptr::null_mut()
}

http_request_handler!(awssigv4_header_handler, |request: &mut Request| {
    // get Module Config from request
    let conf = unsafe { request.get_module_loc_conf::<ModuleConfig>(&*addr_of!(ngx_http_awssigv4_module)) };
    let conf = conf.unwrap();
    ngx_log_debug_http!(request, "AWS signature V4 module {}", {
        if conf.enable {
            "enabled"
        } else {
            "disabled"
        }
    });
    if !conf.enable {
        return core::Status::NGX_DECLINED;
    }

    // TODO: build url properly from the original URL from client
    let method = request.method();
    if !matches!(method, ngx::http::Method::HEAD | ngx::http::Method::GET) {
        return HTTPStatus::FORBIDDEN.into();
    }

    let datetime = chrono::Utc::now();
    let uri = match request.unparsed_uri().to_str() {
        Ok(v) => format!("https://{}.{}{}", conf.s3_bucket, conf.s3_endpoint, v),
        Err(_) => return core::Status::NGX_DECLINED,
    };

    let datetime_now = datetime.format("%Y%m%dT%H%M%SZ");
    let datetime_now = datetime_now.to_string();

    let signature = {
        // NOTE: aws_sign_v4::AwsSign::new() implementation requires a HeaderMap.
        // Iterate over requests headers_in and copy into HeaderMap
        // Copy only headers that will be used to sign the request
        let mut headers = HeaderMap::new();
        for (name, value) in request.headers_in_iterator() {
            if name.to_lowercase() == "host" {
                headers.insert(http::header::HOST, value.parse().unwrap());
            }
        }
        headers.insert("X-Amz-Date", datetime_now.parse().unwrap());
        ngx_log_debug_http!(request, "headers {:?}", headers);
        ngx_log_debug_http!(request, "method {:?}", method);
        ngx_log_debug_http!(request, "uri {:?}", uri);
        ngx_log_debug_http!(request, "datetime_now {:?}", datetime_now);

        let s = aws_sign_v4::AwsSign::new(
            method.as_str(),
            &uri,
            &datetime,
            &headers,
            "us-east-1",
            conf.access_key.as_str(),
            conf.secret_key.as_str(),
            "s3",
            "",
        );
        s.sign()
    };

    request.add_header_in("authorization", signature.as_str());
    request.add_header_in("X-Amz-Date", datetime_now.as_str());

    // done signing, let's print values we have in request.headers_out, request.headers_in
    for (name, value) in request.headers_out_iterator() {
        ngx_log_debug_http!(request, "headers_out {}: {}", name, value);
    }
    for (name, value) in request.headers_in_iterator() {
        ngx_log_debug_http!(request, "headers_in  {}: {}", name, value);
    }

    core::Status::NGX_OK
});
