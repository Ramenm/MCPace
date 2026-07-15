use super::{
    ensure_runtime_dir, normalize_http_path, normalize_public_url,
    resolve_user_config_path_expression, DEFAULT_LOCAL_MCP_PATH,
};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn private_runtime_directory_creation_is_idempotent_under_concurrency() {
    let root = std::env::temp_dir().join(format!(
        "mcpace-runtimepaths-race-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let state_root = Arc::new(root.join("state"));
    let workers = (0..16)
        .map(|_| {
            let state_root = Arc::clone(&state_root);
            std::thread::spawn(move || ensure_runtime_dir(&state_root))
        })
        .collect::<Vec<_>>();
    for worker in workers {
        let path = worker
            .join()
            .expect("directory worker")
            .expect("runtime directory");
        assert!(path.is_dir());
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn normalize_http_path_rejects_request_line_injection_primitives() {
    for candidate in [
        "",
        "relative",
        "/mcp?debug=1",
        "/mcp#frag",
        "/mcp with-space",
        "/mcp\twith-tab",
        "/mcp\r\nInjected: bad",
    ] {
        assert_eq!(
            normalize_http_path(candidate, DEFAULT_LOCAL_MCP_PATH),
            DEFAULT_LOCAL_MCP_PATH
        );
    }
    assert_eq!(
        normalize_http_path("/custom/path", DEFAULT_LOCAL_MCP_PATH),
        "/custom/path"
    );
}

#[test]
fn user_config_path_resolution_rejects_escape_and_pattern_expressions() {
    assert!(resolve_user_config_path_expression("~/../outside.json").is_none());
    assert!(resolve_user_config_path_expression("~/.config/*/mcp.json").is_none());
    assert!(resolve_user_config_path_expression("<project>/mcp.json").is_none());
}

#[test]
fn normalize_public_url_rejects_ambiguous_or_unsafe_authorities() {
    assert_eq!(
        normalize_public_url("https://relay.example/mcp"),
        Some("https://relay.example/mcp".to_string())
    );
    assert_eq!(
        normalize_public_url("https://[::1]:39022/mcp"),
        Some("https://[::1]:39022/mcp".to_string())
    );
    for candidate in [
        "https://relay.example/mcp with-space",
        "https://relay.example/mcp\twith-tab",
        "https://relay.example/mcp\r\nInjected: bad",
        "https://user:pass@relay.example/mcp",
        "https://relay.example:0/mcp",
        "https://relay.example:99999/mcp",
        "https://2001:db8::1/mcp",
        "https://[::1]bad/mcp",
        "https://relay.example/mcp#fragment",
        "ftp://relay.example/mcp",
    ] {
        assert_eq!(normalize_public_url(candidate), None);
    }
}
