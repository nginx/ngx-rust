extern crate duct;

use std::error::Error as StdError;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::{env, fs};

use flate2::read::GzDecoder;
use tar::Archive;

use crate::verifier::SignatureVerifier;

const NGINX_URL_PREFIX: &str = "https://nginx.org/download";
const OPENSSL_URL_PREFIX: &str = "https://github.com/openssl/openssl/releases/download";
const PCRE1_URL_PREFIX: &str = "https://sourceforge.net/projects/pcre/files/pcre";
const PCRE2_URL_PREFIX: &str = "https://github.com/PCRE2Project/pcre2/releases/download";
const ZLIB_URL_PREFIX: &str = "https://github.com/madler/zlib/releases/download";
const UBUNTU_KEYSEVER: &str = "hkps://keyserver.ubuntu.com";

struct SourceSpec<'a> {
    url: fn(&str) -> String,
    variable: &'a str,
    signature: &'a str,
    keyserver: &'a str,
    key_ids: &'a [&'a str],
}

const NGINX_SOURCE: SourceSpec = SourceSpec {
    url: |version| format!("{NGINX_URL_PREFIX}/nginx-{version}.tar.gz"),
    variable: "NGX_VERSION",
    signature: "asc",
    keyserver: UBUNTU_KEYSEVER,
    key_ids: &[
        // Key 1: Konstantin Pavlov's public key. For Nginx 1.25.3 and earlier
        "13C82A63B603576156E30A4EA0EA981B66B0D967",
        // Key 2: Sergey Kandaurov's public key. For Nginx 1.25.4
        "D6786CE303D9A9022998DC6CC8464D549AF75C0A",
        // Key 3: Maxim Dounin's public key. At least used for Nginx 1.18.0
        "B0F4253373F8F6F510D42178520A9993A1C052F8",
        // Key 4: Roman Arutyunyan's public key. For Nginx 1.25.5
        "43387825DDB1BB97EC36BA5D007C8D7C15D87369",
    ],
};

const DEPENDENCIES: &[(&str, SourceSpec)] = &[
    (
        "openssl",
        SourceSpec {
            url: |version| {
                if version.starts_with("1.") {
                    let ver_hyphened = version.replace('.', "_");
                    format!("{OPENSSL_URL_PREFIX}/OpenSSL_{ver_hyphened}/openssl-{version}.tar.gz")
                } else {
                    format!("{OPENSSL_URL_PREFIX}/openssl-{version}/openssl-{version}.tar.gz")
                }
            },
            variable: "OPENSSL_VERSION",
            signature: "asc",
            keyserver: UBUNTU_KEYSEVER,
            key_ids: &[
                "EFC0A467D613CB83C7ED6D30D894E2CE8B3D79F5",
                "A21FAB74B0088AA361152586B8EF1A6BA9DA2D5C",
                "8657ABB260F056B1E5190839D9C4D26D0E604491",
                "B7C1C14360F353A36862E4D5231C84CDDCC69C45",
                "95A9908DDFA16830BE9FB9003D30A3A9FF1360DC",
                "7953AC1FBC3DC8B3B292393ED5E9E43F7DF9EE8C",
                "E5E52560DD91C556DDBDA5D02064C53641C25E5D",
                "C1F33DD8CE1D4CC613AF14DA9195C48241FBF7DD",
                "BA5473A2B0587B07FB27CF2D216094DFD0CB81EF",
            ],
        },
    ),
    (
        "pcre",
        SourceSpec {
            url: |version| {
                // We can distinguish pcre1/pcre2 by checking whether the second character is '.',
                // because the final version of pcre1 is 8.45 and the first one of pcre2 is 10.00.
                if version.chars().nth(1).is_some_and(|c| c == '.') {
                    format!("{PCRE1_URL_PREFIX}/{version}/pcre-{version}.tar.gz")
                } else {
                    format!("{PCRE2_URL_PREFIX}/pcre2-{version}/pcre2-{version}.tar.gz")
                }
            },
            variable: "PCRE2_VERSION",
            signature: "sig",
            keyserver: UBUNTU_KEYSEVER,
            key_ids: &[
                // Key 1: Phillip Hazel's public key. For PCRE2 10.44 and earlier
                "45F68D54BBE23FB3039B46E59766E084FB0F43D8",
                // Key 2: Nicholas Wilson's public key. For PCRE2 10.45
                "A95536204A3BB489715231282A98E77EB6F24CA8",
            ],
        },
    ),
    (
        "zlib",
        SourceSpec {
            url: |version| format!("{ZLIB_URL_PREFIX}/v{version}/zlib-{version}.tar.gz"),
            variable: "ZLIB_VERSION",
            signature: "asc",
            keyserver: UBUNTU_KEYSEVER,
            key_ids: &[
                // Key 1: Mark Adler's public key. For zlib 1.3.1 and earlier
                "5ED46A6721D365587791E2AA783FCD8E58BCAFBA",
            ],
        },
    ),
];

static VERIFIER: LazyLock<Option<SignatureVerifier>> = LazyLock::new(|| {
    SignatureVerifier::new()
        .inspect_err(|err| eprintln!("GnuPG verifier: {err}"))
        .ok()
});

static CACHE_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let base_dir = env::var("OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env::current_dir().expect("Failed to get current directory"));
    // Choose `.cache` relative to the OUT_DIR of the caller (nginx-sys) as the default cache directory
    // Environment variable `CACHE_DIR` overrides this
    // Recommendation: set env "CACHE_DIR = { value = ".cache", relative = true }" in
    // `.cargo/config.toml` in your project
    let cache_dir = env::var("CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or(base_dir.join(".cache"));

    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir)
            .map_err(|err| format!("Failed to create {cache_dir:?}: {err}"))
            .unwrap();
    }

    cache_dir
});

/// Downloads a tarball from the specified URL into the `.cache` directory.
fn download(cache_dir: &Path, url: &str) -> Result<PathBuf, Box<dyn StdError + Send + Sync>> {
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

/// Gets a given tarball and signature file from a remote URL and copies it to the `.cache`
/// directory.
fn get_archive(cache_dir: &Path, source: &SourceSpec, version: &str) -> io::Result<PathBuf> {
    let archive_url = (source.url)(version);
    let archive = download(cache_dir, &archive_url).map_err(io::Error::other)?;

    if let Some(verifier) = &*VERIFIER {
        let signature = format!("{archive_url}.{}", source.signature);

        let verify = || -> io::Result<()> {
            let signature = download(cache_dir, &signature).map_err(io::Error::other)?;
            verifier.import_keys(source.keyserver, source.key_ids)?;
            verifier.verify_signature(&archive, &signature)?;
            Ok(())
        };

        if let Err(err) = verify() {
            let _ = fs::remove_file(&archive);
            let _ = fs::remove_file(&signature);
            return Err(err);
        }
    }

    Ok(archive)
}

/// Extracts a tarball into a subdirectory based on the tarball's name under the source base
/// directory.
fn extract_archive(archive_path: &Path, extract_output_base_dir: &Path) -> io::Result<PathBuf> {
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

    Ok(archive_output_dir)
}

/// Downloads and extracts all requested sources.
pub fn prepare(source_dir: &Path, build_dir: &Path) -> io::Result<(PathBuf, Vec<String>)> {
    let extract_output_base_dir = build_dir.join("lib");
    if !extract_output_base_dir.exists() {
        fs::create_dir_all(&extract_output_base_dir)?;
    }

    let mut options = vec![];

    // Download NGINX only if NGX_VERSION is set.
    let source_dir = if let Ok(version) = env::var(NGINX_SOURCE.variable) {
        let archive_path = get_archive(&CACHE_DIR, &NGINX_SOURCE, version.as_str())?;
        let output_base_dir: PathBuf = env::var("OUT_DIR").unwrap().into();
        extract_archive(&archive_path, &output_base_dir)?
    } else {
        source_dir.to_path_buf()
    };

    for (name, source) in DEPENDENCIES {
        // Download dependencies if a corresponding DEPENDENCY_VERSION is set.
        let Ok(requested) = env::var(source.variable) else {
            continue;
        };

        let archive_path = get_archive(&CACHE_DIR, source, &requested)?;
        let output_dir = extract_archive(&archive_path, &extract_output_base_dir)?;
        let output_dir = output_dir.to_string_lossy();
        options.push(format!("--with-{name}={output_dir}"));
    }

    Ok((source_dir, options))
}
