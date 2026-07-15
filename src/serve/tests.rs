use super::*;
use std::net::TcpListener;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_root() -> PathBuf {
    let unique = format!(
        "mcpace-serve-test-{}-{}-{}",
        std::process::id(),
        now_ms(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let path = std::env::temp_dir().join(unique);
    fs::create_dir_all(&path).unwrap();
    path
}

fn free_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

#[test]
fn serve_url_and_probe_hosts_handle_ipv6_and_wildcards() {
    assert_eq!(
        http_url("127.0.0.1", 39022, "/mcp"),
        "http://127.0.0.1:39022/mcp"
    );
    assert_eq!(http_url("::1", 39022, "/mcp"), "http://[::1]:39022/mcp");
    assert_eq!(
        http_url("[::1]", 39022, "/healthz"),
        "http://[::1]:39022/healthz"
    );
    assert_eq!(
        http_url("0.0.0.0", 39022, "/mcp"),
        "http://127.0.0.1:39022/mcp"
    );
    assert_eq!(http_url("::", 39022, "/mcp"), "http://127.0.0.1:39022/mcp");
    assert_eq!(
        http_probe::probe_host("0.0.0.0"),
        runtimepaths::DEFAULT_LOCAL_HOST
    );
    assert_eq!(
        http_probe::probe_host("::"),
        runtimepaths::DEFAULT_LOCAL_HOST
    );
}

#[test]
fn serve_rejects_deprecated_nonlocal_flags_before_starting_runner() {
    let parsed = parse_cli(&[
        "start".to_string(),
        "--host".to_string(),
        "0.0.0.0".to_string(),
        "--allow-nonlocal-bind".to_string(),
    ]);
    assert!(parsed.error.as_deref().is_some_and(
        |error| error.contains("direct non-loopback HTTP flags are no longer supported")
    ));
}

#[test]
fn serve_start_status_stop_round_trip() {
    let _local_server_guard = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = temp_root();
    if resolve_runner_source().is_err() {
        let _ = fs::remove_dir_all(root);
        return;
    }
    let port = free_port();
    fs::write(
        root.join("mcpace.config.json"),
        format!(
            r#"{{
  "version": "0.3.5",
  "serve": {{ "host": "127.0.0.1", "port": {} }},
  "profiles": {{
"runtime": {{
  "default": "safe",
  "profiles": {{
    "safe": {{ "description": "Safe", "serverOverrides": {{}} }}
  }}
}}
  }},
  "servers": {{}}
}}"#,
            port
        ),
    )
    .unwrap();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let start = run(
        &[
            "start".to_string(),
            "--json".to_string(),
            "--root".to_string(),
            root.display().to_string(),
            "--port".to_string(),
            port.to_string(),
        ],
        None,
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(start, 0, "stderr: {}", String::from_utf8_lossy(&stderr));
    let start_text = String::from_utf8(stdout.clone()).unwrap();
    assert!(
        start_text.contains(r#""status": "running""#),
        "stdout: {}",
        start_text
    );
    assert!(
        health_check("127.0.0.1", port, runtimepaths::DEFAULT_LOCAL_HEALTH_PATH).unwrap_or(false)
    );

    let mut status_stdout = Vec::new();
    let mut status_stderr = Vec::new();
    let status = run(
        &[
            "status".to_string(),
            "--json".to_string(),
            "--root".to_string(),
            root.display().to_string(),
        ],
        None,
        &mut status_stdout,
        &mut status_stderr,
    );
    assert_eq!(
        status,
        0,
        "stderr: {}",
        String::from_utf8_lossy(&status_stderr)
    );
    let status_text = String::from_utf8(status_stdout).unwrap();
    assert!(
        status_text.contains(r#""status": "running""#),
        "stdout: {}",
        status_text
    );
    assert!(
        status_text.contains(&format!(r#""port": {}"#, port)),
        "stdout: {}",
        status_text
    );

    let mut restart_stdout = Vec::new();
    let mut restart_stderr = Vec::new();
    let restart = run(
        &[
            "restart".to_string(),
            "--json".to_string(),
            "--root".to_string(),
            root.display().to_string(),
            "--port".to_string(),
            port.to_string(),
        ],
        None,
        &mut restart_stdout,
        &mut restart_stderr,
    );
    assert_eq!(
        restart,
        0,
        "stderr: {}",
        String::from_utf8_lossy(&restart_stderr)
    );
    let restart_text = String::from_utf8(restart_stdout).unwrap();
    assert!(
        restart_text.contains(r#""status": "running""#),
        "stdout: {}",
        restart_text
    );

    let mut stop_stdout = Vec::new();
    let mut stop_stderr = Vec::new();
    let stop = run(
        &[
            "stop".to_string(),
            "--json".to_string(),
            "--root".to_string(),
            root.display().to_string(),
        ],
        None,
        &mut stop_stdout,
        &mut stop_stderr,
    );
    assert_eq!(stop, 0, "stderr: {}", String::from_utf8_lossy(&stop_stderr));
    let stop_text = String::from_utf8(stop_stdout).unwrap();
    assert!(
        stop_text.contains(r#""status": "stopped""#),
        "stdout: {}",
        stop_text
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn status_reports_healthy_endpoint_without_managed_state() {
    let _local_server_guard = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = temp_root();
    if resolve_runner_source().is_err() {
        let _ = fs::remove_dir_all(root);
        return;
    }
    let port = free_port();
    fs::write(
        root.join("mcpace.config.json"),
        format!(
            r#"{{
  "version": "0.3.5",
  "serve": {{ "host": "127.0.0.1", "port": {} }},
  "profiles": {{
"runtime": {{
  "default": "safe",
  "profiles": {{
    "safe": {{ "description": "Safe", "serverOverrides": {{}} }}
  }}
}}
  }},
  "servers": {{}}
}}"#,
            port
        ),
    )
    .unwrap();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let start = run(
        &[
            "start".to_string(),
            "--json".to_string(),
            "--root".to_string(),
            root.display().to_string(),
            "--port".to_string(),
            port.to_string(),
        ],
        None,
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(start, 0, "stderr: {}", String::from_utf8_lossy(&stderr));

    let state_root = runtimepaths::resolve_state_root(&root);
    let state_path = runtimepaths::serve_state_path(&state_root);
    let state = read_state(&state_path).unwrap();
    fs::remove_file(&state_path).unwrap();

    let mut status_stdout = Vec::new();
    let mut status_stderr = Vec::new();
    let status = run(
        &[
            "status".to_string(),
            "--json".to_string(),
            "--root".to_string(),
            root.display().to_string(),
        ],
        None,
        &mut status_stdout,
        &mut status_stderr,
    );
    assert_eq!(
        status,
        0,
        "stderr: {}",
        String::from_utf8_lossy(&status_stderr)
    );
    let status_text = String::from_utf8(status_stdout).unwrap();
    assert!(
        status_text.contains(r#""status": "running""#),
        "stdout: {}",
        status_text
    );
    assert!(
        status_text.contains("no managed serve state file exists"),
        "stdout: {}",
        status_text
    );

    let _ = kill_process(state.pid);
    for _ in 0..40 {
        if !health_check(
            &state.host,
            state.port,
            runtimepaths::DEFAULT_LOCAL_HEALTH_PATH,
        )
        .unwrap_or(false)
        {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    remove_managed_serve_runner_copy(&state_root, &state);
    cleanup_stale_serve_runner_copies(&state_root, None);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn start_recovers_stale_state_even_with_restart_guard() {
    let _local_server_guard = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = temp_root();
    if resolve_runner_source().is_err() {
        let _ = fs::remove_dir_all(root);
        return;
    }
    let port = free_port();
    fs::write(
        root.join("mcpace.config.json"),
        format!(
            r#"{{
  "version": "0.3.5",
  "serve": {{ "host": "127.0.0.1", "port": {} }},
  "profiles": {{
"runtime": {{
  "default": "safe",
  "profiles": {{
    "safe": {{ "description": "Safe", "serverOverrides": {{}} }}
  }}
}}
  }},
  "servers": {{}}
}}"#,
            port
        ),
    )
    .unwrap();
    let state_root = runtimepaths::resolve_state_root(&root);
    runtimepaths::ensure_runtime_dir(&state_root).unwrap();
    runtimepaths::ensure_serve_dir(&state_root).unwrap();
    let stale_state = ServeState {
        root_path: sanitize_display(&root),
        state_root: sanitize_display(&state_root),
        host: "127.0.0.1".to_string(),
        port,
        max_connections: Some(32),
        io_timeout_ms: None,
        max_body_bytes: None,
        overview_cache_ms: Some(250),
        url: runtimepaths::http_url("127.0.0.1", port, runtimepaths::DEFAULT_LOCAL_MCP_PATH),
        pid: 999_999,
        started_at_ms: now_ms(),
        runner_path: sanitize_display(&runtimepaths::runtime_bin_dir(&state_root).join(
            if cfg!(windows) {
                "mcpace-serve-stale.exe"
            } else {
                "mcpace-serve-stale"
            },
        )),
        stdout_log_path: sanitize_display(&runtimepaths::serve_stdout_log_path(&state_root)),
        stderr_log_path: sanitize_display(&runtimepaths::serve_stderr_log_path(&state_root)),
    };
    write_state(&runtimepaths::serve_state_path(&state_root), &stale_state).unwrap();
    fs::write(
        runtimepaths::serve_restart_guard_path(&state_root),
        now_ms().to_string(),
    )
    .unwrap();

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let start = run(
        &[
            "start".to_string(),
            "--json".to_string(),
            "--root".to_string(),
            root.display().to_string(),
            "--port".to_string(),
            port.to_string(),
            "--max-connections".to_string(),
            "32".to_string(),
            "--overview-cache-ms".to_string(),
            "250".to_string(),
        ],
        None,
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(start, 0, "stderr: {}", String::from_utf8_lossy(&stderr));
    let start_text = String::from_utf8(stdout).unwrap();
    assert!(
        start_text.contains(r#""status": "running""#),
        "stdout: {}",
        start_text
    );

    let mut stop_stdout = Vec::new();
    let mut stop_stderr = Vec::new();
    let stop = run(
        &[
            "stop".to_string(),
            "--json".to_string(),
            "--root".to_string(),
            root.display().to_string(),
        ],
        None,
        &mut stop_stdout,
        &mut stop_stderr,
    );
    assert_eq!(stop, 0, "stderr: {}", String::from_utf8_lossy(&stop_stderr));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn stale_runner_warning_detects_outdated_runner_copy() {
    let root = temp_root();
    let current = root.join(if cfg!(windows) {
        "mcpace-current.exe"
    } else {
        "mcpace-current"
    });
    let runner = root.join(if cfg!(windows) {
        "mcpace-serve.exe"
    } else {
        "mcpace-serve"
    });

    fs::write(&current, b"current binary").unwrap();
    fs::write(&runner, b"old").unwrap();
    let warning = stale_runner_warning_for_paths(&runner, &current).unwrap();
    assert!(
        warning.contains("mcpace serve restart"),
        "warning: {}",
        warning
    );

    fs::write(&runner, b"current binary").unwrap();
    assert!(stale_runner_warning_for_paths(&runner, &current).is_none());

    let _ = fs::remove_dir_all(root);
}
