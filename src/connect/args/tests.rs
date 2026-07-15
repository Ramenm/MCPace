use super::*;

#[test]
fn parses_flag_and_positional_forms() {
    let parsed = parse_cli(&[
        "-client".to_string(),
        "cursor-local".to_string(),
        "filesystem".to_string(),
        "-json".to_string(),
    ]);
    assert!(parsed.error.is_none());
    assert_eq!(parsed.client_id.as_deref(), Some("cursor-local"));
    assert_eq!(parsed.server_name.as_deref(), Some("filesystem"));
    assert!(parsed.json_output);
}

#[test]
fn rejects_extra_positional_when_both_flags_are_set() {
    let parsed = parse_cli(&[
        "--client".to_string(),
        "codex".to_string(),
        "--server".to_string(),
        "filesystem".to_string(),
        "extra".to_string(),
    ]);
    assert!(parsed
        .error
        .unwrap_or_default()
        .contains("unsupported connect argument"));
}
