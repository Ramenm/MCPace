use super::PROXIED_RESOURCE_SCHEME;
use crate::json::JsonValue;

pub(super) fn encode_resource_uri(server: &str, upstream_uri: &str) -> String {
    format!(
        "{}/{}/{}",
        PROXIED_RESOURCE_SCHEME,
        hex_encode(server.as_bytes()),
        hex_encode(upstream_uri.as_bytes())
    )
}

pub(super) fn decode_resource_uri(value: &str) -> Result<(String, String), String> {
    let prefix = format!("{}/", PROXIED_RESOURCE_SCHEME);
    let rest = value.strip_prefix(&prefix).ok_or_else(|| {
        format!(
            "resource uri '{}' is not a MCPace proxied upstream resource",
            value
        )
    })?;
    let (encoded_server, encoded_uri) = rest.split_once('/').ok_or_else(|| {
        format!(
            "resource uri '{}' is missing upstream server or payload",
            value
        )
    })?;
    let server = String::from_utf8(hex_decode(encoded_server)?)
        .map_err(|error| format!("proxied resource server is not UTF-8: {}", error))?;
    let uri = String::from_utf8(hex_decode(encoded_uri)?)
        .map_err(|error| format!("proxied resource uri is not UTF-8: {}", error))?;
    Ok((server, uri))
}

pub(super) fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode(value: &str) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) {
        return Err("hex payload length must be even".to_string());
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    let mut chars = value.as_bytes().iter().copied();
    while let (Some(high), Some(low)) = (chars.next(), chars.next()) {
        bytes.push((hex_value(high)? << 4) | hex_value(low)?);
    }
    Ok(bytes)
}

fn hex_value(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!("invalid hex byte '{}'", byte as char)),
    }
}

pub(super) fn maybe_meta_errors(errors: Vec<JsonValue>) -> (&'static str, JsonValue) {
    (
        "_meta",
        JsonValue::object([("mcpace/errors", JsonValue::array(errors))]),
    )
}

pub(super) fn is_unsupported_method_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("method not found")
        || lower.contains("unsupported")
        || lower.contains("unknown method")
        || lower.contains("-32601")
}
