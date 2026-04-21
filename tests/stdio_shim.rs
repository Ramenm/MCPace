mod common;

use common::*;
use std::fs;

fn write_minimal_config(root: &std::path::Path) {
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.0",
  "client": {
    "keyName": "codex-local"
  },
  "servers": {}
}"#,
    )
    .unwrap();
}

#[test]
fn stdio_shim_json_bootstraps_hub_and_reports_adapter_contract() {
    let temp = TempDir::new();
    let root = temp.path();
    write_minimal_config(root);

    let output = run(&[
        "stdio-shim",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--client-id",
        "codex",
        "--session-id",
        "shim-1",
        "--project-root",
        root.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains(r#""mode": "bootstrap-only""#), "stdout: {}", text);
    assert!(text.contains(r#""hubBootstrapSucceeded": true"#), "stdout: {}", text);
    assert!(text.contains(r#""canForwardMcpToday": false"#), "stdout: {}", text);
    assert!(text.contains(r#""sessionLeaseId": "external:shim-1""#), "stdout: {}", text);
    assert!(text.contains(r#""clientPlan": {"#), "stdout: {}", text);
    assert!(text.contains(r#""adapterPreview": {"#), "stdout: {}", text);
    assert!(text.contains(r#""hubStatus": {"#), "stdout: {}", text);
    assert!(text.contains(r#""type": "stdio-launcher""#), "stdout: {}", text);

    let down = run(&[
        "hub",
        "down",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(down.status.success(), "stderr: {}", stderr(&down));
}

#[test]
fn stdio_shim_without_json_fails_clearly_as_bootstrap_only() {
    let temp = TempDir::new();
    let root = temp.path();
    write_minimal_config(root);

    let output = run(&["stdio-shim", "--root", root.to_str().unwrap()]);
    assert!(!output.status.success());
    let text = stderr(&output);
    assert!(text.contains("bootstrap-only proof surface"), "stderr: {}", text);
    assert!(text.contains("Live MCP stdio forwarding is not implemented yet"), "stderr: {}", text);
}
