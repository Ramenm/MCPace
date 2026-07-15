use super::write::McpServerWriteOptions;
use crate::json::JsonValue;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum McpSourceWriteValidationError {
    UnsupportedServerType { value: String },
    InvalidUrl { reason: &'static str },
    InvalidKeyValue { flag_name: String, reason: String },
}

pub(super) type McpSourceWriteValidationResult<T> =
    std::result::Result<T, McpSourceWriteValidationError>;

impl McpSourceWriteValidationError {
    fn unsupported_server_type(value: impl Into<String>) -> Self {
        Self::UnsupportedServerType {
            value: value.into(),
        }
    }

    fn invalid_url(reason: &'static str) -> Self {
        Self::InvalidUrl { reason }
    }

    fn invalid_key_value(flag_name: &str, reason: impl Into<String>) -> Self {
        Self::InvalidKeyValue {
            flag_name: flag_name.to_string(),
            reason: reason.into(),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(super) fn contains(&self, needle: &str) -> bool {
        self.to_string().contains(needle)
    }
}

impl fmt::Display for McpSourceWriteValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedServerType { value } => write!(
                formatter,
                "unsupported MCP server type '{}'; use stdio or streamable-http",
                value
            ),
            Self::InvalidUrl { reason } => formatter.write_str(reason),
            Self::InvalidKeyValue { reason, .. } => formatter.write_str(reason),
        }
    }
}

impl std::error::Error for McpSourceWriteValidationError {}

impl From<McpSourceWriteValidationError> for String {
    fn from(error: McpSourceWriteValidationError) -> Self {
        error.to_string()
    }
}

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
) -> McpSourceWriteValidationResult<String> {
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
        other => Err(McpSourceWriteValidationError::unsupported_server_type(
            other,
        )),
    }
}

pub(super) fn validate_remote_mcp_url(value: &str) -> McpSourceWriteValidationResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(McpSourceWriteValidationError::invalid_url(
            "server add --url requires a non-empty URL",
        ));
    }
    if trimmed
        .bytes()
        .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err(McpSourceWriteValidationError::invalid_url(
            "server add --url cannot contain whitespace or control characters",
        ));
    }
    if trimmed.contains('#') || trimmed.contains('\\') {
        return Err(McpSourceWriteValidationError::invalid_url(
            "server add --url must not contain a URL fragment or backslash",
        ));
    }
    let normalized = trimmed.to_ascii_lowercase();
    let (plain_http, rest) = if normalized.starts_with("http://") {
        (true, &trimmed["http://".len()..])
    } else if normalized.starts_with("https://") {
        (false, &trimmed["https://".len()..])
    } else {
        return Err(McpSourceWriteValidationError::invalid_url(
            "server add --url currently accepts only http:// or https:// MCP endpoints",
        ));
    };
    let authority = rest.split(['/', '?']).next().unwrap_or("");
    validate_remote_mcp_authority(authority)?;
    if plain_http && !remote_mcp_authority_is_loopback(authority) {
        return Err(McpSourceWriteValidationError::invalid_url(
            "server add --url permits plain http:// only for loopback MCP endpoints; use https:// or a local gateway",
        ));
    }
    Ok(())
}

fn remote_mcp_authority_is_loopback(authority: &str) -> bool {
    let host = if let Some(rest) = authority.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    let normalized = host.trim().trim_matches(['[', ']'].as_ref());
    normalized.eq_ignore_ascii_case("localhost")
        || normalized
            .parse::<std::net::IpAddr>()
            .map(|address| match address {
                std::net::IpAddr::V4(address) => address.is_loopback(),
                std::net::IpAddr::V6(address) => {
                    address.is_loopback()
                        || address
                            .to_ipv4_mapped()
                            .map(|mapped| mapped.is_loopback())
                            .unwrap_or(false)
                }
            })
            .unwrap_or(false)
}

fn validate_remote_mcp_authority(authority: &str) -> McpSourceWriteValidationResult<()> {
    if authority.is_empty()
        || authority.contains('/')
        || authority.contains('@')
        || authority
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err(McpSourceWriteValidationError::invalid_url(
            "server add --url has an invalid authority",
        ));
    }
    if authority.starts_with('[') {
        let Some(end) = authority.find(']') else {
            return Err(McpSourceWriteValidationError::invalid_url(
                "server add --url has an invalid bracketed IPv6 authority",
            ));
        };
        let host = &authority[1..end];
        if host.trim().is_empty()
            || host
                .bytes()
                .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        {
            return Err(McpSourceWriteValidationError::invalid_url(
                "server add --url has an invalid IPv6 host",
            ));
        }
        return validate_remote_mcp_port_suffix(&authority[end + 1..]);
    }
    if authority.matches(':').count() > 1 {
        return Err(McpSourceWriteValidationError::invalid_url(
            "server add --url IPv6 authorities must be bracketed",
        ));
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => {
            validate_remote_mcp_host(host)?;
            validate_remote_mcp_port(port)
        }
        Some(_) => Err(McpSourceWriteValidationError::invalid_url(
            "server add --url has an invalid host or port",
        )),
        None => validate_remote_mcp_host(authority),
    }
}

fn validate_remote_mcp_host(host: &str) -> McpSourceWriteValidationResult<()> {
    if host.trim().is_empty()
        || host
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err(McpSourceWriteValidationError::invalid_url(
            "server add --url has an invalid host",
        ));
    }
    Ok(())
}

fn validate_remote_mcp_port_suffix(value: &str) -> McpSourceWriteValidationResult<()> {
    if value.is_empty() {
        return Ok(());
    }
    let Some(port) = value.strip_prefix(':') else {
        return Err(McpSourceWriteValidationError::invalid_url(
            "server add --url has an invalid bracketed authority suffix",
        ));
    };
    validate_remote_mcp_port(port)
}

fn validate_remote_mcp_port(port: &str) -> McpSourceWriteValidationResult<()> {
    if port
        .parse::<u16>()
        .ok()
        .filter(|value| *value > 0)
        .is_some()
    {
        Ok(())
    } else {
        Err(McpSourceWriteValidationError::invalid_url(
            "server add --url has an invalid port",
        ))
    }
}

pub(super) fn parse_key_value_pairs(
    values: &[String],
    flag_name: &str,
    validate_key: fn(&str) -> bool,
) -> McpSourceWriteValidationResult<BTreeMap<String, String>> {
    let mut parsed = BTreeMap::new();
    for raw in values {
        let Some((key, value)) = raw.split_once('=') else {
            return Err(McpSourceWriteValidationError::invalid_key_value(
                flag_name,
                format!("{} expects KEY=VALUE, got '{}'", flag_name, raw),
            ));
        };
        let key = key.trim();
        if key.is_empty() || !validate_key(key) {
            return Err(McpSourceWriteValidationError::invalid_key_value(
                flag_name,
                format!("{} contains an invalid key '{}'", flag_name, key),
            ));
        }
        if parsed
            .keys()
            .any(|existing: &String| existing.eq_ignore_ascii_case(key))
        {
            return Err(McpSourceWriteValidationError::invalid_key_value(
                flag_name,
                format!(
                    "{} contains duplicate key '{}' with ambiguous casing",
                    flag_name, key
                ),
            ));
        }
        if value.contains('\0') || value.contains('\r') || value.contains('\n') {
            return Err(McpSourceWriteValidationError::invalid_key_value(
                flag_name,
                format!(
                    "{} value for '{}' contains a disallowed control character",
                    flag_name, key
                ),
            ));
        }
        if flag_name == "--header" && !validate_http_header_value(value) {
            return Err(McpSourceWriteValidationError::invalid_key_value(
                flag_name,
                format!(
                    "{} value for '{}' is not a safe HTTP header value",
                    flag_name, key
                ),
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
    crate::text_utils::valid_http_header_name(value)
        && !crate::text_utils::reserved_mcp_http_header_name(value)
}

fn validate_http_header_value(value: &str) -> bool {
    crate::text_utils::valid_http_field_value(value)
}
