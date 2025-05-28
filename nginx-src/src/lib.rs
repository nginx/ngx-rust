#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::{env, io, thread};

mod download;
mod verifier;

static NGINX_DEFAULT_SOURCE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/nginx");

const NGINX_BUILD_INFO: &str = "last-build-info";
const NGINX_BINARY: &str = "nginx";

static NGINX_CONFIGURE_BASE: &[&str] = &[
    "--with-compat",
    "--with-http_realip_module",
    "--with-http_ssl_module",
    "--with-http_v2_module",
    "--with-stream",
    "--with-stream_realip_module",
    "--with-stream_ssl_module",
    "--with-threads",
];

const ENV_VARS_TRIGGERING_RECOMPILE: [&str; 10] = [
    "CACHE_DIR",
    "CARGO_MANIFEST_DIR",
    "CARGO_TARGET_TMPDIR",
    "NGX_CONFIGURE_ARGS",
    "NGX_CFLAGS",
    "NGX_LDFLAGS",
    "NGX_VERSION",
    "OPENSSL_VERSION",
    "PCRE2_VERSION",
    "ZLIB_VERSION",
];

/*
###########################################################################
# NGINX Build Functions - Everything below here is for building NGINX     #
###########################################################################

In order to build Rust bindings for NGINX using the bindgen crate, we need
to do the following:

 1. Obtain a copy of the NGINX source code and the necessary dependencies:
    OpenSSL, PCRE2, Zlib.
 3. Run autoconf `configure` for NGINX.
 4. Compile NGINX.
 5. Read the autoconf generated makefile for NGINX and configure bindgen
    to generate Rust bindings based on the includes in the makefile.
*/

/// Outputs cargo instructions required for using this crate from a buildscript.
pub fn print_cargo_metadata() {
    for file in ["lib.rs", "download.rs", "verifier.rs"] {
        println!(
            "cargo::rerun-if-changed={path}/src/{file}",
            path = env!("CARGO_MANIFEST_DIR")
        )
    }

    for var in ENV_VARS_TRIGGERING_RECOMPILE {
        println!("cargo::rerun-if-env-changed={var}");
    }
}

/// Builds a copy of NGINX sources, either bundled with the crate or downloaded from the network.
pub fn build(build_dir: impl AsRef<Path>) -> io::Result<(PathBuf, PathBuf)> {
    let source_dir = PathBuf::from(NGINX_DEFAULT_SOURCE_DIR);
    let build_dir = build_dir.as_ref().to_owned();

    let (source_dir, vendored_flags) = download::prepare(&source_dir, &build_dir)?;

    let flags = nginx_configure_flags(&vendored_flags);

    configure(&source_dir, &build_dir, &flags)?;

    make(&source_dir, &build_dir, ["build"])?;

    Ok((source_dir, build_dir))
}

/// Returns the options NGINX was built with
fn build_info(source_dir: &Path, configure_flags: &[String]) -> String {
    // Flags should contain strings pointing to OS/platform as well as dependency versions,
    // so if any of that changes, it can trigger a rebuild
    format!("{:?}|{}", source_dir, configure_flags.join(" "))
}

/// Generate the flags to use with autoconf `configure` for NGINX.
fn nginx_configure_flags(vendored: &[String]) -> Vec<String> {
    let mut nginx_opts: Vec<String> = NGINX_CONFIGURE_BASE
        .iter()
        .map(|x| String::from(*x))
        .collect();

    nginx_opts.extend(vendored.iter().map(Into::into));

    if let Ok(extra_args) = env::var("NGX_CONFIGURE_ARGS") {
        // FIXME: shell style split?
        nginx_opts.extend(extra_args.split_whitespace().map(Into::into));
    }

    if let Ok(cflags) = env::var("NGX_CFLAGS") {
        nginx_opts.push(format!("--with-cc-opt={cflags}"));
    }

    if let Ok(ldflags) = env::var("NGX_LDFLAGS") {
        nginx_opts.push(format!("--with-ld-opt={ldflags}"));
    }

    nginx_opts
}

/// Runs external process invoking autoconf `configure` for NGINX.
fn configure(source_dir: &Path, build_dir: &Path, flags: &[String]) -> io::Result<()> {
    let build_info = build_info(source_dir, flags);

    if build_dir.join("Makefile").is_file()
        && build_dir.join(NGINX_BINARY).is_file()
        && matches!(
            std::fs::read_to_string(build_dir.join(NGINX_BUILD_INFO)).map(|x| x == build_info),
            Ok(true)
        )
    {
        println!("Build info unchanged, skipping configure");
        return Ok(());
    }

    println!("Using NGINX source at {source_dir:?}");

    let configure = ["configure", "auto/configure"]
        .into_iter()
        .map(|x| source_dir.join(x))
        .find(|x| x.is_file())
        .ok_or(io::ErrorKind::NotFound)?;

    println!(
        "Running NGINX configure script with flags: {:?}",
        flags.join(" ")
    );

    let mut build_dir_arg: OsString = "--builddir=".into();
    build_dir_arg.push(build_dir);

    let mut flags: Vec<OsString> = flags.iter().map(|x| x.into()).collect();
    flags.push(build_dir_arg);

    let output = duct::cmd(configure, flags)
        .dir(source_dir)
        .stderr_to_stdout()
        .run()?;

    if !output.status.success() {
        println!("configure failed with {:?}", output.status);
        return Err(io::ErrorKind::Other.into());
    }

    let _ = std::fs::write(build_dir.join(NGINX_BUILD_INFO), build_info);

    Ok(())
}

/// Runs `make` within the NGINX source directory as an external process.
fn make<U>(source_dir: &Path, build_dir: &Path, extra_args: U) -> io::Result<Output>
where
    U: IntoIterator,
    U::Item: Into<OsString>,
{
    // Level of concurrency to use when building nginx - cargo nicely provides this information
    let num_jobs = match env::var("NUM_JOBS") {
        Ok(s) => s.parse::<usize>().ok(),
        Err(_) => thread::available_parallelism().ok().map(|n| n.get()),
    }
    .unwrap_or(1);

    let mut args = vec![
        OsString::from("-f"),
        build_dir.join("Makefile").into(),
        OsString::from("-j"),
        num_jobs.to_string().into(),
    ];
    args.extend(extra_args.into_iter().map(Into::into));

    // Use MAKE passed from the parent process if set. Otherwise prefer `gmake` as it provides a
    // better feature-wise implementation on some systems.
    // Notably, we want to avoid SUN make on Solaris (does not support -j) or ancient GNU make 3.81
    // on MacOS.
    let inherited = env::var("MAKE");
    let make_commands: &[&str] = match inherited {
        Ok(ref x) => &[x.as_str(), "gmake", "make"],
        _ => &["gmake", "make"],
    };

    // Give preference to the binary with the name of gmake if it exists because this is typically
    // the GNU 4+ on MacOS (if it is installed via homebrew).
    for make in make_commands {
        /* Use the duct dependency here to merge the output of STDOUT and STDERR into a single stream,
        and to provide the combined output as a reader which can be iterated over line-by-line. We
        use duct to do this because it is a lot of work to implement this from scratch. */
        let result = duct::cmd(*make, &args)
            .dir(source_dir)
            .stderr_to_stdout()
            .run();

        match result {
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                eprintln!("make: command '{make}' not found");
                continue;
            }
            Ok(out) if !out.status.success() => {
                return Err(io::Error::other(format!(
                    "make: '{}' failed with {:?}",
                    make, out.status
                )));
            }
            _ => return result,
        }
    }

    Err(io::ErrorKind::NotFound.into())
}
