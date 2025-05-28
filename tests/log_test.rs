use std::env;
use std::fs;
use std::io;
use std::io::Result;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::Output;

const NGINX_BINARY_NAME: &str = "nginx";

/// Convert a CStr to a PathBuf
pub fn cstr_to_path(val: &std::ffi::CStr) -> Option<PathBuf> {
    if val.is_empty() {
        return None;
    }

    #[cfg(unix)]
    let str = std::ffi::OsStr::from_bytes(val.to_bytes());
    #[cfg(not(unix))]
    let str = std::str::from_utf8(val.to_bytes()).ok()?;

    Some(PathBuf::from(str))
}

/// Find nginx binary in the build directory
pub fn find_nginx_binary() -> io::Result<PathBuf> {
    let path = [
        // TEST_NGINX_BINARY is specified for tests
        env::var("TEST_NGINX_BINARY").ok().map(PathBuf::from),
        // The module is built against an external NGINX source tree
        env::var("NGINX_BUILD_DIR")
            .map(PathBuf::from)
            .map(|x| x.join(NGINX_BINARY_NAME))
            .ok(),
        env::var("NGINX_SOURCE_DIR")
            .map(PathBuf::from)
            .map(|x| x.join("objs").join(NGINX_BINARY_NAME))
            .ok(),
        // Fallback to the build directory exposed by nginx-sys
        option_env!("DEP_NGINX_BUILD_DIR")
            .map(PathBuf::from)
            .map(|x| x.join(NGINX_BINARY_NAME)),
    ]
    .into_iter()
    .flatten()
    .find(|x| x.is_file())
    .ok_or(io::ErrorKind::NotFound)?;

    Ok(path)
}

/// harness to test nginx
pub struct Nginx {
    pub prefix: tempfile::TempDir,
    pub bin_path: PathBuf,
    pub config_path: PathBuf,
}

impl Default for Nginx {
    /// create nginx with default
    fn default() -> Nginx {
        let binary = find_nginx_binary().expect("nginx binary");
        Nginx::new(binary).expect("test harness")
    }
}

impl Nginx {
    pub fn new(binary: impl AsRef<Path>) -> io::Result<Nginx> {
        let prefix = tempfile::tempdir()?;
        let config = prefix.path().join("nginx.conf");

        fs::create_dir(prefix.path().join("logs"))?;

        Ok(Nginx {
            prefix,
            bin_path: binary.as_ref().to_owned(),
            config_path: config,
        })
    }

    /// start nginx process with arguments
    pub fn cmd(&self, args: &[&str]) -> Result<Output> {
        let prefix = self.prefix.path().to_string_lossy();
        let config_path = self.config_path.to_string_lossy();
        let args = [&["-p", &prefix, "-c", &config_path], args].concat();
        let result = Command::new(&self.bin_path).args(args).output();

        match result {
            Err(e) => Err(e),

            Ok(output) => {
                println!("status: {}", output.status);
                println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
                println!("stderr: {}", String::from_utf8_lossy(&output.stderr));
                Ok(output)
            }
        }
    }

    /// complete stop the nginx binary
    pub fn stop(&mut self) -> Result<Output> {
        self.cmd(&["-s", "stop"])
    }

    /// start the nginx binary
    pub fn start(&mut self) -> Result<Output> {
        self.cmd(&[])
    }

    // make sure we stop existing nginx and start new master process
    // intentinally ignore failure in stop
    pub fn restart(&mut self) -> Result<Output> {
        let _ = self.stop();
        self.start()
    }

    // replace config with another config
    pub fn replace_config<P: AsRef<Path>>(&mut self, from: P) -> Result<u64> {
        println!(
            "copying config from: {:?} to: {:?}",
            from.as_ref(),
            self.config_path
        ); // replace with logging
        fs::copy(from, &self.config_path)
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;

    const TEST_NGINX_CONFIG: &str = "tests/nginx.conf";

    #[test]
    fn test() {
        let mut nginx = Nginx::default();

        let current_dir = env::current_dir().expect("Unable to get current directory");
        let test_config_path = current_dir.join(TEST_NGINX_CONFIG);

        assert!(
            test_config_path.exists(),
            "Config file not found: {}\nCurrent directory: {}",
            test_config_path.to_string_lossy(),
            current_dir.to_string_lossy()
        );

        nginx.replace_config(&test_config_path).unwrap_or_else(|_| {
            panic!(
                "Unable to load config file: {}",
                test_config_path.to_string_lossy()
            )
        });
        let output = nginx.restart().expect("Unable to restart NGINX");
        assert!(output.status.success());

        let output = nginx.stop().expect("Unable to stop NGINX");
        assert!(output.status.success());
    }
}
