#[cfg(test)]
mod tests {
    use std::env;

    use ngx::test_util::NginxBuilder;

    const TEST_NGINX_CONFIG: &str = "tests/nginx.conf";

    #[test]
    fn test() {
        let mut nginx = NginxBuilder::default().build();

        let current_dir = env::current_dir().expect("Unable to get current directory");
        let test_config_path = current_dir.join(TEST_NGINX_CONFIG);

        assert!(
            test_config_path.exists(),
            "Config file not found: {}\nCurrent directory: {}",
            test_config_path.to_string_lossy(),
            current_dir.to_string_lossy()
        );

        nginx
            .copy_main_config(&test_config_path)
            .unwrap_or_else(|_| panic!("Unable to load config file: {}", test_config_path.to_string_lossy()));
        let output = nginx.restart().expect("Unable to restart NGINX");
        assert!(output.status.success());

        let output = nginx.stop().expect("Unable to stop NGINX");
        assert!(output.status.success());
    }
}
