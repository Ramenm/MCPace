use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn raw_probe_timeout_is_a_total_deadline_against_trickle_responses() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let _ = stream.read_to_end(&mut request);
        for byte in b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}" {
            if stream.write_all(&[*byte]).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(40));
        }
    });
    let started = Instant::now();
    let result = raw_response(
        "127.0.0.1",
        port,
        "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        Duration::from_millis(250),
        1024,
    );
    assert!(result.is_err());
    assert!(started.elapsed() < Duration::from_millis(600));
    handle.join().unwrap();
}

#[test]
fn mcp_json_rpc_response_ready_waits_for_expected_sse_id() {
    let partial = concat!(
        "HTTP/1.1 200 OK\r\n",
        "Content-Type: text/event-stream\r\n",
        "\r\n",
        "event: message\n",
        "data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n"
    );
    assert!(!mcp_json_rpc_response_ready(partial, Some(2)));

    let complete = format!(
        "{}event: message\ndata: {{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{{}}}}\n\n",
        partial
    );
    assert!(mcp_json_rpc_response_ready(&complete, Some(2)));
}

#[test]
fn parse_response_rejects_truncation_and_ambiguous_framing() {
    let truncated = parse_response(concat!(
        "HTTP/1.1 200 OK\r\n",
        "Content-Length: 10\r\n",
        "\r\n{}"
    ))
    .expect("parse headers");
    assert!(truncated.body_bytes().is_err());

    for raw in [
        concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Length: 2\r\n",
            "Content-Length: 3\r\n",
            "\r\n{}"
        ),
        concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Length: 2\r\n",
            "Content-Length: 2\r\n",
            "\r\n{}"
        ),
        concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Length: 2\r\n",
            "Transfer-Encoding: chunked\r\n",
            "\r\n0\r\n\r\n"
        ),
        concat!(
            "HTTP/1.1 200 OK\r\n",
            "Transfer-Encoding: chunked, chunked\r\n",
            "\r\n0\r\n\r\n"
        ),
        concat!("HTTP/1.1 200 OK\nContent-Length: 2\r\n", "\r\n{}"),
        concat!("NOTHTTP 200 OK\r\n", "Content-Length: 2\r\n", "\r\n{}"),
    ] {
        assert!(
            parse_response(raw).is_err(),
            "response should fail: {raw:?}"
        );
    }
}

#[test]
fn chunked_response_requires_complete_final_terminator() {
    let truncated = parse_response(concat!(
        "HTTP/1.1 200 OK\r\n",
        "Transfer-Encoding: chunked\r\n",
        "\r\n",
        "2\r\n{}\r\n0\r\n"
    ))
    .expect("parse chunked headers");
    assert!(truncated.body_bytes().is_err());

    let complete = parse_response(concat!(
        "HTTP/1.1 200 OK\r\n",
        "Transfer-Encoding: chunked\r\n",
        "\r\n",
        "2\r\n{}\r\n0\r\nX-Proof: yes\r\n\r\n"
    ))
    .expect("parse chunked headers");
    assert_eq!(complete.body_bytes().unwrap(), b"{}");
}

#[test]
fn parse_response_decodes_headers_case_insensitively() {
    let parsed = parse_response(concat!(
        "HTTP/1.1 200 OK\r\n",
        "Content-Type: application/json\r\n",
        "Mcp-Session-Id: abc123\r\n",
        "Content-Length: 2\r\n",
        "\r\n{}tail"
    ))
    .expect("parse response");

    assert_eq!(parsed.status, 200);
    assert_eq!(parsed.header("mcp-session-id"), Some("abc123"));
    assert_eq!(
        String::from_utf8(parsed.body_bytes().expect("body bytes")).unwrap(),
        "{}"
    );
}
