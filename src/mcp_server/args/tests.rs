use super::*;

#[test]
fn parses_live_stdio_context_with_canonical_flags() {
    let args = vec![
        "--root".to_string(),
        "/tmp/mcpace".to_string(),
        "--client-id".to_string(),
        "codex".to_string(),
        "--session-id".to_string(),
        "chat-a".to_string(),
        "--json".to_string(),
    ];
    let parsed = parse_cli(&args);
    assert!(parsed.error.is_none());
    assert_eq!(parsed.root_override, Some(PathBuf::from("/tmp/mcpace")));
    assert_eq!(parsed.client_id.as_deref(), Some("codex"));
    assert_eq!(parsed.session_id.as_deref(), Some("chat-a"));
}

#[test]
fn rejects_unknown_flags() {
    let parsed = parse_cli(&["--shell".to_string()]);
    assert!(parsed
        .error
        .unwrap_or_default()
        .contains("unexpected argument"));
}
