use super::*;

#[test]
fn strips_preview_json_flag_before_forwarding_to_live_server() {
    let input = vec![
        "--json".to_string(),
        "--root".to_string(),
        "/tmp/mcpace".to_string(),
        "--client-id".to_string(),
        "codex".to_string(),
    ];
    let output = normalize_compat_args(&input);
    assert_eq!(
        output,
        vec![
            "--root".to_string(),
            "/tmp/mcpace".to_string(),
            "--client-id".to_string(),
            "codex".to_string(),
        ]
    );
}
