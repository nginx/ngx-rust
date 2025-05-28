# nginx-src

The crate contains a vendored copy of the NGINX source and a logic to build it.
It is intended to be consumed by the [nginx-sys] crate for CI builds, tests or
rustdoc generation.

It is notably not intended for producing binaries suitable for production use.
For such scenarios we recommend building the `ngx-rust` based module against
the packages from <https://nginx.org/> or your preferred distribution.
See the [nginx-sys] documentation for building `ngx-rust` modules against an
existing pre-configured NGINX source tree.

[nginx-sys]: https://docs.rs/nginx-sys/

## Versioning

This crate follows the latest stable branch of NGINX.

 * The major and minor fields are taken from the NGINX version.
 * The patch version is incremented on changes to the build logic or crate
   metadata.
 * The version metadata contains full version of NGINX.

## Build Requirements

The crate can be built on common Unix-like operating systems and requires all
the usual NGINX build dependencies (including development headers for the
libraries) installed in system paths:

 * C compiler and toolchain
 * SSL library, OpenSSL or LibreSSL
 * PCRE or PCRE2
 * Zlib or zlib-ng witn Zlib compatible API enabled

We don't intend to support Windows at the moment, as NGINX does not support
dynamic modules for this target.

## Environment variables

Following variables can be set to customize the build.

 * `NGX_CONFIGURE_ARGS` — additional arguments to pass to the NGINX configure
   script.

   Example: `export NGX_CONFIGURE_ARGS='--with-debug'; cargo build`

 * `NGX_CFLAGS`, `NGX_LDFLAGS` — additional C compiler and linker flags to
   pass to the NGINX configure script.  Internally, this is added to the
   `--with-cc-opt=...` and `--with-ld-opt=...` configure arguments.

   Example:
   ```sh
   export NGX_CFLAGS='-I/opt/boringssl/include'
   export NGX_LDFLAGS='-L/opt/boringssl/build -lstdc++'
   cargo build
   ```

## Download NGINX and dependency sources during build

While we recommend using the system libraries, it is still possible to opt into
downloading the NGINX itself and the dependency sources from the network with
the help of the following variables:

 * `NGX_VERSION` — if specified, the version of NGINX to download and build
   instead of the one bundled with the crate.
 * `OPENSSL_VERSION` — if specified, the version of OpenSSL to download and use
   use instead of the system-provided library.
 * `PCRE2_VERSION` — if specified, the version of PCRE2 to download and use
   instead of the system-provided library.
 * `ZLIB_VERSION` — if specified, the version of Zlib to download and use
   instead of the system-provided library.

If the `gpg` executable is present in the path, the build script will verify
the integrity of the downloaded files using GPG signatures and a known set of
public keys.
This behavior can be disabled by setting `NGX_NO_SIGNATURE_CHECK`.

## License

The code in this crate is licensed under the [Apache License 2.0](../LICENSE).
The crate also contains the source code of NGINX, distributed under the
[BSD 2-Clause License](https://nginx.org/LICENSE).
