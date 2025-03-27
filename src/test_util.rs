use std::borrow::Cow;
use std::ffi::CStr;
use std::fs;
use std::fs::read_dir;
use std::io::Result;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;

use crate::ffi::{
    NGX_CONF_PATH, NGX_CONF_PREFIX, NGX_ERROR_LOG_PATH, NGX_HTTP_CLIENT_TEMP_PATH, NGX_HTTP_FASTCGI_TEMP_PATH,
    NGX_HTTP_LOG_PATH, NGX_HTTP_PROXY_TEMP_PATH, NGX_HTTP_SCGI_TEMP_PATH, NGX_HTTP_UWSGI_TEMP_PATH, NGX_LOCK_PATH,
    NGX_PID_PATH, NGX_PREFIX, NGX_SBIN_PATH,
};

/// Convert a CStr to a PathBuf
pub fn cstr_to_path(val: &std::ffi::CStr) -> Option<Cow<Path>> {
    if val.is_empty() {
        return None;
    }

    #[cfg(unix)]
    {
        let str = std::ffi::OsStr::from_bytes(val.to_bytes());
        Some(Cow::Borrowed(str.as_ref()))
    }
    #[cfg(not(unix))]
    {
        let str = std::str::from_utf8(val.to_bytes()).ok()?;
        Some(&str.as_ref())
    }
}

fn target_dir_cands() -> Option<Vec<PathBuf>> {
    #[cfg(target_os = "macos")]
    {
        match std::env::var("DYLD_FALLBACK_LIBRARY_PATH") {
            Ok(cands) => Some(cands.split(':').map(PathBuf::from).collect()),
            Err(_) => None,
        }
    }
    #[cfg(target_os = "linux")]
    {
        match std::env::var("LD_LIBRARY_PATH") {
            Ok(cands) => Some(cands.split(':').map(PathBuf::from).collect()),
            Err(_) => None,
        }
    }
}

/// search path and return the path to the target
pub fn target_path(target_name: &str) -> std::io::Result<PathBuf> {
    if let Some(cands) = target_dir_cands() {
        for dir in cands {
            if let Ok(iter) = read_dir(dir) {
                for entry in iter.flatten() {
                    if entry.file_name() == target_name {
                        return Ok(entry.path());
                    }
                }
            }
        }
    }
    Err(std::io::ErrorKind::NotFound.into())
}

/// harness to test nginx
#[allow(dead_code)]
pub struct Nginx {
    // these paths have options to change them from default paths (in prefix dir)
    // most of them are not used, but keep them for future uses
    prefix: PathBuf,
    sbin_path: PathBuf,
    modules_prefix: PathBuf, // and only this path is not embedded in bindings.rs, since the module root is same to the prefix
    conf_path: PathBuf,
    conf_prefix: PathBuf,
    error_log_path: PathBuf,
    pid_path: PathBuf,
    lock_path: PathBuf,
    http_log_path: PathBuf,
    http_client_body_temp_path: PathBuf,
    http_proxy_temp_path: PathBuf,
    http_fastcgi_temp_path: PathBuf,
    http_uwsgi_temp_path: PathBuf,
    http_scgi_temp_path: PathBuf,
    // here all path are absolute
    status: Status,
}

#[derive(PartialEq, Eq)]
enum Status {
    Unknown,
    Running,
    Stopped,
}

/// nginx harness builder
pub struct NginxBuilder {
    prefix: PathBuf,
    sbin_path: PathBuf,
    modules_prefix: PathBuf,
    conf_path: PathBuf,
    conf_prefix: PathBuf,
    error_log_path: PathBuf,
    pid_path: PathBuf,
    lock_path: PathBuf,
    http_log_path: PathBuf,
    http_client_body_temp_path: PathBuf,
    http_proxy_temp_path: PathBuf,
    http_fastcgi_temp_path: PathBuf,
    http_uwsgi_temp_path: PathBuf,
    http_scgi_temp_path: PathBuf,
    // in builder path could be relative
}

impl Default for NginxBuilder {
    fn default() -> Self {
        fn conv(raw_path: &CStr) -> Option<PathBuf> {
            cstr_to_path(raw_path).map(|p| p.to_path_buf())
        }
        Self {
            prefix: conv(NGX_PREFIX).expect("installation prefix"),
            sbin_path: conv(NGX_SBIN_PATH).expect("nginx executable path"),
            modules_prefix: conv(NGX_PREFIX).expect("module prefix"),
            conf_path: conv(NGX_CONF_PATH).expect("configuration file path"),
            conf_prefix: conv(NGX_CONF_PREFIX).expect("configuration file prefix"),
            error_log_path: conv(NGX_ERROR_LOG_PATH).expect("error log file path"),
            pid_path: conv(NGX_PID_PATH).expect("pid file path"),
            lock_path: conv(NGX_LOCK_PATH).expect("lock file path"),
            http_log_path: conv(NGX_HTTP_LOG_PATH).expect("http log file path"),
            http_client_body_temp_path: conv(NGX_HTTP_CLIENT_TEMP_PATH).expect("client body temp file path"),
            http_proxy_temp_path: conv(NGX_HTTP_PROXY_TEMP_PATH).expect("proxy temp file path"),
            http_fastcgi_temp_path: conv(NGX_HTTP_FASTCGI_TEMP_PATH).expect("fastcgi temp file path"),
            http_uwsgi_temp_path: conv(NGX_HTTP_UWSGI_TEMP_PATH).expect("uwsgi temp file path"),
            http_scgi_temp_path: conv(NGX_HTTP_SCGI_TEMP_PATH).expect("scgi temp file path"),
        }
    }
}

impl NginxBuilder {
    /// set alternative configuration path
    pub fn conf_path(mut self, path: PathBuf) -> Self {
        self.conf_path = path;
        self
    }

    /// build nginx harness
    pub fn build(self) -> Nginx {
        let prefix = self.prefix;

        let add_prefix = |p: PathBuf| -> PathBuf {
            if p.is_relative() {
                prefix.join(p)
            } else {
                p.to_path_buf()
            }
        };

        let sbin_path = add_prefix(self.sbin_path);
        let modules_path = add_prefix(self.modules_prefix);
        let conf_path = add_prefix(self.conf_path);
        let conf_prefix = add_prefix(self.conf_prefix);
        let error_log_path = add_prefix(self.error_log_path);
        let pid_path = add_prefix(self.pid_path);
        let lock_path = add_prefix(self.lock_path);
        let http_log_path = add_prefix(self.http_log_path);
        let http_client_body_temp_path = add_prefix(self.http_client_body_temp_path);
        let http_proxy_temp_path = add_prefix(self.http_proxy_temp_path);
        let http_fastcgi_temp_path = add_prefix(self.http_fastcgi_temp_path);
        let http_uwsgi_temp_path = add_prefix(self.http_uwsgi_temp_path);
        let http_scgi_temp_path = add_prefix(self.http_scgi_temp_path);

        Nginx {
            prefix,
            sbin_path,
            modules_prefix: modules_path,
            conf_path,
            conf_prefix,
            error_log_path,
            pid_path,
            lock_path,
            http_log_path,
            http_client_body_temp_path,
            http_proxy_temp_path,
            http_fastcgi_temp_path,
            http_uwsgi_temp_path,
            http_scgi_temp_path,
            status: Status::Unknown,
        }
    }
}

impl Nginx {
    /// execute nginx process with arguments
    pub fn cmd(&mut self, args: &[&str]) -> Result<Output> {
        let result = Command::new(&self.sbin_path).args(args).output();

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
        self.status = Status::Stopped;
        self.cmd(&["-s", "stop"])
    }

    /// start the nginx binary
    pub fn start(&mut self) -> Result<Output> {
        let output = self.cmd(&[]);
        if let Ok(output) = &output {
            if output.status.success() {
                self.status = Status::Running;
            }
        }
        output
    }

    /// make sure we stop existing nginx and start new master process
    /// intentinally ignore failure in stop
    pub fn restart(&mut self) -> Result<Output> {
        let _ = self.stop();
        self.start()
    }

    /// replace main config with another config
    pub fn copy_main_config<P: AsRef<Path>>(&mut self, conf_path_from: P) -> Result<u64> {
        let conf_path_to = &self.conf_path;
        println!(
            "copying main config from: {} to: {}",
            conf_path_from.as_ref().display(),
            conf_path_to.display()
        ); // replace with logging
        fs::copy(conf_path_from, conf_path_to)
    }

    /// replace config with another config
    pub fn copy_config<P: AsRef<Path>, Q: AsRef<Path>>(
        &mut self,
        conf_path_from: P,
        conf_path_rel_to: Q,
    ) -> Result<u64> {
        if conf_path_rel_to.as_ref().is_relative() {
            let conf_path_to = self.conf_prefix.join(conf_path_rel_to.as_ref());
            println!(
                "copying config from: {} to: {}",
                conf_path_from.as_ref().display(),
                conf_path_to.display()
            ); // replace with logging
            fs::copy(conf_path_from, conf_path_to)
        } else {
            panic!("absolute path");
        }
    }
    /// create config from &str
    pub fn create_config_from_str<Q: AsRef<Path>>(&mut self, conf_path_rel_to: Q, conf_content: &str) -> Result<()> {
        if conf_path_rel_to.as_ref().is_relative() {
            let conf_path_to = self.conf_prefix.join(conf_path_rel_to.as_ref());
            println!(
                "creating config to: {} content: {}",
                conf_path_to.display(),
                conf_content
            ); // replace with logging
            fs::write(conf_path_to, conf_content)
        } else {
            panic!("absolute path");
        }
    }
    /// copy or replace module
    pub fn copy_module<P: AsRef<Path>, Q: AsRef<Path>>(
        &mut self,
        module_path_from: P,
        module_path_rel_to: Q,
    ) -> Result<u64> {
        if module_path_rel_to.as_ref().is_relative() {
            let module_path_to = self.modules_prefix.join(module_path_rel_to.as_ref());
            println!(
                "copying module from: {} to: {}",
                module_path_from.as_ref().display(),
                module_path_to.display()
            ); // replace with logging
            fs::copy(module_path_from, module_path_to)
        } else {
            panic!("absolute path");
        }
    }

    /// get prefix of nginx instance
    pub fn prefix(&self) -> &Path {
        &self.prefix
    }
    /// get bin path to nginx instance
    pub fn bin_path(&self) -> &Path {
        &self.sbin_path
    }
    /// get module prefix
    pub fn modules_prefix(&self) -> &Path {
        &self.modules_prefix
    }
    /// get configuration file path
    pub fn conf_path(&self) -> &Path {
        &self.conf_path
    }
    /// get configuration file prefix
    pub fn conf_prefix(&self) -> &Path {
        &self.conf_prefix
    }
    /// get error log file path
    pub fn error_log_path(&self) -> &Path {
        &self.error_log_path
    }
    /// get pid file path
    pub fn pid_path(&self) -> &Path {
        &self.pid_path
    }
    /// get lock file path
    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }
    /// get http log file path
    pub fn http_log_path(&self) -> &Path {
        &self.http_log_path
    }
    /// get client body temp file path
    pub fn http_client_body_temp_path(&self) -> &Path {
        &self.http_client_body_temp_path
    }
    /// get proxy temp file path
    pub fn http_proxy_temp_path(&self) -> &Path {
        &self.http_proxy_temp_path
    }
    /// get fastcgi temp file path
    pub fn http_fastcgi_temp_path(&self) -> &Path {
        &self.http_fastcgi_temp_path
    }
    /// get uwsgi temp file path
    pub fn http_uwsgi_temp_path(&self) -> &Path {
        &self.http_uwsgi_temp_path
    }
    /// get scgi temp file path
    pub fn http_scgi_temp_path(&self) -> &Path {
        &self.http_scgi_temp_path
    }
}

impl Drop for Nginx {
    fn drop(&mut self) {
        // exec stop if running or unknown
        if self.status != Status::Stopped {
            let _ = self.stop();
        }
    }
}
