use super::*;

#[test]
fn parses_verify_action_and_compat_flags() {
    let parsed = parse_cli(&[
        "readiness".to_string(),
        "-json".to_string(),
        "-root".to_string(),
        "/tmp/mcpace".to_string(),
    ]);
    assert!(parsed.error.is_none());
    assert_eq!(parsed.action.as_deref(), Some("readiness"));
    assert!(parsed.json_output);
    assert_eq!(parsed.root_override, Some(PathBuf::from("/tmp/mcpace")));
}

#[test]
fn rejects_second_action() {
    let parsed = parse_cli(&["doctor".to_string(), "readiness".to_string()]);
    assert!(parsed
        .error
        .unwrap_or_default()
        .contains("unexpected argument"));
}
