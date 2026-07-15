use super::*;

#[test]
fn negotiate_protocol_version_preserves_supported_versions_and_falls_back() {
    assert_eq!(negotiate_protocol_version("2025-06-18"), "2025-06-18");
    assert_eq!(
        negotiate_protocol_version("2099-01-01"),
        CURRENT_PROTOCOL_VERSION
    );
}

#[test]
fn request_id_distinguishes_requests_from_notifications() {
    let request = JsonValue::object([
        ("jsonrpc", JsonValue::string(JSONRPC_VERSION)),
        ("id", JsonValue::number(7)),
        ("method", JsonValue::string("ping")),
    ]);
    let notification = JsonValue::object([
        ("jsonrpc", JsonValue::string(JSONRPC_VERSION)),
        ("method", JsonValue::string("notifications/initialized")),
    ]);

    assert_eq!(request_id(&request), Some(JsonValue::number(7)));
    assert_eq!(request_id(&notification), None);
    assert_eq!(request_id_or_null(&notification), JsonValue::Null);
}

#[test]
fn validate_request_envelope_rejects_null_ids() {
    let request = JsonValue::object([
        ("jsonrpc", JsonValue::string(JSONRPC_VERSION)),
        ("id", JsonValue::Null),
        ("method", JsonValue::string("tools/list")),
    ]);

    assert!(validate_request_envelope(&request)
        .expect_err("null request IDs must be rejected")
        .contains("must not be null"));
}

#[test]
fn request_id_key_preserves_string_number_distinction() {
    assert_eq!(
        request_id_key(&JsonValue::number(1)).as_deref(),
        Some("n:1")
    );
    assert_eq!(
        request_id_key(&JsonValue::string("1")).as_deref(),
        Some("s:1")
    );
    assert_eq!(request_id_key(&JsonValue::Null), None);
    assert_eq!(
        request_id_key(&JsonValue::number("9007199254740993123456789")).as_deref(),
        Some("n:9007199254740993123456789")
    );
    assert_eq!(request_id_key(&JsonValue::number("1.5")), None);
    assert_eq!(request_id_key(&JsonValue::number("6e+23")), None);
}

#[test]
fn request_id_storage_limit_is_byte_bounded() {
    assert!(request_id_is_within_limit(&JsonValue::string(
        "x".repeat(MAX_REQUEST_ID_BYTES)
    )));
    assert!(!request_id_is_within_limit(&JsonValue::string(
        "x".repeat(MAX_REQUEST_ID_BYTES + 1)
    )));
    assert!(!request_id_is_within_limit(&JsonValue::Null));
}

#[test]
fn validate_request_envelope_rejects_decimal_numeric_ids() {
    let request = JsonValue::object([
        ("jsonrpc", JsonValue::string(JSONRPC_VERSION)),
        ("id", JsonValue::number("1.5")),
        ("method", JsonValue::string("tools/list")),
    ]);

    assert!(validate_request_envelope(&request)
        .expect_err("numeric request IDs must be integers")
        .contains("integer"));
}

#[test]
fn validate_response_envelope_rejects_buggy_or_ambiguous_messages() {
    let valid = result(JsonValue::number(7), JsonValue::Null);
    assert!(validate_response_envelope(&valid, 7).is_ok());

    let cases = [
        JsonValue::object([
            ("jsonrpc", JsonValue::string("1.0")),
            ("id", JsonValue::number(7)),
            ("result", JsonValue::Null),
        ]),
        JsonValue::object([
            ("jsonrpc", JsonValue::string(JSONRPC_VERSION)),
            ("id", JsonValue::number(8)),
            ("result", JsonValue::Null),
        ]),
        JsonValue::object([
            ("jsonrpc", JsonValue::string(JSONRPC_VERSION)),
            ("id", JsonValue::number(7)),
            ("result", JsonValue::Null),
            (
                "error",
                JsonValue::object([
                    ("code", JsonValue::number(-32603)),
                    ("message", JsonValue::string("ambiguous")),
                ]),
            ),
        ]),
        JsonValue::object([
            ("jsonrpc", JsonValue::string(JSONRPC_VERSION)),
            ("id", JsonValue::number(7)),
        ]),
        JsonValue::object([
            ("jsonrpc", JsonValue::string(JSONRPC_VERSION)),
            ("id", JsonValue::number(7)),
            ("error", JsonValue::string("not-an-error-object")),
        ]),
    ];

    for response in cases {
        assert!(
            validate_response_envelope(&response, 7).is_err(),
            "malformed response should be rejected: {}",
            response.to_compact_string()
        );
    }
}

#[test]
fn validate_request_envelope_rejects_array_params() {
    let request = JsonValue::object([
        ("jsonrpc", JsonValue::string(JSONRPC_VERSION)),
        ("id", JsonValue::number(7)),
        ("method", JsonValue::string("tools/list")),
        ("params", JsonValue::array([JsonValue::string("cursor")])),
    ]);

    assert!(validate_request_envelope(&request)
        .expect_err("MCP params must use object form")
        .contains("params"));
}
