extern crate bindgen;

use std::env;
use std::error::Error as StdError;
use std::fs::{read_to_string, File};
use std::io::Write;
use std::path::{Path, PathBuf};

const ENV_VARS_TRIGGERING_RECOMPILE: &[&str] = &["OUT_DIR", "NGINX_BUILD_DIR", "NGINX_SOURCE_DIR"];

/// The feature flags set by the nginx configuration script.
///
/// This list is a subset of NGX_/NGX_HAVE_ macros known to affect the structure layout or module
/// avialiability.
///
/// The flags will be exposed to the buildscripts of _direct_ dependendents of this crate as
/// `DEP_NGINX_FEATURES` environment variable.
/// The list of recognized values will be exported as `DEP_NGINX_FEATURES_CHECK`.
const NGX_CONF_FEATURES: &[&str] = &[
    "compat",
    "debug",
    "have_epollrdhup",
    "have_eventfd",
    "have_file_aio",
    "have_kqueue",
    "have_memalign",
    "have_posix_memalign",
    "have_sched_yield",
    "have_variadic_macros",
    "http",
    "http_cache",
    "http_dav",
    "http_gzip",
    "http_realip",
    "http_ssi",
    "http_ssl",
    "http_upstream_zone",
    "http_v2",
    "http_v3",
    "http_x_forwarded_for",
    "pcre",
    "pcre2",
    "quic",
    "ssl",
    "stream",
    "stream_ssl",
    "stream_upstream_zone",
    "threads",
];

/// The operating systems supported by the nginx configuration script
///
/// The detected value will be exposed to the buildsrcipts of _direct_ dependents of this crate as
/// `DEP_NGINX_OS` environment variable.
/// The list of recognized values will be exported as `DEP_NGINX_OS_CHECK`.
const NGX_CONF_OS: &[&str] = &[
    "darwin", "freebsd", "gnu_hurd", "hpux", "linux", "solaris", "tru64", "win32",
];

type BoxError = Box<dyn StdError>;

/// Function invoked when `cargo build` is executed.
/// This function will download NGINX and all supporting dependencies, verify their integrity,
/// extract them, execute autoconf `configure` for NGINX, compile NGINX and finally install
/// NGINX in a subdirectory with the project.
fn main() -> Result<(), BoxError> {
    // Hint cargo to rebuild if any of the these environment variables values change
    // because they will trigger a recompilation of NGINX with different parameters
    for var in ENV_VARS_TRIGGERING_RECOMPILE {
        println!("cargo:rerun-if-env-changed={var}");
    }
    println!("cargo:rerun-if-changed=build/main.rs");
    println!("cargo:rerun-if-changed=build/wrapper.h");

    let nginx = NginxSource::from_env();
    println!(
        "cargo:rerun-if-changed={}",
        nginx.build_dir.join("Makefile").to_string_lossy()
    );
    println!(
        "cargo:rerun-if-changed={}",
        nginx.build_dir.join("ngx_auto_config.h").to_string_lossy()
    );
    // Read autoconf generated makefile for NGINX and generate Rust bindings based on its includes
    generate_binding(&nginx);
    Ok(())
}

pub struct NginxSource {
    source_dir: PathBuf,
    build_dir: PathBuf,
}

impl NginxSource {
    pub fn new(source_dir: impl AsRef<Path>, build_dir: impl AsRef<Path>) -> Self {
        let source_dir = NginxSource::check_source_dir(source_dir).expect("source directory");
        let build_dir = NginxSource::check_build_dir(build_dir).expect("build directory");

        Self {
            source_dir,
            build_dir,
        }
    }

    pub fn from_env() -> Self {
        match (
            env::var_os("NGINX_SOURCE_DIR"),
            env::var_os("NGINX_BUILD_DIR"),
        ) {
            (Some(source_dir), Some(build_dir)) => NginxSource::new(source_dir, build_dir),
            (Some(source_dir), None) => Self::from_source_dir(source_dir),
            (None, Some(build_dir)) => Self::from_build_dir(build_dir),
            _ => Self::from_vendored(),
        }
    }

    pub fn from_source_dir(source_dir: impl AsRef<Path>) -> Self {
        let build_dir = source_dir.as_ref().join("objs");

        // todo!("Build from source");

        Self::new(source_dir, build_dir)
    }

    pub fn from_build_dir(build_dir: impl AsRef<Path>) -> Self {
        let source_dir = build_dir
            .as_ref()
            .parent()
            .expect("source directory")
            .to_owned();
        Self::new(source_dir, build_dir)
    }

    #[cfg(feature = "vendored")]
    pub fn from_vendored() -> Self {
        nginx_src::print_cargo_metadata();

        let out_dir = env::var("OUT_DIR").unwrap();
        let build_dir = PathBuf::from(out_dir).join("objs");
        let (source_dir, build_dir) = nginx_src::build(build_dir).expect("nginx-src build");

        Self {
            source_dir,
            build_dir,
        }
    }

    #[cfg(not(feature = "vendored"))]
    pub fn from_vendored() -> Self {
        panic!(
            "\"nginx-sys/vendored\" feature is disabled and neither NGINX_SOURCE_DIR nor \
             NGINX_BUILD_DIR is set"
        );
    }

    fn check_source_dir(source_dir: impl AsRef<Path>) -> Result<PathBuf, BoxError> {
        match dunce::canonicalize(&source_dir) {
            Ok(path) if path.join("src/core/nginx.h").is_file() => Ok(path),
            Err(err) => Err(format!(
                "Invalid nginx source directory: {:?}. {}",
                source_dir.as_ref(),
                err
            )
            .into()),
            _ => Err(format!(
                "Invalid nginx source directory: {:?}. NGINX_SOURCE_DIR is not specified or \
                 contains invalid value.",
                source_dir.as_ref()
            )
            .into()),
        }
    }

    fn check_build_dir(build_dir: impl AsRef<Path>) -> Result<PathBuf, BoxError> {
        match dunce::canonicalize(&build_dir) {
            Ok(path) if path.join("ngx_auto_config.h").is_file() => Ok(path),
            Err(err) => Err(format!(
                "Invalid nginx build directory: {:?}. {}",
                build_dir.as_ref(),
                err
            )
            .into()),
            _ => Err(format!(
                "Invalid NGINX build directory: {:?}. NGINX_BUILD_DIR is not specified or \
                 contains invalid value.",
                build_dir.as_ref()
            )
            .into()),
        }
    }
}

/// Generates Rust bindings for NGINX
fn generate_binding(nginx: &NginxSource) {
    let autoconf_makefile_path = nginx.build_dir.join("Makefile");
    let (includes, defines) = parse_makefile(&autoconf_makefile_path);
    let includes: Vec<_> = includes
        .into_iter()
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                nginx.source_dir.join(path)
            }
        })
        .collect();
    let mut clang_args: Vec<String> = includes
        .iter()
        .map(|path| format!("-I{}", path.to_string_lossy()))
        .collect();

    clang_args.extend(defines.iter().map(|(n, ov)| {
        if let Some(v) = ov {
            format!("-D{n}={v}")
        } else {
            format!("-D{n}")
        }
    }));

    print_cargo_metadata(nginx, &includes, &defines).expect("cargo dependency metadata");

    // bindgen targets the latest known stable by default
    let rust_target: bindgen::RustTarget = env::var("CARGO_PKG_RUST_VERSION")
        .expect("rust-version set in Cargo.toml")
        .parse()
        .expect("rust-version is valid and supported by bindgen");

    let bindings = bindgen::Builder::default()
        // Bindings will not compile on Linux without block listing this item
        // It is worth investigating why this is
        .blocklist_item("IPPORT_RESERVED")
        // will be restored later in build.rs
        .blocklist_item("NGX_ALIGNMENT")
        .generate_cstr(true)
        // The input header we would like to generate bindings for.
        .header("build/wrapper.h")
        .clang_args(clang_args)
        .layout_tests(false)
        .rust_target(rust_target)
        .use_core()
        .generate()
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_dir_env =
        env::var("OUT_DIR").expect("The required environment variable OUT_DIR was not set");
    let out_path = PathBuf::from(out_dir_env);
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}

/// Reads through the makefile generated by autoconf and finds all of the includes
/// and definitions used to compile nginx. This is used to generate the correct bindings
/// for the nginx source code.
pub fn parse_makefile(
    nginx_autoconf_makefile_path: &PathBuf,
) -> (Vec<PathBuf>, Vec<(String, Option<String>)>) {
    fn parse_line(
        includes: &mut Vec<String>,
        defines: &mut Vec<(String, Option<String>)>,
        line: &str,
    ) {
        let mut words = shlex::Shlex::new(line);

        while let Some(word) = words.next() {
            if let Some(inc) = word.strip_prefix("-I") {
                let value = if inc.is_empty() {
                    words.next().expect("-I argument")
                } else {
                    inc.to_string()
                };
                includes.push(value);
            } else if let Some(def) = word.strip_prefix("-D") {
                let def = if def.is_empty() {
                    words.next().expect("-D argument")
                } else {
                    def.to_string()
                };

                if let Some((name, value)) = def.split_once("=") {
                    defines.push((name.to_string(), Some(value.to_string())));
                } else {
                    defines.push((def.to_string(), None));
                }
            }
        }
    }

    let mut all_incs = vec![];
    let mut cflags_includes = vec![];

    let mut defines = vec![];

    let makefile_contents = match read_to_string(nginx_autoconf_makefile_path) {
        Ok(path) => path,
        Err(e) => {
            panic!(
                "Unable to read makefile from path [{}]. Error: {}",
                nginx_autoconf_makefile_path.to_string_lossy(),
                e
            );
        }
    };

    let lines = makefile_contents.lines();
    let mut line: String = "".to_string();
    for l in lines {
        if let Some(part) = l.strip_suffix("\\") {
            line += part;
            continue;
        }

        line += l;

        if let Some(tail) = line.strip_prefix("ALL_INCS") {
            parse_line(&mut all_incs, &mut defines, tail);
        } else if let Some(tail) = line.strip_prefix("CFLAGS") {
            parse_line(&mut cflags_includes, &mut defines, tail);
        }

        line.clear();
    }

    cflags_includes.extend(all_incs);

    (
        cflags_includes.into_iter().map(PathBuf::from).collect(),
        defines,
    )
}

/// Collect info about the nginx configuration and expose it to the dependents via
/// `DEP_NGINX_...` variables.
pub fn print_cargo_metadata<T: AsRef<Path>>(
    nginx: &NginxSource,
    includes: &[T],
    defines: &[(String, Option<String>)],
) -> Result<(), Box<dyn StdError>> {
    // Unquote and merge C string constants
    let unquote_re = regex::Regex::new(r#""(.*?[^\\])"\s*"#).unwrap();
    let unquote = |data: &str| -> String {
        unquote_re
            .captures_iter(data)
            .map(|c| c.get(1).unwrap().as_str())
            .collect::<Vec<_>>()
            .concat()
    };

    let mut ngx_features: Vec<String> = vec![];
    let mut ngx_os = String::new();

    let expanded = expand_definitions(includes, defines)?;
    for line in String::from_utf8(expanded)?.lines() {
        let Some((name, value)) = line
            .trim()
            .strip_prefix("RUST_CONF_")
            .and_then(|x| x.split_once('='))
        else {
            continue;
        };

        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();

        if name == "nginx_build" {
            println!("cargo::metadata=build={}", unquote(value));
        } else if name == "nginx_version" {
            println!("cargo::metadata=version={}", unquote(value));
        } else if name == "nginx_version_number" {
            println!("cargo::metadata=version_number={value}");
        } else if NGX_CONF_OS.contains(&name.as_str()) {
            ngx_os = name;
        } else if NGX_CONF_FEATURES.contains(&name.as_str()) && value != "0" {
            ngx_features.push(name);
        }
    }

    println!(
        "cargo::metadata=build_dir={}",
        nginx.build_dir.to_str().expect("Unicode build path")
    );

    println!(
        "cargo::metadata=include={}",
        // The str conversion is necessary because cargo directives must be valid UTF-8
        env::join_paths(includes.iter().map(|x| x.as_ref()))?
            .to_str()
            .expect("Unicode include paths")
    );

    println!(
        "cargo:metadata=cflags={}",
        defines
            .iter()
            .map(|(n, ov)| if let Some(v) = ov {
                format!("-D{n}={v}")
            } else {
                format!("-D{n}")
            })
            .collect::<Vec<_>>()
            .join(" ")
    );

    // A quoted list of all recognized features to be passed to rustc-check-cfg.
    let values = NGX_CONF_FEATURES.join("\",\"");
    println!("cargo::metadata=features_check=\"{values}\"");
    println!("cargo::rustc-check-cfg=cfg(ngx_feature, values(\"{values}\"))");

    // A list of features enabled in the nginx build we're using
    println!("cargo::metadata=features={}", ngx_features.join(","));
    for feature in ngx_features {
        println!("cargo::rustc-cfg=ngx_feature=\"{feature}\"");
    }

    // A quoted list of all recognized operating systems to be passed to rustc-check-cfg.
    let values = NGX_CONF_OS.join("\",\"");
    println!("cargo::metadata=os_check=\"{values}\"");
    println!("cargo::rustc-check-cfg=cfg(ngx_os, values(\"{values}\"))");
    // Current detected operating system
    println!("cargo::metadata=os={ngx_os}");
    println!("cargo::rustc-cfg=ngx_os=\"{ngx_os}\"");

    Ok(())
}

fn expand_definitions<T: AsRef<Path>>(
    includes: &[T],
    defines: &[(String, Option<String>)],
) -> Result<Vec<u8>, Box<dyn StdError>> {
    let path = PathBuf::from(env::var("OUT_DIR")?).join("expand.c");
    let mut writer = std::io::BufWriter::new(File::create(&path)?);

    write!(
        writer,
        "
#include <ngx_config.h>
#include <ngx_core.h>

/* C23 or Clang/GCC/MSVC >= 15.3 extension */
#if defined(__has_include)

#if __has_include(<ngx_http.h>)
RUST_CONF_HTTP=1
#endif

#if __has_include(<ngx_stream.h>)
RUST_CONF_STREAM=1
#endif

#else
/* fallback */
RUST_CONF_HTTP=1
#endif

RUST_CONF_NGINX_BUILD=NGINX_VER_BUILD
RUST_CONF_NGINX_VERSION=NGINX_VER
RUST_CONF_NGINX_VERSION_NUMBER=nginx_version
"
    )?;

    for flag in NGX_CONF_FEATURES.iter().chain(NGX_CONF_OS.iter()) {
        let flag = flag.to_ascii_uppercase();
        write!(
            writer,
            "
#if defined(NGX_{flag})
RUST_CONF_{flag}=NGX_{flag}
#endif"
        )?;
    }

    writer.flush()?;
    drop(writer);

    let mut builder = cc::Build::new();

    builder.includes(includes).file(path);

    for def in defines {
        builder.define(&def.0, def.1.as_deref());
    }

    Ok(builder.try_expand()?)
}
