use super::{
    instructions_text, read_bounded_stdio_line, track_stdio_request_id, BoundedStdioLine,
    RequestIdReplayError, MAX_MCP_STDIO_REQUEST_IDS,
};
use crate::json::JsonValue;
use crate::mcp_protocol::MAX_REQUEST_ID_BYTES;
use std::io::Cursor;

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

#[test]
fn bounded_stdio_line_accepts_the_limit_and_eof_without_newline() {
    let mut line = Vec::new();
    let mut exact = Cursor::new(b"abc\n".to_vec());
    assert_eq!(
        read_bounded_stdio_line(&mut exact, &mut line, 4).unwrap(),
        BoundedStdioLine::Line
    );
    assert_eq!(line, b"abc\n");

    let mut eof = Cursor::new(b"abc".to_vec());
    assert_eq!(
        read_bounded_stdio_line(&mut eof, &mut line, 3).unwrap(),
        BoundedStdioLine::Line
    );
    assert_eq!(line, b"abc");
}

#[test]
fn bounded_stdio_line_rejects_oversize_before_unbounded_allocation() {
    let mut line = Vec::new();
    let mut input = Cursor::new(b"abcd\nremaining".to_vec());
    assert_eq!(
        read_bounded_stdio_line(&mut input, &mut line, 3).unwrap(),
        BoundedStdioLine::TooLong
    );
    assert_eq!(line.len(), 4);
}

#[test]
fn bounded_stdio_line_preserves_invalid_utf8_for_fail_closed_validation() {
    let mut line = Vec::new();
    let mut input = Cursor::new(vec![0xff, b'\n']);
    assert_eq!(
        read_bounded_stdio_line(&mut input, &mut line, 8).unwrap(),
        BoundedStdioLine::Line
    );
    assert!(std::str::from_utf8(&line).is_err());
}

#[test]
fn stdio_request_id_replay_window_is_count_and_byte_bounded() {
    let mut seen = std::collections::BTreeSet::new();
    let largest = JsonValue::string("x".repeat(MAX_REQUEST_ID_BYTES));
    track_stdio_request_id(&mut seen, &largest).expect("largest bounded id should fit");
    assert_eq!(
        track_stdio_request_id(&mut seen, &largest),
        Err(RequestIdReplayError::Duplicate)
    );
    assert_eq!(
        track_stdio_request_id(
            &mut seen,
            &JsonValue::string("x".repeat(MAX_REQUEST_ID_BYTES + 1)),
        ),
        Err(RequestIdReplayError::TooLong)
    );

    for index in 1..MAX_MCP_STDIO_REQUEST_IDS {
        track_stdio_request_id(&mut seen, &JsonValue::number(index))
            .expect("request id within replay-window cap");
    }
    assert_eq!(seen.len(), MAX_MCP_STDIO_REQUEST_IDS);
    assert_eq!(
        track_stdio_request_id(&mut seen, &JsonValue::string("overflow")),
        Err(RequestIdReplayError::Full)
    );
}
