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
    "keyName": "codex-local"
  },
  "servers": {}
}"#,
    )
    .unwrap();

    let output = run(&["client", "list", "--json", "--root", root.to_str().unwrap()]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(r#""configuredClientKeyName": "codex-local""#));
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
fn client_export_json_previews_local_stdio_adapter_contract() {
    let temp = TempDir::new();
    let root = temp.path();
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
    assert!(text.contains(r#""mode": "preview-only""#));
    assert!(text.contains(r#""clientTargetId": "codex""#));
    assert!(text.contains(r#""adapterKeyName": "codex-local""#));
    assert!(text.contains(r#""exportMode": "local-stdio-launcher""#));
    assert!(text.contains(r#""adapterContract": {"#));
    assert!(text.contains(r#""type": "stdio-launcher""#));
    assert!(text.contains(r#""command": "mcpace""#));
    assert!(text.contains(r#""args": ["#));
    assert!(text.contains(r#""stdio-shim""#));
    assert!(text.contains(r#""canConnectToday": false"#));
    assert!(text.contains("live stdio forwarding path is not implemented yet"));
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
