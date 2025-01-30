mod buffer;
mod pool;
/// The prelude module.
///
/// This module provides common constants and types that are used in NGINX modules.
pub mod prelude;
mod status;
mod string;

pub use buffer::*;
pub use pool::*;
pub use status::*;
pub use string::*;

/// Static empty configuration directive initializer for [`ngx_command_t`].
///
/// This is typically used to terminate an array of configuration directives.
///
/// [`ngx_command_t`]: https://nginx.org/en/docs/dev/development_guide.html#config_directives
#[macro_export]
macro_rules! ngx_null_command {
    () => {
        $crate::ffi::ngx_command_t {
            name: $crate::ngx_null_string!(),
            type_: 0,
            set: None,
            conf: 0,
            offset: 0,
            post: ::core::ptr::null_mut(),
        }
    };
}

/// Static empty configuration variable initializer for [`ngx_http_variable_t`].
///
/// This is typically used to terminate an array of HTTP variable types.
///
/// [`ngx_http_variable_t`]: https://nginx.org/en/docs/dev/development_guide.html#http_variables
#[macro_export]
macro_rules! ngx_http_null_variable {
    () => {
        $crate::ffi::ngx_http_variable_t {
            name: $crate::ngx_null_string!(),
            set_handler: None,
            get_handler: None,
            data: 0,
            flags: 0,
            index: 0,
        }
    };
}

/// Static empty configuration variable initializer for [`ngx_stream_variable_t`].
///
/// This is typically used to terminate an array of Stream variable types.
///
/// [`ngx_stream_variable_t`]: https://github.com/nginx/nginx/blob/1a8ef991d92d22eb8aded7f49595dd31a639e8a4/src/stream/ngx_stream_variables.h#L21
#[macro_export]
macro_rules! ngx_stream_null_variable {
    () => {
        $crate::ffi::ngx_stream_variable_t {
            name: $crate::ngx_null_string!(),
            set_handler: None,
            get_handler: None,
            data: 0,
            flags: 0,
            index: 0,
        }
    };
}
