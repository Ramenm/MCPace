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
    let _env_lock = crate::resources::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
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

#[test]
fn public_help_is_small_and_hides_transport_entrypoints() {
    let mut stdout = Vec::new();
    let status = run(vec!["help".to_string()], &mut stdout, &mut Vec::new());
    let output = String::from_utf8(stdout).unwrap();

    assert_eq!(status, 0);
    for command in [
        "up",
        "start",
        "stop",
        "restart",
        "status",
        "install",
        "uninstall",
        "advanced",
    ] {
        assert!(
            output.contains(&format!("  {command}")),
            "missing {command}"
        );
    }
    for hidden in ["stdio", "stdio-shim", "agent", "serve", "hub", "mcp-server"] {
        assert!(
            !output
                .lines()
                .any(|line| line.starts_with(&format!("  {hidden}"))),
            "hidden entrypoint leaked into help: {hidden}"
        );
    }
}

#[test]
fn removed_top_level_aliases_fail_instead_of_changing_meaning() {
    for removed in [
        "setup",
        "quickstart",
        "bootstrap",
        "one-click",
        "auto",
        "server",
        "client",
        "autostart",
        "service",
        "doctor",
        "dashboard",
        "connect",
        "lab",
        "release",
    ] {
        let mut stderr = Vec::new();
        let status = run(vec![removed.to_string()], &mut Vec::new(), &mut stderr);
        assert_eq!(status, 2, "removed command unexpectedly routed: {removed}");
        assert!(String::from_utf8(stderr)
            .unwrap()
            .contains("unknown command"));
    }
}

#[test]
fn advanced_help_groups_operator_and_maintainer_commands() {
    let mut stdout = Vec::new();
    let status = run(
        vec!["advanced".to_string(), "--help".to_string()],
        &mut stdout,
        &mut Vec::new(),
    );
    let output = String::from_utf8(stdout).unwrap();

    assert_eq!(status, 0);
    assert!(output.contains("advanced <command>"));
    assert!(output.contains("autostart"));
    assert!(output.contains("runtime"));
    assert!(output.contains("dev"));
}

#[test]
fn essential_generated_config_entrypoints_remain_callable_but_hidden() {
    for command in ["stdio", "stdio-shim", "mcp-server", "agent", "serve", "hub"] {
        assert!(find(command).is_some(), "missing hidden route: {command}");
    }
    assert!(find("stdio_shim").is_none());
    assert!(find("mcp_server").is_none());
}

#[test]
fn installed_compatibility_commands_still_parse_without_becoming_public() {
    let cases = [
        vec!["stdio", "--help"],
        vec!["stdio-shim", "--help"],
        vec!["mcp-server", "--help"],
        vec!["agent", "run", "--autostart", "--help"],
        vec!["serve", "--managed-service", "--help"],
    ];

    for args in cases {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let status = run(
            args.iter().map(|value| (*value).to_string()).collect(),
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(status, 0, "compatibility command failed: {args:?}");
        assert!(stderr.is_empty(), "unexpected stderr for {args:?}");
    }

    let mut stdout = Vec::new();
    assert_eq!(
        run(
            vec!["serve".into(), "--help".into()],
            &mut stdout,
            &mut Vec::new(),
        ),
        0
    );
    assert!(!String::from_utf8(stdout)
        .unwrap()
        .contains("--managed-service"));
}

#[test]
fn internal_rust_callers_are_migrated_without_reopening_removed_shell_commands() {
    assert_eq!(
        canonical_internal_args(vec!["server".into(), "list".into(), "--json".into()]),
        vec!["advanced", "server", "list", "--json"]
    );
    assert_eq!(
        canonical_internal_args(vec!["verify".into(), "readiness".into(), "--json".into()]),
        vec!["advanced", "doctor", "readiness", "--json"]
    );
    assert_eq!(
        canonical_internal_args(vec!["lab".into(), "report".into(), "--json".into()]),
        vec!["advanced", "dev", "lab", "report", "--json"]
    );
}
