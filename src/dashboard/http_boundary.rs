use super::HttpRequest;
use std::net::IpAddr;

pub(super) fn validate_origin(request: &HttpRequest) -> Result<(), String> {
    let mut hosts = request.headers.iter().filter(|(key, _)| key == "host");
    let Some((_, host)) = hosts.next() else {
        return Err("missing required Host header for local MCPace serve mode".to_string());
    };
    if hosts.next().is_some() {
        return Err(
            "multiple Host headers are not allowed for local MCPace serve mode".to_string(),
        );
    }
    if !is_allowed_local_host(host.trim()) {
        return Err(format!(
            "host '{}' is not allowed for local MCPace serve mode",
            host
        ));
    }

    if let Some((_, origin)) = request.headers.iter().find(|(key, _)| key == "origin") {
        if !is_allowed_local_origin(origin.trim()) {
            return Err(format!(
                "origin '{}' is not allowed for local MCPace serve mode",
                origin
            ));
        }
    }
    Ok(())
}

pub(super) fn is_valid_http_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            matches!(
                byte,
                b'!' | b'#'
                    | b'$'
                    | b'%'
                    | b'&'
                    | b'\''
                    | b'*'
                    | b'+'
                    | b'-'
                    | b'.'
                    | b'^'
                    | b'_'
                    | b'`'
                    | b'|'
                    | b'~'
                    | b'0'..=b'9'
                    | b'A'..=b'Z'
                    | b'a'..=b'z'
            )
        })
}

pub(super) fn is_valid_http_header_value(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte == b' ' || (0x21..=0x7e).contains(&byte))
}

pub(crate) fn is_allowed_local_origin(origin: &str) -> bool {
    if origin == "null" {
        return false;
    }
    let Some(authority) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    is_allowed_local_authority(authority)
}

pub(crate) fn is_allowed_local_host(host_header: &str) -> bool {
    is_allowed_local_authority(host_header)
}

fn is_allowed_local_authority(authority: &str) -> bool {
    if authority.is_empty() || authority.contains('/') || authority.contains('@') {
        return false;
    }

    let Some(host) = origin_host(authority) else {
        return false;
    };
    is_loopback_host(host)
}

pub(crate) fn is_loopback_host(host: &str) -> bool {
    let trimmed = host.trim();
    if trimmed.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let normalized = trimmed
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(trimmed);
    normalized
        .parse::<IpAddr>()
        .map(|address| address.is_loopback())
        .unwrap_or(false)
}

fn origin_host(authority: &str) -> Option<&str> {
    if authority.starts_with('[') {
        let end = authority.find(']')?;
        let host = &authority[..=end];
        let suffix = &authority[end + 1..];
        if suffix.is_empty() || valid_port_suffix(suffix) {
            return Some(host);
        }
        return None;
    }

    if authority.matches(':').count() > 1 {
        return None;
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && valid_port(port) => Some(host),
        Some(_) => None,
        None => Some(authority),
    }
}

fn valid_port_suffix(value: &str) -> bool {
    value.strip_prefix(':').map(valid_port).unwrap_or(false)
}

fn valid_port(value: &str) -> bool {
    value.parse::<u16>().is_ok()
}

pub(super) fn accepts(request: &HttpRequest, media_type: &str) -> bool {
    request
        .headers
        .iter()
        .filter(|(key, _)| key == "accept")
        .any(|(_, value)| {
            value.split(',').any(|item| {
                item.trim()
                    .split(';')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .eq_ignore_ascii_case(media_type)
            })
        })
}

pub(super) fn accepts_streamable_http_post(request: &HttpRequest) -> bool {
    accepts(request, "application/json") && accepts(request, "text/event-stream")
}

pub(super) fn content_type_is(request: &HttpRequest, media_type: &str) -> bool {
    let mut values = request
        .headers
        .iter()
        .filter(|(key, _)| key == "content-type");
    let Some((_, value)) = values.next() else {
        return false;
    };
    if values.next().is_some() {
        return false;
    }
    value
        .trim()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .eq_ignore_ascii_case(media_type)
}

pub(super) fn request_header_string(request: Option<&HttpRequest>, key: &str) -> Option<String> {
    request_header_string_unique(request, key).ok().flatten()
}

pub(super) fn request_header_string_unique(
    request: Option<&HttpRequest>,
    key: &str,
) -> Result<Option<String>, String> {
    let Some(request) = request else {
        return Ok(None);
    };
    let key_lower = key.to_ascii_lowercase();
    let mut matches = request
        .headers
        .iter()
        .filter(|(candidate, _)| candidate == &key_lower);
    let Some((_, value)) = matches.next() else {
        return Ok(None);
    };
    if matches.next().is_some() {
        return Err(format!(
            "multiple {} headers are not allowed for MCP HTTP requests",
            key
        ));
    }
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(trimmed.to_string()))
}
