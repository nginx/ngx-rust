/// Example buildscript for an nginx module.
///
/// Due to the limitations of cargo[1], this buildscript _requires_ adding `nginx-sys` to the
/// direct dependencies of your crate.
///
/// [1]: https://github.com/rust-lang/cargo/issues/3544
fn main() {
    // Generate `ngx_os` and `ngx_feature` cfg values

    // Specify acceptable values for `ngx_feature`
    println!("cargo::rerun-if-env-changed=DEP_NGINX_FEATURES_CHECK");
    println!(
        "cargo::rustc-check-cfg=cfg(ngx_feature, values({}))",
        std::env::var("DEP_NGINX_FEATURES_CHECK").unwrap_or("any()".to_string())
    );
    // Read feature flags detected by nginx-sys and pass to the compiler.
    println!("cargo::rerun-if-env-changed=DEP_NGINX_FEATURES");
    if let Ok(features) = std::env::var("DEP_NGINX_FEATURES") {
        for feature in features.split(',').map(str::trim) {
            println!("cargo::rustc-cfg=ngx_feature=\"{}\"", feature);
        }
    }

    // Specify acceptable values for `ngx_os`
    println!("cargo::rerun-if-env-changed=DEP_NGINX_OS_CHECK");
    println!(
        "cargo::rustc-check-cfg=cfg(ngx_os, values({}))",
        std::env::var("DEP_NGINX_OS_CHECK").unwrap_or("any()".to_string())
    );
    // Read operating system detected by nginx-sys and pass to the compiler.
    println!("cargo::rerun-if-env-changed=DEP_NGINX_OS");
    if let Ok(os) = std::env::var("DEP_NGINX_OS") {
        println!("cargo::rustc-cfg=ngx_os=\"{}\"", os);
    }

    // Generate cfg values for version checks
    // println!("cargo::rustc-check-cfg=cfg(nginx1_27_0)");
    // println!("cargo::rerun-if-env-changed=DEP_NGINX_VERSION_NUMBER");
    // if let Ok(version) = std::env::var("DEP_NGINX_VERSION_NUMBER") {
    //     let version: u64 = version.parse().unwrap();
    //
    //     if version >= 1_027_000 {
    //         println!("cargo::rustc-cfg=nginx1_27_0");
    //     }
    // }

    // Generate required compiler flags
    if cfg!(target_os = "macos") {
        // https://stackoverflow.com/questions/28124221/error-linking-with-cc-failed-exit-code-1
        println!("cargo::rustc-link-arg=-undefined");
        println!("cargo::rustc-link-arg=dynamic_lookup");
    }
}
