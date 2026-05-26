use super::write::McpServerWriteOptions;
use crate::json::JsonValue;
use std::collections::BTreeMap;

pub(super) fn build_server_entry(
    server_type: &str,
    options: &McpServerWriteOptions,
    env: &BTreeMap<String, String>,
    headers: &BTreeMap<String, String>,
) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert("enabled".to_string(), JsonValue::bool(options.enabled));
    object.insert(
        "type".to_string(),
        JsonValue::string(server_type.to_string()),
    );
    if let Some(command) = options
        .command
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        object.insert(
            "command".to_string(),
            JsonValue::string(command.to_string()),
        );
    }
    if !options.args.is_empty() {
        object.insert(
            "args".to_string(),
            JsonValue::array(options.args.iter().cloned().map(JsonValue::string)),
        );
    }
    if let Some(url) = options
        .url
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        object.insert("url".to_string(), JsonValue::string(url.to_string()));
    }
    let mut profile_hints = options
        .profile_hints
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<String>>();
    profile_hints.sort();
    profile_hints.dedup();
    if !profile_hints.is_empty() {
        object.insert(
            "mcpaceProfileHints".to_string(),
            JsonValue::array(profile_hints.into_iter().map(JsonValue::string)),
        );
    }
    if !env.is_empty() {
        object.insert(
            "env".to_string(),
            JsonValue::object(
                env.iter()
                    .map(|(key, value)| (key.clone(), JsonValue::string(value.clone()))),
            ),
        );
    }
    if !headers.is_empty() {
        object.insert(
            "headers".to_string(),
            JsonValue::object(
                headers
                    .iter()
                    .map(|(key, value)| (key.clone(), JsonValue::string(value.clone()))),
            ),
        );
    }
    JsonValue::Object(object)
}

pub(super) fn normalize_server_type(
    requested: Option<&str>,
    has_command: bool,
    has_url: bool,
) -> Result<String, String> {
    let normalized = crate::source_type::infer_public_source_type(
        requested.unwrap_or(""),
        if has_command { "command" } else { "" },
        if has_url {
            "https://example.invalid/mcp"
        } else {
            ""
        },
    );
    match normalized.as_str() {
        "stdio" | "streamable-http" => Ok(normalized),
        other => Err(format!(
            "unsupported MCP server type '{}'; use stdio or streamable-http",
            other
        )),
    }
}

pub(super) fn validate_remote_mcp_url(value: &str) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("server add --url requires a non-empty URL".to_string());
    }
    if trimmed
        .bytes()
        .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err("server add --url cannot contain whitespace or control characters".to_string());
    }
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(
            "server add --url currently accepts only http:// or https:// MCP endpoints".to_string(),
        );
    }
    Ok(())
}

pub(super) fn parse_key_value_pairs(
    values: &[String],
    flag_name: &str,
    validate_key: fn(&str) -> bool,
) -> Result<BTreeMap<String, String>, String> {
    let mut parsed = BTreeMap::new();
    for raw in values {
        let Some((key, value)) = raw.split_once('=') else {
            return Err(format!("{} expects KEY=VALUE, got '{}'", flag_name, raw));
        };
        let key = key.trim();
        if key.is_empty() || !validate_key(key) {
            return Err(format!("{} contains an invalid key '{}'", flag_name, key));
        }
        if value.contains('\0') || value.contains('\r') || value.contains('\n') {
            return Err(format!(
                "{} value for '{}' contains a disallowed control character",
                flag_name, key
            ));
        }
        parsed.insert(key.to_string(), value.to_string());
    }
    Ok(parsed)
}

pub(super) fn validate_env_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

pub(super) fn validate_http_header_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~'))
}
