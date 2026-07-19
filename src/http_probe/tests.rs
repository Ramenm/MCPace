use super::*;
use std::io::{Read, Write};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn raw_probe_timeout_is_a_total_deadline_against_trickle_responses() {
    let _local_server_guard = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let listener = crate::bind_loopback_test_listener();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let server_stop = std::sync::Arc::clone(&stop);
    let handle = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(3);
        let (mut stream, _) = loop {
            if server_stop.load(std::sync::atomic::Ordering::Acquire) || Instant::now() >= deadline
            {
                return false;
            }
            match listener.accept() {
                Ok(connection) => break connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("test listener accept failed: {error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0u8; 256];
        while request.len() < 4096 {
            let Ok(count) = stream.read(&mut buffer) else {
                break;
            };
            if count == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..count]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        for byte in b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}" {
            if stream.write_all(&[*byte]).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(40));
        }
        true
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
    stop.store(true, std::sync::atomic::Ordering::Release);
    assert!(
        handle.join().unwrap(),
        "trickle-response fixture must accept the bounded probe"
    );
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
