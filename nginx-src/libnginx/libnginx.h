
/*
 * Copyright (C) Nginx, Inc
 */


#ifndef _LIBNGINX_H_INCLUDED_
#define _LIBNGINX_H_INCLUDED_

ngx_cycle_t *libngx_init(u_char *prefix);
ngx_int_t libngx_create_cycle(ngx_cycle_t *cycle, ngx_str_t *conf);


#endif /* _LIBNGINX_H_INCLUDED_ */
