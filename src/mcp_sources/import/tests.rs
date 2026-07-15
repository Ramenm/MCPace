use super::*;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_root(label: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_millis();
    let path = std::env::temp_dir().join(format!(
        "mcpace-import-test-{}-{}-{}",
        label,
        std::process::id(),
        millis
    ));
    fs::create_dir_all(&path).expect("create temp root");
    path
}

#[test]
fn imports_servers_shape_with_url_alias_disabled_and_inferred_type() {
    let root = temp_root("remote-alias");
    let source = root.join("source.json");
    let target = root.join("imported.json");
    fs::write(
        &source,
        r#"{
  "servers": {
    "Remote API": {
      "serverUrl": "https://example.com/mcp",
      "disabled": true
    }
  }
}"#,
    )
    .expect("write source");

    let result = import_mcp_server_entries(
        &root,
        McpServerImportOptions {
            source_path: source,
            settings_path: Some(target.clone()),
            dry_run: false,
            force: false,
            disabled: false,
        },
    )
    .expect("import servers shape");

    assert_eq!(result.imported_count, 1);
    let written = json_helpers::read_json_file(&target).expect("read import target");
    let servers = json_helpers::object_at_path(&written, &["mcpServers"]).expect("mcpServers");
    let remote = servers.get("Remote API").expect("Remote API");
    assert_eq!(
        remote.get("url").and_then(JsonValue::as_str),
        Some("https://example.com/mcp")
    );
    assert_eq!(
        remote.get("type").and_then(JsonValue::as_str),
        Some("streamable-http")
    );
    assert_eq!(
        remote.get("enabled").and_then(JsonValue::as_bool),
        Some(false)
    );
}

#[test]
fn skips_mcpace_self_entry_during_import() {
    let root = temp_root("self-entry");
    let source = root.join("source.json");
    fs::write(
        &source,
        r#"{
  "mcpServers": {
    "mcp pace": {
      "url": "http://127.0.0.1:39022/mcp"
    }
  }
}"#,
    )
    .expect("write source");

    let error = import_mcp_server_entries(
        &root,
        McpServerImportOptions {
            source_path: source,
            settings_path: Some(root.join("imported.json")),
            dry_run: false,
            force: false,
            disabled: false,
        },
    )
    .expect_err("self entry should leave no usable servers");
    assert!(
        error.contains("no usable servers"),
        "unexpected error: {error}"
    );
}

#[test]
fn disabled_import_option_parks_enabled_source() {
    let root = temp_root("disabled-import");
    let source = root.join("source.json");
    let target = root.join("imported.json");
    fs::write(
        &source,
        r#"{
  "mcpServers": {
    "Enabled Source": {
      "command": "node",
      "args": ["server.js"],
      "enabled": true
    }
  }
}"#,
    )
    .expect("write source");

    import_mcp_server_entries(
        &root,
        McpServerImportOptions {
            source_path: source,
            settings_path: Some(target.clone()),
            dry_run: false,
            force: false,
            disabled: true,
        },
    )
    .expect("import disabled");

    let written = json_helpers::read_json_file(&target).expect("read import target");
    let servers = json_helpers::object_at_path(&written, &["mcpServers"]).expect("mcpServers");
    let imported = servers.get("Enabled Source").expect("Enabled Source");
    assert_eq!(
        imported.get("enabled").and_then(JsonValue::as_bool),
        Some(false)
    );
    assert_eq!(
        imported.get("disabled").and_then(JsonValue::as_bool),
        Some(true)
    );
}
