use std::time::Duration;

#[derive(Clone, Debug)]
pub(crate) struct HttpTextResponse {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: String,
}

pub(crate) fn local_http_url(host: &str, port: u16, path: &str) -> String {
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    format!("http://{}:{}{}", http_authority_host(host), port, path)
}

pub(crate) fn get_text(url: &str, timeout: Duration) -> Result<HttpTextResponse, String> {
    let response = ureq::get(url)
        .config()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .call()
        .map_err(|error| format!("GET {url}: {error}"))?;
    read_response(response, "GET", url)
}

pub(crate) fn post_json_text(
    url: &str,
    body: &str,
    headers: &[(&str, &str)],
    timeout: Duration,
) -> Result<HttpTextResponse, String> {
    let mut request = ureq::post(url)
        .header("Accept", "application/json, text/event-stream")
        .content_type("application/json");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = request
        .config()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .send(body)
        .map_err(|error| format!("POST {url}: {error}"))?;
    read_response(response, "POST", url)
}

fn read_response(
    mut response: ureq::http::Response<ureq::Body>,
    method: &str,
    url: &str,
) -> Result<HttpTextResponse, String> {
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_string(), value.to_string()))
        })
        .collect::<Vec<_>>();
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|error| format!("read {method} {url} response body: {error}"))?;
    Ok(HttpTextResponse {
        status,
        headers,
        body,
    })
}

fn http_authority_host(host: &str) -> String {
    if host.starts_with('[') || !host.contains(':') {
        host.to_string()
    } else {
        format!("[{host}]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_http_url_brackets_ipv6_hosts() {
        assert_eq!(
            local_http_url("::1", 39022, "/health"),
            "http://[::1]:39022/health"
        );
        assert_eq!(
            local_http_url("127.0.0.1", 39022, "health"),
            "http://127.0.0.1:39022/health"
        );
    }
}
