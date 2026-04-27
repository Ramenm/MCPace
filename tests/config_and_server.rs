mod common;

use common::*;
use std::fs;
use std::process::Command;

#[test]
fn server_list_json_applies_profile_override_and_case_folded_source_settings() {
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
        "safe": { "description": "Safe", "serverOverrides": {} },
        "full": { "description": "Full", "serverOverrides": { "Browser": { "enabled": true } } }
      }
    }
  },
  "servers": {
    "Browser": {
      "kind": "host-bridge",
      "required": false,
      "defaultEnabled": false,
      "requiredCommands": ["node"],
      "policy": {
        "scopeClass": "shared-global",
        "concurrencyPolicy": "single-writer",
        "stateBinding": "host-session",
        "credentialBinding": "none"
      },
      "installer": {
        "installTarget": "none",
        "installMethod": "none",
        "installPackage": "",
        "verifyCommand": ""
      }
    }
  }
}"#,
    )
    .unwrap();
    fs::write(
        root.join("mcp_settings.json"),
        r#"{
  "mcpServers": {
    "browser": {
      "enabled": true,
      "type": "http",
      "url": "http://127.0.0.1:39022/mcp"
    }
  }
}"#,
    )
    .unwrap();

    let output = Command::new(bin_path())
        .env("MCPACE_RUNTIME_PROFILE", "full")
        .args(["server", "list", "--json", "--root", root.to_str().unwrap()])
        .output()
        .expect("run mcpace server list with runtime profile env");
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""name": "Browser""#));
    assert!(text.contains(r#""profileEnabled": true"#));
    assert!(text.contains(r#""sourceEnabled": true"#));
    assert!(text.contains(r#""effectiveEnabled": true"#));
}

#[test]
fn profile_json_reads_legacy_settings_from_root_override() {
    let temp = TempDir::new();
    let root = temp.path();

    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "profiles": {
    "runtime": {
      "default": "safe",
      "profiles": {
        "safe": { "description": "Safe default", "serverOverrides": {} },
        "labs": { "description": "Labs profile", "serverOverrides": { "time": { "enabled": true } } }
      }
    }
  }
}"#,
    )
    .unwrap();
    fs::write(
        root.join("manager.settings.json"),
        r#"{ "runtimeProfile": { "active": "labs" } }"#,
    )
    .unwrap();

    let output = run(&[
        "profile",
        "show",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""activeProfile": "labs""#));
    assert!(text.contains(r#""selectionSource": "legacy-settings""#));
}

#[test]
fn server_capabilities_json_reads_config_and_source_settings() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.2.0",
  "servers": {
    "browser": {
      "kind": "host-bridge",
      "required": true,
      "autoStart": true,
      "supportedTransports": ["http", "stdio-http-bridge"],
      "platforms": ["linux", "macos"],
      "requiredCommands": ["node"],
      "transportPreference": "http",
      "healthUrl": "http://127.0.0.1:39022/health",
      "policy": {
        "scopeClass": "shared-global",
        "concurrencyPolicy": "single-writer",
        "stateBinding": "host-session",
        "credentialBinding": "none"
      },
      "installer": {
        "installTarget": "none",
        "installMethod": "none",
        "installPackage": "",
        "verifyCommand": ""
      }
    }
  }
}"#,
    )
    .unwrap();
    fs::write(
        root.join("mcp_settings.json"),
        r#"{
  "mcpServers": {
    "browser": {
      "enabled": true,
      "type": "http",
      "url": "http://localhost:39022/mcp"
    }
  }
}"#,
    )
    .unwrap();

    let output = run(&[
        "server",
        "capabilities",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--name",
        "browser",
    ]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""name": "browser""#));
    assert!(text.contains(r#""sourceEnabled": true"#));
    assert!(text.contains(r#""supportedTransports": ["#));
}

#[test]
fn candidates_json_reads_catalog_from_root_override() {
    let temp = TempDir::new();
    let root = temp.path();

    fs::write(root.join("mcpace.config.json"), "{}\n").unwrap();
    fs::write(
        root.join("server-candidates.json"),
        r#"[
  {
    "name": "time",
    "status": "candidate",
    "priority": "high",
    "upstreamType": "reference-official",
    "integrationSource": "https://example.invalid/time",
    "scopeClass": "shared-global",
    "concurrencyPolicy": "multi-reader",
    "stateBinding": "ephemeral",
    "credentialBinding": "none",
    "why": "Safe baseline utility server.",
    "evaluationNotes": "Good first candidate."
  }
]"#,
    )
    .unwrap();

    let output = run(&[
        "server",
        "candidates",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""name": "time""#));
    assert!(text.contains(r#""priority": "high""#));
}

#[test]
fn projects_json_reads_registry_from_state_root_override_env() {
    let manager_root = TempDir::new();
    let project_root = TempDir::new();
    let state_root = TempDir::new();
    let runtime_dir = state_root.path().join("data").join("runtime");
    fs::create_dir_all(&runtime_dir).unwrap();

    fs::write(manager_root.path().join("mcpace.config.json"), "{}\n").unwrap();
    let escaped_host_path = project_root
        .path()
        .display()
        .to_string()
        .replace('\\', "\\\\");
    fs::write(
        runtime_dir.join("project-registry.json"),
        format!(
            r#"{{
  "version": 1,
  "projects": {{
    "abc123": {{
      "projectId": "abc123",
      "name": "Example",
      "hostPath": "{}",
      "detectedType": "node",
      "markers": ["package.json"],
      "lastUsedAt": "2026-04-15T12:00:00Z",
      "state": "active"
    }}
  }}
}}"#,
            escaped_host_path
        ),
    )
    .unwrap();

    let output = run_with_env(
        &[
            "projects",
            "list",
            "--json",
            "--root",
            manager_root.path().to_str().unwrap(),
        ],
        "MCPACE_STATE_ROOT",
        state_root.path(),
    );
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""Name": "Example""#));
    assert!(text.contains(r#""DetectedType": "node""#));
}

#[test]
fn verify_readiness_json_tracks_container_runtime_prerequisites_honestly() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.2.0",
  "servers": {
    "filesystem": {
      "kind": "container-stdio",
      "required": true,
      "requiredCommands": ["node"],
      "policy": {
        "scopeClass": "shared-global",
        "concurrencyPolicy": "multi-reader",
        "stateBinding": "workspace-roots",
        "credentialBinding": "none"
      },
      "installer": {
        "installTarget": "none",
        "installMethod": "none",
        "installPackage": "",
        "verifyCommand": ""
      }
    }
  }
}"#,
    )
    .unwrap();
    fs::write(
        root.join("mcp_settings.json"),
        r#"{ "mcpServers": { "filesystem": { "enabled": true, "type": "stdio", "command": "node" } } }"#,
    )
    .unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='x'\nversion='0.1.0'\n",
    )
    .unwrap();
    fs::write(root.join("package.json"), "{}\n").unwrap();
    fs::create_dir_all(root.join("packages")).unwrap();
    fs::write(root.join("release-manifest.json"), "{}\n").unwrap();

    let output = run(&[
        "verify",
        "readiness",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""requiredServerCount": 1"#));
    assert!(text.contains(r#""requiredSourceEnabledCount": 1"#));
    assert!(text.contains(r#""readyForReadOnlyOps": true"#));
    let container_tooling_ready = text.contains(r#""containerToolingReady": true"#);
    let runtime_prerequisites_ready = text.contains(r#""runtimePrerequisitesReady": true"#);
    assert_eq!(
        runtime_prerequisites_ready, container_tooling_ready,
        "stdout: {}",
        text
    );
}
