#!/usr/bin/perl

# (C) Nginx, Inc

# Tests for ngx-rust example modules.

###############################################################################

use warnings;
use strict;

use Test::More;

use Socket qw/ CRLF /;

BEGIN { use FindBin; chdir($FindBin::Bin); }

use lib 'lib';
use Test::Nginx;

###############################################################################

select STDERR; $| = 1;
select STDOUT; $| = 1;

my $t = Test::Nginx->new()->has(qw/http proxy/)->plan(2)
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

        awssigv4_access_key  my-access-key;
        awssigv4_secret_key  my-secret-key;
        awssigv4_s3_bucket   my-bucket;
        awssigv4_s3_endpoint s3.example.com;

        location / {
            awssigv4 on;
            proxy_pass http://127.0.0.1:8081;
        }
    }

    server {
        listen       127.0.0.1:8081;
        server_name  localhost;

        add_header   x-amz-date \$http_x_amz_date;
        add_header   x-authorization \$http_authorization;

        location / { }
    }
}

EOF

$t->write_file('index.html', '');
$t->run();

###############################################################################

like(http_get('/'), qr/x-authorization: AWS4.*Credential=my-access-key/i,
	'awssig header');

like(http(
    'GET / HTTP/1.0' . CRLF
    . 'Foo: foo' . CRLF
    . "Bar: \xFF\xFE\x80\x81" . CRLF
	. "Host: localhost" . CRLF . CRLF
), qr/x-authorization: AWS4.*Credential=my-access-key/,
	'awssig invalid header ignored');

###############################################################################
