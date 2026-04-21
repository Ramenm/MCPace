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
    assert!(text.contains("server capabilities"));
    assert!(text.contains("verify readiness"));
    assert!(text.contains("client list"));
    assert!(text.contains("client plan"));
    assert!(text.contains("stdio-shim"));
    assert!(text.contains("lab report"));
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
  "version": "0.3.0",
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
    assert!(repair_text.contains(r#""hubStatus""#), "stdout: {}", repair_text);
    assert!(repair_text.contains(r#""status": "stopped""#), "stdout: {}", repair_text);
    assert!(
        fs::read_dir(&hub_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .any(|name| name.starts_with("state.json.corrupt-")),
        "hub dir entries were not repaired"
    );
}
