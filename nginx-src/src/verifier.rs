use std::ffi::OsString;
use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::{env, io};

static GNUPG_COMMAND: &str = "gpg";

pub struct SignatureVerifier {
    gnupghome: PathBuf,
}

impl SignatureVerifier {
    pub fn new() -> io::Result<Self> {
        if env::var("NGX_NO_SIGNATURE_CHECK").is_ok() {
            return Err(io::Error::other(
                "signature check disabled by user".to_string(),
            ));
        };

        if let Err(x) = duct::cmd!(GNUPG_COMMAND, "--version").stdout_null().run() {
            return Err(io::Error::other(format!(
                "signature check disabled: \"{GNUPG_COMMAND}\" not found ({x})"
            )));
        }

        // We do not want to mess with the default gpg data for the running user,
        // so we store all gpg data within our build directory.
        let gnupghome = env::var("OUT_DIR")
            .map(PathBuf::from)
            .map_err(io::Error::other)?
            .join(".gnupg");

        if !fs::exists(&gnupghome)? {
            fs::create_dir_all(&gnupghome)?;
        }

        change_permissions_recursively(gnupghome.as_path(), 0o700, 0o600)?;

        Ok(Self { gnupghome })
    }

    /// Imports all the required GPG keys into a temporary directory in order to
    /// validate the integrity of the downloaded tarballs.
    pub fn import_keys(&self, server: &str, key_ids: &[&str]) -> io::Result<()> {
        println!(
            "Importing {} GPG keys for key server: {}",
            key_ids.len(),
            server
        );

        let mut args = vec![
            OsString::from("--homedir"),
            self.gnupghome.clone().into(),
            OsString::from("--keyserver"),
            server.into(),
            OsString::from("--recv-keys"),
        ];
        args.extend(key_ids.iter().map(OsString::from));

        let cmd = duct::cmd(GNUPG_COMMAND, &args);
        let output = cmd.stderr_to_stdout().stdout_capture().unchecked().run()?;

        if !output.status.success() {
            eprintln!("{}", String::from_utf8_lossy(&output.stdout));
            return Err(io::Error::other(format!(
                "Command: {:?}\nFailed to import GPG keys: {}",
                cmd,
                key_ids.join(" ")
            )));
        }

        Ok(())
    }

    /// Validates the integrity of a file against the cryptographic signature associated with
    /// the file.
    pub fn verify_signature(&self, path: &Path, signature: &Path) -> io::Result<()> {
        let cmd = duct::cmd!(
            GNUPG_COMMAND,
            "--homedir",
            &self.gnupghome,
            "--verify",
            signature,
            path
        );
        let output = cmd.stderr_to_stdout().stdout_capture().unchecked().run()?;
        if !output.status.success() {
            eprintln!("{}", String::from_utf8_lossy(&output.stdout));
            return Err(io::Error::other(format!(
                "Command: {:?}\nGPG signature verification of archive failed [{}]",
                cmd,
                path.display()
            )));
        }
        Ok(())
    }
}

fn change_permissions_recursively(
    path: &Path,
    dir_mode: u32,
    file_mode: u32,
) -> std::io::Result<()> {
    if path.is_dir() {
        // Set directory permissions to 700
        fs::set_permissions(path, Permissions::from_mode(dir_mode))?;

        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();

            change_permissions_recursively(&path, dir_mode, file_mode)?;
        }
    } else {
        // Set file permissions to 600
        fs::set_permissions(path, Permissions::from_mode(file_mode))?;
    }

    Ok(())
}
