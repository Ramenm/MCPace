mod common;

use common::*;
use std::fs;
use std::process::Command;

#[test]
fn client_plan_json_uses_metadata_and_arbitrates_unsafe_servers() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.0",
  "servers": {
    "browser": {
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
    "browser": { "enabled": true, "type": "http", "url": "http://127.0.0.1:39022/mcp" },
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
    assert!(text.contains(r#""requestStrategy": "serialize-per-instance""#));
    assert!(text.contains(r#""requestStrategy": "parallel-safe""#));
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
  "version": "0.3.0",
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
  "version": "0.3.0",
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
    assert!(text.contains(r#""id": "codex""#));
    assert!(text.contains(r#""id": "claude-code""#));
    assert!(text.contains(r#""id": "claude-api-connector""#));
    assert!(text.contains(r#""id": "cursor-local""#));
    assert!(text.contains(r#""id": "kiro-ide""#));
    assert!(text.contains(r#""id": "kiro-cli""#));
    assert!(text.contains(r#""id": "github-copilot-cloud-agent""#));
    assert!(text.contains(r#""id": "hermes-agent""#));
    assert!(text.contains(r#""id": "generic-stdio""#));
}

#[test]
fn client_export_json_prefers_local_http_contract_for_codex() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.0",
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
  "version": "0.3.0",
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
  "version": "0.3.0",
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
    assert!(text.contains(r#""urlTemplate": "http://127.0.0.1:39022/mcp""#));
    assert!(text.contains(r#""canConnectToday": true"#));
}

#[test]
fn client_install_codex_json_replaces_unmanaged_table_and_is_idempotent() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.0",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let codex_dir = root.join(".codex");
    fs::create_dir_all(&codex_dir).unwrap();
    fs::write(
        codex_dir.join("config.toml"),
        r#"[mcp_servers.other]
command = "other"
args = ["serve"]
enabled = true

[mcp_servers.MCPace]
command = "old-mcpace"
args = ["stdio-shim"]
enabled = false
"#,
    )
    .unwrap();

    let first = run(&[
        "client",
        "install",
        "codex",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(first.status.success(), "stderr: {}", stderr(&first));
    let first_text = stdout(&first);
    assert!(first_text.contains(r#""mode": "installed""#));
    assert!(first_text.contains(r#""clientTargetId": "codex""#));
    assert!(first_text.contains(r#""writesConfig": true"#));
    assert!(first_text.contains(r#""changed": true"#));
    assert!(first_text.contains(r#""replacedExistingBlock": true"#));
    assert!(first_text.contains(r#""transport": "streamable-http""#));
    assert!(first_text.contains(r#""url": "http://127.0.0.1:39022/mcp""#));

    let config_path = codex_dir.join("config.toml");
    let installed = fs::read_to_string(&config_path).unwrap();
    assert!(installed.contains("# BEGIN MCPACE MANAGED BLOCK: MCPace"));
    assert!(installed.contains("[mcp_servers.MCPace]"));
    assert!(installed.contains(r#"url = "http://127.0.0.1:39022/mcp""#));
    assert!(installed.contains("startup_timeout_sec = 20"));
    assert!(installed.contains("[mcp_servers.other]"));
    assert_eq!(installed.matches("[mcp_servers.MCPace]").count(), 1);

    let second = run(&[
        "client",
        "install",
        "codex",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(second.status.success(), "stderr: {}", stderr(&second));
    let second_text = stdout(&second);
    assert!(second_text.contains(r#""changed": false"#));
    assert!(second_text.contains(r#""replacedExistingBlock": true"#));

    let reinstalled = fs::read_to_string(&config_path).unwrap();
    assert_eq!(installed, reinstalled);
}

#[test]
fn client_install_rejects_unsupported_client_surface() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.0",
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
    assert!(stderr(&output).contains(
        "currently supports Codex, Claude Code, Cursor, Kiro, Gemini CLI, Windsurf, GitHub Copilot CLI, and Hermes Agent"
    ));
}

#[test]
fn client_install_cursor_local_writes_project_json_config() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.0",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run(&[
        "client",
        "install",
        "cursor-local",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains(r#""clientTargetId": "cursor-local""#));
    assert!(text.contains(r#""url": "http://127.0.0.1:39022/mcp""#));

    let config_path = root.join(".cursor").join("mcp.json");
    let installed = fs::read_to_string(&config_path).unwrap();
    assert!(installed.contains(r#""mcpServers""#));
    assert!(installed.contains(r#""MCPace""#));
    assert!(installed.contains(r#""url": "http://127.0.0.1:39022/mcp""#));
}

#[test]
fn client_install_kiro_ide_writes_project_json_config() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.0",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run(&[
        "client",
        "install",
        "kiro-ide",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains(r#""clientTargetId": "kiro-ide""#));
    assert!(text.contains(r#""url": "http://127.0.0.1:39022/mcp""#));

    let config_path = root.join(".kiro").join("settings").join("mcp.json");
    let installed = fs::read_to_string(&config_path).unwrap();
    assert!(installed.contains(r#""mcpServers""#));
    assert!(installed.contains(r#""MCPace""#));
    assert!(installed.contains(r#""disabled": false"#));
}

#[test]
fn client_install_claude_code_writes_project_mcp_json() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.0",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run(&[
        "client",
        "install",
        "claude-code",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains(r#""clientTargetId": "claude-code""#));

    let installed = fs::read_to_string(root.join(".mcp.json")).unwrap();
    assert!(installed.contains(r#""mcpServers""#));
    assert!(installed.contains(r#""MCPace""#));
    assert!(installed.contains(r#""type": "http""#));
    assert!(installed.contains(r#""url": "http://127.0.0.1:39022/mcp""#));
}

#[test]
fn client_install_gemini_cli_writes_project_settings_json() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.0",
  "client": {
    "keyName": "MCPace"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run(&[
        "client",
        "install",
        "gemini-cli",
        "--json",
        "--root",
        root.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains(r#""clientTargetId": "gemini-cli""#));

    let installed = fs::read_to_string(root.join(".gemini").join("settings.json")).unwrap();
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
  "version": "0.3.0",
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
  "version": "0.3.0",
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
  "version": "0.3.0",
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
fn client_export_json_previews_public_http_connector_contract() {
    let temp = TempDir::new();
    let root = temp.path();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.0",
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
  "version": "0.3.0",
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
  "version": "0.3.0",
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
  "version": "0.3.0",
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
  "version": "0.3.0",
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
