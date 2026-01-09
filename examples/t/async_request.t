#!/usr/bin/perl

# (C) Nginx, Inc

# Tests for ngx-rust example modules.

###############################################################################

use warnings;
use strict;

use Test::More;

BEGIN { use FindBin; chdir($FindBin::Bin); }

use lib 'lib';
use Test::Nginx;

###############################################################################

select STDERR; $| = 1;
select STDOUT; $| = 1;

my $t = Test::Nginx->new()->has(qw/http/)->plan(1)
	->write_file_expand('nginx.conf', <<"EOF");

%%TEST_GLOBALS%%

daemon off;

events {
}

http {
    %%TEST_GLOBALS_HTTP%%

    server {
        listen       127.0.0.1:8080;
        server_name  localhost;

        location / {
            async_request on;
        }

        location /proxy {
            internal;
            proxy_pass http://127.0.0.1:8081;
        }
    }

    server {
        listen      127.0.0.1:8081;
        server_name localhost;

        location / {
            return 200 'Hello from backend';
        }
    }
}

EOF

$t->write_file('index.html', '');
$t->run();

like(http_get('/'), qr/200 OK.*Hello from backend/s, 'async subrequest works');
