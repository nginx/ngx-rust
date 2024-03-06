extern crate bindgen;

use std::env;
use std::error::Error as StdError;
use std::fs::read_to_string;
use std::path::PathBuf;

#[cfg(feature = "vendored")]
mod vendored;

const ENV_VARS_TRIGGERING_RECOMPILE: [&str; 2] = ["OUT_DIR", "NGX_OBJS"];

/// Function invoked when `cargo build` is executed.
/// This function will download NGINX and all supporting dependencies, verify their integrity,
/// extract them, execute autoconf `configure` for NGINX, compile NGINX and finally install
/// NGINX in a subdirectory with the project.
fn main() -> Result<(), Box<dyn StdError>> {
    let nginx_build_dir = match std::env::var("NGX_OBJS") {
        Ok(v) => PathBuf::from(v).canonicalize()?,
        #[cfg(feature = "vendored")]
        Err(_) => vendored::build()?,
        #[cfg(not(feature = "vendored"))]
        Err(_) => panic!("\"nginx-sys/vendored\" feature is disabled and NGX_OBJS is not specified"),
    };
    // Hint cargo to rebuild if any of the these environment variables values change
    // because they will trigger a recompilation of NGINX with different parameters
    for var in ENV_VARS_TRIGGERING_RECOMPILE {
        println!("cargo:rerun-if-env-changed={var}");
    }
    println!("cargo:rerun-if-changed=build/main.rs");
    println!("cargo:rerun-if-changed=build/wrapper.h");
    // Read autoconf generated makefile for NGINX and generate Rust bindings based on its includes
    generate_binding(nginx_build_dir);
    Ok(())
}

/// Generates Rust bindings for NGINX
fn generate_binding(nginx_build_dir: PathBuf) {
    let autoconf_makefile_path = nginx_build_dir.join("Makefile");
    let clang_args: Vec<String> = parse_includes_from_makefile(&autoconf_makefile_path)
        .into_iter()
        .map(|path| format!("-I{}", path.to_string_lossy()))
        .collect();

    let bindings = bindgen::Builder::default()
        // Bindings will not compile on Linux without block listing this item
        // It is worth investigating why this is
        .blocklist_item("IPPORT_RESERVED")
        // The input header we would like to generate bindings for.
        .header("build/wrapper.h")
        .clang_args(clang_args)
        .layout_tests(false)
        .generate()
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_dir_env = env::var("OUT_DIR").expect("The required environment variable OUT_DIR was not set");
    let out_path = PathBuf::from(out_dir_env);
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}

/// Reads through the makefile generated by autoconf and finds all of the includes
/// used to compile nginx. This is used to generate the correct bindings for the
/// nginx source code.
fn parse_includes_from_makefile(nginx_autoconf_makefile_path: &PathBuf) -> Vec<PathBuf> {
    fn extract_include_part(line: &str) -> &str {
        line.strip_suffix('\\').map_or(line, |s| s.trim())
    }
    /// Extracts the include path from a line of the autoconf generated makefile.
    fn extract_after_i_flag(line: &str) -> Option<&str> {
        let mut parts = line.split("-I ");
        match parts.next() {
            Some(_) => parts.next().map(extract_include_part),
            None => None,
        }
    }

    let mut includes = vec![];
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

    let mut includes_lines = false;
    for line in makefile_contents.lines() {
        if !includes_lines {
            if let Some(stripped) = line.strip_prefix("ALL_INCS") {
                includes_lines = true;
                if let Some(part) = extract_after_i_flag(stripped) {
                    includes.push(part);
                }
                continue;
            }
        }

        if includes_lines {
            if let Some(part) = extract_after_i_flag(line) {
                includes.push(part);
            } else {
                break;
            }
        }
    }

    let makefile_dir = nginx_autoconf_makefile_path
        .parent()
        .expect("makefile path has no parent")
        .parent()
        .expect("objs dir has no parent")
        .to_path_buf()
        .canonicalize()
        .expect("Unable to canonicalize makefile path");

    includes
        .into_iter()
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                makefile_dir.join(path)
            }
        })
        .collect()
}
