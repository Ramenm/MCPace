use super::*;

#[test]
fn setup_flags_after_a_positional_server_spec_are_not_consumed_as_server_arguments() {
    let args = [
        "pypi:mcp-server-fetch==2026.6.4",
        "--as",
        "fetch",
        "--client",
        "none",
        "--json",
        "--root",
        "/tmp/mcpace-test-root",
        "--port",
        "43123",
    ]
    .into_iter()
    .map(ToString::to_string)
    .collect::<Vec<_>>();

    let parsed = parse_cli(&args);

    assert_eq!(
        parsed.server_spec.as_deref(),
        Some("pypi:mcp-server-fetch==2026.6.4")
    );
    assert_eq!(parsed.server_name.as_deref(), Some("fetch"));
    assert!(parsed.skip_client_install);
    assert!(parsed.json_output);
    assert_eq!(
        parsed.root_override,
        Some(PathBuf::from("/tmp/mcpace-test-root"))
    );
    assert_eq!(parsed.port, 43123);
    assert!(parsed.server_paths.is_empty());
    assert_eq!(parsed.error, None);
}

#[test]
fn setup_endpoint_overrides_are_persisted_for_later_client_installs() {
    let root =
        std::env::temp_dir().join(format!("mcpace-setup-endpoint-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    ensure_setup_root_layout(&root).expect("bootstrap root");

    assert!(
        persist_setup_endpoint_overrides(&root, Some("127.0.0.1"), 43123)
            .expect("persist endpoint")
    );

    let endpoint = runtimepaths::resolve_serve_endpoint(Some(&root));
    assert_eq!(endpoint.host, "127.0.0.1");
    assert_eq!(endpoint.port, 43123);
    assert_eq!(
        runtimepaths::configured_mcp_url(&root),
        "http://127.0.0.1:43123/mcp"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn home_import_normalizes_url_alias_type_and_disabled() {
    let value = JsonValue::object([
        ("serverUrl", JsonValue::string("https://example.com/mcp")),
        ("transport", JsonValue::string("http")),
        ("disabled", JsonValue::bool(true)),
    ]);
    let normalized = normalize_home_imported_server_value(&value);
    assert_eq!(
        normalized.get("url").and_then(JsonValue::as_str),
        Some("https://example.com/mcp")
    );
    assert_eq!(
        normalized.get("type").and_then(JsonValue::as_str),
        Some("streamable-http")
    );
    assert_eq!(
        normalized.get("enabled").and_then(JsonValue::as_bool),
        Some(false)
    );
}

#[test]
fn home_import_skips_mcp_pace_self_name_and_endpoint() {
    let value = JsonValue::object([("url", JsonValue::string("http://127.0.0.1:39022/mcp"))]);
    assert!(is_mcpace_self_entry(
        "mcp pace",
        &value,
        "http://127.0.0.1:39022/mcp"
    ));
}
