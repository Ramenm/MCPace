use super::{
    jsonrpc_result, parse_http_url, run_http_request, validate_configured_headers, HttpResponse,
    UpstreamServerConfig,
};
use crate::execution::ExecutionPolicy;
use crate::json::{parse_str, JsonValue};
use crate::{json_helpers, mcp_protocol as mcp, text_utils};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

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
fn parse_http_url_accepts_ipv4_mapped_loopback_ipv6() {
    let parsed = parse_http_url("http://[::ffff:127.0.0.1]:39022/mcp")
        .expect("parse IPv4-mapped loopback URL");
    assert_eq!(parsed.host, "::ffff:127.0.0.1");
    assert_eq!(parsed.port, 39022);
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
fn parse_http_url_accepts_remote_https_with_tls_defaults() {
    let parsed =
        parse_http_url("HTTPS://mcp.example.com/mcp?tenant=one").expect("parse remote HTTPS URL");
    assert!(parsed.secure);
    assert_eq!(parsed.host, "mcp.example.com");
    assert_eq!(parsed.port, 443);
    assert_eq!(parsed.path, "/mcp?tenant=one");
}

#[test]
fn parse_http_url_rejects_non_loopback_plain_http_upstreams() {
    for url in [
        "http://example.com/mcp",
        "http://127.example.com/mcp",
        "http://10.0.0.10/mcp",
        "http://172.16.0.10/mcp",
        "http://192.168.1.10/mcp",
    ] {
        assert!(
            parse_http_url(url).is_err(),
            "plain HTTP upstream should be loopback-only: {url:?}"
        );
    }
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
        "http://127.0.0.1:0/mcp",
        "http://127.0.0.1:65536/mcp",
        "https://mcp.example.com/path#fragment",
        "https://mcp.example.com\\evil/mcp",
    ];
    for url in rejected {
        assert!(
            parse_http_url(url).is_err(),
            "URL should be rejected: {url:?}"
        );
    }
}

fn test_http_server(port: u16) -> UpstreamServerConfig {
    UpstreamServerConfig {
        name: "fault-http".to_string(),
        enabled: true,
        disabled_reason: None,
        source_type: "http".to_string(),
        command: None,
        args: Vec::new(),
        env: BTreeMap::new(),
        headers: BTreeMap::new(),
        cwd: None,
        url: Some(format!("http://127.0.0.1:{port}/mcp")),
        timeout_ms: 1_000,
        execution: ExecutionPolicy::default(),
        tool_policies: Vec::new(),
    }
}

fn read_test_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut raw = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let count = stream.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        raw.extend_from_slice(&buffer[..count]);
        assert!(raw.len() <= 1024 * 1024, "test HTTP request was unbounded");

        let Some(header_end) = raw.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = std::str::from_utf8(&raw[..header_end]).unwrap();
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().unwrap())
            })
            .unwrap_or(0);
        if raw.len() >= header_end + 4 + content_length {
            break;
        }
    }
    String::from_utf8(raw).unwrap()
}

fn write_test_response(stream: &mut TcpStream, status: &str, body: &str, session: bool) {
    let session = if session {
        "Mcp-Session-Id: fault-session\r\n"
    } else {
        ""
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{session}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).unwrap();
}

fn initialize_response_body() -> String {
    mcp::result(
        JsonValue::number(1),
        JsonValue::object([
            (
                "protocolVersion",
                JsonValue::string(mcp::CURRENT_PROTOCOL_VERSION),
            ),
            ("capabilities", mcp::empty_object()),
            (
                "serverInfo",
                JsonValue::object([
                    ("name", JsonValue::string("fault-http")),
                    ("version", JsonValue::string("1.0.0")),
                ]),
            ),
        ]),
    )
    .to_compact_string()
}

#[test]
fn initialized_notification_failure_is_not_reported_as_a_live_session() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = thread::spawn(move || {
        for step in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_test_request(&mut stream);
            match step {
                0 => write_test_response(&mut stream, "200 OK", &initialize_response_body(), true),
                1 => write_test_response(
                    &mut stream,
                    "500 Internal Server Error",
                    "{\"error\":\"broken notification\"}",
                    false,
                ),
                _ => {
                    assert!(request.starts_with("DELETE "));
                    write_test_response(&mut stream, "204 No Content", "", false);
                }
            }
        }
    });

    let error = run_http_request(
        &test_http_server(port),
        "tools/list",
        None,
        Duration::from_secs(2),
    )
    .expect_err("failed initialized notification must fail")
    .to_string();
    assert!(error.contains("status 500 for notifications/initialized"));
    handle.join().unwrap();
}

#[test]
fn http_timeout_is_a_total_lifecycle_budget_not_a_per_step_multiplier() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = thread::spawn(move || {
        for step in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_test_request(&mut stream);
            if step == 0 {
                write_test_response(&mut stream, "200 OK", &initialize_response_body(), true);
                continue;
            }
            thread::sleep(Duration::from_millis(600));
            if step == 1 {
                write_test_response(&mut stream, "202 Accepted", "", false);
            } else {
                let body = request
                    .split_once("\r\n\r\n")
                    .and_then(|(_, body)| parse_str(body).ok())
                    .and_then(|value| json_helpers::value_at_path(&value, &["id"]).cloned())
                    .map(|id| {
                        mcp::result(id, JsonValue::object([("tools", JsonValue::array([]))]))
                            .to_compact_string()
                    })
                    .unwrap_or_default();
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(), body
                    )
                    .as_bytes(),
                );
            }
        }
    });

    let started = Instant::now();
    let result = run_http_request(
        &test_http_server(port),
        "tools/list",
        None,
        Duration::from_secs(1),
    );
    let elapsed = started.elapsed();
    assert!(result.is_err());
    assert!(
        elapsed < Duration::from_millis(1_500),
        "global timeout was multiplied across lifecycle steps: {elapsed:?}"
    );
    handle.join().unwrap();
}

#[test]
fn jsonrpc_requests_reject_accepted_without_response_and_wrong_protocol_envelopes() {
    let accepted = HttpResponse {
        status: 202,
        content_type: "application/json".to_string(),
        session_id: None,
        body: String::new(),
    };
    assert!(jsonrpc_result("buggy", "tools/list", 2, &accepted).is_err());

    let wrong_version = HttpResponse {
        status: 200,
        content_type: "application/json".to_string(),
        session_id: None,
        body: JsonValue::object([
            ("jsonrpc", JsonValue::string("1.0")),
            ("id", JsonValue::number(2)),
            (
                "result",
                JsonValue::object([("tools", JsonValue::array([]))]),
            ),
        ])
        .to_compact_string(),
    };
    let error = jsonrpc_result("buggy", "tools/list", 2, &wrong_version)
        .expect_err("wrong JSON-RPC version must fail")
        .to_string();
    assert!(error.contains("jsonrpc \"2.0\""));
}

#[test]
fn configured_headers_reject_case_insensitive_duplicates() {
    let headers = BTreeMap::from([
        ("Authorization".to_string(), "Bearer one".to_string()),
        ("authorization".to_string(), "Bearer two".to_string()),
    ]);
    assert!(validate_configured_headers(&headers)
        .expect_err("ambiguous duplicate header must fail")
        .to_string()
        .contains("duplicate header"));
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
