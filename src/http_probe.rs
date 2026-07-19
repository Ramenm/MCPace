use crate::json::{parse_str, JsonValue};
use crate::json_helpers;
use crate::runtimepaths;
use std::fmt;
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

pub(crate) const DEFAULT_MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HttpProbeError {
    message: String,
}

pub(crate) type HttpProbeResult<T> = std::result::Result<T, HttpProbeError>;

impl HttpProbeError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for HttpProbeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for HttpProbeError {}

impl From<String> for HttpProbeError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<&str> for HttpProbeError {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct HttpJsonResponse {
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) json: JsonValue,
}

#[derive(Clone, Debug)]
pub(crate) struct HttpResponse {
    pub(crate) status_line: String,
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) content_type: String,
    pub(crate) content_length: Option<usize>,
    pub(crate) transfer_encoding: String,
    pub(crate) body: String,
}

impl HttpJsonResponse {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

impl HttpResponse {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub(crate) fn body_bytes(&self) -> HttpProbeResult<Vec<u8>> {
        if self
            .transfer_encoding
            .to_ascii_lowercase()
            .contains("chunked")
        {
            return decode_chunked_body(self.body.as_bytes());
        }
        if let Some(content_length) = self.content_length {
            if self.body.len() < content_length {
                return Err(HttpProbeError::new(format!(
                    "HTTP response body is truncated: expected {} bytes, received {}",
                    content_length,
                    self.body.len()
                )));
            }
            return Ok(self.body.as_bytes()[..content_length].to_vec());
        }
        Ok(self.body.as_bytes().to_vec())
    }

    pub(crate) fn is_event_stream(&self) -> bool {
        self.content_type
            .to_ascii_lowercase()
            .contains("text/event-stream")
    }
}

pub(crate) fn json_get(
    host: &str,
    port: u16,
    path: &str,
    timeout: Duration,
    max_response_bytes: usize,
) -> HttpProbeResult<JsonValue> {
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, host
    );
    json_response(host, port, &request, timeout, max_response_bytes).map(|response| response.json)
}

pub(crate) fn json_response(
    host: &str,
    port: u16,
    request: &str,
    timeout: Duration,
    max_response_bytes: usize,
) -> HttpProbeResult<HttpJsonResponse> {
    let response = raw_response(host, port, request, timeout, max_response_bytes)?;
    parse_json_response(&response)
}

pub(crate) fn raw_response(
    host: &str,
    port: u16,
    request: &str,
    timeout: Duration,
    max_response_bytes: usize,
) -> HttpProbeResult<String> {
    raw_response_until(
        host,
        port,
        request,
        timeout,
        max_response_bytes,
        response_ready,
    )
}

pub(crate) fn raw_jsonrpc_response(
    host: &str,
    port: u16,
    request: &str,
    timeout: Duration,
    max_response_bytes: usize,
    expected_id: Option<i64>,
) -> HttpProbeResult<String> {
    raw_response_until(host, port, request, timeout, max_response_bytes, |raw| {
        mcp_json_rpc_response_ready(raw, expected_id)
    })
}

fn connect_probe_addr(addr: &SocketAddr, deadline: Instant) -> std::io::Result<TcpStream> {
    #[cfg(not(windows))]
    {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| {
                std::io::Error::new(ErrorKind::TimedOut, "connection deadline expired")
            })?;
        TcpStream::connect_timeout(addr, remaining)
    }

    #[cfg(windows)]
    {
        if !addr.ip().is_loopback() {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(|| {
                    std::io::Error::new(ErrorKind::TimedOut, "connection deadline expired")
                })?;
            return TcpStream::connect_timeout(addr, remaining);
        }

        // Windows can occasionally leave a loopback connect pending until its
        // whole timeout despite an already-bound listener. Retry only timed-out
        // loopback attempts in short slices while preserving one total deadline.
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(|| {
                    std::io::Error::new(ErrorKind::TimedOut, "connection deadline expired")
                })?;
            let attempt_timeout = remaining.min(Duration::from_millis(250));
            match TcpStream::connect_timeout(addr, attempt_timeout) {
                Ok(stream) => return Ok(stream),
                Err(error)
                    if matches!(error.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock)
                        && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => return Err(error),
            }
        }
    }
}

pub(crate) fn raw_response_until(
    host: &str,
    port: u16,
    request: &str,
    timeout: Duration,
    max_response_bytes: usize,
    ready: impl Fn(&str) -> bool,
) -> HttpProbeResult<String> {
    let deadline = Instant::now() + timeout;
    let probe_host = probe_host(host);
    let addrs = (probe_host.as_str(), port)
        .to_socket_addrs()
        .map_err(|error| format!("resolve {}:{}: {}", probe_host, port, error))?
        .collect::<Vec<_>>();
    if addrs.is_empty() {
        return Err(HttpProbeError::new(format!(
            "{}:{} resolved to no addresses",
            probe_host, port
        )));
    }

    let mut last_error = None;
    for addr in addrs {
        if deadline.checked_duration_since(Instant::now()).is_none() {
            break;
        }
        match connect_probe_addr(&addr, deadline) {
            Ok(mut stream) => {
                let remaining =
                    deadline
                        .checked_duration_since(Instant::now())
                        .ok_or_else(|| {
                            HttpProbeError::new("HTTP probe timed out before request write")
                        })?;
                stream
                    .set_read_timeout(Some(remaining))
                    .map_err(|error| format!("set read timeout: {}", error))?;
                stream
                    .set_write_timeout(Some(remaining))
                    .map_err(|error| format!("set write timeout: {}", error))?;
                stream
                    .write_all(request.as_bytes())
                    .map_err(|error| format!("write request: {}", error))?;
                // Darwin rejects the deadline reader's later SO_RCVTIMEO
                // update after SHUT_WR. The request is already HTTP-framed, so
                // keep its write side open there; retain EOF compatibility for
                // existing non-Darwin peers that read requests to completion.
                #[cfg(not(target_os = "macos"))]
                let _ = stream.shutdown(std::net::Shutdown::Write);
                return read_response_until(&mut stream, max_response_bytes, &ready, deadline);
            }
            Err(error) => last_error = Some(error),
        }
    }

    let reason = if Instant::now() >= deadline {
        "connection deadline expired".to_string()
    } else {
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "no resolved address accepted the connection".to_string())
    };
    Err(HttpProbeError::new(format!(
        "connect {}:{}: {}",
        probe_host, port, reason
    )))
}

fn read_response_until(
    stream: &mut TcpStream,
    max_response_bytes: usize,
    ready: &dyn Fn(&str) -> bool,
    deadline: Instant,
) -> HttpProbeResult<String> {
    let mut raw = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| HttpProbeError::new("timed out while reading HTTP probe response"))?;
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|error| format!("set read timeout: {}", error))?;
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                raw.extend_from_slice(&buffer[..count]);
                if raw.len() > max_response_bytes {
                    return Err(HttpProbeError::new(
                        "HTTP probe response exceeded the maximum supported size",
                    ));
                }
                if let Ok(text) = std::str::from_utf8(&raw) {
                    if ready(text) {
                        return Ok(text.to_string());
                    }
                }
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if let Ok(text) = std::str::from_utf8(&raw) {
                    if ready(text) {
                        return Ok(text.to_string());
                    }
                }
                return Err(HttpProbeError::new(
                    "timed out while reading HTTP probe response",
                ));
            }
            Err(error) => return Err(HttpProbeError::new(format!("read response: {}", error))),
        }
    }

    String::from_utf8(raw)
        .map_err(|_| HttpProbeError::new("HTTP probe returned a non-UTF-8 response"))
}

fn response_ready(raw: &str) -> bool {
    let Ok(parsed) = parse_response(raw) else {
        return false;
    };
    generic_http_body_ready(&parsed)
}

pub(crate) fn mcp_json_rpc_response_ready(raw: &str, expected_id: Option<i64>) -> bool {
    let Ok(parsed) = parse_response(raw) else {
        return false;
    };
    if matches!(parsed.status, 202 | 204 | 304) {
        return true;
    }
    if !(200..300).contains(&parsed.status) {
        return generic_http_body_ready(&parsed);
    }
    if parsed.is_event_stream() {
        return parsed
            .body_bytes()
            .ok()
            .and_then(|body| String::from_utf8(body).ok())
            .and_then(|body| sse_json_rpc_body(&body, expected_id))
            .is_some();
    }
    if !generic_http_body_ready(&parsed) {
        return false;
    }
    let Ok(body) = String::from_utf8(parsed.body_bytes().unwrap_or_default()) else {
        return false;
    };
    parse_str(body.trim())
        .ok()
        .map(|value| json_response_id_matches(&value, expected_id))
        .unwrap_or(false)
}

fn generic_http_body_ready(parsed: &HttpResponse) -> bool {
    if parsed.is_event_stream() {
        return parsed
            .body_bytes()
            .ok()
            .and_then(|body| String::from_utf8(body).ok())
            .and_then(|body| sse_json_body(&body))
            .is_some();
    }
    if parsed
        .transfer_encoding
        .to_ascii_lowercase()
        .contains("chunked")
    {
        return parsed.body_bytes().is_ok();
    }
    if let Some(content_length) = parsed.content_length {
        return parsed.body.len() >= content_length;
    }
    parse_str(parsed.body.trim()).is_ok()
}

pub(crate) fn parse_json_response(response: &str) -> HttpProbeResult<HttpJsonResponse> {
    let parsed = parse_response(response)?;
    if parsed.status != 200 {
        return Err(HttpProbeError::new(format!(
            "HTTP request failed: {}",
            parsed.status_line
        )));
    }
    let body = String::from_utf8(parsed.body_bytes()?)
        .map_err(|_| HttpProbeError::new("HTTP probe returned a non-UTF-8 body"))?;
    let json_body = if parsed.is_event_stream() {
        sse_json_body(&body).ok_or_else(|| {
            HttpProbeError::new("HTTP probe SSE response did not contain a JSON-RPC response")
        })?
    } else {
        body.trim().to_string()
    };
    let json =
        parse_str(&json_body).map_err(|error| format!("parse HTTP JSON response: {}", error))?;
    Ok(HttpJsonResponse {
        headers: parsed.headers,
        json,
    })
}

pub(crate) fn parse_response(raw: &str) -> HttpProbeResult<HttpResponse> {
    let (headers_text, body) = raw
        .split_once("\r\n\r\n")
        .ok_or_else(|| "HTTP response missing header/body separator".to_string())?;
    let mut lines = headers_text.split("\r\n");
    let status_line = lines.next().unwrap_or_default().to_string();
    if status_line
        .bytes()
        .any(|byte| byte == b'\n' || byte == b'\r' || byte == 0)
    {
        return Err(HttpProbeError::new(
            "HTTP response status line contains invalid line framing",
        ));
    }
    let mut status_parts = status_line.split_whitespace();
    let http_version = status_parts.next().unwrap_or_default();
    let status = status_parts
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|value| (100..=599).contains(value))
        .ok_or_else(|| format!("HTTP response has malformed status line: {}", status_line))?;
    if !matches!(http_version, "HTTP/1.0" | "HTTP/1.1") {
        return Err(HttpProbeError::new(format!(
            "HTTP response has unsupported version in status line: {}",
            status_line
        )));
    }
    let mut headers = Vec::new();
    let mut content_type = String::new();
    let mut content_length = None;
    let mut transfer_encoding = String::new();
    let mut transfer_encoding_seen = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return Err(HttpProbeError::new(format!(
                "HTTP response contains malformed header line: {}",
                line
            )));
        };
        let name = name.trim().to_string();
        let value = value.trim().to_string();
        if !crate::text_utils::valid_http_header_name(&name)
            || !value.bytes().all(|byte| {
                byte == b'\t' || byte == b' ' || (0x21..=0x7e).contains(&byte) || byte >= 0x80
            })
        {
            return Err(HttpProbeError::new(format!(
                "HTTP response contains an invalid header: {}",
                name
            )));
        }
        match name.to_ascii_lowercase().as_str() {
            "content-type" => content_type = value.clone(),
            "content-length" => {
                let parsed = value.parse::<usize>().map_err(|_| {
                    HttpProbeError::new("HTTP response has an invalid Content-Length header")
                })?;
                if content_length.is_some() {
                    return Err(HttpProbeError::new(
                        "HTTP response has duplicate Content-Length headers",
                    ));
                }
                content_length = Some(parsed);
            }
            "transfer-encoding" => {
                if transfer_encoding_seen {
                    return Err(HttpProbeError::new(
                        "HTTP response has duplicate Transfer-Encoding headers",
                    ));
                }
                transfer_encoding_seen = true;
                transfer_encoding = value.clone();
            }
            _ => {}
        }
        headers.push((name, value));
    }
    if content_length.is_some() && !transfer_encoding.is_empty() {
        return Err(HttpProbeError::new(
            "HTTP response cannot combine Content-Length with Transfer-Encoding",
        ));
    }
    if !transfer_encoding.is_empty() {
        let codings = transfer_encoding
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        if codings.len() != 1 || !codings[0].eq_ignore_ascii_case("chunked") {
            return Err(HttpProbeError::new(
                "HTTP response uses an unsupported or ambiguous Transfer-Encoding",
            ));
        }
    }
    Ok(HttpResponse {
        status_line,
        status,
        headers,
        content_type,
        content_length,
        transfer_encoding,
        body: body.to_string(),
    })
}

fn decode_chunked_body(body: &[u8]) -> HttpProbeResult<Vec<u8>> {
    let mut decoded = Vec::new();
    let mut offset = 0usize;
    loop {
        let Some(line_end) = find_crlf(body, offset) else {
            return Err(HttpProbeError::new("chunked HTTP body is incomplete"));
        };
        let size_line = std::str::from_utf8(&body[offset..line_end])
            .map_err(|_| "chunked HTTP body has a non-UTF-8 size line".to_string())?;
        let size_hex = size_line.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_hex, 16)
            .map_err(|_| "chunked HTTP body has an invalid chunk size".to_string())?;
        offset = line_end + 2;
        if size == 0 {
            loop {
                let Some(trailer_end) = find_crlf(body, offset) else {
                    return Err(HttpProbeError::new(
                        "chunked HTTP body is missing the final trailer terminator",
                    ));
                };
                if trailer_end == offset {
                    offset = trailer_end + 2;
                    if offset != body.len() {
                        return Err(HttpProbeError::new(
                            "chunked HTTP body has bytes after the final terminator",
                        ));
                    }
                    return Ok(decoded);
                }
                let trailer = std::str::from_utf8(&body[offset..trailer_end]).map_err(|_| {
                    HttpProbeError::new("chunked HTTP body has a non-UTF-8 trailer")
                })?;
                let Some((name, value)) = trailer.split_once(':') else {
                    return Err(HttpProbeError::new(
                        "chunked HTTP body has a malformed trailer",
                    ));
                };
                if !crate::text_utils::valid_http_header_name(name.trim())
                    || !value.bytes().all(|byte| {
                        byte == b'\t'
                            || byte == b' '
                            || (0x21..=0x7e).contains(&byte)
                            || byte >= 0x80
                    })
                {
                    return Err(HttpProbeError::new(
                        "chunked HTTP body has an invalid trailer",
                    ));
                }
                offset = trailer_end + 2;
            }
        }
        if body.len() < offset + size {
            return Err(HttpProbeError::new("chunked HTTP body is incomplete"));
        }
        decoded.extend_from_slice(&body[offset..offset + size]);
        offset += size;
        if body.get(offset..offset + 2) == Some(b"\r\n") {
            offset += 2;
        } else if offset < body.len() {
            return Err(HttpProbeError::new(
                "chunked HTTP body is missing a chunk terminator",
            ));
        }
    }
}

fn find_crlf(body: &[u8], start: usize) -> Option<usize> {
    body.get(start..)?
        .windows(2)
        .position(|window| window == b"\r\n")
        .map(|index| start + index)
}

fn sse_json_body(body: &str) -> Option<String> {
    sse_json_rpc_body(body, None)
}

pub(crate) fn sse_json_rpc_body(body: &str, expected_id: Option<i64>) -> Option<String> {
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

pub(crate) fn probe_host(host: &str) -> String {
    let trimmed = host.trim().trim_start_matches('[').trim_end_matches(']');
    match trimmed {
        "" | "0.0.0.0" | "::" => runtimepaths::DEFAULT_LOCAL_HOST.to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests;
