use super::HttpRequest;

pub(super) fn validate_origin(request: &HttpRequest) -> Result<(), String> {
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

pub(crate) fn is_allowed_local_origin(origin: &str) -> bool {
    if origin == "null" {
        return true;
    }
    let Some(authority) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    if authority.is_empty() || authority.contains('/') || authority.contains('@') {
        return false;
    }

    let Some(host) = origin_host(authority) else {
        return false;
    };
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "[::1]"
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
    !value.is_empty() && value.chars().all(|character| character.is_ascii_digit())
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

pub(super) fn request_header_string(request: Option<&HttpRequest>, key: &str) -> Option<String> {
    let key = key.to_ascii_lowercase();
    request.and_then(|request| {
        request
            .headers
            .iter()
            .find(|(candidate, _)| candidate == &key)
            .map(|(_, value)| value.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}
