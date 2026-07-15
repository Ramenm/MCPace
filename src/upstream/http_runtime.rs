use super::diagnostics::sanitize_upstream_diagnostic;
use super::{
    batch_tool_call_error, empty_object, negotiated_protocol_version, validate_tool_call_result,
    ToolListPagination, UpstreamServerConfig, UpstreamToolCall, INITIALIZE_ID, METHOD_ID,
};
use crate::http_probe;
use crate::json::{parse_str, JsonValue};
use crate::json_helpers;
use crate::mcp_protocol as mcp;
use crate::text_utils;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::Read;
use std::time::{Duration, Instant};

struct ParsedHttpUrl {
    url: String,
    secure: bool,
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

struct HttpsPostOptions<'a> {
    session_id: Option<&'a str>,
    protocol_version: &'a str,
    expected_id: Option<i64>,
    timeout: Duration,
    headers: &'a BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HttpUpstreamError {
    message: String,
}

pub(super) type HttpUpstreamResult<T> = std::result::Result<T, HttpUpstreamError>;

impl HttpUpstreamError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for HttpUpstreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for HttpUpstreamError {}

impl From<String> for HttpUpstreamError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<&str> for HttpUpstreamError {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

impl From<HttpUpstreamError> for String {
    fn from(error: HttpUpstreamError) -> Self {
        error.message
    }
}

pub(super) fn run_http_request(
    server: &UpstreamServerConfig,
    method: &str,
    params: Option<JsonValue>,
    timeout: Duration,
) -> HttpUpstreamResult<JsonValue> {
    let deadline = Instant::now() + timeout;
    let (target, agent, session_id, protocol_version) = initialize_http_session(server, deadline)?;
    let result = (|| {
        let remaining = remaining_http_timeout(deadline, &server.name, method)?;
        let response = post_json(
            &target,
            jsonrpc_request(METHOD_ID, method, params),
            session_id.as_deref(),
            &protocol_version,
            remaining,
            &server.headers,
            agent.as_ref(),
        )?;
        jsonrpc_result(&server.name, method, METHOD_ID, &response)
    })();
    terminate_http_session_before_deadline(
        &target,
        session_id.as_deref(),
        &protocol_version,
        deadline,
        &server.headers,
    );
    result
}

pub(super) fn run_http_tools_list(
    server: &UpstreamServerConfig,
    timeout: Duration,
) -> HttpUpstreamResult<JsonValue> {
    let deadline = Instant::now() + timeout;
    let (target, agent, session_id, protocol_version) = initialize_http_session(server, deadline)?;
    let result = (|| {
        let mut pagination = ToolListPagination::new();
        let mut cursor: Option<String> = None;
        let mut page = 0usize;
        loop {
            let request_id = METHOD_ID.saturating_add(page as i64);
            let params = cursor
                .as_ref()
                .map(|cursor| JsonValue::object([("cursor", JsonValue::string(cursor.clone()))]));
            let response = post_json(
                &target,
                jsonrpc_request(request_id, "tools/list", params),
                session_id.as_deref(),
                &protocol_version,
                remaining_http_timeout(deadline, &server.name, "tools/list pagination")?,
                &server.headers,
                agent.as_ref(),
            )?;
            let page_result = jsonrpc_result(&server.name, "tools/list", request_id, &response)?;
            cursor = pagination
                .add_page(&server.name, &page_result)
                .map_err(HttpUpstreamError::new)?;
            page = page.saturating_add(1);
            if cursor.is_none() {
                return Ok(pagination.finish());
            }
        }
    })();
    terminate_http_session_before_deadline(
        &target,
        session_id.as_deref(),
        &protocol_version,
        deadline,
        &server.headers,
    );
    result
}

pub(super) fn run_http_tool_calls(
    server: &UpstreamServerConfig,
    calls: &[UpstreamToolCall],
    timeout: Duration,
) -> HttpUpstreamResult<Vec<JsonValue>> {
    let deadline = Instant::now() + timeout;
    let (target, agent, session_id, protocol_version) = initialize_http_session(server, deadline)?;
    let mut results = Vec::new();
    let calls_result = (|| {
        for (index, call) in calls.iter().enumerate() {
            let request_id = METHOD_ID.saturating_add(index as i64);
            let remaining = remaining_http_timeout(deadline, &server.name, "tools/call batch")
                .map_err(|error| {
                    HttpUpstreamError::new(batch_tool_call_error(
                        &server.name,
                        index,
                        calls.len(),
                        error,
                    ))
                })?;
            let response = post_json(
                &target,
                jsonrpc_request(
                    request_id,
                    "tools/call",
                    Some(JsonValue::object([
                        ("name", JsonValue::string(&call.tool)),
                        ("arguments", call.arguments.clone()),
                    ])),
                ),
                session_id.as_deref(),
                &protocol_version,
                remaining,
                &server.headers,
                agent.as_ref(),
            )
            .map_err(|error| {
                HttpUpstreamError::new(batch_tool_call_error(
                    &server.name,
                    index,
                    calls.len(),
                    error,
                ))
            })?;
            let result = jsonrpc_result(&server.name, "tools/call", request_id, &response)
                .map_err(|error| {
                    HttpUpstreamError::new(batch_tool_call_error(
                        &server.name,
                        index,
                        calls.len(),
                        error,
                    ))
                })?;
            let upstream_is_error = validate_tool_call_result(&server.name, &call.tool, &result)
                .map_err(|error| {
                    HttpUpstreamError::new(batch_tool_call_error(
                        &server.name,
                        index,
                        calls.len(),
                        error,
                    ))
                })?;
            results.push(JsonValue::object([
                ("index", JsonValue::number(index)),
                ("ok", JsonValue::bool(!upstream_is_error)),
                ("upstreamOk", JsonValue::bool(!upstream_is_error)),
                ("upstreamIsError", JsonValue::bool(upstream_is_error)),
                ("tool", JsonValue::string(&call.tool)),
                ("upstreamResult", result),
            ]));
        }
        Ok(())
    })();
    terminate_http_session_before_deadline(
        &target,
        session_id.as_deref(),
        &protocol_version,
        deadline,
        &server.headers,
    );
    calls_result.map(|()| results)
}

fn initialize_http_session(
    server: &UpstreamServerConfig,
    deadline: Instant,
) -> HttpUpstreamResult<(ParsedHttpUrl, Option<ureq::Agent>, Option<String>, String)> {
    let url = server.url.as_deref().ok_or_else(|| {
        format!(
            "HTTP upstream server '{}' has no url configured",
            server.name
        )
    })?;
    let target = parse_http_url(url)?;
    let initial_timeout = remaining_http_timeout(deadline, &server.name, "initialize")?;
    let agent = target
        .secure
        .then(|| crate::http_client::bounded_agent(initial_timeout));
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
    let initialize_response = post_json(
        &target,
        initialize,
        None,
        mcp::CURRENT_PROTOCOL_VERSION,
        initial_timeout,
        &server.headers,
        agent.as_ref(),
    )?;
    let session_id = initialize_response.session_id.clone();
    let initialize_result = match jsonrpc_result(
        &server.name,
        "initialize",
        INITIALIZE_ID,
        &initialize_response,
    ) {
        Ok(result) => result,
        Err(error) => {
            terminate_http_session_before_deadline(
                &target,
                session_id.as_deref(),
                mcp::CURRENT_PROTOCOL_VERSION,
                deadline,
                &server.headers,
            );
            return Err(error);
        }
    };
    let protocol_version = match negotiated_protocol_version(&server.name, &initialize_result) {
        Ok(version) => version,
        Err(error) => {
            terminate_http_session_before_deadline(
                &target,
                session_id.as_deref(),
                mcp::CURRENT_PROTOCOL_VERSION,
                deadline,
                &server.headers,
            );
            return Err(error.into());
        }
    };
    let initialized = JsonValue::object([
        ("jsonrpc", JsonValue::string("2.0")),
        ("method", JsonValue::string("notifications/initialized")),
    ]);
    let initialized_response = post_json(
        &target,
        initialized,
        session_id.as_deref(),
        &protocol_version,
        remaining_http_timeout(deadline, &server.name, "notifications/initialized")?,
        &server.headers,
        agent.as_ref(),
    );
    match initialized_response {
        Ok(response) if (200..300).contains(&response.status) => {}
        Ok(response) => {
            terminate_http_session_before_deadline(
                &target,
                session_id.as_deref(),
                &protocol_version,
                deadline,
                &server.headers,
            );
            return Err(HttpUpstreamError::new(format!(
                "HTTP upstream server '{}' returned status {} for notifications/initialized",
                server.name, response.status
            )));
        }
        Err(error) => {
            terminate_http_session_before_deadline(
                &target,
                session_id.as_deref(),
                &protocol_version,
                deadline,
                &server.headers,
            );
            return Err(error);
        }
    }
    Ok((target, agent, session_id, protocol_version))
}

fn remaining_http_timeout(
    deadline: Instant,
    server_name: &str,
    phase: &str,
) -> HttpUpstreamResult<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| {
            HttpUpstreamError::new(format!(
                "timed out waiting for HTTP upstream server '{}' during {}",
                server_name, phase
            ))
        })
}

fn terminate_http_session_before_deadline(
    target: &ParsedHttpUrl,
    session_id: Option<&str>,
    protocol_version: &str,
    deadline: Instant,
    headers: &BTreeMap<String, String>,
) {
    let Some(timeout) = deadline.checked_duration_since(Instant::now()) else {
        return;
    };
    terminate_http_session(target, session_id, protocol_version, timeout, headers);
}

fn terminate_http_session(
    target: &ParsedHttpUrl,
    session_id: Option<&str>,
    protocol_version: &str,
    timeout: Duration,
    headers: &BTreeMap<String, String>,
) {
    let Some(session_id) = session_id.filter(|value| text_utils::valid_http_header_value(value))
    else {
        return;
    };
    let cleanup_timeout = timeout.min(Duration::from_secs(2));
    if target.secure {
        let agent = crate::http_client::bounded_agent(cleanup_timeout);
        let mut request = agent
            .delete(&target.url)
            .header("MCP-Protocol-Version", protocol_version)
            .header("Mcp-Session-Id", session_id);
        for (name, value) in headers {
            request = request.header(name, value);
        }
        let _ = request.call();
        return;
    }

    let mut request = format!(
        "DELETE {} HTTP/1.1\r\nHost: {}\r\nMCP-Protocol-Version: {}\r\nMcp-Session-Id: {}\r\nContent-Length: 0\r\nConnection: close\r\n",
        target.path, target.host_header, protocol_version, session_id
    );
    append_configured_headers(&mut request, headers);
    request.push_str("\r\n");
    let _ = http_probe::raw_response(
        &target.host,
        target.port,
        &request,
        cleanup_timeout,
        http_probe::DEFAULT_MAX_RESPONSE_BYTES,
    );
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
    protocol_version: &str,
    timeout: Duration,
    headers: &BTreeMap<String, String>,
    agent: Option<&ureq::Agent>,
) -> HttpUpstreamResult<HttpResponse> {
    let expected_id = json_helpers::value_at_path(&payload, &["id"]).and_then(JsonValue::as_i64);
    let body = payload.to_compact_string();
    validate_configured_headers(headers)?;
    if target.secure {
        let agent = agent.ok_or_else(|| HttpUpstreamError::new("HTTPS agent is unavailable"))?;
        return post_json_https(
            target,
            &body,
            HttpsPostOptions {
                session_id,
                protocol_version,
                expected_id,
                timeout,
                headers,
            },
            agent,
        );
    }
    let mut request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMCP-Protocol-Version: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        target.path,
        target.host_header,
        protocol_version,
        body.len()
    );
    append_configured_headers(&mut request, headers);
    if let Some(session_id) = session_id {
        if !text_utils::valid_http_header_value(session_id) {
            return Err(HttpUpstreamError::new(
                "HTTP upstream returned an invalid MCP session id header",
            ));
        }
        request.push_str(&format!("Mcp-Session-Id: {}\r\n", session_id));
    }
    request.push_str("\r\n");
    request.push_str(&body);

    let raw_response = http_probe::raw_jsonrpc_response(
        &target.host,
        target.port,
        &request,
        timeout,
        http_probe::DEFAULT_MAX_RESPONSE_BYTES,
        expected_id,
    )
    .map_err(|error| format!("HTTP upstream request failed: {}", error))?;
    parse_http_response(&raw_response)
}

fn validate_configured_headers(headers: &BTreeMap<String, String>) -> HttpUpstreamResult<()> {
    let mut normalized_names = BTreeSet::new();
    for (name, value) in headers {
        if !normalized_names.insert(name.to_ascii_lowercase()) {
            return Err(HttpUpstreamError::new(format!(
                "HTTP upstream configured duplicate header name '{}' with different casing",
                name
            )));
        }
        if !text_utils::valid_http_header_name(name) {
            return Err(HttpUpstreamError::new(format!(
                "HTTP upstream configured an invalid header name '{}'",
                name
            )));
        }
        if text_utils::reserved_mcp_http_header_name(name) {
            return Err(HttpUpstreamError::new(format!(
                "HTTP upstream header '{}' is managed by MCPace and cannot be overridden",
                name
            )));
        }
        if !text_utils::valid_http_field_value(value) {
            return Err(HttpUpstreamError::new(format!(
                "HTTP upstream configured an invalid value for header '{}'",
                name
            )));
        }
    }
    Ok(())
}

fn append_configured_headers(request: &mut String, headers: &BTreeMap<String, String>) {
    for (name, value) in headers {
        request.push_str(name);
        request.push_str(": ");
        request.push_str(value);
        request.push_str("\r\n");
    }
}

fn post_json_https(
    target: &ParsedHttpUrl,
    body: &str,
    options: HttpsPostOptions<'_>,
    agent: &ureq::Agent,
) -> HttpUpstreamResult<HttpResponse> {
    let mut request = agent
        .post(&target.url)
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json")
        .header("MCP-Protocol-Version", options.protocol_version);
    for (name, value) in options.headers {
        request = request.header(name, value);
    }
    if let Some(session_id) = options.session_id {
        if !text_utils::valid_http_header_value(session_id) {
            return Err(HttpUpstreamError::new(
                "HTTPS upstream returned an invalid MCP session id header",
            ));
        }
        request = request.header("Mcp-Session-Id", session_id);
    }

    let mut response = request
        .config()
        .timeout_global(Some(options.timeout))
        .build()
        .send(body.as_bytes())
        .map_err(|error| format!("HTTPS upstream request failed: {}", error))?;
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let session_id = response
        .headers()
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let is_sse = content_type
        .to_ascii_lowercase()
        .contains("text/event-stream");
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut reader = response.body_mut().as_reader();
    loop {
        let count = reader
            .read(&mut chunk)
            .map_err(|error| format!("failed to read HTTPS upstream response: {}", error))?;
        if count == 0 {
            break;
        }
        if bytes.len().saturating_add(count) > http_probe::DEFAULT_MAX_RESPONSE_BYTES {
            return Err(HttpUpstreamError::new(format!(
                "HTTPS upstream response exceeds {} bytes",
                http_probe::DEFAULT_MAX_RESPONSE_BYTES
            )));
        }
        bytes.extend_from_slice(&chunk[..count]);
        if is_sse {
            if let Ok(partial) = std::str::from_utf8(&bytes) {
                if http_probe::sse_json_rpc_body(partial, options.expected_id).is_some() {
                    break;
                }
            }
        }
    }
    let body = String::from_utf8(bytes)
        .map_err(|_| HttpUpstreamError::new("HTTPS upstream returned a non-UTF-8 body"))?;
    Ok(HttpResponse {
        status,
        content_type,
        session_id,
        body,
    })
}

fn parse_http_response(raw: &str) -> HttpUpstreamResult<HttpResponse> {
    let parsed = http_probe::parse_response(raw)
        .map_err(|error| format!("HTTP upstream returned a malformed response: {}", error))?;
    let session_id = parsed.header("mcp-session-id").map(ToString::to_string);
    let content_type = parsed.content_type.clone();
    let body = String::from_utf8(
        parsed
            .body_bytes()
            .map_err(|error| format!("failed to decode HTTP upstream response body: {}", error))?,
    )
    .map_err(|_| "HTTP upstream returned a non-UTF-8 body".to_string())?;
    Ok(HttpResponse {
        status: parsed.status,
        content_type,
        session_id,
        body,
    })
}

fn jsonrpc_result(
    server_name: &str,
    method: &str,
    expected_id: i64,
    response: &HttpResponse,
) -> HttpUpstreamResult<JsonValue> {
    if !(200..300).contains(&response.status) {
        let auth_hint = if matches!(response.status, 401 | 403) {
            " Authentication is required; configure an explicit --header NAME=VALUE (environment placeholders are supported) or use an OAuth-capable stdio adapter. MCPace does not perform upstream OAuth authorization flows."
        } else {
            ""
        };
        return Err(HttpUpstreamError::new(format!(
            "HTTP upstream server '{}' returned status {} for {}: {}{}",
            server_name,
            response.status,
            method,
            sanitize_upstream_diagnostic(response.body.trim()),
            auth_hint
        )));
    }
    let body = json_body_from_response(response, Some(expected_id))?;
    let value = parse_str(&body).map_err(|error| {
        format!(
            "HTTP upstream server '{}' returned invalid JSON for {}: {}",
            server_name, method, error
        )
    })?;
    mcp::validate_response_envelope(&value, expected_id).map_err(|error| {
        HttpUpstreamError::new(format!(
            "HTTP upstream server '{}' returned a malformed JSON-RPC response for {}: {}",
            server_name, method, error
        ))
    })?;
    if let Some(error) = json_helpers::value_at_path(&value, &["error"]) {
        return Err(HttpUpstreamError::new(format!(
            "HTTP upstream server '{}' returned JSON-RPC error for {}: {}",
            server_name,
            method,
            sanitize_upstream_diagnostic(&error.to_compact_string())
        )));
    }
    Ok(json_helpers::value_at_path(&value, &["result"])
        .cloned()
        .expect("validated JSON-RPC result"))
}

fn json_body_from_response(
    response: &HttpResponse,
    expected_id: Option<i64>,
) -> HttpUpstreamResult<String> {
    if response
        .content_type
        .to_ascii_lowercase()
        .contains("text/event-stream")
    {
        return http_probe::sse_json_rpc_body(&response.body, expected_id).ok_or_else(|| {
            HttpUpstreamError::new(
                "HTTP upstream SSE response did not contain a matching JSON-RPC response",
            )
        });
    }
    Ok(response.body.trim().to_string())
}

pub(super) fn http_upstream_configuration_error(
    url: &str,
    headers: &BTreeMap<String, String>,
) -> Option<String> {
    parse_http_url(url)
        .and_then(|_| validate_configured_headers(headers))
        .err()
        .map(|error| error.to_string())
}

fn parse_http_url(url: &str) -> HttpUpstreamResult<ParsedHttpUrl> {
    if url.is_empty() || url.chars().any(|ch| ch.is_control() || ch.is_whitespace()) {
        return Err(HttpUpstreamError::new(
            "HTTP upstream URL cannot be empty or contain whitespace/control characters",
        ));
    }
    if url.contains('#') || url.contains('\\') {
        return Err(HttpUpstreamError::new(
            "HTTP upstream URL cannot contain fragments or backslashes",
        ));
    }
    let normalized_url = url.to_ascii_lowercase();
    let (secure, rest, default_port) = if normalized_url.starts_with("https://") {
        (true, &url["https://".len()..], 443)
    } else if normalized_url.starts_with("http://") {
        (false, &url["http://".len()..], 80)
    } else {
        return Err(HttpUpstreamError::new(format!(
            "HTTP upstream URL must start with http:// or https://: {}",
            url
        )));
    };
    let (authority, path) = split_http_authority_and_path(rest)?;
    let (host, port, host_header) = parse_http_authority(authority, default_port)?;
    if !secure && !plain_http_upstream_host_is_loopback(&host) {
        return Err(HttpUpstreamError::new(format!(
            "direct plain-HTTP MCP upstreams are limited to loopback hosts; '{}' must use HTTPS or a local gateway",
            host
        )));
    }
    Ok(ParsedHttpUrl {
        url: url.to_string(),
        secure,
        host,
        port,
        path,
        host_header,
    })
}

fn split_http_authority_and_path(rest: &str) -> HttpUpstreamResult<(&str, String)> {
    let Some(split_index) = rest.find(['/', '?']) else {
        if rest.is_empty() {
            return Err(HttpUpstreamError::new(
                "HTTP upstream URL has an empty authority",
            ));
        }
        return Ok((rest, "/".to_string()));
    };
    let authority = &rest[..split_index];
    if authority.is_empty() {
        return Err(HttpUpstreamError::new(
            "HTTP upstream URL has an empty authority",
        ));
    }
    let suffix = &rest[split_index..];
    let path = if suffix.starts_with('/') {
        suffix.to_string()
    } else {
        format!("/{}", suffix)
    };
    if path.chars().any(|ch| ch.is_control()) {
        return Err(HttpUpstreamError::new(
            "HTTP upstream URL path cannot contain control characters",
        ));
    }
    Ok((authority, path))
}

fn parse_http_authority(
    authority: &str,
    default_port: u16,
) -> HttpUpstreamResult<(String, u16, String)> {
    if authority.is_empty()
        || authority.contains('/')
        || authority.contains('@')
        || authority
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return Err(HttpUpstreamError::new(format!(
            "invalid HTTP upstream authority '{}'",
            authority
        )));
    }

    if authority.starts_with('[') {
        let end = authority
            .find(']')
            .ok_or_else(|| format!("invalid IPv6 HTTP upstream authority '{}'", authority))?;
        let host = authority[1..end].to_string();
        if host.trim().is_empty() || host.chars().any(|ch| ch.is_control() || ch.is_whitespace()) {
            return Err(HttpUpstreamError::new(format!(
                "invalid IPv6 HTTP upstream host '{}'",
                authority
            )));
        }
        let suffix = &authority[end + 1..];
        let port = parse_optional_http_port(suffix, authority, default_port)?;
        let host_header = if suffix.is_empty() {
            format!("[{}]", host)
        } else {
            format!("[{}]:{}", host, port)
        };
        return Ok((host, port, host_header));
    }

    if authority.matches(':').count() > 1 {
        return Err(HttpUpstreamError::new(format!(
            "IPv6 HTTP upstream authorities must be bracketed: '{}'",
            authority
        )));
    }
    let (host, port, explicit_port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => {
            (host, parse_required_http_port(port, authority)?, true)
        }
        Some(_) => {
            return Err(HttpUpstreamError::new(format!(
                "invalid HTTP upstream authority '{}'",
                authority
            )))
        }
        None => (authority, default_port, false),
    };
    if host.trim().is_empty() || host.chars().any(|ch| ch.is_control() || ch.is_whitespace()) {
        return Err(HttpUpstreamError::new(format!(
            "HTTP upstream URL has an invalid host: {}",
            authority
        )));
    }
    let host_header = if explicit_port {
        format!("{}:{}", host, port)
    } else {
        host.to_string()
    };
    Ok((host.to_string(), port, host_header))
}

fn plain_http_upstream_host_is_loopback(host: &str) -> bool {
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

fn parse_optional_http_port(
    suffix: &str,
    authority: &str,
    default_port: u16,
) -> HttpUpstreamResult<u16> {
    if suffix.is_empty() {
        return Ok(default_port);
    }
    let Some(port) = suffix.strip_prefix(':') else {
        return Err(HttpUpstreamError::new(format!(
            "invalid HTTP upstream authority '{}'",
            authority
        )));
    };
    parse_required_http_port(port, authority)
}

fn parse_required_http_port(port: &str, authority: &str) -> HttpUpstreamResult<u16> {
    if port.is_empty() {
        return Err(HttpUpstreamError::new(format!(
            "invalid HTTP upstream port in '{}'",
            authority
        )));
    }
    port.parse::<u16>()
        .ok()
        .filter(|value| *value != 0)
        .ok_or_else(|| {
            HttpUpstreamError::new(format!("invalid HTTP upstream port in '{}'", authority))
        })
}

#[cfg(test)]
mod tests;
