use crate::json::JsonValue;
use crate::json_helpers;
use std::collections::BTreeMap;

pub const CURRENT_PROTOCOL_VERSION: &str = "2025-11-25";
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
    &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
pub const SERVER_NAME: &str = "mcpace";

pub const JSONRPC_VERSION: &str = "2.0";
pub const ERROR_PARSE: i64 = -32700;
pub const ERROR_INVALID_REQUEST: i64 = -32600;
pub const ERROR_METHOD_NOT_FOUND: i64 = -32601;
pub const ERROR_INVALID_PARAMS: i64 = -32602;
pub const ERROR_NOT_INITIALIZED: i64 = -32002;

pub fn negotiate_protocol_version(requested: &str) -> &'static str {
    SUPPORTED_PROTOCOL_VERSIONS
        .iter()
        .copied()
        .find(|candidate| *candidate == requested)
        .unwrap_or(CURRENT_PROTOCOL_VERSION)
}

pub fn request_id(message: &JsonValue) -> Option<JsonValue> {
    json_helpers::value_at_path(message, &["id"]).cloned()
}

pub fn request_id_or_null(message: &JsonValue) -> JsonValue {
    request_id(message).unwrap_or(JsonValue::Null)
}

pub fn result(id: JsonValue, result: JsonValue) -> JsonValue {
    JsonValue::object([
        ("jsonrpc", JsonValue::string(JSONRPC_VERSION)),
        ("id", id),
        ("result", result),
    ])
}

pub fn error(id: JsonValue, code: i64, message: &str, data: Option<JsonValue>) -> JsonValue {
    let error_value = match data {
        Some(value) => JsonValue::object([
            ("code", JsonValue::number(code)),
            ("message", JsonValue::string(message)),
            ("data", value),
        ]),
        None => JsonValue::object([
            ("code", JsonValue::number(code)),
            ("message", JsonValue::string(message)),
        ]),
    };

    JsonValue::object([
        ("jsonrpc", JsonValue::string(JSONRPC_VERSION)),
        ("id", id),
        ("error", error_value),
    ])
}

pub fn empty_object() -> JsonValue {
    JsonValue::Object(BTreeMap::new())
}

#[cfg(test)]
mod tests {
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
}
