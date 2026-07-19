use std::net::{TcpListener, TcpStream};
use std::time::Duration;

pub(crate) fn bind_loopback_test_listener() -> TcpListener {
    let mut last_error = String::new();
    for _ in 0..64 {
        let listener = match TcpListener::bind(("127.0.0.1", 0)) {
            Ok(listener) => listener,
            Err(error) => {
                last_error = error.to_string();
                continue;
            }
        };
        let addr = match listener.local_addr() {
            Ok(addr) => addr,
            Err(error) => {
                last_error = error.to_string();
                continue;
            }
        };
        match TcpStream::connect_timeout(&addr, Duration::from_millis(250)) {
            Ok(probe_stream) => match listener.accept() {
                Ok((accepted_stream, _)) => {
                    drop(accepted_stream);
                    drop(probe_stream);
                    return listener;
                }
                Err(error) => last_error = error.to_string(),
            },
            Err(error) => last_error = error.to_string(),
        }
    }

    panic!("failed to bind reachable loopback test listener: {last_error}");
}
