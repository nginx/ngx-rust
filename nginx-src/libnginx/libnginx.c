
/*
 * Copyright (C) Nginx, Inc.
 */


#include <ngx_config.h>
#include <ngx_core.h>
#include <nginx.h>

#include "libnginx.h"


/* 
 * We need to build nginx.c to correctly initialize ngx_core_module,
 * but exclude an existing definition of main.
 */
#define  main  main_unused
#include "nginx.c"
#undef  main


static ngx_int_t libngx_write_temp_conf_file(ngx_cycle_t *cycle,
    ngx_str_t *data, ngx_str_t *name);


ngx_cycle_t *
libngx_init(u_char *prefix)
{
    static ngx_cycle_t   *cycle, init_cycle;

    ngx_log_t    *log;
    char *const   argv[] = { "nginx" };

    if (cycle != NULL) {
        if (cycle->pool == NULL) {
            cycle->pool = ngx_create_pool(1024, cycle->log);
            if (cycle->pool == NULL || ngx_process_options(cycle) != NGX_OK) {
                return NULL;
            }
        }
        return cycle;
    }

    ngx_conf_params = (u_char *) "daemon off; master_process off;";
    ngx_error_log = (u_char *) "";
    ngx_prefix = prefix;

    ngx_debug_init();

    if (ngx_strerror_init() != NGX_OK) {
        return NULL;
    }

    ngx_max_sockets = -1;

    ngx_time_init();

#if (NGX_PCRE)
    ngx_regex_init();
#endif

    ngx_pid = ngx_getpid();
    ngx_parent = ngx_getppid();

    log = ngx_log_init(ngx_prefix, ngx_error_log);
    if (log == NULL) {
        return NULL;
    }

    log->log_level = NGX_LOG_INFO;

#if (NGX_OPENSSL)
    ngx_ssl_init(log);
#endif

    ngx_memzero(&init_cycle, sizeof(ngx_cycle_t));
    init_cycle.log = log;
    init_cycle.log_use_stderr = 1;
    ngx_cycle = &init_cycle;

    init_cycle.pool = ngx_create_pool(1024, log);
    if (init_cycle.pool == NULL) {
        return NULL;
    }

    if (ngx_save_argv(&init_cycle, sizeof(argv)/sizeof(argv[0]), argv)
        != NGX_OK) 
    {
        return NULL;
    }

    if (ngx_process_options(&init_cycle) != NGX_OK) {
        return NULL;
    }

    if (ngx_os_init(log) != NGX_OK) {
        return NULL;
    }

    if (ngx_crc32_table_init() != NGX_OK) {
        return NULL;
    }

    ngx_slab_sizes_init();

    if (ngx_preinit_modules() != NGX_OK) {
        return NULL;
    }

    cycle = &init_cycle;
    return cycle;
}


void
libngx_cleanup(ngx_cycle_t *cycle)
{
    ngx_destroy_pool(cycle->pool);
    cycle->pool = NULL;
}


ngx_int_t
libngx_create_cycle(ngx_cycle_t *cycle, ngx_str_t *conf)
{
    ngx_str_t  conf_file;

    ngx_cycle = cycle;

    if (libngx_write_temp_conf_file(cycle, conf, &conf_file) != NGX_OK) {
        return NGX_ERROR;
    }

    ngx_conf_file = conf_file.data;

    if (ngx_process_options(cycle) != NGX_OK) {
        return NGX_ERROR;
    }

    cycle = ngx_init_cycle(cycle);
    if (cycle == NULL) {
        return NGX_ERROR;
    }

    ngx_cycle = cycle;

    return NGX_OK;
}


static ngx_int_t
libngx_write_temp_conf_file(ngx_cycle_t *cycle, ngx_str_t *data,
    ngx_str_t *name)
{
    ngx_int_t         rc;
    ngx_path_t       *path;
    ngx_temp_file_t   tf;

    path = ngx_pcalloc(cycle->pool, sizeof(ngx_path_t));
    if (path == NULL) {
        return NGX_ERROR;
    }

    ngx_memzero(&tf, sizeof(ngx_temp_file_t));

    tf.file.fd = NGX_INVALID_FILE;
    tf.file.log = cycle->log;
    tf.access = NGX_FILE_OWNER_ACCESS;
    tf.clean = 1;
    tf.path = path;
    tf.pool = cycle->pool;
    tf.persistent = 1;

    ngx_str_set(&path->name, "conf");

    rc = ngx_conf_full_name(cycle, &path->name, 0);
    if (rc != NGX_OK) {
        return rc;
    }

    if (ngx_create_dir(path->name.data, ngx_dir_access(tf.access))
        == NGX_FILE_ERROR)
    {
        return ngx_errno;
    }

    rc = ngx_create_temp_file(&tf.file, tf.path, tf.pool, tf.persistent,
                              tf.clean, tf.access);
    if (rc != NGX_OK) {
        return rc;
    }

    if (ngx_write_file(&tf.file, data->data, data->len, 0) == NGX_ERROR) {
        return NGX_ERROR;
    }

    *name = tf.file.name;

    return NGX_OK;
}
