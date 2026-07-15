use super::http_boundary;
use crate::json::JsonValue;
use crate::json_helpers;
use crate::runtimepaths;
use std::fmt;
use std::io::Write;
use std::net::{Shutdown, TcpStream};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ResponseWriteError {
    message: String,
}

pub(super) type ResponseWriteResult<T> = std::result::Result<T, ResponseWriteError>;

impl ResponseWriteError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ResponseWriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ResponseWriteError {}

impl From<String> for ResponseWriteError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<&str> for ResponseWriteError {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

impl From<ResponseWriteError> for String {
    fn from(error: ResponseWriteError) -> Self {
        error.message
    }
}

pub(super) fn write_json_response(
    stream: &mut TcpStream,
    status: &str,
    payload: &JsonValue,
) -> ResponseWriteResult<()> {
    write_response(
        stream,
        status,
        "application/json; charset=utf-8",
        payload.to_pretty_string().as_bytes(),
    )
}

pub(super) fn write_json_response_with_owned_headers(
    stream: &mut TcpStream,
    status: &str,
    payload: &JsonValue,
    extra_headers: &[(String, String)],
) -> ResponseWriteResult<()> {
    let headers = extra_headers
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect::<Vec<_>>();
    write_response_with_headers(
        stream,
        status,
        "application/json; charset=utf-8",
        payload.to_pretty_string().as_bytes(),
        &headers,
    )
}

pub(super) fn write_text_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) -> ResponseWriteResult<()> {
    write_response(stream, status, content_type, body.as_bytes())
}

pub(super) fn write_empty_response(
    stream: &mut TcpStream,
    status: &str,
) -> ResponseWriteResult<()> {
    write_response(stream, status, "text/plain; charset=utf-8", &[])
}

pub(super) fn write_empty_response_with_headers(
    stream: &mut TcpStream,
    status: &str,
    extra_headers: &[(&str, &str)],
) -> ResponseWriteResult<()> {
    write_response_with_headers(
        stream,
        status,
        "text/plain; charset=utf-8",
        &[],
        extra_headers,
    )
}

pub(super) fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> ResponseWriteResult<()> {
    write_response_with_headers(stream, status, content_type, body, &[])
}

pub(super) fn write_response_with_headers(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
    extra_headers: &[(&str, &str)],
) -> ResponseWriteResult<()> {
    let mut header = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nX-Frame-Options: DENY\r\nCross-Origin-Resource-Policy: same-origin\r\nPermissions-Policy: camera=(), geolocation=(), microphone=()\r\nContent-Security-Policy: default-src 'none'; connect-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'\r\n",
        status,
        content_type,
        body.len()
    );
    for (name, value) in extra_headers {
        if !http_boundary::is_valid_http_header_name(name) {
            return Err(ResponseWriteError::new(format!(
                "invalid response header name: {}",
                name
            )));
        }
        if !http_boundary::is_valid_http_header_value(value) {
            return Err(ResponseWriteError::new(format!(
                "invalid response header value for {}",
                name
            )));
        }
        header.push_str(name);
        header.push_str(": ");
        header.push_str(value);
        header.push_str("\r\n");
    }
    header.push_str("Connection: close\r\n\r\n");
    stream
        .write_all(header.as_bytes())
        .map_err(|error| format!("write response header: {}", error))?;
    stream
        .write_all(body)
        .map_err(|error| format!("write response body: {}", error))?;
    stream
        .flush()
        .map_err(|error| format!("flush response: {}", error))?;
    // Half-close the response side after flushing. Closing both directions while
    // request bytes are still buffered (for example, an early 413/503) makes
    // Winsock send RST and the client loses the HTTP status entirely.
    let _ = stream.shutdown(Shutdown::Write);
    Ok(())
}

pub(super) fn split_target(target: &str) -> (&str, &str) {
    match target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (target, ""),
    }
}

pub(super) fn query_parameter<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query
        .split('&')
        .find_map(|pair| match pair.split_once('=') {
            Some((candidate, value)) if candidate == key => Some(value),
            _ => None,
        })
}

pub(super) fn empty_object() -> JsonValue {
    json_helpers::empty_object()
}

pub(super) fn sanitize_root_path(root_path: &str) -> String {
    runtimepaths::strip_windows_extended_path_prefix(root_path)
}

pub(super) fn now_ms() -> u128 {
    runtimepaths::unix_time_ms()
}
