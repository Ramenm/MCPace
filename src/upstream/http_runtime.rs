use super::{empty_object, UpstreamServerConfig, INITIALIZE_ID, METHOD_ID};
use crate::json::{parse_str, JsonValue};
use crate::json_helpers;
use crate::mcp_protocol as mcp;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

struct ParsedHttpUrl {
    host: String,
    port: u16,
    path: String,
}

struct HttpResponse {
    status: u16,
    content_type: String,
    session_id: Option<String>,
    body: String,
}

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
    let body = payload.to_compact_string();
    let mut addrs = (target.host.as_str(), target.port)
        .to_socket_addrs()
        .map_err(|error| format!("failed to resolve HTTP upstream {}: {}", target.host, error))?;
    let addr = addrs
        .next()
        .ok_or_else(|| format!("HTTP upstream {} resolved to no addresses", target.host))?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout)
        .map_err(|error| format!("failed to connect HTTP upstream {}: {}", target.host, error))?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));

    let mut request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMCP-Protocol-Version: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        target.path,
        target.host,
        mcp::CURRENT_PROTOCOL_VERSION,
        body.len()
    );
    if let Some(session_id) = session_id {
        request.push_str(&format!("Mcp-Session-Id: {}\r\n", session_id));
    }
    request.push_str("\r\n");
    request.push_str(&body);
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("failed to write HTTP upstream request: {}", error))?;
    let mut raw_response = String::new();
    stream
        .read_to_string(&mut raw_response)
        .map_err(|error| format!("failed to read HTTP upstream response: {}", error))?;
    parse_http_response(&raw_response)
}

fn parse_http_response(raw: &str) -> Result<HttpResponse, String> {
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
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        match name.as_str() {
            "content-type" => content_type = value.to_string(),
            "mcp-session-id" => session_id = Some(value.to_string()),
            _ => {}
        }
    }
    Ok(HttpResponse {
        status,
        content_type,
        session_id,
        body: body.to_string(),
    })
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
    let body = json_body_from_response(response)?;
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

fn json_body_from_response(response: &HttpResponse) -> Result<String, String> {
    if response
        .content_type
        .to_ascii_lowercase()
        .contains("text/event-stream")
    {
        for event in response.body.split("\n\n") {
            let data = event
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .map(str::trim)
                .collect::<Vec<_>>()
                .join("\n");
            if !data.trim().is_empty() && parse_str(data.trim()).is_ok() {
                return Ok(data);
            }
        }
        return Err("HTTP upstream SSE response did not contain JSON data".to_string());
    }
    Ok(response.body.trim().to_string())
}

fn parse_http_url(url: &str) -> Result<ParsedHttpUrl, String> {
    let trimmed = url.trim();
    if trimmed.to_ascii_lowercase().starts_with("https://") {
        return Err(
            "direct HTTPS upstream forwarding is not available without a TLS adapter; use a stdio bridge such as mcp-remote or a local HTTP gateway"
                .to_string(),
        );
    }
    let rest = trimmed
        .strip_prefix("http://")
        .ok_or_else(|| format!("HTTP upstream URL must start with http://: {}", trimmed))?;
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let path = format!("/{}", path);
    let (host, port) = if authority.starts_with('[') {
        let end = authority
            .find(']')
            .ok_or_else(|| format!("invalid IPv6 HTTP upstream authority '{}'", authority))?;
        let host = authority[1..end].to_string();
        let port = authority[end + 1..]
            .strip_prefix(':')
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(80);
        (host, port)
    } else {
        let (host, port) = authority.split_once(':').unwrap_or((authority, "80"));
        let port = port
            .parse::<u16>()
            .map_err(|_| format!("invalid HTTP upstream port in '{}'", authority))?;
        (host.to_string(), port)
    };
    if host.trim().is_empty() {
        return Err(format!("HTTP upstream URL has an empty host: {}", trimmed));
    }
    Ok(ParsedHttpUrl { host, port, path })
}
