use super::*;

#[test]
fn request_id_replay_window_is_bounded_and_preserves_duplicate_detection() {
    let mut store = McpHttpSessionStore::new(1, 60_000);
    store
        .create_or_replace("session".to_string(), "2025-11-25", None, None, None, 1)
        .expect("session should be created");

    for index in 0..MAX_MCP_HTTP_REQUEST_IDS_PER_SESSION {
        store
            .track_request_id("session", &format!("n:{index}"))
            .expect("request id within the replay-window cap");
    }

    let duplicate = store
        .track_request_id("session", "n:0")
        .expect_err("duplicate ids stay rejected even when the window is full");
    assert_eq!(duplicate.kind, McpHttpSessionErrorKind::DuplicateRequestId);

    let full = store
        .track_request_id("session", "n:overflow")
        .expect_err("new ids must not grow the replay window without bound");
    assert_eq!(full.kind, McpHttpSessionErrorKind::RequestIdLimit);
    assert_eq!(full.http_status(), "429 Too Many Requests");
    assert_eq!(
        store
            .sessions
            .get("session")
            .expect("session remains available")
            .seen_request_ids
            .len(),
        MAX_MCP_HTTP_REQUEST_IDS_PER_SESSION
    );
}

#[test]
fn request_id_replay_window_rejects_large_ids_and_aggregate_bytes() {
    let mut store = McpHttpSessionStore::new(1, 60_000);
    store
        .create_or_replace("session".to_string(), "2025-11-25", None, None, None, 1)
        .expect("session should be created");

    let oversized = format!(
        "s:{}",
        "x".repeat(crate::mcp_protocol::MAX_REQUEST_ID_BYTES + 1)
    );
    let error = store
        .track_request_id("session", &oversized)
        .expect_err("oversized request ids must be rejected");
    assert_eq!(error.kind, McpHttpSessionErrorKind::Invalid);

    let key_width = MAX_MCP_HTTP_REQUEST_ID_STORAGE_BYTES - 2;
    let accepted = MAX_MCP_HTTP_REQUEST_ID_REPLAY_BYTES / MAX_MCP_HTTP_REQUEST_ID_STORAGE_BYTES;
    for index in 0..accepted {
        let key = format!("s:{index:0>key_width$}");
        store
            .track_request_id("session", &key)
            .expect("request id within aggregate replay budget");
    }
    let overflow = format!("s:{accepted:0>key_width$}");
    let error = store
        .track_request_id("session", &overflow)
        .expect_err("aggregate request-id bytes must be bounded");
    assert_eq!(error.kind, McpHttpSessionErrorKind::RequestIdLimit);
    let session = store
        .sessions
        .get("session")
        .expect("session remains available");
    assert!(session.seen_request_id_bytes <= MAX_MCP_HTTP_REQUEST_ID_REPLAY_BYTES);
}

#[test]
fn request_id_replay_bytes_have_a_global_store_budget() {
    let full_sessions =
        MAX_MCP_HTTP_GLOBAL_REQUEST_ID_REPLAY_BYTES / MAX_MCP_HTTP_REQUEST_ID_REPLAY_BYTES;
    let mut store = McpHttpSessionStore::new(full_sessions + 1, 60_000);
    for index in 0..full_sessions {
        let session_id = format!("full-{index}");
        store
            .create_or_replace(session_id.clone(), "2025-11-25", None, None, None, 1)
            .expect("session should be created");
        store
            .sessions
            .get_mut(&session_id)
            .expect("session remains available")
            .seen_request_id_bytes = MAX_MCP_HTTP_REQUEST_ID_REPLAY_BYTES;
    }
    store
        .create_or_replace("target".to_string(), "2025-11-25", None, None, None, 1)
        .expect("empty target session should be created at the byte boundary");

    let error = store
        .track_request_id("target", "n:1")
        .expect_err("global replay bytes must not exceed the store budget");
    assert_eq!(error.kind, McpHttpSessionErrorKind::RequestIdLimit);
    assert!(error.message.contains("global"));
}

#[test]
fn initialize_metadata_is_byte_bounded() {
    let mut store = McpHttpSessionStore::new(1, 60_000);
    let error = store
        .create_or_replace(
            "session".to_string(),
            "2025-11-25",
            Some("x".repeat(MAX_MCP_HTTP_CLIENT_INFO_FIELD_BYTES + 1)),
            None,
            None,
            1,
        )
        .expect_err("oversized client metadata must be rejected");
    assert_eq!(error.kind, McpHttpSessionErrorKind::Invalid);
    assert!(store.sessions.is_empty());
}
