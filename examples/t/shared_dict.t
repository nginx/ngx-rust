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

my $t = Test::Nginx->new()->has(qw/http rewrite/)->plan(12)
	->write_file_expand('nginx.conf', <<'EOF');

%%TEST_GLOBALS%%

daemon off;

worker_processes 2;

events {
}

http {
    %%TEST_GLOBALS_HTTP%%

    shared_dict_zone z 64k;
    shared_dict $arg_key $foo;

    server {
        listen       127.0.0.1:8080;
        server_name  localhost;

        add_header X-Value $foo;
        add_header X-Process $pid;

        location /set/ {
            add_header X-Process $pid;
            set $foo $arg_value;
            return 200;
        }

        location /entries/ {
            add_header X-Process $pid;
            return 200 $shared_dict_entries;
        }

        location /clear/ {
            add_header X-Process $pid;
            set $shared_dict_entries "";
            return 200;
        }
    }
}

EOF

$t->write_file('index.html', '');
$t->run();

###############################################################################

like(http_get('/set/?key=fst&value=hello'), qr/200 OK/, 'set value 1');
like(http_get('/set/?key=snd&value=world'), qr/200 OK/, 'set value 2');

ok(check('/?key=fst', qr/X-Value: hello/i), 'check value 1');
ok(check('/?key=snd', qr/X-Value: world/i), 'check value 2');

like(http_get('/set/?key=fst&value=new_value'), qr/200 OK/, 'update value 1');
ok(check('/?key=fst', qr/X-Value: new_value/i), 'check updated value');

like(http_get('/entries/'), qr/^2; ((?:fst = new_value|snd = world); ){2}$/ms,
	'get entries');

like(http_delete('/set/?key=snd'), qr/200 OK/, 'delete value 2');

unlike(http_get('/?key=snd'), qr/X-Value:/i, 'check deleted value');

like(http_get('/entries/'), qr/^1; fst = new_value; $/ms,
	'get entries - deleted');

like(http_get('/clear/'), qr/200 OK/, 'clear');

like(http_get('/entries/'), qr/^0; $/ms, 'get entries - clear');

###############################################################################

sub check {
	my ($uri, $like) = @_; 

	my $r = http_get($uri);

	return unless ($r =~ $like && $r =~ /X-Process: (\d+)/);

	return 1 if $^O eq 'MSWin32'; # only one active worker process

	my $pid = $1;

	for (1 .. 25) {
		$r = http_get($uri);
        
		return unless ($r =~ $like && $r =~ /X-Process: (\d+)/);
		return 1 if $pid != $1;
	}
}

sub http_delete {
	my ($url, %extra) = @_;
	return http(<<EOF, %extra);
DELETE $url HTTP/1.0
Host: localhost

EOF

}

###############################################################################
