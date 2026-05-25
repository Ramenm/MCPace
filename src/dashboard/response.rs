use crate::json::JsonValue;
use crate::json_helpers;
use crate::runtimepaths;
use std::io::Write;
use std::net::{Shutdown, TcpStream};

pub(super) fn write_json_response(
    stream: &mut TcpStream,
    status: &str,
    payload: &JsonValue,
) -> Result<(), String> {
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
) -> Result<(), String> {
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
) -> Result<(), String> {
    write_response(stream, status, content_type, body.as_bytes())
}

pub(super) fn write_empty_response(stream: &mut TcpStream, status: &str) -> Result<(), String> {
    write_response(stream, status, "text/plain; charset=utf-8", &[])
}

pub(super) fn write_empty_response_with_headers(
    stream: &mut TcpStream,
    status: &str,
    extra_headers: &[(&str, &str)],
) -> Result<(), String> {
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
) -> Result<(), String> {
    write_response_with_headers(stream, status, content_type, body, &[])
}

pub(super) fn write_response_with_headers(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
    extra_headers: &[(&str, &str)],
) -> Result<(), String> {
    let mut header = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\n",
        status,
        content_type,
        body.len()
    );
    for (name, value) in extra_headers {
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
    let _ = stream.shutdown(Shutdown::Both);
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
