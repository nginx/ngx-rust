#include <ngx_http.h>
#include <ngx_conf_file.h>
#include <ngx_config.h>
#include <ngx_core.h>

const char *NGX_RS_MODULE_SIGNATURE = NGX_MODULE_SIGNATURE;

// `--prefix=` results in not emitting the declaration
#ifndef NGX_PREFIX
#define NGX_PREFIX ""
#endif

#ifndef NGX_CONF_PREFIX
#define NGX_CONF_PREFIX NGX_PREFIX
#endif
