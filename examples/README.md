- [Examples](#examples)
  - [CURL](#curl)
  - [AWSSIG](#awssig)
  - [HTTPORIGDST  - NGINX Destination IP recovery module for HTTP](#httporigdst----nginx-destination-ip-recovery-module-for-http)
    - [Dependencies](#dependencies)
    - [Example Configuration](#example-configuration)
      - [HTTP](#http)
    - [Embedded Variables](#embedded-variables)
    - [Usage](#usage)
    - [Caveats](#caveats)


# Examples
This crate provides a couple of example using [ngx](https://crates.io/crates/ngx) crate:

- [awssig.rs](./awssig.rs) - An example of NGINX dynamic module that can sign GET request using AWS Signature v4.
- [curl](./curl.rs) - An example of the Access Phase NGINX dynamic module that blocks HTTP requests if `user-agent` header starts with `curl`.
- [httporigdst](./httporigdst.rs) - A dynamic module recovers the original IP address and port number of the destination packet.
- [rustyindex](./rustyindex.rs) - A Content Phase module that lists directory contents as a markdown table, using the file and directory wrappers from `ngx::core::file`.
- [upstream](./upstream.rs) - A dynamic module demonstrating the setup code to write an upstream filter or load balancer.

To build all these examples simply run:

```
cargo build --package=examples --examples
```


## CURL

This module demonstrates how to create a minimal dynamic module with `http_request_handler`, that checks for User-Agent headers and returns status code 403 if UA starts with `curl`, if a module is disabled then uses `core::Status::NGX_DECLINED` to indicate the operation is rejected, for example, because it is disabled in the configuration (`curl off`). Additionally, it demonstrates how to write a defective parser.

An example of nginx configuration file that uses that module can be found at [curl.conf](./curl.conf).

How to build and run in a [Docker](../Dockerfile) container curl example:
```
# build all dynamic modules examples and specify NGINX version to use
docker buildx build --build-arg NGX_VERSION=1.23.3 -t ngx-rust .

# start NGINX using curl.conf module example:
docker run --rm -d  -p 8000:8000 ngx-rust nginx -c examples/curl.conf

# test it - you should see 403 Forbidden
curl http://127.0.0.1:8000 -v -H "user-agent: curl"


# test it - you should see 404 Not Found
curl http://127.0.0.1:8000 -v -H "user-agent: foo"
```

## AWSSIG

This module uses [NGX_HTTP_PRECONTENT_PHASE](https://nginx.org/en/docs/dev/development_guide.html#http_phases) and provides examples, of how to use external dependency and manipulate HTTP headers before sending client requests upstream.

An example of nginx configuration file that uses that module can be found at [awssig.conf](./awssig.conf).

## HTTPORIGDST  - NGINX Destination IP recovery module for HTTP

This dynamic module recovers the original IP address and port number of the destination packet. It is useful, for example, with container sidecars where all outgoing traffic is redirected to a separate container with iptables before reaching the target.

This module can only be built with the "linux" feature enabled, and will only successfully build on a Linux OS.

### Dependencies
This module uses the Rust crate libc and Linux **getsockopt** socket API.

### Example Configuration
#### HTTP

```nginx configuration
load_module "modules/libhttporigdst.so";

http {
    server {
        # use iptables to capture all outgoing traffic and REDIRECT
        # to listening port 15501
        listen 15501;

        # binding variables provided by module will lazily activate it
        # and store a context
        # variables can be used in config
        location / {
            # Return if no backend is available or proxy_pass
            # return 200 "recv'd: $server_addr:$server_port\n\nproxy_pass http://$server_orig_addr:$server_orig_port\n";
            proxy_pass http://$server_orig_addr:$server_orig_port;
        }
    }
}
```

### Embedded Variables

The following embedded variables are provided:

* **server_orig_addr**
  * Original IP address
* **server_orig_port**
  * Original port

### Usage

1. Clone the git repository.
  ```
  git clone git@github.com:nginx/ngx-rust.git
  ```

2. Compile the module from the cloned repo.
  ```
  cd ${CLONED_DIRECTORY}/ngx-rust
  cargo build --package=examples --example=httporigdst --features=linux
  ```

3. Copy the shared object to the modules directory, /etc/nginx/modules.
  ```
  cp ./target/debug/examples/libhttporigdst.so /etc/nginx/modules
  ```

4. Add the `load_module` directive to your configuration.
  ```
  load_module "modules/libhttporigdst.so";
  ```

5. Reload NGINX.
  ```
  nginx -t && nginx -s reload
  ```

6. Redirect traffic outbound.
  ```
  iptables -t nat -N NGINX_REDIRECT && \
  iptables -t nat -A NGINX_REDIRECT -p tcp -j REDIRECT --to-port 15501 --dport 15001 && \
  iptables -t nat -N NGINX_OUTPUT && \
  iptables -t nat -A OUTPUT -p tcp -j NGINX_OUTPUT && \
  iptables -t nat -A NGINX_OUTPUT -j NGINX_REDIRECT
  ```

7. Redirect traffic inbound.
  ```
  iptables -t nat -N NGINX_IN_REDIRECT && \
  iptables -t nat -A NGINX_IN_REDIRECT -p tcp -j REDIRECT --to-port 15501 --dport 15001 && \
  iptables -t nat -N NGINX_INBOUND && \
  iptables -t nat -A PREROUTING -p tcp -j NGINX_INBOUND && \
  iptables -t nat -A NGINX_INBOUND -p tcp -j NGINX_IN_REDIRECT
  ```

8. Test with `curl` (this step assumes you've uncommented the return directive).
  ```
  curl --output - ${LISTEN_IP}:15001
  recv'd: ${LISTEN_IP}:15501

  proxy_pass http://${LISTEN_IP}:15001
  ```
### Caveats

This module only supports IPv4.

## UPSTREAM - Example upstream / load balancing module for HTTP

This module simply proxies requests through a custom load balancer to the previously configured balancer. This is for demonstration purposes only. As a module writer, you can start with this structure and adjust to your needs, then implement the proper algorithm for your usage.

The module replaces the `peer` callback functions with its own, logs, and then calls through to the originally saved `peer` functions. This may look confusing at first, but rest assured, it's intentionally not implementing an algorithm of its own.

### Attributions

This module was converted from https://github.com/gabihodoroaga/nginx-upstream-module and also highly inspired by the same techniques used in NGINX source: `ngx_http_upstream_keepalive_module.c`.

### Example Configuration
#### HTTP

```nginx configuration
load_module "modules/upstream.so"

http {
    upstream backend {
        server localhost:15501;
        custom 32;
    }

    server {
        listen 15500;
        server_name _;

        location / {
            proxy_pass http://backend;
        }
    }

    server {
        listen 15501;

        location / {
            return 418;
        }
    }
}
```

### Usage

1. Clone the git repository.
  ```
  git clone git@github.com:nginx/ngx-rust.git
  ```

2. Compile the module from the cloned repo.
  ```
  cd ${CLONED_DIRECTORY}/ngx-rust
  cargo build --package=examples --example=upstream
  ```

3. Copy the shared object to the modules directory, /etc/nginx/modules.
  ```
  cp ./target/debug/examples/libupstream.so /etc/nginx/modules
  ```

4. Add the `load_module` directive to your configuration.
  ```
  load_module "modules/libupstream.so";
  ```

5. Add the example `server` and `upstream` block from the example above.

6. Reload NGINX.
  ```
  nginx -t && nginx -s reload
  ```

7. Test with `curl`. Traffic should pass to your listener on port 8081 (this could be another NGINX server for example). With debug logging enabled you should notice the upstream log messages (see the source code for log examples, prefixed with "CUSTOM UPSTREAM").
