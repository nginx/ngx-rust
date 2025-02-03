# nginx-sys

The `nginx-sys` crate provides low-level bindings for the nginx C API, allowing
Rust applications to interact with nginx servers and modules.

## Usage

Add `nginx-sys` as a dependency in your `Cargo.toml`:

```toml
[dependencies]
nginx-sys = "0.5.0"
```

Set the path to your nginx sources and build directory using the
[environment variables](#input-variables), or enable the `nginx-sys/vendored`
feature to use a downloaded copy of the nginx sources.

Note that it is recommended to use the exact source and `configure` arguments
of the nginx build you are planning to use in production.
`configure` arguments alter the APIs and the symbols visible to the Rust code,
and some OS distributions are known to ship nginx packages with API-breaking
patches applied.

## Features

- `vendored`: Enables the build scripts to download and build a copy of nginx
  source and link against it.

## Input variables

`NGINX_SOURCE_DIR`, `NGINX_BUILD_DIR` control paths to an external nginx source
tree and a configured build directory.  Normally, specifying either of these is
sufficient, as the build directory defaults to `objs` under the source dir.

However, it's possible to set the latter to any valid path with
`configure --builddir=<...>`, and we need _both_ paths to support such
configuration.

The variables above are optional, but take preference when the `vendored` crate
feature is enabled.

## Output variables

Following metadata variables are passed to the build scripts of any **direct**
dependents of the package.

Check the [using another sys crate] and the [links manifest key] sections of the
Cargo Book for more details about passing metadata between packages.

### `DEP_NGINX_FEATURES`

nginx has various configuration options that may affect the availability of
functions, constants and structure fields. This is not something that can be
detected at runtime, as an attempt to use a symbol unavailable in the bindings
would result in compilation error.

`DEP_NGINX_FEATURES_CHECK` contains the full list of feature flags supported
by `nginx-sys`, i.e. everything that can be used for feature checks.
The most common use for this variable is to specify [`cargo::rustc-check-cfg`].

`DEP_NGINX_FEATURES` contains the list of features enabled in the version of
nginx being built against.

An example of a build script with these variables:
```rust
// Specify acceptable values for `ngx_feature`.
println!("cargo::rerun-if-env-changed=DEP_NGINX_FEATURES_CHECK");
println!(
    "cargo::rustc-check-cfg=cfg(ngx_feature, values({}))",
    std::env::var("DEP_NGINX_FEATURES_CHECK").unwrap_or("any()".to_string())
);
// Read feature flags detected by nginx-sys and pass to the compiler.
println!("cargo::rerun-if-env-changed=DEP_NGINX_FEATURES");
if let Ok(features) = std::env::var("DEP_NGINX_FEATURES") {
    for feature in features.split(',').map(str::trim) {
        println!("cargo::rustc-cfg=ngx_feature=\"{}\"", feature);
    }
}
```

And an usage example:
```rust
#[cfg(ngx_feature = "debug")]
println!("this nginx binary was built with debug logging enabled");
```

### `DEP_NGINX_OS`

Version, as detected by the nginx configuration script.

`DEP_NGINX_OS_CHECK` contains the full list of supported values, and
`DEP_NGINX_OS` the currently detected one.

Usage examples:
```rust
// Specify acceptable values for `ngx_os`
println!("cargo::rerun-if-env-changed=DEP_NGINX_OS_CHECK");
println!(
    "cargo::rustc-check-cfg=cfg(ngx_os, values({}))",
    std::env::var("DEP_NGINX_OS_CHECK").unwrap_or("any()".to_string())
);
// Read operating system detected by nginx-sys and pass to the compiler.
println!("cargo::rerun-if-env-changed=DEP_NGINX_OS");
if let Ok(os) = std::env::var("DEP_NGINX_OS") {
    println!("cargo::rustc-cfg=ngx_os=\"{}\"", os);
}
```

```rust
#[cfg(ngx_os = "freebsd")]
println!("this nginx binary was built on FreeBSD");
```

### Version and build information

- `DEP_NGINX_VERSION_NUMBER`:
  a numeric representation with 3 digits for each component: `1026002`
- `DEP_NGINX_VERSION`:
  human-readable string in a product/version format: `nginx/1.26.2`
- `DEP_NGINX_BUILD`:
  version string with the optional build name (`--build=`) included:
  `nginx/1.25.5 (nginx-plus-r32)`

Usage example:
```rust
println!("cargo::rustc-check-cfg=cfg(nginx1_27_0)");
println!("cargo::rerun-if-env-changed=DEP_NGINX_VERSION_NUMBER");
if let Ok(version) = std::env::var("DEP_NGINX_VERSION_NUMBER") {
    let version: u64 = version.parse().unwrap();

    if version >= 1_027_000 {
        println!("cargo::rustc-cfg=nginx1_27_0");
    }
}
```

## Examples

### Get nginx Version

This example demonstrates how to retrieve the version of the nginx server.

```rust,no_run
use nginx_sys::nginx_version;

println!("nginx version: {}", nginx_version);
```
[using another sys crate]: https://doc.rust-lang.org/nightly/cargo/reference/build-script-examples.html#using-another-sys-crate
[links manifest key]: https://doc.rust-lang.org/nightly/cargo/reference/build-scripts.html#the-links-manifest-key
[`cargo::rustc-check-cfg`]: https://doc.rust-lang.org/nightly/cargo/reference/build-scripts.html#rustc-check-cfg
