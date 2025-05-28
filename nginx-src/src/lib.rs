use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::{env, io, thread};

pub static NGINX_DEFAULT_SOURCE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/nginx");

const NGINX_BUILD_INFO: &str = "last-build-info";
const NGINX_BINARY: &str = "nginx";

static NGINX_CONFIGURE_FLAGS: &[&str] = &[
    "--with-compat",
    "--with-http_realip_module",
    "--with-http_ssl_module",
    "--with-http_v2_module",
    "--with-stream_realip_module",
    "--with-stream_ssl_module",
    "--with-stream",
    "--with-threads",
];

pub fn build(build_dir: impl AsRef<Path>) -> io::Result<(PathBuf, PathBuf)> {
    let source_dir = PathBuf::from(NGINX_DEFAULT_SOURCE_DIR);
    let build_dir = build_dir.as_ref().to_owned();

    configure(&source_dir, &build_dir, NGINX_CONFIGURE_FLAGS)?;

    make(&source_dir, &build_dir)?;

    Ok((source_dir, build_dir))
}

/// Run external process invoking autoconf `configure` for NGINX.
pub fn configure(source_dir: &Path, build_dir: &Path, configure_flags: &[&str]) -> io::Result<()> {
    let build_info = build_info(configure_flags);

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

    println!("Using NGINX source at {:?}", source_dir);

    let configure = ["configure", "auto/configure"]
        .into_iter()
        .map(|x| source_dir.join(x))
        .find(|x| x.is_file())
        .ok_or(io::ErrorKind::NotFound)?;

    println!(
        "Running NGINX configure script with flags: {:?}",
        configure_flags.join(" ")
    );

    let mut build_dir_arg: OsString = "--builddir=".into();
    build_dir_arg.push(build_dir);

    let mut flags: Vec<OsString> = configure_flags.iter().map(|x| x.into()).collect();
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

/// Run `make` within the NGINX source directory as an external process.
fn make(source_dir: &Path, build_dir: &Path) -> io::Result<()> {
    // Level of concurrency to use when building nginx - cargo nicely provides this information
    let num_jobs = match env::var("NUM_JOBS") {
        Ok(s) => s.parse::<usize>().ok(),
        Err(_) => thread::available_parallelism().ok().map(|n| n.get()),
    }
    .unwrap_or(1);

    let run_make = |x| -> io::Result<_> {
        /* Use the duct dependency here to merge the output of STDOUT and STDERR into a single stream,
        and to provide the combined output as a reader which can be iterated over line-by-line. We
        use duct to do this because it is a lot of work to implement this from scratch. */
        let out = duct::cmd!(
            x,
            "-j",
            num_jobs.to_string(),
            "-f",
            build_dir.join("Makefile")
        )
        .dir(source_dir)
        .stderr_to_stdout()
        .run()?;

        if !out.status.success() {
            println!("{} failed with {:?}", x, out.status);
            return Err(io::ErrorKind::Other.into());
        }

        Ok(())
    };

    // Give preference to the binary with the name of gmake if it exists because this is typically
    // the GNU 4+ on MacOS (if it is installed via homebrew).
    match run_make("gmake") {
        Ok(out) => Ok(out),
        Err(err) if err.kind() == io::ErrorKind::NotFound => run_make("make"),
        Err(err) => Err(err),
    }
}

/// Returns the options in which NGINX was built with
fn build_info(nginx_configure_flags: &[&str]) -> String {
    // Flags should contain strings pointing to OS/platform as well as dependency versions,
    // so if any of that changes, it can trigger a rebuild
    nginx_configure_flags.join(" ")
}
