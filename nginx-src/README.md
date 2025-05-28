# nginx-src

This crate contains a vendored copy of the NGINX source and the logic to
build it.  It is intended to be consumed by the `nginx-sys` crate for CI
builds, tests or rustdoc generation.

It is notably not intended for producing binaries suitable for production
use.  For such scenaros we recommend building the ngx-rust based module
against prebuilt packages from https://nginx.org/ or your preferred
distribution.  See the `nginx-sys` documentation for building ngx-rust
modules against an existing pre-configured NGINX source tree.

## Versioning

This crate follows the latest stable branch of NGINX.

 * The major version is derived from the major and minor version of the
   NGINX stable branch being used: (`nginx.major` * 1000 + `nginx.minor`).
 * The minor version is taken from the NGINX patch version.
 * The patch version is incremented on changes to the build logic or crate
   metadata.

## Build Requirements

The crate can be built on common Unix-like operating systems and requires
all the usual NGINX build dependencies (including development headers
for the libraries) installed in system paths:

* C compiler and toolchain
* SSL library, OpenSSL or LibreSSL
* PCRE or PCRE2
* Zlib or zlib-ng witn zlib compatibile API enabled

We don't intend to support Windows at the moment, as NGINX does not
support dynamic modules for this target.

## License

This crate contains the source code of NGINX and thus inherits the
[BSD-2-Clause](nginx/LICENSE) license.
