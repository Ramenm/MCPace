use super::*;
use std::ffi::OsString;
use std::fs;

struct EnvGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &std::path::Path) -> Self {
        let previous = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

#[test]
fn version_reports_binary_version_not_project_config_version() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "mcpace-version-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{ "version": "999.999.999" }"#,
    )
    .unwrap();
    let _root_env = EnvGuard::set("MCPACE_ROOT", &root);

    let mut stdout = Vec::new();
    let status = run(vec!["--version".to_string()], &mut stdout, &mut Vec::new());

    assert_eq!(status, 0);
    assert_eq!(
        String::from_utf8(stdout).unwrap().trim(),
        env!("CARGO_PKG_VERSION")
    );

    let _ = fs::remove_dir_all(root);
}
