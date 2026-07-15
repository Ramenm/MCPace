use super::*;

#[test]
fn parses_probe_options_with_compat_flags() {
    let parsed = parse_cli(&[
        "probe".to_string(),
        "-id".to_string(),
        "filesystem".to_string(),
        "-timeout-ms".to_string(),
        "500".to_string(),
        "-refresh".to_string(),
        "-json".to_string(),
    ]);
    assert!(parsed.error.is_none());
    assert_eq!(parsed.action.as_deref(), Some("probe"));
    assert_eq!(parsed.id_filter.as_deref(), Some("filesystem"));
    assert_eq!(parsed.timeout_ms, Some(500));
    assert!(parsed.refresh);
    assert!(parsed.json_output);
}

#[test]
fn rejects_zero_timeout() {
    let parsed = parse_cli(&[
        "probe".to_string(),
        "--timeout-ms".to_string(),
        "0".to_string(),
    ]);
    assert_eq!(
        parsed.error.as_deref(),
        Some("lab probe --timeout-ms must be a positive integer")
    );
}
