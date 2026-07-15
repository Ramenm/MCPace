use super::*;
use std::fs;
use std::io::Cursor;

fn temporary_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("mcpace-operations-{}-{}", label, now_ms()));
    fs::create_dir_all(&root).expect("temporary root");
    root
}

#[test]
fn bounded_operation_reader_drains_oversized_lines_and_recovers() {
    let mut input = Cursor::new(b"0123456789abcdef\n{\"tsMs\":1}\n".to_vec());
    let mut line = Vec::new();
    assert_eq!(
        read_bounded_operation_line(&mut input, &mut line, 8).expect("oversized line"),
        BoundedOperationLine::TooLong
    );
    assert!(line.len() <= 9);
    assert_eq!(
        read_bounded_operation_line(&mut input, &mut line, 8).expect("next line"),
        BoundedOperationLine::TooLong
    );

    let mut input = Cursor::new(b"oversized-line\n{}\n".to_vec());
    assert_eq!(
        read_bounded_operation_line(&mut input, &mut line, 4).expect("oversized line"),
        BoundedOperationLine::TooLong
    );
    assert_eq!(
        read_bounded_operation_line(&mut input, &mut line, 4).expect("valid line"),
        BoundedOperationLine::Line
    );
    assert_eq!(line, b"{}\n");
}

#[test]
fn retained_operations_reads_archive_then_active_and_keeps_latest_limit() {
    let root = temporary_root("limit");
    let state_root = runtimepaths::resolve_state_root(&root);
    runtimepaths::ensure_hub_dir(&state_root).expect("hub directory");
    let active = runtimepaths::hub_log_path(&state_root);
    let archive = rotated_log_path(&active);
    fs::write(
        &archive,
        "{\"tsMs\":1,\"event\":\"old\"}\n{\"tsMs\":2,\"event\":\"middle\"}\n",
    )
    .expect("archive");
    fs::write(&active, "{\"tsMs\":3,\"event\":\"new\"}\n").expect("active");

    let response = retained_operations_response(&root, 2);
    assert_eq!(
        response.get("totalParsed").and_then(JsonValue::as_i64),
        Some(3)
    );
    assert_eq!(
        response.get("returned").and_then(JsonValue::as_i64),
        Some(2)
    );
    assert_eq!(
        response.get("truncated").and_then(JsonValue::as_bool),
        Some(true)
    );
    let events = response
        .get("events")
        .and_then(JsonValue::as_array)
        .expect("events");
    assert_eq!(
        events[0].get("event").and_then(JsonValue::as_str),
        Some("middle")
    );
    assert_eq!(
        events[1].get("event").and_then(JsonValue::as_str),
        Some("new")
    );
    let _ = fs::remove_dir_all(root);
}
