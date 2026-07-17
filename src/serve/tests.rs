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

fn loopback_response_server(status: &str, body: &str) -> (u16, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        body.len(),
        body
    );
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 1024];
        let _ = std::io::Read::read(&mut stream, &mut request);
        std::io::Write::write_all(&mut stream, response.as_bytes()).unwrap();
    });
    (port, handle)
}

#[test]
fn health_probe_accepts_complete_readiness_response() {
    let (port, handle) =
        loopback_response_server("200 OK", r#"{"readiness":{"readyForRuntimeOps":true}}"#);

    health_probe_detail("127.0.0.1", port, "/healthz").unwrap();

    handle.join().unwrap();
}

#[test]
fn startup_health_failure_retains_probe_detail() {
    let (port, handle) = loopback_response_server("503 Service Unavailable", "{}");
    let probe_error = health_probe_detail("127.0.0.1", port, "/healthz").unwrap_err();
    assert_eq!(probe_error, "health endpoint returned HTTP status 503");
    handle.join().unwrap();

    let unavailable_port = free_port();
    let startup_error =
        wait_for_health("127.0.0.1", unavailable_port, "/healthz", 1, Duration::ZERO).unwrap_err();
    assert!(startup_error.contains("probe failures: 1x connect 127.0.0.1:"));
}

#[test]
fn cooperative_stop_tokens_are_random_and_strictly_validated() {
    let first = random_stop_token().unwrap();
    let second = random_stop_token().unwrap();
    assert!(valid_stop_token(&first));
    assert!(valid_stop_token(&second));
    assert_ne!(first, second);
    assert!(!valid_stop_token("short"));
    assert!(!valid_stop_token(&"g".repeat(64)));
}

#[test]
fn systemd_stop_errors_are_classified_narrowly() {
    assert!(systemd_service_absent(
        "Failed to stop mcpace-agent.service: Unit mcpace-agent.service not loaded."
    ));
    assert!(systemd_service_absent(
        "Unit mcpace-agent.service could not be found."
    ));
    assert!(systemd_user_manager_unavailable(
        "Failed to connect to bus: No medium found"
    ));
    assert!(systemd_user_manager_unavailable(
        "System has not been booted with systemd as init system (PID 1). Can't operate."
    ));
    assert!(!systemd_service_absent("Access denied"));
    assert!(!systemd_user_manager_unavailable(
        "Failed to connect to bus: Access denied"
    ));
    assert!(!systemd_user_manager_unavailable(
        "Failed to connect to bus: No medium found; Access denied"
    ));
    assert!(!systemd_user_manager_unavailable(
        "Failed to connect to bus: No medium found; Permission denied"
    ));
    assert!(systemd_stop_failure_is_ignorable(
        "Unit mcpace-agent.service not loaded.",
        false
    ));
    assert!(systemd_stop_failure_is_ignorable(
        "Failed to connect to bus: No medium found",
        true
    ));
    assert!(!systemd_stop_failure_is_ignorable(
        "Failed to connect to bus: No medium found",
        false
    ));
    assert!(!systemd_stop_failure_is_ignorable(
        "Unit mcpace-agent.service not loaded; Permission denied",
        true
    ));
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
    let state_root = runtimepaths::resolve_state_root(&root);
    let state_path = runtimepaths::serve_state_path(&state_root);
    let direct_state = read_state(&state_path).unwrap();
    assert_eq!(direct_state.supervisor_managed, Some(false));
    assert!(recorded_runtime_is_explicitly_direct(&root));
    let mut supervisor_state = direct_state.clone();
    supervisor_state.supervisor_managed = Some(true);
    write_state(&state_path, &supervisor_state).unwrap();
    assert!(!recorded_runtime_is_explicitly_direct(&root));
    supervisor_state.supervisor_managed = None;
    write_state(&state_path, &supervisor_state).unwrap();
    assert!(!recorded_runtime_is_explicitly_direct(&root));
    write_state(&state_path, &direct_state).unwrap();

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

    request_cooperative_serve_stop(&state_root, &state, state.stop_token.as_deref().unwrap())
        .unwrap();
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
        process_identity: None,
        stop_token: None,
        supervisor_managed: None,
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
fn serve_start_lock_reclaims_stale_identity_and_rejects_live_owner() {
    let root = temp_root();
    let state_root = runtimepaths::resolve_state_root(&root);
    runtimepaths::ensure_runtime_dir(&state_root).unwrap();
    runtimepaths::ensure_serve_dir(&state_root).unwrap();
    let lock_path = runtimepaths::serve_start_lock_path(&state_root);
    fs::write(
        &lock_path,
        JsonValue::object([
            ("pid", JsonValue::number(std::process::id())),
            (
                "processIdentity",
                JsonValue::string("reused-process-identity"),
            ),
            ("startedAtMs", JsonValue::number(now_ms())),
        ])
        .to_pretty_string(),
    )
    .unwrap();

    let guard = acquire_serve_start_lock(&state_root).unwrap();
    let (_, guard_identity) = read_serve_start_lock_owner(&lock_path).unwrap();
    fs::write(
        &lock_path,
        JsonValue::object([
            ("pid", JsonValue::number(1)),
            (
                "processIdentity",
                JsonValue::string(guard_identity.unwrap()),
            ),
            ("startedAtMs", JsonValue::number(now_ms())),
        ])
        .to_pretty_string(),
    )
    .unwrap();
    drop(guard);
    assert!(
        lock_path.is_file(),
        "drop must not remove another pid's lock"
    );
    fs::remove_file(&lock_path).unwrap();

    let current_identity = process_identity_token(std::process::id()).unwrap();
    fs::write(
        &lock_path,
        JsonValue::object([
            ("pid", JsonValue::number(std::process::id())),
            ("processIdentity", JsonValue::string(current_identity)),
            ("startedAtMs", JsonValue::number(now_ms())),
        ])
        .to_pretty_string(),
    )
    .unwrap();
    let error = acquire_serve_start_lock(&state_root).unwrap_err();
    assert!(error.contains("already in progress"), "error: {}", error);
    fs::remove_file(&lock_path).unwrap();

    let replacement = acquire_serve_start_lock(&state_root).unwrap();
    drop(replacement);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn serve_start_lock_reclaims_only_old_malformed_records() {
    let root = temp_root();
    let state_root = runtimepaths::resolve_state_root(&root);
    runtimepaths::ensure_runtime_dir(&state_root).unwrap();
    runtimepaths::ensure_serve_dir(&state_root).unwrap();
    let lock_path = runtimepaths::serve_start_lock_path(&state_root);
    fs::write(&lock_path, b"").unwrap();

    let recent_error = acquire_serve_start_lock(&state_root).unwrap_err();
    assert!(
        recent_error.contains("newer than 30 seconds"),
        "error: {}",
        recent_error
    );

    let file = OpenOptions::new().write(true).open(&lock_path).unwrap();
    file.set_times(
        std::fs::FileTimes::new().set_modified(
            std::time::SystemTime::now()
                .checked_sub(Duration::from_secs(31))
                .unwrap(),
        ),
    )
    .unwrap();
    let guard = acquire_serve_start_lock(&state_root).unwrap();
    drop(guard);
    assert!(!lock_path.exists());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn stop_refuses_to_signal_a_reused_live_pid() {
    let root = temp_root();
    let state_root = runtimepaths::resolve_state_root(&root);
    runtimepaths::ensure_runtime_dir(&state_root).unwrap();
    runtimepaths::ensure_serve_dir(&state_root).unwrap();
    let state_path = runtimepaths::serve_state_path(&state_root);
    let current_exe = std::env::current_exe().unwrap();
    let state = ServeState {
        root_path: sanitize_display(&root),
        state_root: sanitize_display(&state_root),
        host: "127.0.0.1".to_string(),
        port: free_port(),
        max_connections: None,
        io_timeout_ms: None,
        max_body_bytes: None,
        overview_cache_ms: None,
        url: "http://127.0.0.1/".to_string(),
        pid: std::process::id(),
        process_identity: Some("reused-process-identity".to_string()),
        stop_token: None,
        supervisor_managed: None,
        started_at_ms: now_ms(),
        runner_path: sanitize_display(&current_exe),
        stdout_log_path: "stdout".to_string(),
        stderr_log_path: "stderr".to_string(),
    };
    write_state(&state_path, &state).unwrap();

    let error = stop_existing_serve(&root).unwrap_err();
    assert!(error.contains("refusing to signal"), "error: {}", error);
    assert!(state_path.is_file());
    assert!(process_identity::capture(std::process::id())
        .unwrap()
        .is_some());

    let current_identity = process_identity_token(std::process::id()).unwrap();
    let mut tokenless_state = state.clone();
    tokenless_state.process_identity = Some(current_identity);
    tokenless_state.stop_token = None;
    write_state(&state_path, &tokenless_state).unwrap();
    let tokenless_error = stop_existing_serve(&root).unwrap_err();
    assert!(
        tokenless_error.contains("predates cooperative stop tokens"),
        "error: {}",
        tokenless_error
    );

    let mut pid_only_state = state;
    pid_only_state.process_identity = None;
    pid_only_state.runner_path.clear();
    write_state(&state_path, &pid_only_state).unwrap();
    let pid_only_error = stop_existing_serve(&root).unwrap_err();
    assert!(
        pid_only_error.contains("refusing to signal"),
        "error: {}",
        pid_only_error
    );

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
