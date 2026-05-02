use super::http_boundary::request_header_string;
use super::HttpRequest;
use crate::json::JsonValue;
use crate::resources;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub(super) fn generated_mcp_http_session_id(
    request: &HttpRequest,
    id: &JsonValue,
    protocol: &str,
) -> String {
    if let Some(random) = os_random_hex(16) {
        return format!("mcpace-{}", random);
    }

    let mut hasher = DefaultHasher::new();
    "mcpace-http-session-v1".hash(&mut hasher);
    super::now_ms().hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    request_header_string(Some(request), "host").hash(&mut hasher);
    request.body.hash(&mut hasher);
    id.to_compact_string().hash(&mut hasher);
    protocol.hash(&mut hasher);
    format!("mcpace-fallback-{:016x}", hasher.finish())
}

pub(super) fn normalize_mcp_http_session_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > resources::MAX_HTTP_HEADER_LINE_BYTES {
        return None;
    }
    if trimmed.bytes().all(|byte| (0x21..=0x7e).contains(&byte)) {
        Some(trimmed.to_string())
    } else {
        None
    }
}

fn os_random_hex(byte_count: usize) -> Option<String> {
    let mut bytes = vec![0u8; byte_count];
    getrandom::getrandom(&mut bytes).ok()?;
    Some(hex_bytes(&bytes))
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
