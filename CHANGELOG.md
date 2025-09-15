# Changelog

All notable changes to this project will be documented in this file.

## Release 0.5.0

### Breaking changes

The release contains too many breaking changes to offer a complete migration
guide. We will strive to do better in future releases; meanwhile, we suggest to
look at the changes in example modules:

    git diff v0.4.1..v0.5.0 -- examples/*.rs

### Build experience

In this release, we shifted the focus to an integration into the NGINX build
process. 

The default set of features now requires providing a preconfigured NGINX source
tree with `NGINX_SOURCE_DIR` and `NGINX_BUILD_DIR` variables ([#67], [#124]).

The option to build a copy of NGINX as a part of the module process was
preserved as a feature flag `vendored` and no longer requires downloading
sources from the network ([#160]).
We encourage you to limit use of this feature to testing only.

The [`examples`](./examples/) directory provides `config`, `config.make` and
`auto/rust` scripts to add a Rust-based module to the nginx build, linked
statically with `--add-module=` or dynamically with `--add-dynamic-module=`.
[nginx-acme] offers another, more complete example ([#67], [#95], [#124],
[#154], [#176]).

In addition, we now have a proper detection of the features available in the
build of NGINX ([#97]). See [build.rs](./build.rs) for the necessary setup for
your project.

[nginx-acme]: https://github.com/nginx/nginx-acme
[#67]:  https://github.com/nginx/ngx-rust/pull/67
[#95]:  https://github.com/nginx/ngx-rust/pull/95
[#97]:  https://github.com/nginx/ngx-rust/pull/97
[#124]: https://github.com/nginx/ngx-rust/pull/124
[#154]: https://github.com/nginx/ngx-rust/pull/154
[#160]: https://github.com/nginx/ngx-rust/pull/160
[#176]: https://github.com/nginx/ngx-rust/pull/176

### Module declaration

The traits for declaring a module and accessing module configuration underwent
significant changes in [#142] with the goal of reducing the amount of unsafe
code and enforcing type safety.

See <https://github.com/nginx/ngx-rust/pull/142#issuecomment-2755624647> for a
summary and migration steps.

[#142]: https://github.com/nginx/ngx-rust/pull/142

### Allocators

`ngx` now offers custom allocator support based on the [allocator-api2] crate.
The `ngx::core::Pool` and `ngx::core::SlabPool` can be used for failible
allocations within the NGINX pools and shared zones correspondingly
([#164], [#171]).

Most of the crates with [allocator-api2 support] are compatible with this
implementation. We also provide wrappers for common data structures implemented
in NGINX in [`ngx::collections`](./src/collections/) ([#164], [#181]).

[allocator-api2]: https://crates.io/crates/allocator_api2
[allocator-api2 support]: https://crates.io/crates/allocator_api2/reverse_dependencies
[#164]: https://github.com/nginx/ngx-rust/pull/164
[#171]: https://github.com/nginx/ngx-rust/pull/171
[#181]: https://github.com/nginx/ngx-rust/pull/181


### Other

* We audited the code and fixed or removed most of the methods that made wrong
  assumptions and could panic or crash ([#91], [#152], [#183]).
* `no_std` build support ([#111]).
* Logging API improvements ([#113], [#187])
* The SDK and the example modules can be built and are tested in CI on Windows
  ([#124], [#161]). No further porting or testing work was done.
* Reimplementations for `nginx-sys` methods and macros that cannot be translated
  with bindgen ([#131], [#162], [#167])
* Initial work on the NGINX async runtime ([#170])
* The default branch was renamed to `main`.

[#91]:  https://github.com/nginx/ngx-rust/pull/91
[#111]: https://github.com/nginx/ngx-rust/pull/111
[#113]: https://github.com/nginx/ngx-rust/pull/113
[#131]: https://github.com/nginx/ngx-rust/pull/131
[#152]: https://github.com/nginx/ngx-rust/pull/152
[#161]: https://github.com/nginx/ngx-rust/pull/161
[#162]: https://github.com/nginx/ngx-rust/pull/162
[#167]: https://github.com/nginx/ngx-rust/pull/167
[#170]: https://github.com/nginx/ngx-rust/pull/170
[#183]: https://github.com/nginx/ngx-rust/pull/183
[#187]: https://github.com/nginx/ngx-rust/pull/187

### Supported versions

The minimum supported Rust version is 1.81.0. The version was chosen to support
the packaged Rust toolchain in the recent versions of popular Linux and BSD
distributions.

The minimum supported NGINX version is 1.22. The bindings may compile with an
older version of NGINX, but we do not test that regularly.

Full changelog: [v0.4.1..v0.5.0](https://github.com/nginx/ngx-rust/compare/v0.4.1...v0.5.0)

## Release v0.4.1
 * release:     ngx 0.4.1                                                       (9d2ce0d)
 * release:     nginx-sys 0.2.1                                                 (89eb277)
 * fix(#50):    user_agent method returns 'Option<&NgxStr>' and not '&NgxStr'   (8706830)
 * fix(#41):    zlib version update                                             (89e0fcc)
 * fix:         check user_agent for null                                       (0ca0bc9)
 * feat:        (2 of 2) Upstream module example (#37)                          (840d789)
 * feat:        (1 of 2) Supporting changes for upstream example module. (#36)  (8b3a119)
 * Revert "ci:  use GH cache for .cache folder on MAC OS"                       (7972ae7)
 * docs:        updated changelog                                               (74604e2)

## Release 0.4.0-beta
 * realease:                                                                                                         bump nginx-sys to 0.2.0                                    (ad093d8)
 * feat:                                                                                                             unsafe updates for raw pointer arguments                   (1b88323)
 * feat:                                                                                                             Add debug log mask support (#40)                           (57c68ff)
 * docs:                                                                                                             add support badge to README                                (4aa2e35)
 * cargo:                                                                                                            make macOS linker flags workaround apply to Apple silicon  (f4ea26f)
 * docs:                                                                                                             added repostatus and crates badges                         (dd687a4)

 ## Initial release 0.3.0-beta
 * docs:                                                                                                             prepare ngx to be published to crates.io                   (a1bff29)
 * fix:                                                                                                              nginx-sys enable a few modules by default                  (f23e4b1)
 *  !misc:                                                                                                           project refactor and new module structure                  (b3e8f45)
 * update README (#18)                                                                                               (d2c0b3a)
 * Fix usage example in README (#16)                                                                                 (8deaec0)
 * use nginxinc namespace                                                                                            (9bb9ef6)
 * upgrade to rust 1.26 (#15)                                                                                        (bbfc114)
 * 1.39 upgrade (#12)                                                                                                (be9d532)
 * add pkg-config (#11)                                                                                              (a33c329)
 * upgrade to nginx 1.13.7 (#10)                                                                                     (8c6b968)
 * update the README (#9)                                                                                            (7693ea2)
 * Rust 1.21 (#8)                                                                                                    (4fa395c)
 * Rust 1.21 (#7)                                                                                                    (9517b56)
 * add rust docker tooling                                                                                           (8b4d492)
 * revert back tool tag                                                                                              (82bd0d6)
 * update the tag for rust tool image                                                                                (f2418c0)
 * bump up tool image to 1.22                                                                                        (7db2ced)
 * add rust tool on top of the nginx                                                                                 (9aa4fa0)
 * add developer version of nginx which can do ps                                                                    (994c703)
 * upgrade nginx to 1.13.5 bump up cargo version number respectively separate out nginx make file                    (7095777)
 * consolidate makefile by reducing duplicates simplify docker command                                               (cfe1756)
 * set default rust  tooling to 1.20 generate linux binding in the cargo build fix the linux configuration           (d396803)
 * build nginx binary for darwin add target for darwin                                                               (d2e04ce)
 * add configuration for linux                                                                                       (b3bf4da)
 * remove unnecessary reference to libc invoke make for build.rs in the mac                                          (8e1c544)
 * add unit test for nginx server                                                                                    (d0ff3df)
 * Merge branch 'master' into module                                                                                 (4f6ed43)
 * update the README                                                                                                 (a8cfe50)
 * add license file                                                                                                  (805e70b)
 * add license file                                                                                                  (8c574be)
 * add nginx instance module where you can start and stop nginx sample test for instance                             (acfc545)
 * upgrade bindgen to latest version remove redudant c module code update the nginx config                           (3102b5a)
 * update the tools make file to use Dockerhub                                                                       (36ae1ca)
 * update the README                                                                                                 (ba563fc)
 * add tools directory to generate docker image for rust toolchain check for nginx directory when setting up source  (6842329)
 * add targets for building on linux                                                                                 (6176272)
 * make target for setting up darwin source add os specific build path                                               (fbc5e2f)
 * initial check in generate binding                                                                                 (a38b8ab)

