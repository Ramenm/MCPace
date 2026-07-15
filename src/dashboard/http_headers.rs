use super::http_boundary::request_header_string_unique;
use super::HttpRequest;
use crate::json::JsonValue;
use crate::json_helpers;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum McpHeaderValidationError {
    Boundary {
        source: String,
    },
    MethodMismatch {
        header_method: String,
        method: String,
    },
    NameMismatch {
        actual: String,
        expected: String,
    },
    NameNotValidForMethod {
        actual: String,
        method: String,
    },
}

impl McpHeaderValidationError {
    #[cfg(test)]
    #[allow(dead_code)]
    pub(super) fn contains(&self, needle: &str) -> bool {
        self.to_string().contains(needle)
    }
}

impl fmt::Display for McpHeaderValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            McpHeaderValidationError::Boundary { source } => formatter.write_str(source),
            McpHeaderValidationError::MethodMismatch {
                header_method,
                method,
            } => write!(
                formatter,
                "Mcp-Method header '{}' does not match JSON-RPC method '{}'",
                header_method, method
            ),
            McpHeaderValidationError::NameMismatch { actual, expected } => write!(
                formatter,
                "Mcp-Name header '{}' does not match JSON-RPC body value '{}'",
                actual, expected
            ),
            McpHeaderValidationError::NameNotValidForMethod { actual, method } => write!(
                formatter,
                "Mcp-Name header '{}' is not valid for JSON-RPC method '{}'",
                actual, method
            ),
        }
    }
}

impl std::error::Error for McpHeaderValidationError {}

impl From<super::http_boundary::HttpBoundaryError> for McpHeaderValidationError {
    fn from(error: super::http_boundary::HttpBoundaryError) -> Self {
        McpHeaderValidationError::Boundary {
            source: error.to_string(),
        }
    }
}

impl From<McpHeaderValidationError> for String {
    fn from(error: McpHeaderValidationError) -> Self {
        error.to_string()
    }
}

pub(super) fn validate_mcp_standard_headers(
    request: &HttpRequest,
    message: &JsonValue,
    method: &str,
) -> Result<(), McpHeaderValidationError> {
    if let Some(header_method) = request_header_string_unique(Some(request), "mcp-method")? {
        if header_method != method {
            return Err(McpHeaderValidationError::MethodMismatch {
                header_method,
                method: method.to_string(),
            });
        }
    }

    let header_name = request_header_string_unique(Some(request), "mcp-name")?;
    let expected_name = mcp_standard_header_name(message, method);
    match (header_name, expected_name) {
        (Some(actual), Some(expected)) if actual != expected => {
            Err(McpHeaderValidationError::NameMismatch {
                actual,
                expected: expected.to_string(),
            })
        }
        (Some(actual), None) => Err(McpHeaderValidationError::NameNotValidForMethod {
            actual,
            method: method.to_string(),
        }),
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
