use super::PROXIED_RESOURCE_SCHEME;
use crate::json::JsonValue;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ProxyResourceUriError {
    NotProxied { uri: String },
    MissingPayload { uri: String },
    InvalidHexLength,
    InvalidHexByte { byte: char },
    ServerUtf8 { source: String },
    UriUtf8 { source: String },
}

impl ProxyResourceUriError {
    #[cfg(test)]
    #[allow(dead_code)]
    pub(super) fn contains(&self, needle: &str) -> bool {
        self.to_string().contains(needle)
    }
}

impl fmt::Display for ProxyResourceUriError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProxyResourceUriError::NotProxied { uri } => write!(
                formatter,
                "resource uri '{}' is not a MCPace proxied upstream resource",
                uri
            ),
            ProxyResourceUriError::MissingPayload { uri } => {
                write!(
                    formatter,
                    "resource uri '{}' is missing upstream server or payload",
                    uri
                )
            }
            ProxyResourceUriError::InvalidHexLength => {
                formatter.write_str("hex payload length must be even")
            }
            ProxyResourceUriError::InvalidHexByte { byte } => {
                write!(formatter, "invalid hex byte '{}'", byte)
            }
            ProxyResourceUriError::ServerUtf8 { source } => {
                write!(
                    formatter,
                    "proxied resource server is not UTF-8: {}",
                    source
                )
            }
            ProxyResourceUriError::UriUtf8 { source } => {
                write!(formatter, "proxied resource uri is not UTF-8: {}", source)
            }
        }
    }
}

impl std::error::Error for ProxyResourceUriError {}

impl From<ProxyResourceUriError> for String {
    fn from(error: ProxyResourceUriError) -> Self {
        error.to_string()
    }
}

pub(super) fn encode_resource_uri(server: &str, upstream_uri: &str) -> String {
    format!(
        "{}/{}/{}",
        PROXIED_RESOURCE_SCHEME,
        hex_encode(server.as_bytes()),
        hex_encode(upstream_uri.as_bytes())
    )
}

pub(super) fn decode_resource_uri(value: &str) -> Result<(String, String), ProxyResourceUriError> {
    let prefix = format!("{}/", PROXIED_RESOURCE_SCHEME);
    let rest = value
        .strip_prefix(&prefix)
        .ok_or_else(|| ProxyResourceUriError::NotProxied {
            uri: value.to_string(),
        })?;
    let (encoded_server, encoded_uri) =
        rest.split_once('/')
            .ok_or_else(|| ProxyResourceUriError::MissingPayload {
                uri: value.to_string(),
            })?;
    let server = String::from_utf8(hex_decode(encoded_server)?).map_err(|error| {
        ProxyResourceUriError::ServerUtf8 {
            source: error.to_string(),
        }
    })?;
    let uri = String::from_utf8(hex_decode(encoded_uri)?).map_err(|error| {
        ProxyResourceUriError::UriUtf8 {
            source: error.to_string(),
        }
    })?;
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

fn hex_decode(value: &str) -> Result<Vec<u8>, ProxyResourceUriError> {
    if !value.len().is_multiple_of(2) {
        return Err(ProxyResourceUriError::InvalidHexLength);
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    let mut chars = value.as_bytes().iter().copied();
    while let (Some(high), Some(low)) = (chars.next(), chars.next()) {
        bytes.push((hex_value(high)? << 4) | hex_value(low)?);
    }
    Ok(bytes)
}

fn hex_value(byte: u8) -> Result<u8, ProxyResourceUriError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(ProxyResourceUriError::InvalidHexByte { byte: byte as char }),
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
