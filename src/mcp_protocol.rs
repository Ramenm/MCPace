use crate::json::JsonValue;
use crate::json_helpers;
use std::fmt;

pub const CURRENT_PROTOCOL_VERSION: &str = "2025-11-25";
pub const STREAMABLE_HTTP_DEFAULT_PROTOCOL_VERSION: &str = "2025-03-26";
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
    &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
pub const SERVER_NAME: &str = "mcpace";

pub const JSONRPC_VERSION: &str = "2.0";
pub const ERROR_PARSE: i64 = -32700;
pub const ERROR_INVALID_REQUEST: i64 = -32600;
pub const ERROR_METHOD_NOT_FOUND: i64 = -32601;
pub const ERROR_INVALID_PARAMS: i64 = -32602;
pub const ERROR_INTERNAL: i64 = -32603;
pub const ERROR_HEADER_MISMATCH: i64 = -32001;
pub const ERROR_NOT_INITIALIZED: i64 = -32002;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpProtocolError {
    MessageNotObject,
    JsonRpcVersionMismatch { actual: Option<String> },
    MethodMissing,
    MethodNotString,
    MethodEmpty,
    ParamsNotObject,
    IdMustBeIntegerNumber,
    IdMustNotBeNull,
    IdMustBeStringOrInteger,
    ArgumentsNotObject { method_label: String },
    ResponseJsonRpcVersionMismatch { actual: Option<String> },
    ResponseIdMismatch { expected: i64 },
    ResponseHasBothResultAndError,
    ResponseMissingResultOrError,
    ResponseErrorNotObject,
}

impl McpProtocolError {
    #[cfg(test)]
    pub fn contains(&self, needle: &str) -> bool {
        self.to_string().contains(needle)
    }
}

impl fmt::Display for McpProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            McpProtocolError::MessageNotObject => {
                formatter.write_str("JSON-RPC message must be an object")
            }
            McpProtocolError::JsonRpcVersionMismatch {
                actual: Some(actual),
            } => write!(
                formatter,
                "JSON-RPC request must declare jsonrpc \"2.0\"; got '{}'",
                actual
            ),
            McpProtocolError::JsonRpcVersionMismatch { actual: None } => {
                formatter.write_str("JSON-RPC request must declare jsonrpc \"2.0\"")
            }
            McpProtocolError::MethodMissing => {
                formatter.write_str("JSON-RPC request requires a method")
            }
            McpProtocolError::MethodNotString => {
                formatter.write_str("JSON-RPC method must be a string")
            }
            McpProtocolError::MethodEmpty => {
                formatter.write_str("JSON-RPC method must be non-empty")
            }
            McpProtocolError::ParamsNotObject => {
                formatter.write_str("MCP params must be a JSON object when present")
            }
            McpProtocolError::IdMustBeIntegerNumber => {
                formatter.write_str("JSON-RPC request id must be an integer number when numeric")
            }
            McpProtocolError::IdMustNotBeNull => {
                formatter.write_str("JSON-RPC request id must not be null")
            }
            McpProtocolError::IdMustBeStringOrInteger => {
                formatter.write_str("JSON-RPC id must be a string or integer number when present")
            }
            McpProtocolError::ArgumentsNotObject { method_label } => write!(
                formatter,
                "{} arguments must be a JSON object when present",
                method_label
            ),
            McpProtocolError::ResponseJsonRpcVersionMismatch {
                actual: Some(actual),
            } => write!(
                formatter,
                "JSON-RPC response must declare jsonrpc \"2.0\"; got '{}'",
                actual
            ),
            McpProtocolError::ResponseJsonRpcVersionMismatch { actual: None } => {
                formatter.write_str("JSON-RPC response must declare jsonrpc \"2.0\"")
            }
            McpProtocolError::ResponseIdMismatch { expected } => write!(
                formatter,
                "JSON-RPC response id does not match request id {}",
                expected
            ),
            McpProtocolError::ResponseHasBothResultAndError => {
                formatter.write_str("JSON-RPC response cannot contain both result and error")
            }
            McpProtocolError::ResponseMissingResultOrError => {
                formatter.write_str("JSON-RPC response must contain exactly one of result or error")
            }
            McpProtocolError::ResponseErrorNotObject => {
                formatter.write_str("JSON-RPC response error must be an object")
            }
        }
    }
}

impl std::error::Error for McpProtocolError {}

impl From<McpProtocolError> for String {
    fn from(error: McpProtocolError) -> Self {
        error.to_string()
    }
}

pub fn is_supported_protocol_version(requested: &str) -> bool {
    SUPPORTED_PROTOCOL_VERSIONS.contains(&requested)
}

pub fn method_is_notification(method: &str) -> bool {
    method.starts_with("notifications/")
}

pub fn negotiate_protocol_version(requested: &str) -> &'static str {
    SUPPORTED_PROTOCOL_VERSIONS
        .iter()
        .copied()
        .find(|candidate| *candidate == requested)
        .unwrap_or(CURRENT_PROTOCOL_VERSION)
}

pub const MAX_REQUEST_ID_BYTES: usize = 256;

pub fn request_id(message: &JsonValue) -> Option<JsonValue> {
    json_helpers::value_at_path(message, &["id"]).cloned()
}

pub fn request_id_key(id: &JsonValue) -> Option<String> {
    match id {
        JsonValue::String(value) => Some(format!("s:{}", value)),
        JsonValue::Number(value) if is_integer_number_text(value) => Some(format!("n:{}", value)),
        _ => None,
    }
}

pub fn request_id_byte_len(id: &JsonValue) -> Option<usize> {
    match id {
        JsonValue::String(value) => Some(value.len()),
        JsonValue::Number(value) if is_integer_number_text(value) => Some(value.len()),
        _ => None,
    }
}

pub fn request_id_is_within_limit(id: &JsonValue) -> bool {
    request_id_byte_len(id).is_some_and(|length| length <= MAX_REQUEST_ID_BYTES)
}

pub fn is_request_id_value(id: &JsonValue) -> bool {
    match id {
        JsonValue::String(_) => true,
        JsonValue::Number(value) => is_integer_number_text(value),
        _ => false,
    }
}

fn is_integer_number_text(value: &str) -> bool {
    let digits = value.strip_prefix('-').unwrap_or(value);
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
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
    json_helpers::empty_object()
}

pub fn validate_request_envelope(message: &JsonValue) -> Result<(), McpProtocolError> {
    let object = message
        .as_object()
        .ok_or(McpProtocolError::MessageNotObject)?;

    match object.get("jsonrpc").and_then(JsonValue::as_str) {
        Some(JSONRPC_VERSION) => {}
        Some(value) => {
            return Err(McpProtocolError::JsonRpcVersionMismatch {
                actual: Some(value.to_string()),
            });
        }
        None => {
            return Err(McpProtocolError::JsonRpcVersionMismatch { actual: None });
        }
    }

    match object.get("method") {
        Some(JsonValue::String(value)) if !value.trim().is_empty() => {}
        Some(JsonValue::String(_)) => return Err(McpProtocolError::MethodEmpty),
        Some(_) => return Err(McpProtocolError::MethodNotString),
        None => return Err(McpProtocolError::MethodMissing),
    }

    if let Some(params) = object.get("params") {
        if !matches!(params, JsonValue::Object(_)) {
            return Err(McpProtocolError::ParamsNotObject);
        }
    }

    if let Some(id) = object.get("id") {
        match id {
            JsonValue::String(_) => {}
            JsonValue::Number(value) if is_integer_number_text(value) => {}
            JsonValue::Number(_) => return Err(McpProtocolError::IdMustBeIntegerNumber),
            JsonValue::Null => return Err(McpProtocolError::IdMustNotBeNull),
            _ => return Err(McpProtocolError::IdMustBeStringOrInteger),
        }
    }

    Ok(())
}

pub fn validate_response_envelope(
    message: &JsonValue,
    expected_id: i64,
) -> Result<(), McpProtocolError> {
    let object = message
        .as_object()
        .ok_or(McpProtocolError::MessageNotObject)?;

    match object.get("jsonrpc").and_then(JsonValue::as_str) {
        Some(JSONRPC_VERSION) => {}
        Some(value) => {
            return Err(McpProtocolError::ResponseJsonRpcVersionMismatch {
                actual: Some(value.to_string()),
            });
        }
        None => {
            return Err(McpProtocolError::ResponseJsonRpcVersionMismatch { actual: None });
        }
    }

    let id_matches = object
        .get("id")
        .and_then(JsonValue::as_i64)
        .map(|id| id == expected_id)
        .unwrap_or(false);
    if !id_matches {
        return Err(McpProtocolError::ResponseIdMismatch {
            expected: expected_id,
        });
    }

    match (object.contains_key("result"), object.get("error")) {
        (true, Some(_)) => Err(McpProtocolError::ResponseHasBothResultAndError),
        (false, None) => Err(McpProtocolError::ResponseMissingResultOrError),
        (false, Some(JsonValue::Object(_))) | (true, None) => Ok(()),
        (false, Some(_)) => Err(McpProtocolError::ResponseErrorNotObject),
    }
}

pub fn params_arguments_object_or_empty(
    message: &JsonValue,
    method_label: &str,
) -> Result<JsonValue, McpProtocolError> {
    match json_helpers::value_at_path(message, &["params", "arguments"]) {
        Some(value @ JsonValue::Object(_)) => Ok(value.clone()),
        Some(JsonValue::Null) | None => Ok(empty_object()),
        Some(_) => Err(McpProtocolError::ArgumentsNotObject {
            method_label: method_label.to_string(),
        }),
    }
}

pub fn tool_call_arguments_or_empty(message: &JsonValue) -> Result<JsonValue, McpProtocolError> {
    params_arguments_object_or_empty(message, "tools/call")
}

#[cfg(test)]
mod tests;
