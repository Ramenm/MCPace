use super::{empty_object, UpstreamServerConfig, INITIALIZE_ID, METHOD_ID};
use crate::json::{parse_str, JsonValue};
use crate::json_helpers;
use crate::mcp_protocol as mcp;
use crate::text_utils;
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::time::Duration;

struct ParsedHttpUrl {
    host: String,
    port: u16,
    path: String,
    host_header: String,
}

struct HttpResponse {
    status: u16,
    content_type: String,
    session_id: Option<String>,
    body: String,
}

struct ParsedHttpResponse<'a> {
    status: u16,
    content_type: String,
    session_id: Option<String>,
    content_length: Option<usize>,
    transfer_encoding: String,
    body: &'a str,
}

const MAX_HTTP_UPSTREAM_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

pub(super) fn run_http_request(
    server: &UpstreamServerConfig,
    method: &str,
    params: Option<JsonValue>,
    timeout: Duration,
) -> Result<JsonValue, String> {
    let url = server.url.as_deref().ok_or_else(|| {
        format!(
            "HTTP upstream server '{}' has no url configured",
            server.name
        )
    })?;
    let target = parse_http_url(url)?;

    let initialize = jsonrpc_request(
        INITIALIZE_ID,
        "initialize",
        Some(JsonValue::object([
            (
                "protocolVersion",
                JsonValue::string(mcp::CURRENT_PROTOCOL_VERSION),
            ),
            ("capabilities", empty_object()),
            (
                "clientInfo",
                JsonValue::object([
                    ("name", JsonValue::string("mcpace-upstream-http-bridge")),
                    ("version", JsonValue::string(env!("CARGO_PKG_VERSION"))),
                ]),
            ),
        ])),
    );
    let initialize_response = post_json(&target, initialize, None, timeout)?;
    let _initialize_result = jsonrpc_result(
        &server.name,
        "initialize",
        INITIALIZE_ID,
        &initialize_response,
    )?;
    let session_id = initialize_response.session_id;

    let initialized = JsonValue::object([
        ("jsonrpc", JsonValue::string("2.0")),
        ("method", JsonValue::string("notifications/initialized")),
    ]);
    let _ = post_json(&target, initialized, session_id.as_deref(), timeout)?;

    let response = post_json(
        &target,
        jsonrpc_request(METHOD_ID, method, params),
        session_id.as_deref(),
        timeout,
    )?;
    jsonrpc_result(&server.name, method, METHOD_ID, &response)
}

fn jsonrpc_request(id: i64, method: &str, params: Option<JsonValue>) -> JsonValue {
    let mut entries = vec![
        ("jsonrpc", JsonValue::string("2.0")),
        ("id", JsonValue::number(id)),
        ("method", JsonValue::string(method)),
    ];
    if let Some(params) = params {
        entries.push(("params", params));
    }
    JsonValue::object(entries)
}

fn post_json(
    target: &ParsedHttpUrl,
    payload: JsonValue,
    session_id: Option<&str>,
    timeout: Duration,
) -> Result<HttpResponse, String> {
    let expected_id = json_helpers::value_at_path(&payload, &["id"]).and_then(JsonValue::as_i64);
    let body = payload.to_compact_string();
    let mut stream = connect_http_upstream(target, timeout)?;

    let mut request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMCP-Protocol-Version: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        target.path,
        target.host_header,
        mcp::CURRENT_PROTOCOL_VERSION,
        body.len()
    );
    if let Some(session_id) = session_id {
        if !text_utils::valid_http_header_value(session_id) {
            return Err("HTTP upstream returned an invalid MCP session id header".to_string());
        }
        request.push_str(&format!("Mcp-Session-Id: {}\r\n", session_id));
    }
    request.push_str("\r\n");
    request.push_str(&body);
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("failed to write HTTP upstream request: {}", error))?;
    let _ = stream.shutdown(Shutdown::Write);
    let raw_response = read_http_response(&mut stream, expected_id)?;
    parse_http_response(&raw_response)
}

fn connect_http_upstream(target: &ParsedHttpUrl, timeout: Duration) -> Result<TcpStream, String> {
    let addrs = (target.host.as_str(), target.port)
        .to_socket_addrs()
        .map_err(|error| format!("failed to resolve HTTP upstream {}: {}", target.host, error))?
        .collect::<Vec<_>>();
    if addrs.is_empty() {
        return Err(format!(
            "HTTP upstream {} resolved to no addresses",
            target.host
        ));
    }

    let mut last_error = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => {
                let _ = stream.set_read_timeout(Some(timeout));
                let _ = stream.set_write_timeout(Some(timeout));
                return Ok(stream);
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(format!(
        "failed to connect HTTP upstream {}: {}",
        target.host,
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "no resolved address accepted the connection".to_string())
    ))
}

fn read_http_response(stream: &mut TcpStream, expected_id: Option<i64>) -> Result<String, String> {
    let mut raw = Vec::new();
    let mut buffer = [0u8; 8192];

    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                raw.extend_from_slice(&buffer[..count]);
                if raw.len() > MAX_HTTP_UPSTREAM_RESPONSE_BYTES {
                    return Err(
                        "HTTP upstream response exceeded the maximum supported size".to_string()
                    );
                }
                if let Ok(text) = std::str::from_utf8(&raw) {
                    if http_response_ready(text, expected_id) {
                        return Ok(text.to_string());
                    }
                }
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if let Ok(text) = std::str::from_utf8(&raw) {
                    if http_response_ready(text, expected_id) {
                        return Ok(text.to_string());
                    }
                }
                return Err("timed out while reading HTTP upstream response".to_string());
            }
            Err(error) => return Err(format!("failed to read HTTP upstream response: {}", error)),
        }
    }

    String::from_utf8(raw).map_err(|_| "HTTP upstream returned a non-UTF-8 response".to_string())
}

fn http_response_ready(raw: &str, expected_id: Option<i64>) -> bool {
    let Ok(parsed) = parse_http_headers(raw) else {
        return false;
    };
    if matches!(parsed.status, 202 | 204 | 304) {
        return true;
    }

    let transfer_encoding = parsed.transfer_encoding.to_ascii_lowercase();
    let content_type = parsed.content_type.to_ascii_lowercase();
    if content_type.contains("text/event-stream") {
        if transfer_encoding.contains("chunked") {
            if let Ok(body) = decode_chunked_body(parsed.body.as_bytes()) {
                return std::str::from_utf8(&body)
                    .map(|body| sse_body_has_json_response(body, expected_id))
                    .unwrap_or(false);
            }
        }
        return sse_body_has_json_response(parsed.body, expected_id);
    }

    if transfer_encoding.contains("chunked") {
        return decode_chunked_body(parsed.body.as_bytes()).is_ok();
    }

    if let Some(content_length) = parsed.content_length {
        return parsed.body.len() >= content_length;
    }

    parse_str(parsed.body.trim())
        .ok()
        .map(|value| json_response_id_matches(&value, expected_id))
        .unwrap_or(false)
}

fn parse_http_response(raw: &str) -> Result<HttpResponse, String> {
    let parsed = parse_http_headers(raw)?;
    let body_bytes = if parsed
        .transfer_encoding
        .to_ascii_lowercase()
        .contains("chunked")
    {
        decode_chunked_body(parsed.body.as_bytes())
            .unwrap_or_else(|_| parsed.body.as_bytes().to_vec())
    } else if let Some(content_length) = parsed.content_length {
        parsed
            .body
            .as_bytes()
            .get(..content_length.min(parsed.body.len()))
            .unwrap_or(parsed.body.as_bytes())
            .to_vec()
    } else {
        parsed.body.as_bytes().to_vec()
    };
    let body = String::from_utf8(body_bytes)
        .map_err(|_| "HTTP upstream returned a non-UTF-8 body".to_string())?;
    Ok(HttpResponse {
        status: parsed.status,
        content_type: parsed.content_type,
        session_id: parsed.session_id,
        body,
    })
}

fn parse_http_headers(raw: &str) -> Result<ParsedHttpResponse<'_>, String> {
    let (headers, body) = raw
        .split_once("\r\n\r\n")
        .ok_or_else(|| "HTTP upstream returned a malformed response".to_string())?;
    let mut lines = headers.lines();
    let status_line = lines.next().unwrap_or_default();
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| format!("HTTP upstream returned malformed status '{}'", status_line))?;
    let mut content_type = String::new();
    let mut session_id = None;
    let mut content_length = None;
    let mut transfer_encoding = String::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        match name.as_str() {
            "content-type" => content_type = value.to_string(),
            "mcp-session-id" => session_id = Some(value.to_string()),
            "content-length" => content_length = value.parse::<usize>().ok(),
            "transfer-encoding" => transfer_encoding = value.to_string(),
            _ => {}
        }
    }
    Ok(ParsedHttpResponse {
        status,
        content_type,
        session_id,
        content_length,
        transfer_encoding,
        body,
    })
}

fn decode_chunked_body(body: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoded = Vec::new();
    let mut offset = 0usize;
    loop {
        let Some(line_end) = find_crlf(body, offset) else {
            return Err("chunked HTTP body is incomplete".to_string());
        };
        let size_line = std::str::from_utf8(&body[offset..line_end])
            .map_err(|_| "chunked HTTP body has a non-UTF-8 size line".to_string())?;
        let size_hex = size_line.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_hex, 16)
            .map_err(|_| "chunked HTTP body has an invalid chunk size".to_string())?;
        offset = line_end + 2;
        if size == 0 {
            return Ok(decoded);
        }
        if body.len() < offset + size {
            return Err("chunked HTTP body is incomplete".to_string());
        }
        decoded.extend_from_slice(&body[offset..offset + size]);
        offset += size;
        if body.get(offset..offset + 2) == Some(b"\r\n") {
            offset += 2;
        } else if offset < body.len() {
            return Err("chunked HTTP body is missing a chunk terminator".to_string());
        }
    }
}

fn find_crlf(body: &[u8], start: usize) -> Option<usize> {
    body.get(start..)?
        .windows(2)
        .position(|window| window == b"\r\n")
        .map(|index| start + index)
}

fn jsonrpc_result(
    server_name: &str,
    method: &str,
    expected_id: i64,
    response: &HttpResponse,
) -> Result<JsonValue, String> {
    if response.status == 202 {
        return Ok(JsonValue::Null);
    }
    if !(200..300).contains(&response.status) {
        return Err(format!(
            "HTTP upstream server '{}' returned status {} for {}: {}",
            server_name,
            response.status,
            method,
            response.body.trim()
        ));
    }
    let body = json_body_from_response(response, Some(expected_id))?;
    let value = parse_str(&body).map_err(|error| {
        format!(
            "HTTP upstream server '{}' returned invalid JSON for {}: {}",
            server_name, method, error
        )
    })?;
    if let Some(error) = json_helpers::value_at_path(&value, &["error"]) {
        return Err(format!(
            "HTTP upstream server '{}' returned JSON-RPC error for {}: {}",
            server_name,
            method,
            error.to_compact_string()
        ));
    }
    let id_ok = json_helpers::value_at_path(&value, &["id"])
        .and_then(JsonValue::as_i64)
        .map(|id| id == expected_id)
        .unwrap_or(false);
    if !id_ok {
        return Err(format!(
            "HTTP upstream server '{}' returned an unexpected response id for {}",
            server_name, method
        ));
    }
    json_helpers::value_at_path(&value, &["result"])
        .cloned()
        .ok_or_else(|| {
            format!(
                "HTTP upstream server '{}' returned no result for {}",
                server_name, method
            )
        })
}

fn json_body_from_response(
    response: &HttpResponse,
    expected_id: Option<i64>,
) -> Result<String, String> {
    if response
        .content_type
        .to_ascii_lowercase()
        .contains("text/event-stream")
    {
        return sse_json_body(&response.body, expected_id).ok_or_else(|| {
            "HTTP upstream SSE response did not contain a matching JSON-RPC response".to_string()
        });
    }
    Ok(response.body.trim().to_string())
}

fn sse_body_has_json_response(body: &str, expected_id: Option<i64>) -> bool {
    sse_json_body(body, expected_id).is_some()
}

fn sse_json_body(body: &str, expected_id: Option<i64>) -> Option<String> {
    let normalized = body.replace("\r\n", "\n");
    for event in normalized.split("\n\n") {
        let data = event
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim)
            .collect::<Vec<_>>()
            .join("\n");
        let data = data.trim();
        if data.is_empty() {
            continue;
        }
        let Ok(value) = parse_str(data) else {
            continue;
        };
        if json_response_id_matches(&value, expected_id) {
            return Some(data.to_string());
        }
    }
    None
}

fn json_response_id_matches(value: &JsonValue, expected_id: Option<i64>) -> bool {
    match expected_id {
        Some(expected_id) => json_helpers::value_at_path(value, &["id"])
            .and_then(JsonValue::as_i64)
            .map(|id| id == expected_id)
            .unwrap_or(false),
        None => true,
    }
}

fn parse_http_url(url: &str) -> Result<ParsedHttpUrl, String> {
    if url.is_empty() || url.chars().any(|ch| ch.is_control() || ch.is_whitespace()) {
        return Err(
            "HTTP upstream URL cannot be empty or contain whitespace/control characters"
                .to_string(),
        );
    }
    if url.to_ascii_lowercase().starts_with("https://") {
        return Err(
            "direct HTTPS upstream forwarding is not available without a TLS adapter; use a stdio bridge such as mcp-remote or a local HTTP gateway"
                .to_string(),
        );
    }
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("HTTP upstream URL must start with http://: {}", url))?;
    let (authority, path) = split_http_authority_and_path(rest)?;
    let (host, port, host_header) = parse_http_authority(authority)?;
    Ok(ParsedHttpUrl {
        host,
        port,
        path,
        host_header,
    })
}

fn split_http_authority_and_path(rest: &str) -> Result<(&str, String), String> {
    let Some(split_index) = rest.find(['/', '?']) else {
        if rest.is_empty() {
            return Err("HTTP upstream URL has an empty authority".to_string());
        }
        return Ok((rest, "/".to_string()));
    };
    let authority = &rest[..split_index];
    if authority.is_empty() {
        return Err("HTTP upstream URL has an empty authority".to_string());
    }
    let suffix = &rest[split_index..];
    let path = if suffix.starts_with('/') {
        suffix.to_string()
    } else {
        format!("/{}", suffix)
    };
    if path.chars().any(|ch| ch.is_control()) {
        return Err("HTTP upstream URL path cannot contain control characters".to_string());
    }
    Ok((authority, path))
}

fn parse_http_authority(authority: &str) -> Result<(String, u16, String), String> {
    if authority.is_empty()
        || authority.contains('/')
        || authority.contains('@')
        || authority
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return Err(format!("invalid HTTP upstream authority '{}'", authority));
    }

    if authority.starts_with('[') {
        let end = authority
            .find(']')
            .ok_or_else(|| format!("invalid IPv6 HTTP upstream authority '{}'", authority))?;
        let host = authority[1..end].to_string();
        if host.trim().is_empty() || host.chars().any(|ch| ch.is_control() || ch.is_whitespace()) {
            return Err(format!("invalid IPv6 HTTP upstream host '{}'", authority));
        }
        let suffix = &authority[end + 1..];
        let port = parse_optional_http_port(suffix, authority)?;
        let host_header = if suffix.is_empty() {
            format!("[{}]", host)
        } else {
            format!("[{}]:{}", host, port)
        };
        return Ok((host, port, host_header));
    }

    if authority.matches(':').count() > 1 {
        return Err(format!(
            "IPv6 HTTP upstream authorities must be bracketed: '{}'",
            authority
        ));
    }
    let (host, port, explicit_port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => {
            (host, parse_required_http_port(port, authority)?, true)
        }
        Some(_) => return Err(format!("invalid HTTP upstream authority '{}'", authority)),
        None => (authority, 80, false),
    };
    if host.trim().is_empty() || host.chars().any(|ch| ch.is_control() || ch.is_whitespace()) {
        return Err(format!(
            "HTTP upstream URL has an invalid host: {}",
            authority
        ));
    }
    let host_header = if explicit_port {
        format!("{}:{}", host, port)
    } else {
        host.to_string()
    };
    Ok((host.to_string(), port, host_header))
}

fn parse_optional_http_port(suffix: &str, authority: &str) -> Result<u16, String> {
    if suffix.is_empty() {
        return Ok(80);
    }
    let Some(port) = suffix.strip_prefix(':') else {
        return Err(format!("invalid HTTP upstream authority '{}'", authority));
    };
    parse_required_http_port(port, authority)
}

fn parse_required_http_port(port: &str, authority: &str) -> Result<u16, String> {
    if port.is_empty() {
        return Err(format!("invalid HTTP upstream port in '{}'", authority));
    }
    port.parse::<u16>()
        .map_err(|_| format!("invalid HTTP upstream port in '{}'", authority))
}

#[cfg(test)]
mod tests {
    use super::parse_http_url;
    use crate::text_utils;

    #[test]
    fn parse_http_url_preserves_explicit_host_port_for_host_header() {
        let parsed = parse_http_url("http://127.0.0.1:39022/mcp").expect("parse localhost URL");
        assert_eq!(parsed.host, "127.0.0.1");
        assert_eq!(parsed.port, 39022);
        assert_eq!(parsed.path, "/mcp");
        assert_eq!(parsed.host_header, "127.0.0.1:39022");
    }

    #[test]
    fn parse_http_url_brackets_ipv6_host_header_and_keeps_query_path() {
        let parsed = parse_http_url("http://[::1]:39022/mcp?x=1").expect("parse IPv6 URL");
        assert_eq!(parsed.host, "::1");
        assert_eq!(parsed.port, 39022);
        assert_eq!(parsed.path, "/mcp?x=1");
        assert_eq!(parsed.host_header, "[::1]:39022");
    }

    #[test]
    fn parse_http_url_turns_query_only_suffix_into_root_query_path() {
        let parsed = parse_http_url("http://127.0.0.1?x=1").expect("parse query-only URL");
        assert_eq!(parsed.host, "127.0.0.1");
        assert_eq!(parsed.port, 80);
        assert_eq!(parsed.path, "/?x=1");
        assert_eq!(parsed.host_header, "127.0.0.1");
    }

    #[test]
    fn parse_http_url_rejects_header_injection_and_ambiguous_authorities() {
        let rejected = [
            " http://127.0.0.1:39022/mcp",
            "http://127.0.0.1:39022/mcp ",
            "http://127.0.0.1\r\nInjected: bad/mcp",
            "http://[::1]:not-a-port/mcp",
            "http://::1:39022/mcp",
            "http://user@127.0.0.1/mcp",
            "http://127.0.0.1:65536/mcp",
        ];
        for url in rejected {
            assert!(
                parse_http_url(url).is_err(),
                "URL should be rejected: {url:?}"
            );
        }
    }

    #[test]
    fn mcp_session_id_forwarding_rejects_control_characters() {
        assert!(text_utils::valid_http_header_value("session-123"));
        assert!(!text_utils::valid_http_header_value(""));
        assert!(!text_utils::valid_http_header_value(
            "session\r\nInjected: bad"
        ));
        assert!(!text_utils::valid_http_header_value("session with spaces"));
    }
}
