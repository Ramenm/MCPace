mod common;

use common::*;
use mcpace::client_catalog;
use std::fs;
use std::process::Command;

#[test]
fn client_plan_json_uses_metadata_and_arbitrates_unsafe_servers() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "servers": {
    "interactive-demo": {
      "kind": "host-bridge",
      "required": true,
      "transportPreference": "http",
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
    },
    "filesystem": {
      "kind": "container-stdio",
      "required": true,
      "policy": {
        "scopeClass": "project-local",
        "concurrencyPolicy": "isolated-per-project",
        "stateBinding": "workspace-roots",
        "credentialBinding": "none"
      },
      "installer": {
        "installTarget": "none",
        "installMethod": "none",
        "installPackage": "",
        "verifyCommand": ""
      }
    },
    "memory": {
      "kind": "container-stdio",
      "required": true,
      "policy": {
        "scopeClass": "shared-global",
        "concurrencyPolicy": "multi-reader",
        "stateBinding": "runtime-memory",
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
    "interactive-demo": { "enabled": true, "type": "http", "url": "http://127.0.0.1:39022/mcp" },
    "filesystem": { "enabled": true, "type": "stdio", "command": "node" },
    "memory": { "enabled": true, "type": "stdio", "command": "node" }
  }
}"#,
    )
    .unwrap();

    let metadata = r#"{"client":{"id":"codex"},"session":{"id":"sess-42"},"workspaceRoots":["/work/project-a"]}"#;
    let output = Command::new(bin_path())
        .env("MCPACE_CLIENT_METADATA_JSON", metadata)
        .args(["client", "plan", "--json", "--root", root.to_str().unwrap()])
        .output()
        .expect("run mcpace client plan");
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""clientId": "codex""#));
    assert!(text.contains(r#""sessionId": "sess-42""#));
    assert!(text.contains(r#""projectRoot": "/work/project-a""#));
    assert!(text.contains(r#""hubLifecycleImplemented": true"#));
    assert!(text.contains(r#""clientInstallImplemented": true"#));
    assert!(text.contains(r#""clientExportImplemented": true"#));
    assert!(text.contains(r#""requiresHubOwnedStdio": true"#));
    assert!(text.contains(r#""requestStrategy": "serialize-per-project-instance""#));
    assert!(text.contains(r#""requestStrategy": "serialize-per-state-profile""#));
    assert!(text.contains(r#""requestStrategy": "parallel-safe""#));
    assert!(text.contains(r#""stateProfileKey": "state-profile:interactive-demo"#));
    assert!(text.contains(r#""projectBindingKey": "project:/work/project-a""#));
    assert!(text.contains(r#""schedulerLane": "state-profile-queue""#));
    assert!(text.contains(r#""sessionLeaseId": "external:sess-42""#));
    assert!(text.contains(r#""clientTargetId": "codex""#));
    assert!(text.contains(
        r#""sessionBindingKey": "client:codex|session:external:sess-42|project:/work/project-a""#
    ));
}

#[test]
fn client_plan_json_warns_when_project_local_servers_have_no_project_root() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "servers": {
    "lean-ctx": {
      "kind": "container-stdio",
      "required": false,
      "defaultEnabled": true,
      "policy": {
        "scopeClass": "project-local",
        "concurrencyPolicy": "isolated-per-project",
        "stateBinding": "project-index",
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

    let output = run(&[
        "client",
        "plan",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--client-id",
        "cursor",
    ]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""clientId": "cursor""#));
    assert!(text.contains("Project-local servers exist but no project root was resolved"));
    assert!(text.contains(r#""requestStrategy": "serialize-per-project-instance""#));
}

#[test]
fn client_list_json_shows_verified_catalog_and_configured_key_name() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run(&["client", "list", "--json", "--root", root.to_str().unwrap()]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""configuredClientKeyName": "MCPace""#));
    assert!(text.contains(r#""familyCounts": {"#));
    assert!(text.contains(r#""surfaceClassCounts": {"#));
    assert!(text.contains(r#""proofTierCounts": {"#));
    assert!(text.contains(r#""installSupportedTargetIds": ["#));
    assert!(text.contains(r#""id": "codex""#));
    assert!(text.contains(r#""proofTier": "tier-1""#));
    assert!(text.contains(r#""installSupported": true"#));
    assert!(text.contains(r#""preferredConfigPath": "~/.codex/config.toml""#));
    assert!(text.contains(r#""id": "claude-code""#));
    assert!(text.contains(r#""id": "claude-api-connector""#));
    assert!(text.contains(r#""id": "cursor-local""#));
    assert!(text.contains(r#""id": "kiro-ide""#));
    assert!(text.contains(r#""id": "kiro-cli""#));
    assert!(text.contains(r#""id": "github-copilot-cloud-agent""#));
    assert!(text.contains(r#""id": "hermes-agent""#));
    assert!(text.contains(r#""id": "generic-stdio""#));
    assert!(text.contains(r#""catalogSources": ["#));
    assert!(text.contains(r#""source": "builtin""#));
}

#[test]
fn client_catalog_extensions_add_clients_without_recompiling() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "clientCatalog": {
    "targets": [
      {
        "id": "custom-local",
        "familyId": "custom",
        "displayName": "Custom Local Host",
        "aliases": ["custom"],
        "maturity": "external",
        "surfaceClass": "local",
        "surfaceKind": "local-cli",
        "proofTier": "external",
        "configFormat": "json",
        "configPaths": ["~/.custom/mcp.json"],
        "configPrecedence": ["user"],
        "nativeScopes": ["user"],
        "supportedIngresses": ["streamable-http"],
        "documentedFeatures": ["tools"],
        "documentedConstraints": [],
        "notes": ["Injected by project config."],
        "installSupport": {
          "kind": "json-mcp-servers",
          "preferredScope": "user",
          "preferredConfigPath": "~/.custom/mcp.json",
          "jsonServerShape": { "urlField": "url", "includeTypeHttp": true }
        }
      }
    ]
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let list = run(&["client", "list", "--json", "--root", root.to_str().unwrap()]);
    assert!(list.status.success(), "stderr: {}", stderr(&list));
    let list_text = stdout(&list);
    assert!(list_text.contains(r#""id": "custom-local""#));
    assert!(list_text.contains(r#""source": "clientCatalog.targets""#));
    assert!(list_text.contains(r#""catalogSources": ["#));

    let export = run(&[
        "client",
        "export",
        "custom",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(export.status.success(), "stderr: {}", stderr(&export));
    let export_text = stdout(&export);
    assert!(export_text.contains(r#""clientTargetId": "custom-local""#));
    assert!(export_text.contains(r#""exportMode": "local-streamable-http""#));
    assert!(export_text.contains(r#""recommendedInstallPath": "~/.custom/mcp.json""#));
}

#[test]
fn client_export_json_prefers_local_http_contract_for_codex() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run(&[
        "client",
        "export",
        "codex",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""mode": "connectable-preview""#));
    assert!(text.contains(r#""clientTargetId": "codex""#));
    assert!(text.contains(r#""adapterKeyName": "MCPace""#));
    assert!(text.contains(r#""preferredIngress": "streamable-http""#));
    assert!(text.contains(r#""exportMode": "local-streamable-http""#));
    assert!(text.contains(r#""recommendedInstallScope": "user""#));
    assert!(text.contains(r#""recommendedInstallPath": "~/.codex/config.toml""#));
    assert!(text.contains(r#""adapterContract": {"#));
    assert!(text.contains(r#""type": "local-streamable-http""#));
    assert!(text.contains(r#""urlTemplate": "http://127.0.0.1:39022/mcp""#));
    assert!(text.contains(r#""canConnectToday": true"#));
    assert!(!text.contains("preview-only"));
}

#[test]
fn client_export_json_keeps_stdio_fallback_for_stdio_only_hosts() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run(&[
        "client",
        "export",
        "generic-stdio",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""clientTargetId": "generic-stdio""#));
    assert!(text.contains(r#""exportMode": "local-stdio-launcher""#));
    assert!(text.contains(r#""type": "stdio-launcher""#));
    assert!(text.contains(r#""command": "mcpace""#));
    assert!(text.contains(r#""mcp-server""#));
}

#[test]
fn client_export_json_prefers_local_http_contract_for_other_http_capable_clients() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run(&[
        "client",
        "export",
        "cursor-local",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""clientTargetId": "cursor-local""#));
    assert!(text.contains(r#""preferredIngress": "streamable-http""#));
    assert!(text.contains(r#""exportMode": "local-streamable-http""#));
    assert!(text.contains(r#""recommendedInstallScope": "global""#));
    assert!(text.contains(r#""recommendedInstallPath": "~/.cursor/mcp.json""#));
    assert!(text.contains(r#""urlTemplate": "http://127.0.0.1:39022/mcp""#));
    assert!(text.contains(r#""canConnectToday": true"#));
}

#[test]
fn client_install_codex_json_replaces_unmanaged_user_table_and_is_idempotent() {
    let temp = TempDir::new();
    let root = temp.path();
    let home = TempDir::new();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let codex_dir = home.path().join(".codex");
    fs::create_dir_all(&codex_dir).unwrap();
    fs::write(
        codex_dir.join("config.toml"),
        r#"[mcp_servers.other]
command = "definitely-missing-mcpace-command-xyz"
args = ["serve"]
enabled = true

[mcp_servers.MCPace]
command = "old-mcpace"
args = ["stdio-shim"]
enabled = false
"#,
    )
    .unwrap();

    let first = run_with_envs(
        &[
            "client",
            "install",
            "codex",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(first.status.success(), "stderr: {}", stderr(&first));
    let first_text = stdout(&first);
    assert!(first_text.contains(r#""mode": "installed""#));
    assert!(first_text.contains(r#""clientTargetId": "codex""#));
    assert!(first_text.contains(r#""writesConfig": true"#));
    assert!(first_text.contains(r#""backupCreated": true"#));
    assert!(first_text.contains(r#""restoreCommand": "mcpace client restore codex --backup "#));
    assert!(first_text.contains(r#""configScope": "user""#));
    assert!(first_text.contains(r#""changed": true"#));
    assert!(first_text.contains(r#""replacedExistingBlock": true"#));
    assert!(first_text.contains(r#""transport": "streamable-http""#));
    assert!(first_text.contains(r#""url": "http://127.0.0.1:39022/mcp""#));
    assert!(first_text.contains("definitely-missing-mcpace-command-xyz"));
    assert!(first_text.contains("MCP clients can fail startup before reaching MCPace"));

    let config_path = codex_dir.join("config.toml");
    let installed = fs::read_to_string(&config_path).unwrap();
    assert!(installed.contains("# BEGIN MCPACE MANAGED BLOCK: MCPace"));
    assert!(installed.contains("[mcp_servers.MCPace]"));
    assert!(installed.contains(r#"url = "http://127.0.0.1:39022/mcp""#));
    assert!(installed.contains("startup_timeout_sec = 20"));
    assert!(installed.contains("[mcp_servers.other]"));
    assert_eq!(installed.matches("[mcp_servers.MCPace]").count(), 1);

    let second = run_with_envs(
        &[
            "client",
            "install",
            "codex",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(second.status.success(), "stderr: {}", stderr(&second));
    let second_text = stdout(&second);
    assert!(second_text.contains(r#""changed": false"#));
    assert!(second_text.contains(r#""backupCreated": false"#));
    assert!(second_text.contains(r#""replacedExistingBlock": true"#));

    let reinstalled = fs::read_to_string(&config_path).unwrap();
    assert_eq!(installed, reinstalled);
}

#[test]
fn client_install_restore_latest_restores_previous_config() {
    let temp = TempDir::new();
    let root = temp.path();
    let home = TempDir::new();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let codex_dir = home.path().join(".codex");
    fs::create_dir_all(&codex_dir).unwrap();
    let config_path = codex_dir.join("config.toml");
    let original = r#"[mcp_servers.other]
command = "other"
args = ["serve"]
enabled = true

[mcp_servers.MCPace]
command = "old-mcpace"
args = ["stdio-shim"]
enabled = false
"#;
    fs::write(&config_path, original).unwrap();

    let install = run_with_envs(
        &[
            "client",
            "install",
            "codex",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(install.status.success(), "stderr: {}", stderr(&install));
    let install_text = stdout(&install);
    assert!(install_text.contains(r#""backupCreated": true"#));
    assert!(install_text.contains(r#""backupId": ""#));
    assert_ne!(fs::read_to_string(&config_path).unwrap(), original);

    fs::write(&config_path, "broken = true\n").unwrap();
    let restore = run_with_envs(
        &[
            "client",
            "restore",
            "codex",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(restore.status.success(), "stderr: {}", stderr(&restore));
    let restore_text = stdout(&restore);
    assert!(restore_text.contains(r#""mode": "restored""#));
    assert!(restore_text.contains(r#""clientTargetId": "codex""#));
    assert!(restore_text.contains(r#""restoredExistingConfig": true"#));
    assert!(restore_text.contains(r#""wroteConfigFile": true"#));
    assert_eq!(fs::read_to_string(&config_path).unwrap(), original);
}

#[test]
fn client_install_restore_latest_removes_config_created_by_install() {
    let temp = TempDir::new();
    let root = temp.path();
    let home = TempDir::new();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let config_path = home.path().join(".codex").join("config.toml");
    assert!(!config_path.exists());
    let install = run_with_envs(
        &[
            "client",
            "install",
            "codex",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(install.status.success(), "stderr: {}", stderr(&install));
    assert!(config_path.is_file());

    let restore = run_with_envs(
        &[
            "client",
            "restore",
            "codex",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(restore.status.success(), "stderr: {}", stderr(&restore));
    let restore_text = stdout(&restore);
    assert!(restore_text.contains(r#""restoredExistingConfig": false"#));
    assert!(restore_text.contains(r#""removedConfigFile": true"#));
    assert!(!config_path.exists());
}

#[test]
fn client_restore_all_restores_latest_install_backups() {
    let temp = TempDir::new();
    let root = temp.path();
    let home = TempDir::new();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run_with_envs(
        &[
            "client",
            "install",
            "all",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let codex_config = home.path().join(".codex").join("config.toml");
    let claude_config = home.path().join(".claude.json");
    let cursor_config = home.path().join(".cursor").join("mcp.json");
    assert!(codex_config.is_file());
    assert!(claude_config.is_file());
    assert!(cursor_config.is_file());

    let restore = run_with_envs(
        &[
            "client",
            "restore",
            "all",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(restore.status.success(), "stderr: {}", stderr(&restore));
    let restore_text = stdout(&restore);
    assert!(restore_text.contains(r#""mode": "restored-all""#));
    assert!(restore_text.contains(r#""clientTargetId": "codex""#));
    assert!(restore_text.contains(r#""clientTargetId": "claude-code""#));
    assert!(restore_text.contains(r#""clientTargetId": "cursor-local""#));
    assert!(!codex_config.exists());
    assert!(!claude_config.exists());
    assert!(!cursor_config.exists());
}

#[test]
fn client_install_codex_dry_run_does_not_write_and_can_emit_diff() {
    let temp = TempDir::new();
    let root = temp.path();
    let home = TempDir::new();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let codex_dir = home.path().join(".codex");
    fs::create_dir_all(&codex_dir).unwrap();
    let config_path = codex_dir.join("config.toml");
    let original = r#"[mcp_servers.other]
command = "other"
args = ["serve"]
enabled = true
api_key = "super-secret"
env = { api_secret = "nested-secret" }
args = ["serve", "--api-token=array-secret"]
private_key = """
-----BEGIN MCPACE TEST REDACTION BLOCK-----
multi-line-secret
-----END MCPACE TEST REDACTION BLOCK-----
"""
[mcp_servers.other.extra]
enabled = true
"#;
    fs::write(&config_path, original).unwrap();

    let output = run_with_envs(
        &[
            "client",
            "install",
            "codex",
            "--dry-run",
            "--diff",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains(r#""mode": "install-preview""#));
    assert!(text.contains(r#""dryRun": true"#));
    assert!(text.contains(r#""writesConfig": false"#));
    assert!(text.contains(r#""persisted": false"#));
    assert!(text.contains(r#""backupCreated": false"#));
    assert!(text.contains(r#""changed": false"#));
    assert!(text.contains(r#""wouldChange": true"#));
    assert!(text.contains(r#""diff": "--- "#));
    assert!(!text.contains("super-secret"));
    assert!(!text.contains("nested-secret"));
    assert!(!text.contains("array-secret"));
    assert!(!text.contains("multi-line-secret"));
    assert!(!text.contains("BEGIN MCPACE TEST REDACTION BLOCK"));
    assert!(text.contains(r#"-api_key = \"[redacted]\""#));
    assert!(text.contains("-[REDACTED]"));
    assert!(text.contains("[mcp_servers.other.extra]"));
    assert!(text.contains("+# BEGIN MCPACE MANAGED BLOCK: MCPace"));
    assert!(text.contains(r#"+url = \"http://127.0.0.1:39022/mcp\""#));

    assert_eq!(fs::read_to_string(&config_path).unwrap(), original);
}

#[test]
fn client_install_dry_run_on_current_config_reports_no_candidate_change() {
    let temp = TempDir::new();
    let root = temp.path();
    let home = TempDir::new();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let install = run_with_envs(
        &[
            "client",
            "install",
            "codex",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(install.status.success(), "stderr: {}", stderr(&install));

    let preview = run_with_envs(
        &[
            "client",
            "install",
            "codex",
            "--dry-run",
            "--diff",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(preview.status.success(), "stderr: {}", stderr(&preview));
    let text = stdout(&preview);
    assert!(text.contains(r#""mode": "install-preview""#));
    assert!(text.contains(r#""dryRun": true"#));
    assert!(text.contains(r#""backupCreated": false"#));
    assert!(text.contains(r#""changed": false"#));
    assert!(text.contains(r#""wouldChange": false"#));
    assert!(text.contains(r#""diff": """#));
}

#[test]
fn client_install_all_dry_run_previews_without_creating_config_files() {
    let temp = TempDir::new();
    let root = temp.path();
    let home = TempDir::new();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run_with_envs(
        &[
            "client",
            "install",
            "all",
            "--dry-run",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains(r#""mode": "install-preview-all""#));
    assert!(text.contains(r#""dryRun": true"#));
    assert!(text.contains(r#""clientTargetId": "codex""#));
    assert!(text.contains(r#""clientTargetId": "cursor-local""#));
    assert!(text.contains(r#""wouldChange": true"#));
    assert!(!home.path().join(".codex").join("config.toml").exists());
    assert!(!home.path().join(".cursor").join("mcp.json").exists());
    assert!(!home.path().join(".claude.json").exists());
    assert!(!home.path().join(".hermes").join("config.yaml").exists());
}

#[test]
fn client_rejects_dry_run_and_diff_outside_install() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "servers": {}
}"#,
    )
    .unwrap();

    let export = run(&[
        "client",
        "export",
        "codex",
        "--dry-run",
        "--diff",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(!export.status.success());
    assert!(stderr(&export).contains("supported only for 'mcpace client install'"));

    let install = run(&[
        "client",
        "install",
        "codex",
        "--backup",
        "latest",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(!install.status.success());
    assert!(stderr(&install).contains("supported only for 'mcpace client restore'"));
}

#[test]
fn client_install_all_patches_every_catalog_declared_local_patcher() {
    let temp = TempDir::new();
    let root = temp.path();
    let home = TempDir::new();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run_with_envs(
        &[
            "client",
            "install",
            "all",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains(r#""mode": "installed-all""#));
    assert!(text.contains(r#""clientTargetId": "codex""#));
    assert!(text.contains(r#""clientTargetId": "cursor-local""#));
    assert!(text.contains(r#""clientTargetId": "claude-code""#));
    assert!(text.contains(r#""clientTargetId": "hermes-agent""#));
    assert!(home.path().join(".codex").join("config.toml").is_file());
    assert!(home.path().join(".cursor").join("mcp.json").is_file());
    assert!(home.path().join(".claude.json").is_file());
    assert!(home.path().join(".hermes").join("config.yaml").is_file());
}

#[test]
fn client_install_rejects_unsupported_client_surface() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run(&[
        "client",
        "install",
        "claude-api-connector",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains(&format!(
        "currently supports {}",
        client_catalog::client_install_support_summary()
    )));
}

#[test]
fn client_install_cursor_local_writes_global_json_config() {
    let temp = TempDir::new();
    let root = temp.path();
    let home = TempDir::new();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run_with_envs(
        &[
            "client",
            "install",
            "cursor-local",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains(r#""clientTargetId": "cursor-local""#));
    assert!(text.contains(r#""configScope": "global""#));
    assert!(text.contains(r#""url": "http://127.0.0.1:39022/mcp""#));

    let config_path = home.path().join(".cursor").join("mcp.json");
    let installed = fs::read_to_string(&config_path).unwrap();
    assert!(installed.contains(r#""mcpServers""#));
    assert!(installed.contains(r#""MCPace""#));
    assert!(installed.contains(r#""url": "http://127.0.0.1:39022/mcp""#));
}

#[test]
fn client_install_kiro_ide_writes_user_json_config() {
    let temp = TempDir::new();
    let root = temp.path();
    let home = TempDir::new();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run_with_envs(
        &[
            "client",
            "install",
            "kiro-ide",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains(r#""clientTargetId": "kiro-ide""#));
    assert!(text.contains(r#""configScope": "user""#));
    assert!(text.contains(r#""url": "http://127.0.0.1:39022/mcp""#));

    let config_path = home.path().join(".kiro").join("settings").join("mcp.json");
    let installed = fs::read_to_string(&config_path).unwrap();
    assert!(installed.contains(r#""mcpServers""#));
    assert!(installed.contains(r#""MCPace""#));
    assert!(installed.contains(r#""disabled": false"#));
}

#[test]
fn client_install_claude_code_writes_user_json_config() {
    let temp = TempDir::new();
    let root = temp.path();
    let home = TempDir::new();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run_with_envs(
        &[
            "client",
            "install",
            "claude-code",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains(r#""clientTargetId": "claude-code""#));
    assert!(text.contains(r#""configScope": "user""#));

    let installed = fs::read_to_string(home.path().join(".claude.json")).unwrap();
    assert!(installed.contains(r#""mcpServers""#));
    assert!(installed.contains(r#""MCPace""#));
    assert!(installed.contains(r#""type": "http""#));
    assert!(installed.contains(r#""url": "http://127.0.0.1:39022/mcp""#));
}

#[test]
fn client_install_claude_code_preserves_realistic_user_config_and_restore_round_trip() {
    let temp = TempDir::new();
    let root = temp.path();
    let home = TempDir::new();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();
    let config_path = home.path().join(".claude.json");
    let original = r#"{
  "theme": "dark",
  "permissions": {
    "allow": ["Bash(git status:*)", "Read(**/*.rs)"],
    "deny": ["Bash(rm -rf:*)"]
  },
  "hooks": {
    "Stop": [
      { "matcher": "*", "hooks": [{ "type": "command", "command": "echo done" }] }
    ]
  },
  "mcpServers": {
    "existing-local": {
      "type": "stdio",
      "command": "node",
      "args": ["server.js"]
    },
    "MCPace": {
      "type": "http",
      "url": "http://127.0.0.1:1/old"
    }
  }
}"#;
    fs::write(&config_path, original).unwrap();

    let install = run_with_envs(
        &[
            "client",
            "install",
            "claude-code",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(install.status.success(), "stderr: {}", stderr(&install));
    let install_text = stdout(&install);
    assert!(install_text.contains(r#""backupCreated": true"#));
    assert!(install_text.contains(r#""replacedExistingBlock": true"#));

    let installed = fs::read_to_string(&config_path).unwrap();
    assert!(installed.contains(r#""theme": "dark""#));
    assert!(installed.contains(r#""permissions""#));
    assert!(installed.contains(r#""hooks""#));
    assert!(installed.contains(r#""existing-local""#));
    assert!(installed.contains(r#""command": "node""#));
    assert!(installed.contains(r#""url": "http://127.0.0.1:39022/mcp""#));
    assert!(!installed.contains("http://127.0.0.1:1/old"));

    let restore = run_with_envs(
        &[
            "client",
            "restore",
            "claude-code",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(restore.status.success(), "stderr: {}", stderr(&restore));
    assert!(stdout(&restore).contains(r#""mode": "restored""#));
    assert_eq!(fs::read_to_string(&config_path).unwrap(), original);
}

#[test]
fn client_install_gemini_cli_writes_user_settings_json() {
    let temp = TempDir::new();
    let root = temp.path();
    let home = TempDir::new();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run_with_envs(
        &[
            "client",
            "install",
            "gemini-cli",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains(r#""clientTargetId": "gemini-cli""#));
    assert!(text.contains(r#""configScope": "user""#));

    let installed = fs::read_to_string(home.path().join(".gemini").join("settings.json")).unwrap();
    assert!(installed.contains(r#""mcpServers""#));
    assert!(installed.contains(r#""MCPace""#));
    assert!(installed.contains(r#""httpUrl": "http://127.0.0.1:39022/mcp""#));
}

#[test]
fn client_install_windsurf_writes_user_json_config() {
    let temp = TempDir::new();
    let root = temp.path();
    let home = TempDir::new();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run_with_envs(
        &[
            "client",
            "install",
            "windsurf",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains(r#""clientTargetId": "windsurf""#));

    let installed = fs::read_to_string(
        home.path()
            .join(".codeium")
            .join("windsurf")
            .join("mcp_config.json"),
    )
    .unwrap();
    assert!(installed.contains(r#""mcpServers""#));
    assert!(installed.contains(r#""MCPace""#));
    assert!(installed.contains(r#""serverUrl": "http://127.0.0.1:39022/mcp""#));
}

#[test]
fn client_install_copilot_cli_writes_user_json_config() {
    let temp = TempDir::new();
    let root = temp.path();
    let home = TempDir::new();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run_with_envs(
        &[
            "client",
            "install",
            "github-copilot-cli",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains(r#""clientTargetId": "github-copilot-cli""#));

    let installed =
        fs::read_to_string(home.path().join(".copilot").join("mcp-config.json")).unwrap();
    assert!(installed.contains(r#""mcpServers""#));
    assert!(installed.contains(r#""MCPace""#));
    assert!(installed.contains(r#""type": "http""#));
    assert!(installed.contains(r#""tools""#));
    assert!(installed.contains(r#""*""#));
}

#[test]
fn client_install_hermes_agent_writes_user_yaml_config_and_stays_idempotent() {
    let temp = TempDir::new();
    let root = temp.path();
    let home = TempDir::new();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();
    fs::create_dir_all(home.path().join(".hermes")).unwrap();
    fs::write(
        home.path().join(".hermes").join("config.yaml"),
        r#"model: nous/hermes-3
mcp_servers:
  existing:
    url: "https://mcp.example.com"
"#,
    )
    .unwrap();

    let first = run_with_envs(
        &[
            "client",
            "install",
            "hermes-agent",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(first.status.success(), "stderr: {}", stderr(&first));
    let first_text = stdout(&first);
    assert!(first_text.contains(r#""clientTargetId": "hermes-agent""#));
    assert!(first_text.contains(r#""transport": "streamable-http""#));
    assert!(first_text.contains(r#""changed": true"#));

    let config_path = home.path().join(".hermes").join("config.yaml");
    let installed = fs::read_to_string(&config_path).unwrap();
    assert!(installed.contains("model: nous/hermes-3"));
    assert!(installed.contains("mcp_servers:"));
    assert!(installed.contains("  existing:"));
    assert!(installed.contains("  # BEGIN MCPACE MANAGED BLOCK: MCPace"));
    assert!(installed.contains("  MCPace:"));
    assert!(installed.contains(r#"    url: "http://127.0.0.1:39022/mcp""#));
    assert_eq!(installed.matches("  MCPace:").count(), 1);

    let second = run_with_envs(
        &[
            "client",
            "install",
            "hermes-agent",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(second.status.success(), "stderr: {}", stderr(&second));
    let second_text = stdout(&second);
    assert!(second_text.contains(r#""changed": false"#));
    assert!(second_text.contains(r#""replacedExistingBlock": true"#));

    let reinstalled = fs::read_to_string(&config_path).unwrap();
    assert_eq!(installed, reinstalled);
}

#[test]
fn client_install_hermes_agent_preserves_comments_and_later_yaml_sections() {
    let temp = TempDir::new();
    let root = temp.path();
    let home = TempDir::new();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();
    fs::create_dir_all(home.path().join(".hermes")).unwrap();
    fs::write(
        home.path().join(".hermes").join("config.yaml"),
        r#"# user-owned Hermes config
model: nous/hermes-3
mcp_servers:
  # keep this user server
  research:
    url: "https://mcp.example.com/research"
tools:
  enabled: true
"#,
    )
    .unwrap();

    let output = run_with_envs(
        &[
            "client",
            "install",
            "hermes-agent",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
        &[("HOME", home.path()), ("USERPROFILE", home.path())],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let installed = fs::read_to_string(home.path().join(".hermes").join("config.yaml")).unwrap();
    assert!(installed.contains("# user-owned Hermes config"));
    assert!(installed.contains("  # keep this user server"));
    assert!(installed.contains("  research:"));
    assert!(installed.contains("  MCPace:"));
    assert!(installed.contains("tools:"));
    assert!(installed.contains("  enabled: true"));
    assert!(
        installed.find("  MCPace:").unwrap() < installed.find("tools:").unwrap(),
        "MCPace entry should stay inside mcp_servers section:\n{}",
        installed
    );
}

#[test]
fn client_export_json_previews_public_http_connector_contract() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run(&[
        "client",
        "export",
        "claude-api-connector",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--transport",
        "streamable-http",
    ]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""clientTargetId": "claude-api-connector""#));
    assert!(text.contains(r#""exportMode": "public-http-connector""#));
    assert!(text.contains(r#""type": "public-http-connector""#));
    assert!(text.contains(r#""urlTemplate": "https://YOUR-MCPACE-RELAY/mcp""#));
    assert!(text.contains("public HTTP MCP endpoint or relay"));
    assert!(text.contains("only reaches public HTTP MCP servers"));
}

#[test]
fn client_plan_json_reports_install_support_for_hermes_agent() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run(&[
        "client",
        "plan",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--client-id",
        "hermes-agent",
    ]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains(r#""clientTargetId": "hermes-agent""#));
    assert!(text.contains(r#""clientInstallImplemented": true"#));
    assert!(text.contains(r#""preferredIngress": "streamable-http""#));
    assert!(text.contains(r#""preferredIngressSource": "serve-default""#));
}

#[test]
fn client_plan_json_handles_mcp_roots_and_generates_internal_lease() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "servers": {
    "lean-ctx": {
      "kind": "container-stdio",
      "required": false,
      "defaultEnabled": true,
      "policy": {
        "scopeClass": "project-local",
        "concurrencyPolicy": "isolated-per-project",
        "stateBinding": "project-index",
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

    let metadata = r#"{
  "params": {
    "clientInfo": { "name": "Claude Code" },
    "roots": [
      { "uri": "file:///work/src/project-a" },
      { "uri": "file:///work/src/project-b" }
    ],
    "_meta": {
      "com.mcpace/context": {
        "cwd": "file:///work/src/project-b"
      }
    }
  }
}"#;

    let output = Command::new(bin_path())
        .env("MCPACE_CLIENT_METADATA_JSON", metadata)
        .args(["client", "plan", "--json", "--root", root.to_str().unwrap()])
        .output()
        .expect("run mcpace client plan with MCP-style metadata");
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""clientTargetId": "claude-code""#));
    assert!(text.contains(r#""projectRoot": "/work/src/project-b""#));
    assert!(text.contains(r#""projectRootSource": "metadata-roots+cwd""#));
    assert!(text.contains(r#""sessionLeaseSource": "planned-fallback""#));
    assert!(text.contains(r#""workspaceRoots": ["#));
    assert!(text.contains(
        r#"No external session id was resolved; the plan derived an internal session lease"#
    ));
}

#[test]
fn client_plan_json_treats_unknown_scope_class_as_lease_local() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "servers": {
    "mystery": {
      "kind": "container-stdio",
      "required": false,
      "defaultEnabled": true,
      "policy": {
        "scopeClass": "mystery-scope",
        "concurrencyPolicy": "multi-reader",
        "stateBinding": "opaque",
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

    let output = run(&[
        "client",
        "plan",
        "--json",
        "--root",
        root.to_str().unwrap(),
        "--client-id",
        "generic-stdio",
        "--session-id",
        "sess-77",
    ]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains("treating it as lease-local until the policy is tightened"));
    assert!(text.contains(r#"partition:lease:external:sess-77"#));
}

#[test]
fn client_plan_json_uses_credential_profile_and_surface_constraints() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "servers": {
    "calendar": {
      "kind": "remote-http",
      "required": false,
      "defaultEnabled": true,
      "policy": {
        "scopeClass": "credential-scoped",
        "concurrencyPolicy": "single-writer",
        "stateBinding": "credential-state",
        "credentialBinding": "oauth"
      },
      "installer": {
        "installTarget": "none",
        "installMethod": "none",
        "installPackage": "",
        "verifyCommand": ""
      }
    },
    "git": {
      "kind": "container-stdio",
      "required": false,
      "defaultEnabled": true,
      "policy": {
        "scopeClass": "project-local",
        "concurrencyPolicy": "isolated-per-project",
        "stateBinding": "project-index",
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

    let metadata = r#"{
  "params": {
    "clientInfo": { "name": "Claude API MCP Connector" },
    "_meta": {
      "com.mcpace/context": {
        "credentialProfileId": "team-prod"
      }
    }
  }
}"#;

    let output = Command::new(bin_path())
        .env("MCPACE_CLIENT_METADATA_JSON", metadata)
        .args([
            "client",
            "plan",
            "--json",
            "--root",
            root.to_str().unwrap(),
            "--client-id",
            "claude-api-connector",
            "--transport",
            "streamable-http",
        ])
        .output()
        .expect("run mcpace client plan with credential profile metadata");
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""clientTargetId": "claude-api-connector""#));
    assert!(text.contains(r#""clientTargetSurfaceClass": "cloud""#));
    assert!(text.contains(r#""credentialProfileId": "team-prod""#));
    assert!(text.contains(r#""credentialProfileIdSource": "metadata""#));
    assert!(text.contains("partition:credential-profile:team-prod"));
    assert!(text.contains("tools-only"));
    assert!(text.contains("public HTTP MCP servers"));
    assert!(text.contains("cannot consume MCPace as a local stdio launcher"));
}
