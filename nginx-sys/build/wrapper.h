#include <ngx_config.h>
#include <ngx_core.h>
#include <ngx_event.h>
#include <ngx_event_connect.h>

/* __has_include was a compiler-specific extension until C23,
 * but it's safe to assume that bindgen supports it via libclang.
 */
#if defined(__has_include)

#if defined(NGX_RS_FEATURE_HTTP) && __has_include(<ngx_http.h>)
#include <ngx_http.h>
#endif

#if defined(NGX_RS_FEATURE_MAIL) && __has_include(<ngx_mail.h>)
#include <ngx_mail.h>
#endif

#if defined(NGX_RS_FEATURE_STREAM) && __has_include(<ngx_stream.h>)
#include <ngx_stream.h>
#endif

#else
#error "libclang does not support __has_include"
#endif

const char *NGX_RS_MODULE_SIGNATURE = NGX_MODULE_SIGNATURE;

// NGX_ALIGNMENT could be defined as a constant or an expression, with the
// latter being unsupported by bindgen.
const size_t NGX_RS_ALIGNMENT = NGX_ALIGNMENT;

// `--prefix=` results in not emitting the declaration
#ifndef NGX_PREFIX
#define NGX_PREFIX ""
#endif

#ifndef NGX_CONF_PREFIX
#define NGX_CONF_PREFIX NGX_PREFIX
#endif
