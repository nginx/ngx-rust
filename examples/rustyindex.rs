use core::ffi::CStr;
use core::fmt::{self, Write};
use core::ptr;

use nginx_sys::{
    NGX_CONF_FLAG, NGX_HTTP_LOC_CONF, NGX_HTTP_LOC_CONF_OFFSET, NGX_HTTP_MODULE, NGX_HTTP_SRV_CONF,
    NGX_LOG_CRIT, NGX_LOG_ERR, ngx_chain_t, ngx_command_t, ngx_conf_t, ngx_err_t, ngx_flag_t,
    ngx_http_module_t, ngx_int_t, ngx_module_t, ngx_str_t, ngx_uint_t, off_t,
};
use ngx::core::file::{FileInfo, ReadDir};
use ngx::core::{Buffer, Status};
use ngx::http::{
    self, HTTPStatus, HttpModule, HttpModuleLocationConf, HttpRequestHandler, Merge,
    MergeConfigError, Method, Request,
};
use ngx::{ngx_log_debug_http, ngx_log_error, ngx_string};

struct Module;

impl HttpModule for Module {
    fn module() -> &'static ngx_module_t {
        unsafe { &*::core::ptr::addr_of!(ngx_http_rustyindex_module) }
    }

    unsafe extern "C" fn postconfiguration(cf: *mut ngx_conf_t) -> ngx_int_t {
        // SAFETY: this function is called with non-NULL cf always
        let cf = unsafe { &mut *cf };
        http::add_phase_handler::<RustyIndexHandler>(cf)
            .map_or(Status::NGX_ERROR, |_| Status::NGX_OK)
            .into()
    }
}

#[derive(Debug)]
struct ModuleConfig {
    enable: ngx_flag_t,
}

impl Default for ModuleConfig {
    fn default() -> Self {
        Self { enable: nginx_sys::NGX_CONF_UNSET as _ }
    }
}

impl Merge for ModuleConfig {
    fn merge(&mut self, prev: &ModuleConfig) -> Result<(), MergeConfigError> {
        if self.enable == nginx_sys::NGX_CONF_UNSET as ngx_flag_t {
            self.enable = if prev.enable == nginx_sys::NGX_CONF_UNSET as ngx_flag_t {
                0
            } else {
                prev.enable
            };
        }
        Ok(())
    }
}

unsafe impl HttpModuleLocationConf for Module {
    type LocationConf = ModuleConfig;
}

static mut NGX_HTTP_RUSTYINDEX_COMMANDS: [ngx_command_t; 2] = [
    ngx_command_t {
        name: ngx_string!("rustyindex"),
        type_: NGX_HTTP_SRV_CONF | NGX_HTTP_LOC_CONF | NGX_CONF_FLAG,
        set: Some(nginx_sys::ngx_conf_set_flag_slot),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: core::mem::offset_of!(ModuleConfig, enable),
        post: ptr::null_mut(),
    },
    ngx_command_t::empty(),
];

static NGX_HTTP_RUSTYINDEX_MODULE_CTX: ngx_http_module_t = ngx_http_module_t {
    preconfiguration: None,
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
ngx::ngx_modules!(ngx_http_rustyindex_module);

#[used]
#[allow(non_upper_case_globals)]
#[cfg_attr(not(feature = "export-modules"), unsafe(no_mangle))]
pub static mut ngx_http_rustyindex_module: ngx_module_t = ngx_module_t {
    ctx: &raw const NGX_HTTP_RUSTYINDEX_MODULE_CTX as _,
    commands: unsafe { &raw mut NGX_HTTP_RUSTYINDEX_COMMANDS[0] },
    type_: NGX_HTTP_MODULE,
    ..ngx_module_t::default()
};

struct Entry {
    name: String,
    kind: &'static str,
    mode: ngx_uint_t,
    size: Option<off_t>,
}

struct RustyIndexHandler;

impl HttpRequestHandler for RustyIndexHandler {
    const PHASE: http::HttpPhase = http::HttpPhase::Content;
    type Output = Status;

    fn handler(request: &mut Request) -> Self::Output {
        let co = Module::location_conf(request).expect("module config is none");

        if co.enable != 1 || !request.path().as_bytes().ends_with(b"/") {
            return Status::NGX_DECLINED;
        }

        let method = request.method();
        if method != Method::GET && method != Method::HEAD {
            return HTTPStatus::NOT_ALLOWED.into();
        }

        let rc = request.discard_request_body();
        if rc != Status::NGX_OK {
            return rc;
        }

        let Some(path) = map_uri_to_dir(request) else {
            return HTTPStatus::INTERNAL_SERVER_ERROR.into();
        };

        ngx_log_debug_http!(request, "rustyindex: \"{}\"", path.to_string_lossy());

        match FileInfo::new_for_file(path) {
            Ok(fi) if fi.is_dir() => (),
            Ok(_) => return Status::NGX_DECLINED,
            Err(err) => return file_error(request, path, "stat", err),
        }

        let mut dir = match ReadDir::new(path) {
            Ok(dir) => dir,
            Err(err) => return file_error(request, path, "opendir", err),
        };

        let mut entries = Vec::new();

        loop {
            let entry = match dir.next_entry() {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(err) => return file_error(request, path, "readdir", err),
            };

            let name = entry.name();
            if name == b"." || name == b".." {
                continue;
            }

            let mode = entry.access();

            let (kind, size) = if entry.is_link() {
                ("link", Some(entry.size()))
            } else if entry.is_dir() {
                ("dir", None)
            } else {
                ("file", Some(entry.size()))
            };

            entries.push(Entry {
                name: String::from_utf8_lossy(name).into_owned(),
                kind,
                mode,
                size,
            });
        }

        drop(dir);

        entries.sort_unstable_by(|a, b| a.name.cmp(&b.name));

        let body = format_table(&entries);

        send_response(request, &body)
    }
}

/// Maps the request URI to a NUL-terminated directory path allocated from the request pool.
fn map_uri_to_dir(request: &mut Request) -> Option<&'static CStr> {
    let r = request.as_mut();
    let mut path = ngx_str_t::empty();
    let mut root: usize = 0;

    let last = unsafe { nginx_sys::ngx_http_map_uri_to_path(r, &raw mut path, &raw mut root, 0) };
    if last.is_null() {
        return None;
    }

    // Strip the trailing slash, like ngx_http_autoindex_module does.
    let mut len = unsafe { last.offset_from(path.data) } as usize;
    if len > 1 {
        len -= 1;
    }

    // SAFETY: the buffer has at least `len + 1` bytes and outlives the request handler.
    unsafe {
        *path.data.add(len) = b'\0';
        Some(CStr::from_bytes_with_nul_unchecked(core::slice::from_raw_parts(path.data, len + 1)))
    }
}

fn file_error(request: &Request, path: &CStr, op: &str, err: impl Into<i32>) -> Status {
    // Errno stores a Win32 error code as i32, while ngx_err_t is DWORD on Windows.
    #[allow(clippy::unnecessary_cast)]
    let err = err.into() as ngx_err_t;

    let (level, status) = match err {
        nginx_sys::NGX_ENOENT | nginx_sys::NGX_ENOTDIR | nginx_sys::NGX_ENAMETOOLONG => {
            (NGX_LOG_ERR, HTTPStatus::NOT_FOUND)
        }
        nginx_sys::NGX_EACCES => (NGX_LOG_ERR, HTTPStatus::FORBIDDEN),
        _ => (NGX_LOG_CRIT, HTTPStatus::INTERNAL_SERVER_ERROR),
    };

    ngx_log_error!(
        level,
        request.log(),
        "rustyindex: {op}() \"{}\" failed (error {err})",
        path.to_string_lossy()
    );

    status.into()
}

fn format_table(entries: &[Entry]) -> String {
    struct Escape<'a>(&'a str);

    impl fmt::Display for Escape<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            for c in self.0.chars() {
                if c == '|' || c == '\\' {
                    f.write_char('\\')?;
                }
                f.write_char(c)?;
            }
            Ok(())
        }
    }

    let mut out = String::from("| name | type | mode | size |\n| --- | --- | --- | --- |\n");

    for entry in entries {
        let _ = write!(out, "| {} | {} | {:04o} | ", Escape(&entry.name), entry.kind, entry.mode);
        match entry.size {
            Some(size) => {
                let _ = write!(out, "{size}");
            }
            None => out.push('-'),
        }
        out.push_str(" |\n");
    }

    out
}

fn send_response(request: &mut Request, body: &str) -> Status {
    request.set_status(HTTPStatus::OK);
    request.set_content_length_n(body.len());

    let ct = ngx_string!("text/markdown; charset=utf-8");
    let r = request.as_mut();
    r.headers_out.content_type = ct;
    r.headers_out.content_type_len = ct.len;
    r.headers_out.content_type_lowcase = ptr::null_mut();

    let rc = request.send_header();
    if rc == Status::NGX_ERROR || rc > Status::NGX_OK || request.header_only() {
        return rc;
    }

    let Some(mut buf) = request.pool().create_buffer_from_str(body) else {
        return Status::NGX_ERROR;
    };

    buf.set_last_buf(request.is_main());
    buf.set_last_in_chain(true);

    let mut out = ngx_chain_t { buf: buf.as_ngx_buf_mut(), next: ptr::null_mut() };
    request.output_filter(&mut out)
}
