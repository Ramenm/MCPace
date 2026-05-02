use super::http_boundary::request_header_string;
use super::HttpRequest;
use crate::json::JsonValue;
use crate::json_helpers;

pub(super) fn validate_mcp_standard_headers(
    request: &HttpRequest,
    message: &JsonValue,
    method: &str,
) -> Result<(), String> {
    if let Some(header_method) = request_header_string(Some(request), "mcp-method") {
        if header_method != method {
            return Err(format!(
                "Mcp-Method header '{}' does not match JSON-RPC method '{}'",
                header_method, method
            ));
        }
    }

    let header_name = request_header_string(Some(request), "mcp-name");
    let expected_name = mcp_standard_header_name(message, method);
    match (header_name, expected_name) {
        (Some(actual), Some(expected)) if actual != expected => Err(format!(
            "Mcp-Name header '{}' does not match JSON-RPC body value '{}'",
            actual, expected
        )),
        (Some(actual), None) => Err(format!(
            "Mcp-Name header '{}' is not valid for JSON-RPC method '{}'",
            actual, method
        )),
        _ => Ok(()),
    }
}

fn mcp_standard_header_name<'a>(message: &'a JsonValue, method: &str) -> Option<&'a str> {
    match method {
        "tools/call" | "prompts/get" => json_helpers::string_at_path(message, &["params", "name"]),
        "resources/read" => json_helpers::string_at_path(message, &["params", "uri"]),
        _ => None,
    }
}
