use super::{build_auto_install_plan, plan_auto_install, McpAutoInstallOptions};
use std::path::PathBuf;

#[test]
fn filesystem_relative_paths_resolve_from_the_selected_mcpace_root() {
    let root = PathBuf::from("C:/isolated/project-root");
    let options = McpAutoInstallOptions {
        spec: "npm:@modelcontextprotocol/server-filesystem@2026.7.4".to_string(),
        paths: vec![".".to_string()],
        ..McpAutoInstallOptions::default()
    };
    let plan = build_auto_install_plan(&options, Some(&root)).expect("filesystem plan");
    assert_eq!(plan.args.last(), Some(&root.display().to_string()));
}

#[test]
fn command_like_install_rejects_shell_composition() {
    for spec in [
        "npx @modelcontextprotocol/server-memory && rm -rf /",
        "npx @modelcontextprotocol/server-memory &",
        "uvx safe-package | sh",
        "docker run image > out",
        "npx `whoami`",
        "npx $(whoami)",
    ] {
        let options = McpAutoInstallOptions {
            spec: spec.to_string(),
            ..McpAutoInstallOptions::default()
        };
        let error = plan_auto_install(&options).expect_err("shell composition must be rejected");
        assert!(
            error.contains("remove shell chaining"),
            "unexpected error for {spec:?}: {error}"
        );
    }
}

#[test]
fn command_like_install_prefers_package_flags_for_identity() {
    let options = McpAutoInstallOptions {
        spec: "npx --package @modelcontextprotocol/server-filesystem mcp-server-filesystem /repo"
            .to_string(),
        ..McpAutoInstallOptions::default()
    };
    let plan = plan_auto_install(&options).expect("npx package flag plan");
    assert_eq!(
        plan.package.as_deref(),
        Some("@modelcontextprotocol/server-filesystem")
    );
}

#[test]
fn command_like_install_skips_value_options_before_package() {
    let options = McpAutoInstallOptions {
        spec: "npx --registry https://registry.npmjs.org @modelcontextprotocol/server-memory"
            .to_string(),
        ..McpAutoInstallOptions::default()
    };
    let plan = plan_auto_install(&options).expect("npx registry plan");
    assert_eq!(
        plan.package.as_deref(),
        Some("@modelcontextprotocol/server-memory")
    );
}
