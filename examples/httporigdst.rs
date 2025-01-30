use std::ffi::{c_int, c_void};
use std::ptr::addr_of;

use ngx::core;
use ngx::core::prelude::*;
use ngx::ffi::{
    in_port_t, ngx_conf_t, ngx_connection_local_sockaddr, ngx_inet_get_port, ngx_sock_ntop, sockaddr, sockaddr_storage,
    INET_ADDRSTRLEN,
};
use ngx::http::prelude::*;
use ngx::http::{self, HTTPModule};
use ngx::{http_variable_get, ngx_http_null_variable, ngx_log_debug_http, ngx_null_string, ngx_string};

const IPV4_STRLEN: usize = INET_ADDRSTRLEN as usize;

#[derive(Debug)]
struct NgxHttpOrigDstCtx {
    orig_dst_addr: ngx_str_t,
    orig_dst_port: ngx_str_t,
}

impl Default for NgxHttpOrigDstCtx {
    fn default() -> NgxHttpOrigDstCtx {
        NgxHttpOrigDstCtx {
            orig_dst_addr: ngx_null_string!(),
            orig_dst_port: ngx_null_string!(),
        }
    }
}

impl NgxHttpOrigDstCtx {
    pub fn save(&mut self, addr: &str, port: in_port_t, pool: &mut core::Pool) -> core::Status {
        let addr_data = pool.alloc_unaligned(addr.len());
        if addr_data.is_null() {
            return core::Status::NGX_ERROR;
        }
        unsafe { libc::memcpy(addr_data, addr.as_ptr() as *const c_void, addr.len()) };
        self.orig_dst_addr.len = addr.len();
        self.orig_dst_addr.data = addr_data as *mut u8;

        let port_str = port.to_string();
        let port_data = pool.alloc_unaligned(port_str.len());
        if port_data.is_null() {
            return core::Status::NGX_ERROR;
        }
        unsafe { libc::memcpy(port_data, port_str.as_bytes().as_ptr() as *const c_void, port_str.len()) };
        self.orig_dst_port.len = port_str.len();
        self.orig_dst_port.data = port_data as *mut u8;

        core::Status::NGX_OK
    }

    pub unsafe fn bind_addr(&self, v: *mut ngx_variable_value_t) {
        if self.orig_dst_addr.len == 0 {
            (*v).set_not_found(1);
            return;
        }

        (*v).set_valid(1);
        (*v).set_no_cacheable(0);
        (*v).set_not_found(0);
        (*v).set_len(self.orig_dst_addr.len as u32);
        (*v).data = self.orig_dst_addr.data;
    }

    pub unsafe fn bind_port(&self, v: *mut ngx_variable_value_t) {
        if self.orig_dst_port.len == 0 {
            (*v).set_not_found(1);
            return;
        }

        (*v).set_valid(1);
        (*v).set_no_cacheable(0);
        (*v).set_not_found(0);
        (*v).set_len(self.orig_dst_port.len as u32);
        (*v).data = self.orig_dst_port.data;
    }
}

static NGX_HTTP_ORIG_DST_MODULE_CTX: ngx_http_module_t = ngx_http_module_t {
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
ngx::ngx_modules!(ngx_http_orig_dst_module);

#[used]
#[allow(non_upper_case_globals)]
#[cfg_attr(not(feature = "export-modules"), no_mangle)]
pub static mut ngx_http_orig_dst_module: ngx_module_t = ngx_module_t {
    ctx: std::ptr::addr_of!(NGX_HTTP_ORIG_DST_MODULE_CTX) as _,
    commands: std::ptr::null_mut(),
    type_: NGX_HTTP_MODULE as _,
    ..ngx_module_t::default()
};

static mut NGX_HTTP_ORIG_DST_VARS: [ngx_http_variable_t; 3] = [
    // ngx_str_t name
    // ngx_http_set_variable_pt set_handler
    // ngx_http_get_variable_pt get_handler
    // uintptr_t data
    // ngx_uint_t flags
    // ngx_uint_t index
    ngx_http_variable_t {
        name: ngx_string!("server_orig_addr"),
        set_handler: None,
        get_handler: Some(ngx_http_orig_dst_addr_variable),
        data: 0,
        flags: 0,
        index: 0,
    },
    ngx_http_variable_t {
        name: ngx_string!("server_orig_port"),
        set_handler: None,
        get_handler: Some(ngx_http_orig_dst_port_variable),
        data: 0,
        flags: 0,
        index: 0,
    },
    ngx_http_null_variable!(),
];

unsafe fn ngx_get_origdst(request: &mut http::Request) -> Result<(String, in_port_t), core::Status> {
    let c = request.connection();

    if (*c).type_ != libc::SOCK_STREAM {
        ngx_log_debug_http!(request, "httporigdst: connection is not type SOCK_STREAM");
        return Err(core::Status::NGX_DECLINED);
    }

    if ngx_connection_local_sockaddr(c, std::ptr::null_mut(), 0) != core::Status::NGX_OK.into() {
        ngx_log_debug_http!(request, "httporigdst: no local sockaddr from connection");
        return Err(core::Status::NGX_ERROR);
    }

    let level: c_int;
    let optname: c_int;
    match (*(*c).local_sockaddr).sa_family as i32 {
        libc::AF_INET => {
            level = libc::SOL_IP;
            optname = libc::SO_ORIGINAL_DST;
        }
        _ => {
            ngx_log_debug_http!(request, "httporigdst: only support IPv4");
            return Err(core::Status::NGX_DECLINED);
        }
    }

    let mut addr: sockaddr_storage = { std::mem::zeroed() };
    let mut addrlen: libc::socklen_t = std::mem::size_of_val(&addr) as libc::socklen_t;
    let rc = libc::getsockopt(
        (*c).fd,
        level,
        optname,
        &mut addr as *mut _ as *mut _,
        &mut addrlen as *mut u32,
    );
    if rc == -1 {
        ngx_log_debug_http!(request, "httporigdst: getsockopt failed");
        return Err(core::Status::NGX_DECLINED);
    }
    let mut ip: Vec<u8> = vec![0; IPV4_STRLEN];
    let e = unsafe {
        ngx_sock_ntop(
            std::ptr::addr_of_mut!(addr) as *mut sockaddr,
            std::mem::size_of::<sockaddr>() as u32,
            ip.as_mut_ptr(),
            IPV4_STRLEN,
            0,
        )
    };
    if e == 0 {
        ngx_log_debug_http!(request, "httporigdst: ngx_sock_ntop failed to convert sockaddr");
        return Err(core::Status::NGX_ERROR);
    }
    ip.truncate(e);

    let port = unsafe { ngx_inet_get_port(std::ptr::addr_of_mut!(addr) as *mut sockaddr) };

    Ok((String::from_utf8(ip).unwrap(), port))
}

http_variable_get!(
    ngx_http_orig_dst_addr_variable,
    |request: &mut http::Request, v: *mut ngx_variable_value_t, _: usize| {
        let ctx = request.get_module_ctx::<NgxHttpOrigDstCtx>(&*addr_of!(ngx_http_orig_dst_module));
        if let Some(obj) = ctx {
            ngx_log_debug_http!(request, "httporigdst: found context and binding variable",);
            obj.bind_addr(v);
            return core::Status::NGX_OK;
        }
        // lazy initialization:
        //   get original dest information
        //   create context
        //   set context
        // bind address
        ngx_log_debug_http!(request, "httporigdst: context not found, getting address");
        let r = ngx_get_origdst(request);
        match r {
            Err(e) => {
                return e;
            }
            Ok((ip, port)) => {
                // create context,
                // set context
                let new_ctx = request.pool().allocate::<NgxHttpOrigDstCtx>(Default::default());

                if new_ctx.is_null() {
                    return core::Status::NGX_ERROR;
                }

                ngx_log_debug_http!(request, "httporigdst: saving ip - {:?}, port - {}", ip, port,);
                (*new_ctx).save(&ip, port, &mut request.pool());
                (*new_ctx).bind_addr(v);
                request.set_module_ctx(new_ctx as *mut c_void, &*addr_of!(ngx_http_orig_dst_module));
            }
        }
        core::Status::NGX_OK
    }
);

http_variable_get!(
    ngx_http_orig_dst_port_variable,
    |request: &mut http::Request, v: *mut ngx_variable_value_t, _: usize| {
        let ctx = request.get_module_ctx::<NgxHttpOrigDstCtx>(&*addr_of!(ngx_http_orig_dst_module));
        if let Some(obj) = ctx {
            ngx_log_debug_http!(request, "httporigdst: found context and binding variable",);
            obj.bind_port(v);
            return core::Status::NGX_OK;
        }
        // lazy initialization:
        //   get original dest information
        //   create context
        //   set context
        // bind port
        ngx_log_debug_http!(request, "httporigdst: context not found, getting address");
        let r = ngx_get_origdst(request);
        match r {
            Err(e) => {
                return e;
            }
            Ok((ip, port)) => {
                // create context,
                // set context
                let new_ctx = request.pool().allocate::<NgxHttpOrigDstCtx>(Default::default());

                if new_ctx.is_null() {
                    return core::Status::NGX_ERROR;
                }

                ngx_log_debug_http!(request, "httporigdst: saving ip - {:?}, port - {}", ip, port,);
                (*new_ctx).save(&ip, port, &mut request.pool());
                (*new_ctx).bind_port(v);
                request.set_module_ctx(new_ctx as *mut c_void, &*addr_of!(ngx_http_orig_dst_module));
            }
        }
        core::Status::NGX_OK
    }
);

struct Module;

impl HTTPModule for Module {
    type MainConf = ();
    type SrvConf = ();
    type LocConf = ();

    // static ngx_int_t ngx_http_orig_dst_add_variables(ngx_conf_t *cf)
    unsafe extern "C" fn preconfiguration(cf: *mut ngx_conf_t) -> ngx_int_t {
        for mut v in NGX_HTTP_ORIG_DST_VARS {
            if v.name.len == 0 {
                break;
            }
            let var = ngx_http_add_variable(cf, &mut v.name, v.flags);
            if var.is_null() {
                return core::Status::NGX_ERROR.into();
            }
            (*var).get_handler = v.get_handler;
            (*var).data = v.data;
        }
        core::Status::NGX_OK.into()
    }
}
