use std::fs;
use std::io::Result;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::Output;

use ngx::ffi::{NGX_CONF_PATH, NGX_PREFIX, NGX_SBIN_PATH};

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

/// harness to test nginx
pub struct Nginx {
    pub install_path: PathBuf,
    pub config_path: PathBuf,
}

impl Default for Nginx {
    /// create nginx with default
    fn default() -> Nginx {
        let install_path = cstr_to_path(NGX_PREFIX).expect("installation prefix");
        Nginx::new(install_path)
    }
}

impl Nginx {
    pub fn new<P: AsRef<Path>>(path: P) -> Nginx {
        let install_path = path.as_ref();
        let config_path = cstr_to_path(NGX_CONF_PATH).expect("configuration path");
        let config_path = install_path.join(config_path);

        Nginx {
            install_path: install_path.into(),
            config_path,
        }
    }

    /// get bin path to nginx instance
    pub fn bin_path(&mut self) -> PathBuf {
        let bin_path = cstr_to_path(NGX_SBIN_PATH).expect("binary path");
        self.install_path.join(bin_path)
    }

    /// start nginx process with arguments
    pub fn cmd(&mut self, args: &[&str]) -> Result<Output> {
        let bin_path = self.bin_path();
        let result = Command::new(bin_path).args(args).output();

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
