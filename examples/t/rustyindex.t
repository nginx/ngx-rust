#!/usr/bin/perl

# (C) Aleksei Bavshin
# (C) Nginx, Inc

# Tests for ngx-rust example modules: rustyindex.

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

my $t = Test::Nginx->new()->has(qw/http/)->plan(6)
	->write_file_expand('nginx.conf', <<'EOF');

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
            rustyindex on;
        }
    }
}

EOF

mkdir($t->testdir() . '/subdir');

$t->run();

###############################################################################

my $r = http_get('/');

like($r, qr/^HTTP\/1\.1 200 /, 'status');
like($r, qr/^Content-Type: text\/markdown/mi, 'content type');
like($r, qr/^\| nginx\.conf \| file \| \d+ \| \d+ \|$/m, 'nginx.conf');
like($r, qr/^\| error\.log \| file \| \d+ \| \d+ \|$/m, 'error.log');
like($r, qr/^\| subdir \| dir \| \d+ \| - \|$/m, 'directory');

like(http_get('/missing/'), qr/ 404 Not Found/, 'not found');

###############################################################################
