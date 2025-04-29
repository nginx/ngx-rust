extern crate duct;

use std::collections::HashMap;
use std::error::Error as StdError;
use std::ffi::OsString;
use std::fs::{read_to_string, File, Permissions};
use std::io::ErrorKind::NotFound;
use std::io::{Error as IoError, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::{env, fs, thread};

use duct::cmd;
use flate2::read::GzDecoder;
use tar::Archive;
use which::which;

const UBUNTU_KEYSEVER: &str = "hkps://keyserver.ubuntu.com";
/// The default version of zlib to use if the `ZLIB_VERSION` environment variable is not present
const ZLIB_DEFAULT_VERSION: &str = "1.3.1";
/// Key 1: Mark Adler's public key. For zlib 1.3.1 and earlier
const ZLIB_GPG_SERVER_AND_KEY_ID: (&str, &str) =
    (UBUNTU_KEYSEVER, "5ED46A6721D365587791E2AA783FCD8E58BCAFBA");
const ZLIB_DOWNLOAD_URL_PREFIX: &str = "https://github.com/madler/zlib/releases/download";
/// The default version of pcre to use if the `PCRE2_VERSION` environment variable is not present
const PCRE1_DEFAULT_VERSION: &str = "8.45";
const PCRE2_DEFAULT_VERSION: &str = "10.42";
/// Key 1: Phillip Hazel's public key. For PCRE2 10.42 and earlier
const PCRE2_GPG_SERVER_AND_KEY_ID: (&str, &str) =
    (UBUNTU_KEYSEVER, "45F68D54BBE23FB3039B46E59766E084FB0F43D8");
const PCRE1_DOWNLOAD_URL_PREFIX: &str = "https://sourceforge.net/projects/pcre/files/pcre";
const PCRE2_DOWNLOAD_URL_PREFIX: &str = "https://github.com/PCRE2Project/pcre2/releases/download";
/// The default version of openssl to use if the `OPENSSL_VERSION` environment variable is not
/// present
const OPENSSL1_DEFAULT_VERSION: &str = "1.1.1w";
const OPENSSL3_DEFAULT_VERSION: &str = "3.2.4";
const OPENSSL_GPG_SERVER_AND_KEY_IDS: (&str, &str) = (
    UBUNTU_KEYSEVER,
    "\
EFC0A467D613CB83C7ED6D30D894E2CE8B3D79F5 \
A21FAB74B0088AA361152586B8EF1A6BA9DA2D5C \
8657ABB260F056B1E5190839D9C4D26D0E604491 \
B7C1C14360F353A36862E4D5231C84CDDCC69C45 \
95A9908DDFA16830BE9FB9003D30A3A9FF1360DC \
7953AC1FBC3DC8B3B292393ED5E9E43F7DF9EE8C \
E5E52560DD91C556DDBDA5D02064C53641C25E5D \
C1F33DD8CE1D4CC613AF14DA9195C48241FBF7DD \
BA5473A2B0587B07FB27CF2D216094DFD0CB81EF",
);
const OPENSSL_DOWNLOAD_URL_PREFIX: &str = "https://github.com/openssl/openssl/releases/download";
/// The default version of NGINX to use if the `NGX_VERSION` environment variable is not present
const NGX_DEFAULT_VERSION: &str = "1.26.3";

/// Key 1: Konstantin Pavlov's public key. For Nginx 1.25.3 and earlier
/// Key 2: Sergey Kandaurov's public key. For Nginx 1.25.4
/// Key 3: Maxim Dounin's public key. At least used for Nginx 1.18.0
/// Key 4: Roman Arutyunyan's public key. For Nginx 1.25.5
const NGX_GPG_SERVER_AND_KEY_IDS: (&str, &str) = (
    UBUNTU_KEYSEVER,
    "\
13C82A63B603576156E30A4EA0EA981B66B0D967 \
D6786CE303D9A9022998DC6CC8464D549AF75C0A \
B0F4253373F8F6F510D42178520A9993A1C052F8 \
43387825DDB1BB97EC36BA5D007C8D7C15D87369",
);

const NGX_DOWNLOAD_URL_PREFIX: &str = "https://nginx.org/download";

/// If you are adding another dependency, you will need to add the server/public key tuple below.
const ALL_SERVERS_AND_PUBLIC_KEY_IDS: [(&str, &str); 4] = [
    ZLIB_GPG_SERVER_AND_KEY_ID,
    PCRE2_GPG_SERVER_AND_KEY_ID,
    OPENSSL_GPG_SERVER_AND_KEY_IDS,
    NGX_GPG_SERVER_AND_KEY_IDS,
];

/// List of configure switches specifying the modules to build nginx with
const NGX_BASE_MODULES: [&str; 20] = [
    "--with-compat",
    "--with-http_addition_module",
    "--with-http_auth_request_module",
    "--with-http_flv_module",
    "--with-http_gunzip_module",
    "--with-http_gzip_static_module",
    "--with-http_random_index_module",
    "--with-http_realip_module",
    "--with-http_secure_link_module",
    "--with-http_slice_module",
    "--with-http_slice_module",
    "--with-http_ssl_module",
    "--with-http_stub_status_module",
    "--with-http_sub_module",
    "--with-http_v2_module",
    "--with-stream_realip_module",
    "--with-stream_ssl_module",
    "--with-stream_ssl_preread_module",
    "--with-stream",
    "--with-threads",
];
/// Additional configuration flags to use when building on Linux.
const NGX_LINUX_ADDITIONAL_OPTS: [&str; 1] = ["--with-file-aio"];
const ENV_VARS_TRIGGERING_RECOMPILE: [&str; 12] = [
    "DEBUG",
    "ZLIB_VERSION",
    "PCRE2_VERSION",
    "OPENSSL_VERSION",
    "NGX_VERSION",
    "CARGO_CFG_TARGET_OS",
    "CARGO_MANIFEST_DIR",
    "CARGO_TARGET_TMPDIR",
    "CACHE_DIR",
    "NGX_DEBUG",
    "NGX_INSTALL_ROOT_DIR",
    "NGX_INSTALL_DIR",
];

/// This function will download NGINX and all supporting dependencies, verify their integrity,
/// extract them, execute autoconf `configure` for NGINX, compile NGINX and finally install
/// NGINX in a subdirectory with the project.
pub fn build() -> Result<PathBuf, Box<dyn StdError>> {
    println!("Building NGINX");
    // Create .cache directory
    let cache_dir = make_cache_dir()?;
    println!("Cache directory created");
    // Import GPG keys used to verify dependency tarballs
    import_gpg_keys(&cache_dir)?;
    println!("GPG keys imported");
    // Ensure GPG directory has the correct permissions, if gpg is available
    if gpg_path().is_some() {
        ensure_gpg_permissions(&cache_dir)?;
    }
    println!("Verified GPG permissions");
    // Configure and Compile NGINX
    let (_nginx_install_dir, nginx_src_dir) = compile_nginx(&cache_dir)?;
    // Hint cargo to rebuild if any of the these environment variables values change
    // because they will trigger a recompilation of NGINX with different parameters
    for var in ENV_VARS_TRIGGERING_RECOMPILE {
        println!("cargo:rerun-if-env-changed={var}");
    }
    println!("cargo:rerun-if-changed=build/vendored.rs");

    Ok(nginx_src_dir.join("objs"))
}

/*
###########################################################################
# NGINX Build Functions - Everything below here is for building NGINX     #
###########################################################################

In order to build Rust bindings for NGINX using the bindgen crate, we need
to do the following:

 1. Download NGINX source code and all dependencies (zlib, pcre2, openssl)
 2. Verify the integrity of the downloaded files using GPG signatures
 3. Extract the downloaded files
 4. Run autoconf `configure` for NGINX
 5. Compile NGINX
 6. Install NGINX in a subdirectory of the project
 7. Read the autoconf generated makefile for NGINX and configure bindgen
    to generate Rust bindings based on the includes in the makefile.

Additionally, we want to provide the following features as part of the
build process:
 * Allow the user to specify the version of NGINX to build
 * Allow the user to specify the version of each dependency to build
 * Only reconfigure and recompile NGINX if any of the above versions
   change or the configuration flags change (like enabling or disabling
   the debug mode)
 * Not rely on the user having NGINX dependencies installed on their
   system (zlib, pcre2, openssl)
 * Keep source code and binaries confined to a subdirectory of the
   project to avoid having to track files outside of the project
 * If GPG is not installed, the build will still continue. However, the
   integrity of the downloaded files will not be verified.
*/

fn zlib_archive_url(version: &String) -> String {
    format!("{ZLIB_DOWNLOAD_URL_PREFIX}/v{version}/zlib-{version}.tar.gz")
}

fn pcre_archive_url(version: &String) -> String {
    // We can distinguish pcre1/pcre2 by checking whether the second character is '.', because the
    // final version of pcre1 is 8.45 and the first one of pcre2 is 10.00.
    if version.chars().nth(1).is_some_and(|c| c == '.') {
        format!("{PCRE1_DOWNLOAD_URL_PREFIX}/{version}/pcre-{version}.tar.gz")
    } else {
        format!("{PCRE2_DOWNLOAD_URL_PREFIX}/pcre2-{version}/pcre2-{version}.tar.gz")
    }
}

fn openssl_archive_url(version: &String) -> String {
    if version.starts_with("1.1.1") {
        let version_hyphened = version.replace('.', "_");
        format!("{OPENSSL_DOWNLOAD_URL_PREFIX}/OpenSSL_{version_hyphened}/openssl-{version}.tar.gz")
    } else {
        format!("{OPENSSL_DOWNLOAD_URL_PREFIX}/openssl-{version}/openssl-{version}.tar.gz")
    }
}

fn nginx_archive_url(version: &String) -> String {
    format!("{NGX_DOWNLOAD_URL_PREFIX}/nginx-{version}.tar.gz")
}

/// Returns a list of tuples containing the URL to a tarball archive and the GPG signature used
/// to validate the integrity of the tarball.
fn all_archives() -> Vec<(String, String)> {
    let ngx_version = env::var("NGX_VERSION").unwrap_or_else(|_| NGX_DEFAULT_VERSION.into());
    let zlib_version = env::var("ZLIB_VERSION").unwrap_or_else(|_| ZLIB_DEFAULT_VERSION.into());
    // While Nginx 1.22.0 and later support pcre2 and openssl3, earlier ones only support pcre1 and
    // openssl1. Here provides the appropriate (and as latest as possible) versions of these two
    // dependencies as default, switching `***[major_version]_DEFAULT_VERSION` based on
    // `is_after_1_22`. This facilitates to compile backport versions targeted for Nginx ealier than
    // 1.22.0, which are still used in LTS releases of major Linux distributions.
    let ngx_version_vec: Vec<i16> = ngx_version
        .split('.')
        .map(|s| s.parse().unwrap_or(-1))
        .collect();
    let is_after_1_22 = (ngx_version_vec.len() >= 2)
        && (ngx_version_vec[0] > 1 || (ngx_version_vec[0] == 1 && ngx_version_vec[1] >= 22));
    // keep env name `PCRE2_VERSION` for compat
    let pcre_version = env::var("PCRE2_VERSION").unwrap_or_else(|_| {
        if is_after_1_22 {
            PCRE2_DEFAULT_VERSION
        } else {
            PCRE1_DEFAULT_VERSION
        }
        .into()
    });
    let openssl_version = env::var("OPENSSL_VERSION").unwrap_or_else(|_| {
        if is_after_1_22 {
            OPENSSL3_DEFAULT_VERSION
        } else {
            OPENSSL1_DEFAULT_VERSION
        }
        .into()
    });
    fn url_pair(tar_url: String, pgp_ext: &str) -> (String, String) {
        (tar_url.clone(), format!("{tar_url}.{pgp_ext}"))
    }
    vec![
        url_pair(zlib_archive_url(&zlib_version), "asc"),
        url_pair(pcre_archive_url(&pcre_version), "sig"),
        url_pair(openssl_archive_url(&openssl_version), "asc"),
        url_pair(nginx_archive_url(&ngx_version), "asc"),
    ]
}

fn gpg_path() -> Option<PathBuf> {
    which::which("gpg").ok()
}

/// Returns the base path to extract tarball contents into
fn source_output_dir(cache_dir: &Path, target: &str) -> PathBuf {
    env::var("CARGO_TARGET_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| cache_dir.join("src").join(target))
}

#[allow(clippy::ptr_arg)]
/// Returns the path to install NGINX to
fn nginx_install_dir(base_dir: &Path, target: &str) -> PathBuf {
    let nginx_version = env::var("NGX_VERSION").unwrap_or_else(|_| NGX_DEFAULT_VERSION.to_string());
    base_dir.join(nginx_version).join(target)
}

/// Ensure the correct permissions are applied to the local gnupg directory
fn ensure_gpg_permissions(cache_dir: &Path) -> Result<(), Box<dyn StdError>> {
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

    let gnupghome = cache_dir.join(".gnupg");
    change_permissions_recursively(gnupghome.as_path(), 0o700, 0o600)
        .map_err(|e| Box::new(e) as Box<dyn StdError>)
}

/// Iterates through the tuples in `ALL_SERVERS_AND_PUBLIC_KEY_IDS` and returns a map of
/// key servers to public key IDs.
fn keys_indexed_by_key_server() -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();

    for tuple in ALL_SERVERS_AND_PUBLIC_KEY_IDS {
        let key = tuple.0.to_string();
        let value: Vec<String> = tuple.1.split_whitespace().map(|s| s.to_string()).collect();
        match map.get_mut(&key) {
            Some(keys) => keys.extend(value),
            None => {
                map.insert(key, value);
            }
        }
    }

    map
}

/// Imports all the required GPG keys into the `.cache/.gnupu` directory in order to
/// validate the integrity of the downloaded tarballs.
fn import_gpg_keys(cache_dir: &Path) -> Result<(), Box<dyn StdError>> {
    if let Some(gpg) = gpg_path() {
        // We do not want to mess with the default gpg data for the running user,
        // so we store all gpg data with our cache directory.
        let gnupghome = cache_dir.join(".gnupg");
        if !gnupghome.exists() {
            fs::create_dir_all(&gnupghome)?;
        }
        ensure_gpg_permissions(cache_dir)?;

        for (server, key_ids) in keys_indexed_by_key_server() {
            println!(
                "Importing {} GPG keys for key server: {}",
                key_ids.len(),
                server
            );

            let homedir = gnupghome.clone();
            let homedir_str = homedir.to_string_lossy().to_string();
            let base_args = vec![
                "--homedir",
                homedir_str.as_str(),
                "--keyserver",
                server.as_str(),
                "--recv-keys",
            ];
            let key_ids_str = key_ids.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
            let args = [base_args, key_ids_str].concat();
            let cmd = duct::cmd(&gpg, &args);

            let output = cmd.stderr_to_stdout().stderr_capture().unchecked().run()?;

            if !output.status.success() {
                eprintln!("{}", String::from_utf8_lossy(&output.stdout));
                return Err(format!(
                    "Command: {:?}\nFailed to import GPG keys: {}",
                    cmd,
                    key_ids.join(" ")
                )
                .into());
            }
        }
    }
    Ok(())
}

fn make_cache_dir() -> Result<PathBuf, Box<dyn StdError>> {
    let base_dir = env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env::current_dir().expect("Failed to get current directory"));
    // Choose `.cache` relative to the manifest directory (nginx-sys) as the default cache directory
    // Environment variable `CACHE_DIR` overrides this
    // Recommendation: set env "CACHE_DIR = { value = ".cache", relative = true }" in
    // `.cargo/config.toml` in your project
    let cache_dir = env::var("CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or(base_dir.join(".cache"));
    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir)?;
    }
    Ok(cache_dir)
}

/// Downloads a tarball from the specified URL into the `.cache` directory.
fn download(cache_dir: &Path, url: &str) -> Result<PathBuf, Box<dyn StdError>> {
    fn proceed_with_download(file_path: &Path) -> bool {
        // File does not exist or is zero bytes
        !file_path.exists() || file_path.metadata().is_ok_and(|m| m.len() < 1)
    }
    let filename = url.split('/').next_back().unwrap();
    let file_path = cache_dir.join(filename);
    if proceed_with_download(&file_path) {
        println!("Downloading: {} -> {}", url, file_path.display());
        let mut response = ureq::get(url).call()?;
        let mut reader = response.body_mut().as_reader();
        let mut file = File::create(&file_path)?;
        std::io::copy(&mut reader, &mut file)?;
    }

    if !file_path.exists() {
        return Err(
            format!("Downloaded file was not written to the expected location: {url}",).into(),
        );
    }
    Ok(file_path)
}

/// Validates that a file is a valid GPG signature file.
fn verify_signature_file(cache_dir: &Path, signature_path: &Path) -> Result<(), Box<dyn StdError>> {
    if !signature_path.exists() {
        return Err(Box::new(std::io::Error::new(
            NotFound,
            format!("GPG signature file not found: {}", signature_path.display()),
        )));
    }
    if let Some(gpg) = gpg_path() {
        let gnupghome = cache_dir.join(".gnupg");
        let cmd = cmd!(
            gpg,
            "--homedir",
            &gnupghome,
            "--list-packets",
            signature_path
        );
        let output = cmd.stderr_to_stdout().stdout_capture().unchecked().run()?;

        if !output.status.success() {
            eprintln!("{}", String::from_utf8_lossy(&output.stdout));
            return Err(Box::new(std::io::Error::other(format!(
                "Command: {:?} \nGPG signature file verification failed for signature: {}",
                cmd,
                signature_path.display()
            ))));
        }
    } else {
        println!("GPG not found, skipping signature file verification");
    }
    Ok(())
}

/// Validates the integrity of a tarball file against the cryptographic signature associated with
/// the file.
fn verify_archive_signature(
    cache_dir: &Path,
    archive_path: &Path,
    signature_path: &Path,
) -> Result<(), Box<dyn StdError>> {
    if let Some(gpg) = gpg_path() {
        let gnupghome = cache_dir.join(".gnupg");
        let cmd = cmd!(
            gpg,
            "--homedir",
            &gnupghome,
            "--verify",
            signature_path,
            archive_path
        );
        let output = cmd.stderr_to_stdout().stdout_capture().unchecked().run()?;
        if !output.status.success() {
            eprintln!("{}", String::from_utf8_lossy(&output.stdout));
            return Err(Box::new(std::io::Error::other(format!(
                "Command: {:?}\nGPG signature verification failed of archive failed [{}]",
                cmd,
                archive_path.display()
            ))));
        }
    } else {
        println!("GPG not found, skipping signature verification");
    }
    Ok(())
}

/// Get a given tarball and signature file from a remote URL and copy it to the `.cache` directory.
fn get_archive(
    cache_dir: &Path,
    archive_url: &str,
    signature_url: &str,
) -> Result<PathBuf, Box<dyn StdError>> {
    let signature_path = download(cache_dir, signature_url)?;
    if let Err(e) = verify_signature_file(cache_dir, &signature_path) {
        fs::remove_file(&signature_path)?;
        return Err(e);
    }
    let archive_path = download(cache_dir, archive_url)?;
    match verify_archive_signature(cache_dir, &archive_path, &signature_path) {
        Ok(_) => Ok(archive_path),
        Err(e) => {
            fs::remove_file(&archive_path)?;
            Err(e)
        }
    }
}

/// Extract a tarball into a subdirectory based on the tarball's name under the source base
/// directory.
fn extract_archive(
    archive_path: &Path,
    extract_output_base_dir: &Path,
) -> Result<(String, PathBuf), Box<dyn StdError>> {
    if !extract_output_base_dir.exists() {
        fs::create_dir_all(extract_output_base_dir)?;
    }
    let archive_file = File::open(archive_path)
        .unwrap_or_else(|_| panic!("Unable to open archive file: {}", archive_path.display()));
    let stem = archive_path
        .file_name()
        .and_then(|s| s.to_str())
        .and_then(|s| s.rsplitn(3, '.').last())
        .expect("Unable to determine archive file name stem");
    let dependency_name = stem
        .split_once('-')
        .map(|(s, _)| s.to_owned())
        .unwrap_or_else(|| panic!("Unable to determine dependency name based on stem: {stem}"));

    let extract_output_dir = extract_output_base_dir.to_owned();
    let archive_output_dir = extract_output_dir.join(stem);
    if !archive_output_dir.exists() {
        Archive::new(GzDecoder::new(archive_file))
            .entries()?
            .filter_map(|e| e.ok())
            .for_each(|mut entry| {
                let path = entry.path().unwrap();
                let stripped_path = path.components().skip(1).collect::<PathBuf>();
                entry
                    .unpack(archive_output_dir.join(stripped_path))
                    .unwrap();
            });
    } else {
        println!(
            "Archive [{}] already extracted to directory: {}",
            stem,
            archive_output_dir.display()
        );
    }

    Ok((dependency_name, archive_output_dir))
}

/// Extract all of the tarballs into subdirectories within the source base directory.
fn extract_all_archives(
    cache_dir: &Path,
    target: &str,
) -> Result<Vec<(String, PathBuf)>, Box<dyn StdError>> {
    let archives = all_archives();
    let mut sources = Vec::new();
    let extract_output_base_dir = source_output_dir(cache_dir, target);
    if !extract_output_base_dir.exists() {
        fs::create_dir_all(&extract_output_base_dir)?;
    }
    for (archive_url, signature_url) in archives {
        let archive_path = get_archive(cache_dir, &archive_url, &signature_url)?;
        let (name, output_dir) = extract_archive(&archive_path, &extract_output_base_dir)?;
        sources.push((name, output_dir));
    }

    Ok(sources)
}

/// Invoke external processes to run autoconf `configure` to generate a makefile for NGINX and
/// then run `make install`.
fn compile_nginx(cache_dir: &Path) -> Result<(PathBuf, PathBuf), Box<dyn StdError>> {
    fn find_dependency_path<'a>(
        sources: &'a [(String, PathBuf)],
        name: &str,
    ) -> Result<&'a PathBuf, String> {
        sources
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, p)| p)
            .ok_or(format!("Unable to find dependency [{name}] path"))
    }
    let target = env::var("TARGET")?;
    let nginx_install_base_dir = env::var("NGX_INSTALL_ROOT_DIR")
        .map(PathBuf::from)
        .unwrap_or(cache_dir.join("nginx"));
    let nginx_install_dir = env::var("NGX_INSTALL_DIR")
        .map(PathBuf::from)
        .unwrap_or(nginx_install_dir(&nginx_install_base_dir, &target));
    let sources = extract_all_archives(cache_dir, &target)?;
    let zlib_src_dir = find_dependency_path(&sources, "zlib")?;
    let openssl_src_dir = find_dependency_path(&sources, "openssl")?;
    let pcre2_src_dir =
        find_dependency_path(&sources, "pcre2").or(find_dependency_path(&sources, "pcre"))?;
    let nginx_src_dir = find_dependency_path(&sources, "nginx")?;
    let nginx_configure_flags = nginx_configure_flags(
        &nginx_install_dir,
        zlib_src_dir,
        openssl_src_dir,
        pcre2_src_dir,
    );
    let nginx_binary_exists = nginx_install_dir.join("sbin").join("nginx").exists();
    let autoconf_makefile_exists = nginx_src_dir.join("Makefile").exists();
    // We find out how NGINX was configured last time, so that we can compare it to what
    // we are going to configure it to this time. If there are no changes, then we can assume
    // that we do not need to reconfigure and rebuild NGINX.
    let build_info_path = nginx_src_dir.join("last-build-info");
    let current_build_info = build_info(&nginx_configure_flags);
    let build_info_no_change = if build_info_path.exists() {
        read_to_string(&build_info_path).is_ok_and(|s| s == current_build_info)
    } else {
        false
    };

    println!("NGINX already installed: {nginx_binary_exists}");
    println!("NGINX autoconf makefile already created: {autoconf_makefile_exists}");
    println!("NGINX build info changed: {}", !build_info_no_change);

    if !nginx_binary_exists || !autoconf_makefile_exists || !build_info_no_change {
        fs::create_dir_all(&nginx_install_dir)?;
        configure(nginx_configure_flags, nginx_src_dir)?;
        make(nginx_src_dir, "install")?;
        let mut output = File::create(build_info_path)?;
        // Store the configure flags of the last successful build
        output.write_all(current_build_info.as_bytes())?;
    }
    Ok((nginx_install_dir, nginx_src_dir.to_owned()))
}

/// Returns the options in which NGINX was built with
fn build_info(nginx_configure_flags: &[String]) -> String {
    // Flags should contain strings pointing to OS/platform as well as dependency versions,
    // so if any of that changes, it can trigger a rebuild
    nginx_configure_flags.join(" ")
}

/// Generate the flags to use with autoconf `configure` for NGINX based on the downloaded
/// dependencies' paths. Note: the paths differ based on cargo targets because they may be
/// configured differently for different os/platform targets.
fn nginx_configure_flags(
    nginx_install_dir: &Path,
    zlib_src_dir: &Path,
    openssl_src_dir: &Path,
    pcre2_src_dir: &Path,
) -> Vec<String> {
    fn format_source_path(flag: &str, path: &Path) -> String {
        format!(
            "{}={}",
            flag,
            path.as_os_str()
                .to_str()
                .expect("Unable to read source path as string")
        )
    }
    let modules = || -> Vec<String> {
        let mut modules = vec![
            format_source_path("--with-zlib", zlib_src_dir),
            format_source_path("--with-pcre", pcre2_src_dir),
            format_source_path("--with-openssl", openssl_src_dir),
        ];
        for module in NGX_BASE_MODULES {
            modules.push(module.to_string());
        }
        modules
    };
    let mut nginx_opts = vec![format_source_path("--prefix", nginx_install_dir)];
    if env::var("NGX_DEBUG").is_ok_and(|s| s == "true") {
        println!("Enabling --with-debug");
        nginx_opts.push("--with-debug".to_string());
    }
    if env::var("CARGO_CFG_TARGET_OS").map_or(env::consts::OS == "linux", |s| s == "linux") {
        for flag in NGX_LINUX_ADDITIONAL_OPTS {
            nginx_opts.push(flag.to_string());
        }
    }
    for flag in modules() {
        nginx_opts.push(flag);
    }

    nginx_opts
}

/// Run external process invoking autoconf `configure` for NGINX.
fn configure(nginx_configure_flags: Vec<String>, nginx_src_dir: &Path) -> std::io::Result<Output> {
    let flags = nginx_configure_flags
        .iter()
        .map(OsString::from)
        .collect::<Vec<OsString>>();
    let configure_executable = nginx_src_dir.join("configure");
    if !configure_executable.exists() {
        panic!(
            "Unable to find NGINX configure script at: {}",
            configure_executable.to_string_lossy()
        );
    }
    println!(
        "Running NGINX configure script with flags: {:?}",
        nginx_configure_flags.join(" ")
    );
    duct::cmd(configure_executable, flags)
        .dir(nginx_src_dir)
        .stderr_to_stdout()
        .run()
}

/// Run `make` within the NGINX source directory as an external process.
fn make(nginx_src_dir: &Path, arg: &str) -> std::io::Result<Output> {
    // Give preference to the binary with the name of gmake if it exists because this is typically
    // the GNU 4+ on MacOS (if it is installed via homebrew).
    let make_bin_path = match (which("gmake"), which("make")) {
        (Ok(path), _) => Ok(path),
        (_, Ok(path)) => Ok(path),
        _ => Err(IoError::new(
            NotFound,
            "Unable to find make in path (gmake or make)",
        )),
    }?;

    // Level of concurrency to use when building nginx - cargo nicely provides this information
    let num_jobs = match env::var("NUM_JOBS") {
        Ok(s) => s.parse::<usize>().ok(),
        Err(_) => thread::available_parallelism().ok().map(|n| n.get()),
    }
    .unwrap_or(1);

    /* Use the duct dependency here to merge the output of STDOUT and STDERR into a single stream,
    and to provide the combined output as a reader which can be iterated over line-by-line. We
    use duct to do this because it is a lot of work to implement this from scratch. */
    cmd!(make_bin_path, "-j", num_jobs.to_string(), arg)
        .dir(nginx_src_dir)
        .stderr_to_stdout()
        .run()
}
