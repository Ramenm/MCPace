use super::instructions_text;

#[test]
fn initialize_instructions_redact_bootstrap_failure_details() {
    let instructions = instructions_text(&[
            "hub was started automatically for this MCP session".to_string(),
            "failed to start hub automatically: stderr Authorization: Bearer abc123 password=secret-token".to_string(),
        ]);

    assert!(instructions.contains("hub was started automatically for this MCP session"));
    assert!(instructions
        .contains("failed to start hub automatically; details withheld from initialize response"));
    assert!(!instructions.contains("Bearer abc123"));
    assert!(!instructions.contains("password=secret-token"));
}
