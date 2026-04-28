mod common;

use common::*;
use std::fs;

#[test]
fn help_mentions_grouped_native_read_paths() {
    let output = run(&["help"]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains("doctor/profile/projects/candidates/client-plan/lab/server/verify have native Rust read paths"));
    assert!(text.contains("repair [--json] [--root <path>]"));
    assert!(text.contains("release [build] [--json] [--root <path>]"));
    assert!(text.contains("setup [--json]"));
    assert!(text.contains("setup starts the one-port MCPace endpoint"));
    assert!(text.contains("service install|status|uninstall|print"));
    assert!(text.contains("service installs user-level autostart entries"));
    assert!(text.contains("server capabilities"));
    assert!(text.contains("verify readiness"));
    assert!(text.contains("dashboard"));
    assert!(text.contains("serve"));
    assert!(text.contains("serve start|stop|status"));
    assert!(text.contains("client list"));
    assert!(text.contains("client plan"));
    assert!(text.contains("client install"));
    assert!(text.contains("stdio-shim"));
    assert!(text.contains("mcp-server"));
    assert!(text.contains("lab report"));
    assert!(text.contains("release build now wraps the local artifact/proof bundle"));
}

#[test]
fn version_uses_mcpace_root_env_override_before_repo_scan() {
    let temp = TempDir::new();
    let root = temp.path();

    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "9.9.9-test"
}"#,
    )
    .unwrap();

    let output = run_with_envs(&["version"], &[("MCPACE_ROOT", root)]);
    assert!(output.status.success());
    assert_eq!(stdout(&output).trim(), "9.9.9-test");
}

#[test]
fn check_alias_routes_to_grouped_verify_doctor() {
    let output = run(&["check"]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains("Project root:"));
    assert!(text.contains("Tools:"));
}

#[test]
fn smoke_command_still_fails_clearly_as_planned() {
    let output = run(&["smoke"]);
    assert!(!output.status.success());
    let text = stderr(&output);
    assert!(text.contains("not implemented yet in the Rust-only repo"));
}

#[test]
fn repair_command_routes_to_grouped_hub_repair() {
    let temp = TempDir::new();
    let root = temp.path();

    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "profiles": {
    "runtime": {
      "default": "safe",
      "profiles": {
        "safe": { "description": "Safe", "serverOverrides": {} }
      }
    }
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let hub_dir = root.join("data").join("runtime").join("hub");
    fs::create_dir_all(&hub_dir).unwrap();
    fs::write(hub_dir.join("state.json"), "{ not-valid-json").unwrap();

    let repair = run(&["repair", "--json", "--root", root.to_str().unwrap()]);
    assert!(repair.status.success(), "stderr: {}", stderr(&repair));
    let repair_text = stdout(&repair);
    assert!(
        repair_text.contains(r#""hubStatus""#),
        "stdout: {}",
        repair_text
    );
    assert!(
        repair_text.contains(r#""status": "stopped""#),
        "stdout: {}",
        repair_text
    );
    assert!(
        fs::read_dir(&hub_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .any(|name| name.starts_with("state.json.corrupt-")),
        "hub dir entries were not repaired"
    );
}

#[test]
fn release_help_is_honest_about_local_artifacts_only() {
    let output = run(&["release", "--help"]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains("Usage: mcpace release [build] [--json] [--root <path>]"));
    assert!(text.contains("Build local release artifacts"));
    assert!(text.contains("does not publish to npm or GitHub"));
}

#[test]
fn release_build_json_fails_before_node_when_script_is_missing() {
    let temp = TempDir::new();
    let output = run(&[
        "release",
        "build",
        "--json",
        "--root",
        temp.path().to_str().unwrap(),
    ]);
    assert!(!output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""status": "failed""#));
    assert!(text.contains("release artifact script"));
    assert!(text.contains("scripts/build-release-artifacts.mjs"));
}
