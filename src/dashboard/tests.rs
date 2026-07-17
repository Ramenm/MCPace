use super::overview::{
    join_json_command_handles, overview_build_invocations, reset_overview_build_invocations,
};
use super::{
    build_overview_json, cached_health_json, cached_overview_json, is_allowed_local_host,
    is_allowed_local_origin, query_bool_flag, run_http_tool, run_json_command, runtime_status_json,
    serve_listener, OverviewCacheState,
};
use crate::json::JsonValue;
use crate::json_helpers;
use std::ffi::OsString;
use std::fs;
use std::io::{BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant};

struct EnvVarGuard {
    key: &'static str,
    value: Option<OsString>,
}

impl EnvVarGuard {
    fn remove(key: &'static str) -> Self {
        let value = std::env::var_os(key);
        std::env::remove_var(key);
        Self { key, value }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.value.as_ref() {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

fn write_minimal_config(root: &std::path::Path) {
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": {
    "keyName": "MCPace"
  },
  "profiles": {
    "runtime": {
      "default": "safe",
      "profiles": {
        "safe": { "description": "Safe", "serverOverrides": {} }
      }
    }
  },
  "servers": {}
}"#,
    )
    .unwrap();
}

fn temp_root() -> PathBuf {
    let unique = format!(
        "mcpace-dashboard-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let path = std::env::temp_dir().join(unique);
    fs::create_dir_all(&path).unwrap();
    path
}

fn bind_loopback_test_listener() -> TcpListener {
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

fn write_fake_upstream_config(root: &std::path::Path) {
    let script = root.join("fake-upstream.js");
    fs::write(
            &script,
            r#"
const readline = require('readline');
const rl = readline.createInterface({ input: process.stdin });

function send(id, result) {
  process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id, result }) + '\n');
}

rl.on('line', (line) => {
  const message = JSON.parse(line);
  if (message.method === 'initialize') {
    send(message.id, { protocolVersion: '2025-11-25', capabilities: { tools: {} }, serverInfo: { name: 'fake', version: '0.1.0' } });
  } else if (message.method === 'tools/call') {
    send(message.id, { content: [{ type: 'text', text: 'ok' }], isError: false });
  } else if (message.method === 'tools/list') {
    send(message.id, { tools: [{ name: 'echo', inputSchema: { type: 'object' } }] });
  }
});
"#,
        )
        .unwrap();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": { "keyName": "MCPace" },
  "profiles": {
    "runtime": {
      "default": "safe",
      "profiles": { "safe": { "description": "Safe", "serverOverrides": {} } }
    }
  },
  "servers": {
    "fake": {
      "kind": "host-stdio",
      "required": true,
      "policy": {
        "scopeClass": "shared-global",
        "concurrencyPolicy": "single-writer",
        "stateBinding": "none",
        "credentialBinding": "none",
        "parallelismLimit": 1,
        "conflictDomain": "fake-shared"
      },
      "installer": {
        "installTarget": "none",
        "installMethod": "none",
        "installPackage": "",
        "verifyCommand": ""
      }
    }
  }
}"#,
    )
    .unwrap();
    fs::write(
        root.join("mcp_settings.json"),
        format!(
            r#"{{
  "mcpServers": {{
    "fake": {{
      "enabled": true,
      "type": "stdio",
      "command": "node",
      "args": ["{}"]
    }}
  }}
}}"#,
            json_escape(&script.display().to_string())
        ),
    )
    .unwrap();
}

fn json_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn write_dangerous_upstream_config(root: &std::path::Path) {
    let script = root.join("dangerous-upstream.js");
    fs::write(
        &script,
        r#"
const readline = require('readline');
const rl = readline.createInterface({ input: process.stdin });
function send(id, result) { process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id, result }) + '\n'); }
rl.on('line', (line) => {
  const message = JSON.parse(line);
  if (message.method === 'initialize') {
    send(message.id, { protocolVersion: '2025-11-25', capabilities: { tools: {} }, serverInfo: { name: 'dangerous', version: '0.1.0' } });
  } else if (message.method === 'tools/list') {
    send(message.id, { tools: [
      { name: 'read_status', description: 'Read status only', annotations: { readOnlyHint: true }, inputSchema: { type: 'object', properties: {}, additionalProperties: false } },
      { name: 'apply_plan', description: 'Delete files under a supplied path', annotations: { destructiveHint: true, readOnlyHint: false, openWorldHint: true }, inputSchema: { type: 'object', properties: { path: { type: 'string' } }, required: ['path'], additionalProperties: false } }
    ] });
  } else if (message.method === 'tools/call') {
    send(message.id, { content: [{ type: 'text', text: 'ok' }], isError: false });
  }
});
"#,
    )
    .unwrap();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "version": "0.3.5",
  "client": { "keyName": "MCPace" },
  "profiles": {
    "runtime": {
      "default": "safe",
      "profiles": { "safe": { "description": "Safe", "serverOverrides": {} } }
    }
  },
  "servers": {}
}"#,
    )
    .unwrap();
    fs::write(
        root.join("mcp_settings.json"),
        format!(
            r#"{{
  "mcpServers": {{
    "dangerous": {{
      "enabled": true,
      "type": "stdio",
      "command": "node",
      "args": ["{}"]
    }}
  }}
}}"#,
            json_escape(&script.display().to_string())
        ),
    )
    .unwrap();
}

fn dashboard_ui_assets() -> String {
    [
        super::DASHBOARD_HTML,
        super::DASHBOARD_JS,
        super::DASHBOARD_MODEL_JS,
        super::DASHBOARD_RENDER_JS,
        super::DASHBOARD_ACTIONS_JS,
        super::DASHBOARD_BOOT_JS,
        super::DASHBOARD_PRODUCT_JS,
        super::DASHBOARD_CSS,
        super::DASHBOARD_PRODUCT_CSS,
    ]
    .join("\n")
}

fn assert_dashboard_assets_lack(parts: &[&str]) {
    let marker = parts.concat();
    let assets = dashboard_ui_assets();
    assert!(
        !assets.contains(&marker),
        "dashboard UI assets still contain removed marker: {}",
        marker
    );
}

#[test]
fn dashboard_ui_keeps_the_routine_path_compact_and_evidence_first() {
    let assets = dashboard_ui_assets();
    assert!(assets.contains("Getting your sources ready"));
    assert!(assets.contains("Open servers"));
    assert!(assets.contains("function serverVerdict"));
    assert!(assets.contains("function serverDecision"));
    assert!(assets.contains("function serverViewModel"));
    assert!(assets.contains("function serverToolEvidence"));
    assert!(assets.contains("function normalizeProbeEvidence"));
    assert!(assets.contains("function serverEvidenceSummary"));
    assert!(assets.contains("server-row-summary"));
    assert!(assets.contains("Test enabled sources"));
    assert!(assets.contains("Not tested"));
    assert!(assets.contains("Tool evidence"));
    assert!(assets.contains("Review setting"));
    assert!(assets.contains("function serverNextActionProfile"));
    assert!(assets.contains("Enable to apply"));
    assert!(assets.contains("Manual worker changes stay in the Routing task"));
    assert!(assets.contains("A recommended setting is available."));
    assert!(assets.contains("allowHidden"));
    assert!(assets.contains("refreshDashboard({ allowHidden: true, reason: \"initial\" })"));
    assert!(assets.contains("const MAX_SERVER_ROWS = 64"));
    assert!(assets.contains("server-autotune"));
    assert!(assets.contains("/api/actions/ping"));
    assert!(assets.contains("/api/resources"));
    assert!(assets.contains("REQUEST_TIMEOUT_MS"));
    assert!(!assets.contains("function serverHumanSummary"));
    assert!(!assets.contains("server-human-card"));
    assert!(!assets.contains("server-status-rail"));
    assert!(!assets.contains("server-action-note"));
    assert!(!assets.contains("Auto</strong> ${tuned ? \"OK\""));
    assert_dashboard_assets_lack(&["SERVER", "_USE", "_GUIDANCE"]);
    assert_dashboard_assets_lack(&["Browser", " QA"]);
    assert_dashboard_assets_lack(&["Sentry", " issues"]);
    assert_dashboard_assets_lack(&["Example MCP", " sandbox"]);
    assert_dashboard_assets_lack(&["mcpace advanced dev lab", " probe"]);
    assert_dashboard_assets_lack(&["cannot safely", " prove"]);
    assert_dashboard_assets_lack(&["before increasing", " workers"]);
    assert_dashboard_assets_lack(&["auto", "-safe"]);
    assert_dashboard_assets_lack(&["Safety", " Automatic"]);
    assert_dashboard_assets_lack(&["Safe", " On"]);
}

fn response_header(response: &str, name: &str) -> Option<String> {
    response.lines().find_map(|line| {
        let (candidate, value) = line.split_once(':')?;
        if candidate.trim().eq_ignore_ascii_case(name) {
            Some(value.trim().to_string())
        } else {
            None
        }
    })
}

fn connect_to_test_listener(addr: SocketAddr) -> TcpStream {
    let started_at = Instant::now();
    let retry_for = Duration::from_secs(10);
    let connect_timeout = Duration::from_millis(250);
    let retry_sleep = Duration::from_millis(25);
    let mut attempts = 0usize;

    loop {
        attempts = attempts.saturating_add(1);
        match TcpStream::connect_timeout(&addr, connect_timeout) {
            Ok(stream) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
                return stream;
            }
            Err(error) if started_at.elapsed() < retry_for => {
                let _ = error;
                thread::sleep(retry_sleep);
            }
            Err(error) => {
                panic!(
                    "failed to connect to dashboard test listener at {} after {} attempts over {:?}: {}",
                    addr,
                    attempts,
                    started_at.elapsed(),
                    error
                );
            }
        }
    }
}

fn test_config(
    root_path: PathBuf,
    max_requests: Option<usize>,
    surface: super::ServeSurface,
) -> super::DashboardConfig {
    super::DashboardConfig {
        root_path,
        max_requests,
        max_connections: crate::resources::default_http_connection_limit(),
        io_timeout: crate::resources::default_http_io_timeout(),
        max_body_bytes: crate::resources::DEFAULT_MAX_HTTP_BODY_BYTES,
        overview_cache_ttl: crate::resources::default_dashboard_overview_cache_ttl(),
        health_cache_ttl: crate::resources::default_dashboard_health_cache_ttl(),
        overview_cache: Mutex::new(OverviewCacheState::default()),
        health_cache: Mutex::new(None),
        request_latencies: Mutex::new(super::RequestLatencyTracker::default()),
        operation_traces: Mutex::new(super::OperationTraceTracker::default()),
        rate_limiter: Mutex::new(super::HttpRateLimiter::default()),
        admission: super::admission::HttpAdmissionController::default(),
        resource_governor: super::GlobalResourceGovernor::default(),
        http_session_store: Mutex::new(super::http_session::McpHttpSessionStore::default()),
        metrics: super::HttpRuntimeMetrics::default(),
        surface,
        upstream_session_pool: super::new_upstream_session_pool(),
        auth_token: None,
    }
}

#[test]
fn http_request_line_uses_one_absolute_deadline_against_trickle_input() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    let writer = thread::spawn(move || {
        let mut stream = TcpStream::connect(addr).unwrap();
        for byte in b"GET /healthz HTTP/1.1\r\n" {
            if stream.write_all(&[*byte]).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(35));
        }
    });
    let (stream, _) = listener.accept().unwrap();
    let mut reader = BufReader::new(stream);
    let started = Instant::now();
    let result = super::read_limited_http_line(
        &mut reader,
        crate::resources::MAX_HTTP_REQUEST_LINE_BYTES,
        "request line",
        started + Duration::from_millis(140),
    );
    assert!(result.is_err(), "trickle input must hit the total deadline");
    assert!(started.elapsed() < Duration::from_millis(500));
    writer.join().unwrap();
}

#[test]
fn unauthorized_request_is_rejected_before_declared_body_is_read() {
    let root = temp_root();
    write_minimal_config(&root);
    let listener = bind_loopback_test_listener();
    let addr = listener.local_addr().unwrap();
    let mut config = test_config(root.clone(), Some(1), super::ServeSurface::UnifiedServe);
    config.auth_token = Some("expected-secret".to_string());
    config.io_timeout = Duration::from_secs(2);
    let handle = thread::spawn(move || {
        let mut stderr = Vec::new();
        serve_listener(listener, config, &mut stderr)
    });

    let started = Instant::now();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: 100000\r\nConnection: close\r\n\r\n",
        addr
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 401 Unauthorized"));
    assert!(started.elapsed() < Duration::from_secs(1));

    assert_eq!(handle.join().unwrap(), 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn overview_json_contains_expected_sections() {
    let root = temp_root();
    write_minimal_config(&root);
    let overview = build_overview_json(&root).expect("build overview");
    let object = overview.as_object().expect("overview object");
    assert!(object.contains_key("doctor"));
    assert!(object.contains_key("hub"));
    assert!(object.contains_key("readiness"));
    assert!(object.contains_key("servers"));
    assert!(object.contains_key("clients"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn overview_parallel_error_drains_all_started_workers() {
    let slow_completed = Arc::new(AtomicBool::new(false));
    let slow_flag = Arc::clone(&slow_completed);
    let handles = vec![
        (
            "fast-failure",
            thread::spawn(|| Err::<JsonValue, String>("boom".to_string())),
        ),
        (
            "slow-failure",
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(75));
                slow_flag.store(true, Ordering::Release);
                Err::<JsonValue, String>("slow boom".to_string())
            }),
        ),
    ];

    let error = join_json_command_handles(handles).expect_err("workers must fail");
    assert!(error.to_string().contains("fast-failure"));
    assert!(
        slow_completed.load(Ordering::Acquire),
        "the slow worker must be joined before the failed generation returns"
    );
}

#[test]
fn overview_runtime_control_uses_cached_tools_list_evidence_for_risk() {
    if which::which("node").is_err() {
        eprintln!("skipping cached evidence risk test because node is not on PATH");
        return;
    }
    let _env_lock = crate::resources::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _env = [
        EnvVarGuard::remove("MCPACE_MCP_SETTINGS"),
        EnvVarGuard::remove("MCPACE_MCP_SETTINGS_DIRS"),
    ];
    let root = temp_root();
    write_dangerous_upstream_config(&root);
    crate::upstream::warm_tool_list_cache(&root, Some(5_000), true).expect("warm tools cache");

    let overview = build_overview_json(&root).expect("build overview");
    assert_eq!(
        json_helpers::value_at_path(&overview, &["cachedToolEvidence", "toolCount"])
            .and_then(JsonValue::as_i64),
        Some(2)
    );
    let items = json_helpers::array_at_path(&overview, &["runtimeControlPlane", "items"])
        .expect("runtime control items");
    let dangerous = items
        .iter()
        .find(|item| json_helpers::string_at_path(item, &["name"]) == Some("dangerous"))
        .expect("dangerous runtime item");
    assert_eq!(
        json_helpers::string_at_path(dangerous, &["evidenceState"]),
        Some("cached-tools")
    );
    assert_eq!(
        json_helpers::string_at_path(dangerous, &["toolRisk", "risk"]),
        Some("destructive")
    );
    assert_eq!(
        json_helpers::bool_at_path(dangerous, &["toolRisk", "approvalRequired"]),
        Some(true)
    );
    assert_eq!(
        json_helpers::string_at_path(dangerous, &["isolation", "mode"]),
        Some("container-required")
    );
    assert_eq!(
        json_helpers::string_at_path(dangerous, &["nextGate"]),
        Some("choose-isolation")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn overview_cache_cold_start_is_single_flight_for_concurrent_callers() {
    let root = temp_root();
    write_minimal_config(&root);
    let config = Arc::new(test_config(
        root.clone(),
        None,
        super::ServeSurface::Dashboard,
    ));
    let barrier = Arc::new(Barrier::new(8));
    let mut handles = Vec::new();
    for _ in 0..8 {
        let config = Arc::clone(&config);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            cached_overview_json(&config, false).expect("concurrent overview")
        }));
    }
    let responses = handles
        .into_iter()
        .map(|handle| handle.join().expect("overview thread"))
        .collect::<Vec<_>>();
    assert_eq!(
        responses
            .iter()
            .filter(|response| {
                json_helpers::bool_at_path(response, &["cache", "hit"]) == Some(false)
            })
            .count(),
        1,
        "exactly one cold caller should build the overview"
    );
    assert_eq!(
        responses
            .iter()
            .filter(|response| {
                json_helpers::bool_at_path(response, &["cache", "hit"]) == Some(true)
            })
            .count(),
        7,
        "all other cold callers should reuse the first immutable snapshot"
    );
    drop(config);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn overview_cache_cold_failure_is_shared_by_concurrent_callers() {
    let root = temp_root();
    write_minimal_config(&root);
    fs::write(root.join("mcpace.config.json"), "{").expect("write invalid config");
    fs::write(root.join(".count-overview-builds"), "1").expect("write count marker");
    let config = Arc::new(test_config(
        root.clone(),
        None,
        super::ServeSurface::Dashboard,
    ));
    reset_overview_build_invocations();
    let barrier = Arc::new(Barrier::new(8));
    let mut handles = Vec::new();
    for _ in 0..8 {
        let config = Arc::clone(&config);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            cached_overview_json(&config, false).expect_err("invalid overview must fail")
        }));
    }
    let errors = handles
        .into_iter()
        .map(|handle| handle.join().expect("overview failure thread"))
        .collect::<Vec<_>>();
    assert!(errors.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(
        overview_build_invocations(),
        1,
        "one failing cold generation must be shared by all waiting callers"
    );
    drop(config);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn overview_cache_reuses_recent_payload_and_allows_refresh_bypass() {
    let root = temp_root();
    write_minimal_config(&root);
    let config = test_config(root.clone(), None, super::ServeSurface::Dashboard);

    let first = cached_overview_json(&config, false).expect("first overview");
    assert_eq!(
        json_helpers::bool_at_path(&first, &["cache", "hit"]),
        Some(false)
    );
    assert_eq!(
        json_helpers::bool_at_path(&first, &["cache", "bypassed"]),
        Some(false)
    );

    let second = cached_overview_json(&config, false).expect("cached overview");
    assert_eq!(
        json_helpers::bool_at_path(&second, &["cache", "hit"]),
        Some(true)
    );
    assert_eq!(
        json_helpers::value_at_path(&second, &["cache", "ttlMs"]).and_then(JsonValue::as_i64),
        Some(crate::resources::DEFAULT_DASHBOARD_OVERVIEW_CACHE_MS as i64)
    );

    {
        let mut cache = config
            .overview_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache
            .entry
            .as_mut()
            .expect("cached overview entry")
            .stored_at = Instant::now() - config.overview_cache_ttl - Duration::from_millis(1);
        cache.refreshing = true;
    }
    let stale = cached_overview_json(&config, false).expect("stale overview during refresh");
    assert_eq!(
        json_helpers::bool_at_path(&stale, &["cache", "hit"]),
        Some(true)
    );
    assert_eq!(
        json_helpers::bool_at_path(&stale, &["cache", "stale"]),
        Some(true)
    );
    config
        .overview_cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .refreshing = false;

    let refresh = cached_overview_json(&config, true).expect("refresh overview");
    assert_eq!(
        json_helpers::bool_at_path(&refresh, &["cache", "hit"]),
        Some(false)
    );
    assert_eq!(
        json_helpers::bool_at_path(&refresh, &["cache", "bypassed"]),
        Some(true)
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn health_cache_reuses_recent_payload_and_exposes_runtime_status() {
    let root = temp_root();
    write_minimal_config(&root);
    let config = test_config(root.clone(), None, super::ServeSurface::UnifiedServe);

    let first = cached_health_json(&config, false).expect("first health");
    assert_eq!(
        json_helpers::bool_at_path(&first, &["cache", "hit"]),
        Some(false)
    );
    assert_eq!(
        json_helpers::bool_at_path(&first, &["cache", "stale"]),
        Some(false)
    );
    assert_eq!(
        json_helpers::value_at_path(&first, &["runtime", "caches", "healthTtlMs"])
            .and_then(JsonValue::as_i64),
        Some(crate::resources::DEFAULT_DASHBOARD_HEALTH_CACHE_MS as i64)
    );
    assert_eq!(
        json_helpers::value_at_path(&first, &["runtime", "http", "maxConnections"])
            .and_then(JsonValue::as_i64),
        Some(crate::resources::default_http_connection_limit() as i64)
    );

    let second = cached_health_json(&config, false).expect("cached health");
    assert_eq!(
        json_helpers::bool_at_path(&second, &["cache", "hit"]),
        Some(true)
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn health_cache_returns_stale_snapshot_when_refresh_fails() {
    let root = temp_root();
    write_minimal_config(&root);
    let config = test_config(root.clone(), None, super::ServeSurface::UnifiedServe);

    let first = cached_health_json(&config, false).expect("first health");
    assert_eq!(
        json_helpers::bool_at_path(&first, &["cache", "stale"]),
        Some(false)
    );

    fs::remove_file(root.join("mcpace.config.json")).expect("remove config to force failure");
    let stale = cached_health_json(&config, true).expect("stale health fallback");
    assert_eq!(
        json_helpers::bool_at_path(&stale, &["cache", "hit"]),
        Some(true)
    );
    assert_eq!(
        json_helpers::bool_at_path(&stale, &["cache", "bypassed"]),
        Some(true)
    );
    assert_eq!(
        json_helpers::bool_at_path(&stale, &["cache", "stale"]),
        Some(true)
    );
    assert!(json_helpers::string_at_path(&stale, &["cache", "refreshError"]).is_some());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_status_reports_live_connection_metrics() {
    let root = temp_root();
    write_minimal_config(&root);
    let config = test_config(root.clone(), None, super::ServeSurface::Dashboard);

    {
        let _guard = config.metrics.begin();
        let status = runtime_status_json(&config);
        assert_eq!(
            json_helpers::value_at_path(&status, &["http", "activeConnections"])
                .and_then(JsonValue::as_i64),
            Some(1)
        );
        assert_eq!(
            json_helpers::value_at_path(&status, &["http", "acceptedConnections"])
                .and_then(JsonValue::as_i64),
            Some(1)
        );
    }

    let status = runtime_status_json(&config);
    assert_eq!(
        json_helpers::value_at_path(&status, &["http", "activeConnections"])
            .and_then(JsonValue::as_i64),
        Some(0)
    );
    assert_eq!(
        json_helpers::value_at_path(&status, &["http", "completedConnections"])
            .and_then(JsonValue::as_i64),
        Some(1)
    );
    assert!(
        json_helpers::value_at_path(&status, &["http", "requestDurationTotalMs"])
            .and_then(JsonValue::as_i64)
            .unwrap_or(-1)
            >= 0
    );
    assert!(
        json_helpers::value_at_path(&status, &["http", "requestDurationAverageMs"])
            .and_then(JsonValue::as_i64)
            .unwrap_or(-1)
            >= 0
    );
    assert!(
        json_helpers::value_at_path(&status, &["http", "requestDurationMaxMs"])
            .and_then(JsonValue::as_i64)
            .unwrap_or(-1)
            >= 0
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_resources_response_reports_live_limits_and_session_manager() {
    let root = temp_root();
    write_minimal_config(&root);
    let config = test_config(root.clone(), None, super::ServeSurface::Dashboard);

    let response = super::runtime_resources_response(&config);
    assert_eq!(json_helpers::bool_at_path(&response, &["ok"]), Some(true));
    assert_eq!(
        json_helpers::value_at_path(&response, &["runtime", "http", "maxConnections"])
            .and_then(JsonValue::as_i64),
        Some(crate::resources::default_http_connection_limit() as i64)
    );
    assert!(
        json_helpers::value_at_path(
            &response,
            &["runtime", "upstreamSessionPool", "managerCount"]
        )
        .and_then(JsonValue::as_i64)
        .unwrap_or(0)
            >= 1
    );
    assert!(
        json_helpers::value_at_path(&response, &["runtime", "upstreamSessionPool", "maxSize"])
            .and_then(JsonValue::as_i64)
            .unwrap_or(0)
            >= crate::resources::default_upstream_session_pool_limit() as i64
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn query_bool_flag_accepts_common_truthy_refresh_values() {
    assert!(query_bool_flag("refresh=1", "refresh"));
    assert!(query_bool_flag("tail=20&noCache=true", "noCache"));
    assert!(query_bool_flag("refresh", "refresh"));
    assert!(!query_bool_flag("refresh=0", "refresh"));
    assert!(!query_bool_flag("other=true", "refresh"));
}

#[test]
fn bounded_query_usize_clamps_invalid_or_large_values() {
    assert_eq!(super::bounded_query_usize("tail=40", "tail", 20, 500), 40);
    assert_eq!(super::bounded_query_usize("tail=0", "tail", 20, 500), 20);
    assert_eq!(super::bounded_query_usize("tail=-1", "tail", 20, 500), 20);
    assert_eq!(
        super::bounded_query_usize("tail=999999", "tail", 20, 500),
        500
    );
    assert_eq!(super::bounded_query_usize("other=1", "tail", 20, 500), 20);
}

#[test]
fn action_body_requires_json_object_with_json_content_type() {
    let json_request = super::HttpRequest {
        method: "POST".to_string(),
        path: "/api/actions/server-test".to_string(),
        query: String::new(),
        headers: vec![(
            "content-type".to_string(),
            "application/json; charset=utf-8".to_string(),
        )],
        body: br#"{"server":"fake"}"#.to_vec(),
    };
    assert!(matches!(
        super::parse_action_body(&json_request),
        Ok(JsonValue::Object(_))
    ));

    let text_request = super::HttpRequest {
        headers: vec![("content-type".to_string(), "text/plain".to_string())],
        ..json_request
    };
    assert!(super::parse_action_body(&text_request)
        .expect_err("text/plain action body should be rejected")
        .contains("Content-Type: application/json"));

    let array_request = super::HttpRequest {
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: br#"[]"#.to_vec(),
        ..text_request
    };
    assert!(super::parse_action_body(&array_request)
        .expect_err("non-object JSON action body should be rejected")
        .contains("JSON object"));
}

#[test]
fn bearer_authorization_is_case_insensitive_and_strict() {
    let root = temp_root();
    write_minimal_config(&root);
    let mut config = test_config(root.clone(), None, super::ServeSurface::Dashboard);
    config.auth_token = Some("secret-token".to_string());

    let request = super::HttpRequest {
        method: "GET".to_string(),
        path: "/api/resources".to_string(),
        query: String::new(),
        headers: vec![(
            "authorization".to_string(),
            "bearer secret-token".to_string(),
        )],
        body: Vec::new(),
    };
    assert!(super::is_authorized_http_request(&request, &config));

    let extra = super::HttpRequest {
        headers: vec![(
            "authorization".to_string(),
            "Bearer secret-token extra".to_string(),
        )],
        ..request
    };
    assert!(!super::is_authorized_http_request(&extra, &config));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn http_header_name_validation_rejects_empty_or_whitespace_names() {
    assert!(super::http_boundary::is_valid_http_header_name("Host"));
    assert!(super::http_boundary::is_valid_http_header_name(
        "Mcp-Session-Id"
    ));
    assert!(!super::http_boundary::is_valid_http_header_name(""));
    assert!(!super::http_boundary::is_valid_http_header_name(
        "Bad Header"
    ));
    assert!(!super::http_boundary::is_valid_http_header_name(
        "Bad:Header"
    ));
    assert!(super::http_boundary::is_valid_http_header_value(
        "Bearer token"
    ));
    assert!(!super::http_boundary::is_valid_http_header_value(
        "ok\r\nX-Bad: injected"
    ));
}

#[test]
fn content_type_validation_requires_one_exact_json_media_type() {
    let json_request = super::HttpRequest {
        method: "POST".to_string(),
        path: "/api/actions/ping".to_string(),
        query: String::new(),
        headers: vec![(
            "content-type".to_string(),
            "application/json; charset=utf-8".to_string(),
        )],
        body: b"{}".to_vec(),
    };
    assert!(super::http_boundary::content_type_is(
        &json_request,
        "application/json"
    ));

    let duplicate_request = super::HttpRequest {
        headers: vec![
            ("content-type".to_string(), "application/json".to_string()),
            ("content-type".to_string(), "text/plain".to_string()),
        ],
        ..json_request
    };
    assert!(!super::http_boundary::content_type_is(
        &duplicate_request,
        "application/json"
    ));
}

#[test]
fn http_request_line_validation_is_strictly_http1() {
    assert!(super::is_supported_http_version("HTTP/1.1"));
    assert!(super::is_supported_http_version("HTTP/1.0"));
    assert!(!super::is_supported_http_version("HTTP/2"));
    assert!(!super::is_supported_http_version("HTTP/3"));
}

#[test]
fn http_upstream_call_attaches_and_releases_runtime_lease() {
    if which::which("node").is_err() {
        eprintln!("skipping upstream HTTP lease smoke because node is not on PATH");
        return;
    }
    let root = temp_root();
    write_fake_upstream_config(&root);
    let config = test_config(root.clone(), None, super::ServeSurface::UnifiedServe);
    let result = run_http_tool(
        &config,
        "upstream_call",
        &JsonValue::object([
            ("server", JsonValue::string("fake")),
            ("tool", JsonValue::string("echo")),
            (
                "arguments",
                JsonValue::object::<String, Vec<(String, JsonValue)>>(Vec::new()),
            ),
            ("timeoutMs", JsonValue::number(5_000)),
        ]),
        None,
    )
    .expect("upstream_call");

    assert_eq!(
        json_helpers::bool_at_path(&result, &["upstreamOk"]),
        Some(true)
    );
    assert_eq!(
        json_helpers::bool_at_path(&result, &["leaseAttached"]),
        Some(true)
    );
    assert_eq!(
        json_helpers::bool_at_path(&result, &["leaseReleased"]),
        Some(true)
    );

    let leases =
        run_json_command(&root, &["hub", "lease", "list", "--json"]).expect("runtime_leases");
    assert_eq!(
        json_helpers::value_at_path(&leases, &["activeLeaseCount"]).and_then(JsonValue::as_i64),
        Some(0)
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn http_upstream_lease_context_derives_affinity_from_metadata_and_headers() {
    let request = super::HttpRequest {
        method: "POST".to_string(),
        path: "/mcp".to_string(),
        query: String::new(),
        headers: vec![
            ("mcp-session-id".to_string(), "header-session".to_string()),
            (
                "x-mcpace-client-id".to_string(),
                "header-client".to_string(),
            ),
        ],
        body: Vec::new(),
    };

    let header_context = super::http_upstream_lease_context(&super::empty_object(), Some(&request))
        .expect("header context");
    assert_eq!(header_context.client_id.as_deref(), Some("header-client"));
    assert_eq!(header_context.session_id.as_deref(), Some("header-session"));
    assert_eq!(header_context.transport.as_deref(), Some("streamable-http"));
    assert!(header_context.allow_arguments.is_empty());
    assert!(header_context.allowed_tool_risk_classes.is_empty());

    let metadata_context = super::http_upstream_lease_context(
        &JsonValue::object([(
            "metadata",
            JsonValue::object([
                (
                    "session",
                    JsonValue::object([("id", JsonValue::string("metadata-session"))]),
                ),
                ("clientId", JsonValue::string("metadata-client")),
                ("projectRoot", JsonValue::string("C:/metadata-project")),
                ("transport", JsonValue::string("metadata-transport")),
            ]),
        )]),
        Some(&request),
    )
    .expect("metadata context");
    assert_eq!(
        metadata_context.client_id.as_deref(),
        Some("metadata-client")
    );
    assert_eq!(
        metadata_context.session_id.as_deref(),
        Some("metadata-session")
    );
    assert_eq!(
        metadata_context.project_root.as_deref(),
        Some("C:/metadata-project")
    );
    assert_eq!(
        metadata_context.transport.as_deref(),
        Some("metadata-transport")
    );

    let explicit_context = super::http_upstream_lease_context(
        &JsonValue::object([
            ("clientId", JsonValue::string("explicit-client")),
            ("sessionId", JsonValue::string("explicit-session")),
            (
                "allowToolRiskClasses",
                JsonValue::array([JsonValue::string("custom-risk")]),
            ),
            (
                "allowArguments",
                JsonValue::array([JsonValue::string("allowCustomRisk")]),
            ),
        ]),
        Some(&request),
    )
    .expect("explicit context");
    assert_eq!(
        explicit_context.client_id.as_deref(),
        Some("explicit-client")
    );
    assert_eq!(
        explicit_context.session_id.as_deref(),
        Some("explicit-session")
    );
    assert!(explicit_context
        .allowed_tool_risk_classes
        .contains("custom-risk"));
    assert!(explicit_context.allow_arguments.contains("allowCustomRisk"));
}

#[test]
fn dashboard_cli_rejects_resource_limits_above_hard_caps() {
    let cases = [
        (
            "--max-connections",
            crate::resources::HTTP_CONNECTION_LIMIT_MAX
                .saturating_add(1)
                .to_string(),
        ),
        (
            "--io-timeout-ms",
            crate::resources::HTTP_IO_TIMEOUT_MS_MAX
                .saturating_add(1)
                .to_string(),
        ),
        (
            "--max-body-bytes",
            crate::resources::HTTP_BODY_BYTES_MAX
                .saturating_add(1)
                .to_string(),
        ),
    ];
    for (flag, value) in cases {
        let parsed = super::parse_cli(&[flag.to_string(), value]);
        assert!(parsed.error.is_some(), "{flag} should enforce its hard cap");
    }
}

#[test]
fn dashboard_action_paths_reject_network_device_and_traversal_forms() {
    for value in [
        "mcp_settings.json",
        "/home/user/.config/mcp.json",
        r"C:\Users\user\mcp.json",
    ] {
        assert!(
            super::validate_action_path_field("sourcePath", value).is_ok(),
            "local path should be accepted: {value}"
        );
    }
    for value in [
        "../secret.json",
        "config/../secret.json",
        r"\\server\share\config.json",
        "//server/share/config.json",
        r"\\?\C:\secret.json",
        r"C:relative.json",
        r"C:\temp\payload:stream",
        r"C:\temp\CON.txt",
        "https://example.test/config.json",
        "file://localhost/config.json",
    ] {
        assert!(
            super::validate_action_path_field("sourcePath", value).is_err(),
            "unsafe path should be rejected: {value}"
        );
    }
}

#[test]
fn origin_validation_allows_only_exact_loopback_hosts() {
    for origin in [
        "http://127.0.0.1",
        "http://127.0.0.1:39022",
        "http://127.42.0.1:39022",
        "https://127.0.0.1:39022",
        "http://localhost",
        "http://localhost:39022",
        "https://LOCALHOST:39022",
        "http://[::1]",
        "http://[::1]:39022",
    ] {
        assert!(
            is_allowed_local_origin(origin),
            "origin should be allowed: {origin}"
        );
    }

    for origin in [
        "",
        "null",
        "file://local",
        "http://127.0.0.1.evil.example",
        "http://localhost.evil.example",
        "http://127.0.0.1@evil.example",
        "http://evil.example/127.0.0.1",
        "http://[::1].evil.example",
        "http://[::1]:not-a-port",
        "http://127.0.0.1:65536",
    ] {
        assert!(
            !is_allowed_local_origin(origin),
            "origin should be rejected: {origin}"
        );
    }
}

#[test]
fn host_validation_allows_only_exact_loopback_authorities() {
    for host in [
        "127.0.0.1",
        "127.0.0.1:39022",
        "127.42.0.1:39022",
        "localhost",
        "LOCALHOST:39022",
        "[::1]",
        "[::1]:39022",
    ] {
        assert!(
            is_allowed_local_host(host),
            "host should be allowed: {host}"
        );
    }

    for host in [
        "",
        "0.0.0.0",
        "::",
        "192.168.1.10",
        "127.0.0.1.evil.example",
        "localhost.evil.example",
        "127.0.0.1@evil.example",
        "evil.example/127.0.0.1",
        "[::1].evil.example",
        "[::1]:not-a-port",
        "127.0.0.1:65536",
    ] {
        assert!(
            !is_allowed_local_host(host),
            "host should be rejected: {host}"
        );
    }
}

fn origin_policy_request(host: &str, origin: Option<&str>) -> super::HttpRequest {
    let mut headers = vec![("host".to_string(), host.to_string())];
    if let Some(origin) = origin {
        headers.push(("origin".to_string(), origin.to_string()));
    }
    super::HttpRequest {
        method: "GET".to_string(),
        path: "/healthz".to_string(),
        query: String::new(),
        headers,
        body: Vec::new(),
    }
}

#[test]
fn origin_policy_requires_same_loopback_authority() {
    let local = origin_policy_request("127.0.0.1:39022", Some("http://127.0.0.1:39022"));
    assert!(super::http_boundary::validate_origin_for_bind(&local).is_ok());

    let other_loopback = origin_policy_request("127.0.0.1:39022", Some("http://localhost:39022"));
    assert!(super::http_boundary::validate_origin_for_bind(&other_loopback).is_err());

    let nonlocal = origin_policy_request("192.168.1.10:39022", Some("http://192.168.1.10:39022"));
    assert!(super::http_boundary::validate_origin_for_bind(&nonlocal).is_err());

    let cross_origin =
        origin_policy_request("127.0.0.1:39022", Some("http://attacker.example:39022"));
    assert!(super::http_boundary::validate_origin_for_bind(&cross_origin).is_err());

    let spoofed_loopback = origin_policy_request("localhost.evil.example", None);
    assert!(super::http_boundary::validate_origin_for_bind(&spoofed_loopback).is_err());
}

#[test]
fn serve_refuses_nonlocal_bind_without_explicit_opt_in() {
    let root = temp_root();
    write_minimal_config(&root);

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = super::run_serve(
        &[
            "--host".to_string(),
            "0.0.0.0".to_string(),
            "--port".to_string(),
            "0".to_string(),
        ],
        Some(root.clone()),
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(exit_code, 2);
    assert!(
        stdout.is_empty(),
        "nonlocal bind refusal should not start a server: {}",
        String::from_utf8_lossy(&stdout)
    );
    let stderr_text = String::from_utf8_lossy(&stderr);
    assert!(
        stderr_text.contains("refusing to bind non-loopback host '0.0.0.0'"),
        "stderr: {}",
        stderr_text
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn serve_refuses_deprecated_nonlocal_opt_in_flags() {
    let root = temp_root();
    write_minimal_config(&root);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = super::run_serve(
        &[
            "--host".to_string(),
            "0.0.0.0".to_string(),
            "--allow-nonlocal-bind".to_string(),
        ],
        Some(root.clone()),
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(exit_code, 2);
    assert!(stdout.is_empty());
    assert!(String::from_utf8_lossy(&stderr)
        .contains("direct non-loopback HTTP flags are no longer supported"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn loopback_bind_host_policy_accepts_only_loopback_names_or_ips() {
    for host in [
        "localhost",
        "LOCALHOST",
        "127.0.0.1",
        "127.42.0.1",
        "::1",
        "[::1]",
    ] {
        assert!(
            super::is_loopback_bind_host(host),
            "host should be loopback: {host}"
        );
    }
    for host in [
        "",
        "0.0.0.0",
        "::",
        "192.168.1.10",
        "localhost.evil.example",
        "127.0.0.1.evil.example",
    ] {
        assert!(
            !super::is_loopback_bind_host(host),
            "host should be rejected: {host}"
        );
    }
}

#[test]
fn dashboard_serves_root_and_overview() {
    let _local_server_guard = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = temp_root();
    write_minimal_config(&root);

    let listener = bind_loopback_test_listener();
    let addr = listener.local_addr().unwrap();
    let server_root = root.clone();
    let handle = thread::spawn(move || {
        let mut stderr = Vec::new();
        serve_listener(
            listener,
            test_config(server_root, Some(5), super::ServeSurface::Dashboard),
            &mut stderr,
        )
    });

    let mut root_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "GET / HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        addr
    )
    .unwrap();
    stream.read_to_string(&mut root_response).unwrap();
    assert!(root_response.contains("MCPace dashboard"));
    assert!(root_response.contains("/api/overview"));
    assert!(root_response.contains("X-Content-Type-Options: nosniff"));
    assert!(root_response.contains("Content-Security-Policy:"));

    let mut favicon_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "GET /favicon.ico HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        addr
    )
    .unwrap();
    stream.read_to_string(&mut favicon_response).unwrap();
    assert!(favicon_response.contains("HTTP/1.1 200 OK"));
    assert!(favicon_response.contains("image/svg+xml"));
    assert!(favicon_response.contains("<svg"));

    let mut api_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "GET /api/overview HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        addr
    )
    .unwrap();
    stream.read_to_string(&mut api_response).unwrap();
    assert!(api_response.contains("\"doctor\""));
    assert!(api_response.contains("\"servers\""));

    let mut resources_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "GET /api/resources HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        addr
    )
    .unwrap();
    stream.read_to_string(&mut resources_response).unwrap();
    assert!(resources_response.contains("\"upstreamSessionPool\""));
    assert!(resources_response.contains("\"activeConnections\""));

    let mut ping_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "POST /api/actions/ping HTTP/1.1\r\nHost: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        addr
    )
    .unwrap();
    stream.read_to_string(&mut ping_response).unwrap();
    assert!(ping_response.contains("HTTP/1.1 200 OK"));
    assert!(ping_response.contains("\"action\": \"ping\""));
    assert!(ping_response.contains("\"status\": \"ok\""));

    assert_eq!(handle.join().unwrap(), 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn dashboard_rejects_http_payloads_above_limit_without_reading_body() {
    let _local_server_guard = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = temp_root();
    write_minimal_config(&root);

    let listener = bind_loopback_test_listener();
    let addr = listener.local_addr().unwrap();
    let server_root = root.clone();
    let handle = thread::spawn(move || {
        let mut stderr = Vec::new();
        let mut config = test_config(server_root, Some(1), super::ServeSurface::UnifiedServe);
        config.max_body_bytes = 8;
        config.max_connections = 1;
        serve_listener(listener, config, &mut stderr)
    });

    let mut response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: 128\r\nConnection: close\r\n\r\n",
            addr
        )
        .unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    stream.read_to_string(&mut response).unwrap();
    assert!(
        response.starts_with("HTTP/1.1 413 Payload Too Large"),
        "oversized response: {}",
        response
    );

    assert_eq!(handle.join().unwrap(), 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn unified_serve_exposes_health_and_mcp_routes() {
    let _local_server_guard = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = temp_root();
    write_minimal_config(&root);

    let listener = bind_loopback_test_listener();
    let addr = listener.local_addr().unwrap();
    let server_root = root.clone();
    let handle = thread::spawn(move || {
        let mut stderr = Vec::new();
        serve_listener(
            listener,
            test_config(server_root, Some(11), super::ServeSurface::UnifiedServe),
            &mut stderr,
        )
    });

    let health_request = format!(
        "GET /healthz HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        addr
    );
    let health_response = crate::http_probe::raw_response(
        "127.0.0.1",
        addr.port(),
        &health_request,
        Duration::from_secs(5),
        256 * 1024,
    )
    .expect("shared HTTP probe should accept the unified health response");
    assert!(health_response.contains("\"ok\""));
    assert!(health_response.contains("\"readiness\""));

    let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"serve-test","version":"0.1.0"}}}"#;
    let mut mcp_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            addr,
            initialize.len(),
            initialize
        )
        .unwrap();
    stream.read_to_string(&mut mcp_response).unwrap();
    assert!(mcp_response.contains("\"protocolVersion\": \"2025-11-25\""));
    assert!(mcp_response.contains("\"serverInfo\""));
    let session_id = response_header(&mcp_response, "Mcp-Session-Id")
        .expect("initialize should return Mcp-Session-Id");

    let initialized = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    let mut initialized_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMcp-Session-Id: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            addr,
            session_id,
            initialized.len(),
            initialized
        )
        .unwrap();
    stream.read_to_string(&mut initialized_response).unwrap();
    assert!(
        initialized_response.starts_with("HTTP/1.1 202 Accepted"),
        "initialized response: {}",
        initialized_response
    );
    assert!(
        initialized_response.contains("Content-Length: 0"),
        "initialized response: {}",
        initialized_response
    );

    let mut sse_get_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "GET /mcp HTTP/1.1\r\nHost: {}\r\nAccept: text/event-stream\r\nConnection: close\r\n\r\n",
        addr
    )
    .unwrap();
    stream.read_to_string(&mut sse_get_response).unwrap();
    assert!(
        sse_get_response.starts_with("HTTP/1.1 405 Method Not Allowed"),
        "sse GET response: {}",
        sse_get_response
    );
    assert!(
        sse_get_response.contains("Allow: POST, DELETE"),
        "sse GET response should advertise the supported MCP methods: {}",
        sse_get_response
    );

    for accept_header in ["", "Accept: application/json\r\n"] {
        let mut get_response = String::new();
        let mut stream = connect_to_test_listener(addr);
        write!(
            stream,
            "GET /mcp HTTP/1.1\r\nHost: {}\r\n{}Connection: close\r\n\r\n",
            addr, accept_header
        )
        .unwrap();
        stream.read_to_string(&mut get_response).unwrap();
        assert!(
            get_response.starts_with("HTTP/1.1 405 Method Not Allowed"),
            "all MCP GET variants must reject until SSE listening is implemented: {}",
            get_response
        );
        assert!(get_response.contains("Allow: POST, DELETE"));
    }

    let tools_list = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
    let mut tools_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMcp-Session-Id: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            addr,
            session_id,
            tools_list.len(),
            tools_list
        )
        .unwrap();
    stream.read_to_string(&mut tools_response).unwrap();
    assert!(tools_response.contains("\"adapter_profile\""));
    assert!(tools_response.contains("\"adapter_route\""));
    assert!(tools_response.contains("\"upstream_search\""));
    assert!(tools_response.contains("\"surface_manifest\""));
    assert!(tools_response.contains("\"upstream_tools\""));
    assert!(tools_response.contains("\"upstream_catalog\""));
    assert!(tools_response.contains("\"upstream_call\""));
    assert!(tools_response.contains("\"upstream_batch\""));
    assert!(
        !tools_response.contains("\"doctor\""),
        "default adapter surface should keep diagnostic helpers callable but unlisted: {}",
        tools_response
    );

    let unsupported_call = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"unsupported_tool","arguments":{}}}"#;
    let mut unsupported_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMcp-Session-Id: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            addr,
            session_id,
            unsupported_call.len(),
            unsupported_call
        )
        .unwrap();
    stream.read_to_string(&mut unsupported_response).unwrap();
    assert!(
        unsupported_response.contains("\"error\""),
        "unsupported response: {}",
        unsupported_response
    );
    assert!(
        unsupported_response.contains("Unknown tool: unsupported_tool"),
        "unsupported response: {}",
        unsupported_response
    );

    let diagnostics_call = r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"runtime_diagnostics","arguments":{}}}"#;
    let mut diagnostics_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMcp-Session-Id: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            addr,
            session_id,
            diagnostics_call.len(),
            diagnostics_call
        )
        .unwrap();
    stream.read_to_string(&mut diagnostics_response).unwrap();
    assert!(
        diagnostics_response.contains("\"upstreamForwarding\""),
        "diagnostics response: {}",
        diagnostics_response
    );
    assert!(
        diagnostics_response.contains("\"surfaceContract\""),
        "diagnostics response: {}",
        diagnostics_response
    );
    assert!(
        diagnostics_response.contains("\"implemented\": true"),
        "diagnostics response: {}",
        diagnostics_response
    );

    let malformed_body = "{ definitely-not-json";
    let mut malformed_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            addr,
            malformed_body.len(),
            malformed_body
        )
        .unwrap();
    stream.read_to_string(&mut malformed_response).unwrap();
    assert!(
        malformed_response.starts_with("HTTP/1.1 400 Bad Request"),
        "malformed response: {}",
        malformed_response
    );
    assert!(
        malformed_response.contains("\"code\": -32700"),
        "malformed response: {}",
        malformed_response
    );
    assert!(
        malformed_response.contains("invalid JSON-RPC body"),
        "malformed response: {}",
        malformed_response
    );

    let missing_method = r#"{"jsonrpc":"2.0","id":9}"#;
    let mut invalid_request_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        missing_method.len(),
        missing_method
    )
    .unwrap();
    stream
        .read_to_string(&mut invalid_request_response)
        .unwrap();
    assert!(invalid_request_response.starts_with("HTTP/1.1 400 Bad Request"));
    assert!(invalid_request_response.contains("\"code\": -32600"));
    assert!(invalid_request_response.contains("missing JSON-RPC method"));
    assert!(!invalid_request_response.contains("\"code\": -32700"));

    assert_eq!(handle.join().unwrap(), 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn unified_serve_generates_session_id_instead_of_trusting_initialize_header() {
    let _local_server_guard = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = temp_root();
    write_minimal_config(&root);

    let listener = bind_loopback_test_listener();
    let addr = listener.local_addr().unwrap();
    let server_root = root.clone();
    let handle = thread::spawn(move || {
        let mut stderr = Vec::new();
        serve_listener(
            listener,
            test_config(server_root, Some(4), super::ServeSurface::UnifiedServe),
            &mut stderr,
        )
    });

    let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"fixation-test","version":"0.1.0"}}}"#;
    let mut initialize_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMcp-Session-Id: attacker-fixed-session\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        initialize.len(),
        initialize
    )
    .unwrap();
    stream.read_to_string(&mut initialize_response).unwrap();
    let generated_session_id = response_header(&initialize_response, "Mcp-Session-Id")
        .expect("initialize should return a session id");
    assert_ne!(
        generated_session_id, "attacker-fixed-session",
        "server must not trust a client-supplied session id during initialize"
    );

    let tools_list = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
    let mut fixed_session_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMcp-Session-Id: attacker-fixed-session\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        tools_list.len(),
        tools_list
    )
    .unwrap();
    stream.read_to_string(&mut fixed_session_response).unwrap();
    assert!(
        fixed_session_response.starts_with("HTTP/1.1 404 Not Found"),
        "client-supplied session id should not be registered: {}",
        fixed_session_response
    );

    let initialized = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    let mut initialized_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMcp-Session-Id: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        generated_session_id,
        initialized.len(),
        initialized
    )
    .unwrap();
    stream.read_to_string(&mut initialized_response).unwrap();
    assert!(
        initialized_response.starts_with("HTTP/1.1 202 Accepted"),
        "initialized response: {}",
        initialized_response
    );

    let mut generated_session_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMcp-Session-Id: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        generated_session_id,
        tools_list.len(),
        tools_list
    )
    .unwrap();
    stream
        .read_to_string(&mut generated_session_response)
        .unwrap();
    assert!(
        generated_session_response.starts_with("HTTP/1.1 200 OK"),
        "server-generated session id should remain valid: {}",
        generated_session_response
    );

    assert_eq!(handle.join().unwrap(), 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn unified_serve_enforces_mcp_http_session_lifecycle() {
    let _local_server_guard = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = temp_root();
    write_minimal_config(&root);

    let listener = bind_loopback_test_listener();
    let addr = listener.local_addr().unwrap();
    let server_root = root.clone();
    let handle = thread::spawn(move || {
        let mut stderr = Vec::new();
        serve_listener(
            listener,
            test_config(server_root, Some(10), super::ServeSurface::UnifiedServe),
            &mut stderr,
        )
    });

    let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"session-test","version":"0.1.0"}}}"#;
    let mut initialize_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        initialize.len(),
        initialize
    )
    .unwrap();
    stream.read_to_string(&mut initialize_response).unwrap();
    assert!(
        initialize_response.starts_with("HTTP/1.1 200 OK"),
        "initialize response: {}",
        initialize_response
    );
    let session_id = response_header(&initialize_response, "Mcp-Session-Id")
        .expect("initialize should return a session id");

    let tools_list = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
    let mut missing_session_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        tools_list.len(),
        tools_list
    )
    .unwrap();
    stream
        .read_to_string(&mut missing_session_response)
        .unwrap();
    assert!(
        missing_session_response.starts_with("HTTP/1.1 400 Bad Request"),
        "missing session response: {}",
        missing_session_response
    );
    assert!(
        missing_session_response.contains("Mcp-Session-Id"),
        "missing session response: {}",
        missing_session_response
    );

    let mut unknown_session_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMcp-Session-Id: mcpace-unknown-session\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        tools_list.len(),
        tools_list
    )
    .unwrap();
    stream
        .read_to_string(&mut unknown_session_response)
        .unwrap();
    assert!(
        unknown_session_response.starts_with("HTTP/1.1 404 Not Found"),
        "unknown session response: {}",
        unknown_session_response
    );
    assert!(
        unknown_session_response.contains("unknown MCP HTTP session"),
        "unknown session response: {}",
        unknown_session_response
    );

    let mut protocol_mismatch_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "POST /mcp HTTP/1.1
Host: {}
Accept: application/json, text/event-stream
Content-Type: application/json
Mcp-Session-Id: {}
MCP-Protocol-Version: 2025-06-18
Content-Length: {}
Connection: close

{}",
        addr,
        session_id,
        tools_list.len(),
        tools_list
    )
    .unwrap();
    stream
        .read_to_string(&mut protocol_mismatch_response)
        .unwrap();
    assert!(
        protocol_mismatch_response.starts_with("HTTP/1.1 400 Bad Request"),
        "protocol mismatch response: {}",
        protocol_mismatch_response
    );
    assert!(
        protocol_mismatch_response.contains("does not match initialized session protocol"),
        "protocol mismatch response: {}",
        protocol_mismatch_response
    );

    let pre_initialized_tools_list = r#"{"jsonrpc":"2.0","id":3,"method":"tools/list"}"#;
    let mut pre_initialized_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMcp-Session-Id: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        session_id,
        pre_initialized_tools_list.len(),
        pre_initialized_tools_list
    )
    .unwrap();
    stream
        .read_to_string(&mut pre_initialized_response)
        .unwrap();
    assert!(
        pre_initialized_response.starts_with("HTTP/1.1 400 Bad Request"),
        "pre-initialized response: {}",
        pre_initialized_response
    );
    assert!(
        pre_initialized_response.contains("notifications/initialized"),
        "pre-initialized response: {}",
        pre_initialized_response
    );

    let initialized = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    let mut initialized_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMcp-Session-Id: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        session_id,
        initialized.len(),
        initialized
    )
    .unwrap();
    stream.read_to_string(&mut initialized_response).unwrap();
    assert!(
        initialized_response.starts_with("HTTP/1.1 202 Accepted"),
        "initialized response: {}",
        initialized_response
    );

    let duplicate_pre_initialized_id = r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#;
    let mut duplicate_id_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "POST /mcp HTTP/1.1
Host: {}
Accept: application/json, text/event-stream
Content-Type: application/json
Mcp-Session-Id: {}
Content-Length: {}
Connection: close

{}",
        addr,
        session_id,
        duplicate_pre_initialized_id.len(),
        duplicate_pre_initialized_id
    )
    .unwrap();
    stream.read_to_string(&mut duplicate_id_response).unwrap();
    assert!(
        duplicate_id_response.starts_with("HTTP/1.1 400 Bad Request"),
        "duplicate id response: {}",
        duplicate_id_response
    );
    assert!(
        duplicate_id_response.contains("already used"),
        "duplicate id response: {}",
        duplicate_id_response
    );

    let tools_list = r#"{"jsonrpc":"2.0","id":4,"method":"tools/list"}"#;
    let mut valid_session_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMcp-Session-Id: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        session_id,
        tools_list.len(),
        tools_list
    )
    .unwrap();
    stream.read_to_string(&mut valid_session_response).unwrap();
    assert!(
        valid_session_response.starts_with("HTTP/1.1 200 OK"),
        "valid session response: {}",
        valid_session_response
    );
    assert!(valid_session_response.contains("adapter_profile"));

    let mut delete_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "DELETE /mcp HTTP/1.1\r\nHost: {}\r\nMcp-Session-Id: {}\r\nConnection: close\r\n\r\n",
        addr, session_id
    )
    .unwrap();
    stream.read_to_string(&mut delete_response).unwrap();
    assert!(
        delete_response.starts_with("HTTP/1.1 202 Accepted"),
        "delete response: {}",
        delete_response
    );

    let mut closed_session_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMcp-Session-Id: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        session_id,
        tools_list.len(),
        tools_list
    )
    .unwrap();
    stream.read_to_string(&mut closed_session_response).unwrap();
    assert!(
        closed_session_response.starts_with("HTTP/1.1 404 Not Found"),
        "closed session response: {}",
        closed_session_response
    );

    assert_eq!(handle.join().unwrap(), 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn unified_serve_rejects_mcp_cross_origin_and_missing_accept() {
    let _local_server_guard = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = temp_root();
    write_minimal_config(&root);

    let listener = bind_loopback_test_listener();
    let addr = listener.local_addr().unwrap();
    let server_root = root.clone();
    let handle = thread::spawn(move || {
        let mut stderr = Vec::new();
        serve_listener(
            listener,
            test_config(server_root, Some(5), super::ServeSurface::UnifiedServe),
            &mut stderr,
        )
    });

    let mut cross_origin_get = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
            stream,
            "GET /mcp HTTP/1.1\r\nHost: {}\r\nOrigin: http://127.0.0.1.evil.example\r\nAccept: text/event-stream\r\nConnection: close\r\n\r\n",
            addr
        )
        .unwrap();
    stream.read_to_string(&mut cross_origin_get).unwrap();
    assert!(
        cross_origin_get.starts_with("HTTP/1.1 403 Forbidden"),
        "cross-origin GET response: {}",
        cross_origin_get
    );

    let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"origin-test","version":"0.1.0"}}}"#;
    let mut cross_origin_post = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nOrigin: http://localhost.evil.example\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            addr,
            initialize.len(),
            initialize
        )
        .unwrap();
    stream.read_to_string(&mut cross_origin_post).unwrap();
    assert!(
        cross_origin_post.starts_with("HTTP/1.1 403 Forbidden"),
        "cross-origin POST response: {}",
        cross_origin_post
    );

    let mut missing_accept_post = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            addr,
            initialize.len(),
            initialize
        )
        .unwrap();
    stream.read_to_string(&mut missing_accept_post).unwrap();
    assert!(
        missing_accept_post.starts_with("HTTP/1.1 400 Bad Request"),
        "missing Accept response: {}",
        missing_accept_post
    );
    assert!(
        missing_accept_post.contains("application/json and text/event-stream"),
        "missing Accept response: {}",
        missing_accept_post
    );

    let mismatched_method = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"upstream_tools","arguments":{}}}"#;
    let mut mismatched_method_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMcp-Method: tools/list\r\nMcp-Name: upstream_tools\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            addr,
            mismatched_method.len(),
            mismatched_method
        )
        .unwrap();
    stream
        .read_to_string(&mut mismatched_method_response)
        .unwrap();
    assert!(
        mismatched_method_response.starts_with("HTTP/1.1 400 Bad Request"),
        "mismatched Mcp-Method response: {}",
        mismatched_method_response
    );
    assert!(
        mismatched_method_response.contains("Mcp-Method header"),
        "mismatched Mcp-Method response: {}",
        mismatched_method_response
    );

    let mismatched_name = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"upstream_tools","arguments":{}}}"#;
    let mut mismatched_name_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMcp-Method: tools/call\r\nMcp-Name: upstream_call\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            addr,
            mismatched_name.len(),
            mismatched_name
        )
        .unwrap();
    stream
        .read_to_string(&mut mismatched_name_response)
        .unwrap();
    assert!(
        mismatched_name_response.starts_with("HTTP/1.1 400 Bad Request"),
        "mismatched Mcp-Name response: {}",
        mismatched_name_response
    );
    assert!(
        mismatched_name_response.contains("Mcp-Name header"),
        "mismatched Mcp-Name response: {}",
        mismatched_name_response
    );

    assert_eq!(handle.join().unwrap(), 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn unified_serve_rejects_spoofed_host_for_non_mcp_routes() {
    let _local_server_guard = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = temp_root();
    write_minimal_config(&root);

    let listener = bind_loopback_test_listener();
    let addr = listener.local_addr().unwrap();
    let server_root = root.clone();
    let handle = thread::spawn(move || {
        let mut stderr = Vec::new();
        serve_listener(
            listener,
            test_config(server_root, Some(2), super::ServeSurface::UnifiedServe),
            &mut stderr,
        )
    });

    let mut health_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "GET /healthz HTTP/1.1\r\nHost: 127.0.0.1.evil.example\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    stream.read_to_string(&mut health_response).unwrap();
    assert!(
        health_response.starts_with("HTTP/1.1 403 Forbidden"),
        "spoofed health Host response: {}",
        health_response
    );

    let mut overview_response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "GET /api/overview HTTP/1.1\r\nHost: localhost.evil.example\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    stream.read_to_string(&mut overview_response).unwrap();
    assert!(
        overview_response.starts_with("HTTP/1.1 403 Forbidden"),
        "spoofed overview Host response: {}",
        overview_response
    );

    assert_eq!(handle.join().unwrap(), 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn dashboard_returns_json_500_for_internal_route_errors() {
    let _local_server_guard = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = temp_root();
    fs::remove_dir_all(&root).unwrap();

    let listener = bind_loopback_test_listener();
    let addr = listener.local_addr().unwrap();
    let server_root = root.clone();
    let handle = thread::spawn(move || {
        let mut stderr = Vec::new();
        serve_listener(
            listener,
            test_config(server_root, Some(1), super::ServeSurface::Dashboard),
            &mut stderr,
        )
    });

    let mut response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "GET /api/overview?refresh=1 HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        addr
    )
    .unwrap();
    stream.read_to_string(&mut response).unwrap();
    assert!(
        response.starts_with("HTTP/1.1 500 Internal Server Error"),
        "internal error response: {}",
        response
    );
    assert!(
        response.contains("\"ok\": false"),
        "internal error response: {}",
        response
    );
    assert!(
        response.contains("\"code\": \"internal_error\""),
        "internal error response: {}",
        response
    );

    assert_eq!(handle.join().unwrap(), 0);
}

#[test]
fn dashboard_actions_reject_cross_origin_posts() {
    let _local_server_guard = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = temp_root();
    write_minimal_config(&root);

    let listener = bind_loopback_test_listener();
    let addr = listener.local_addr().unwrap();
    let server_root = root.clone();
    let handle = thread::spawn(move || {
        let mut stderr = Vec::new();
        serve_listener(
            listener,
            test_config(server_root, Some(1), super::ServeSurface::Dashboard),
            &mut stderr,
        )
    });

    let mut response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
            stream,
            "POST /api/actions/repair HTTP/1.1\r\nHost: {}\r\nOrigin: http://localhost.evil.example\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            addr
        )
        .unwrap();
    stream.read_to_string(&mut response).unwrap();
    assert!(
        response.starts_with("HTTP/1.1 403 Forbidden"),
        "action response: {}",
        response
    );
    assert!(
        response.contains("not allowed for MCPace serve mode"),
        "action response: {}",
        response
    );

    assert_eq!(handle.join().unwrap(), 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn dashboard_server_toggle_action_updates_source_enabled_flag() {
    let _local_server_guard = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = temp_root();
    write_fake_upstream_config(&root);

    let listener = bind_loopback_test_listener();
    let addr = listener.local_addr().unwrap();
    let server_root = root.clone();
    let handle = thread::spawn(move || {
        let mut stderr = Vec::new();
        serve_listener(
            listener,
            test_config(server_root, Some(1), super::ServeSurface::Dashboard),
            &mut stderr,
        )
    });

    let body = r#"{"server":"fake"}"#;
    let mut response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "POST /api/actions/server-disable HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        body.len(),
        body
    )
    .unwrap();
    stream.read_to_string(&mut response).unwrap();
    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "toggle response: {}",
        response
    );
    assert!(response.contains("\"action\": \"server-disable\""));

    let settings = json_helpers::read_json_file(&root.join("mcp_settings.json")).unwrap();
    assert_eq!(
        json_helpers::bool_at_path(&settings, &["mcpServers", "fake", "enabled"]),
        Some(false)
    );

    assert_eq!(handle.join().unwrap(), 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn dashboard_server_remove_action_previews_then_removes_exact_source() {
    let _local_server_guard = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = temp_root();
    write_fake_upstream_config(&root);

    let listener = bind_loopback_test_listener();
    let addr = listener.local_addr().unwrap();
    let server_root = root.clone();
    let handle = thread::spawn(move || {
        let mut stderr = Vec::new();
        serve_listener(
            listener,
            test_config(server_root, Some(2), super::ServeSurface::Dashboard),
            &mut stderr,
        )
    });

    let preview_body = r#"{"server":"fake","settingsPath":"mcp_settings.json","dryRun":true}"#;
    let mut preview_response = String::new();
    let mut preview_stream = connect_to_test_listener(addr);
    write!(
        preview_stream,
        "POST /api/actions/server-remove HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        preview_body.len(),
        preview_body
    )
    .unwrap();
    preview_stream
        .read_to_string(&mut preview_response)
        .unwrap();
    assert!(
        preview_response.starts_with("HTTP/1.1 200 OK"),
        "remove preview response: {}",
        preview_response
    );
    assert!(preview_response.contains("\"action\": \"server-remove\""));
    assert!(preview_response.contains("\"dryRun\": true"));
    let preview_settings = json_helpers::read_json_file(&root.join("mcp_settings.json")).unwrap();
    assert!(
        json_helpers::value_at_path(&preview_settings, &["mcpServers", "fake"]).is_some(),
        "dry-run must not remove the source"
    );

    let remove_body = r#"{"server":"fake","settingsPath":"mcp_settings.json","dryRun":false}"#;
    let mut remove_response = String::new();
    let mut remove_stream = connect_to_test_listener(addr);
    write!(
        remove_stream,
        "POST /api/actions/server-remove HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        remove_body.len(),
        remove_body
    )
    .unwrap();
    remove_stream.read_to_string(&mut remove_response).unwrap();
    assert!(
        remove_response.starts_with("HTTP/1.1 200 OK"),
        "remove response: {}",
        remove_response
    );
    assert!(remove_response.contains("\"action\": \"server-remove\""));
    assert!(remove_response.contains("\"dryRun\": false"));
    let removed_settings = json_helpers::read_json_file(&root.join("mcp_settings.json")).unwrap();
    assert!(
        json_helpers::value_at_path(&removed_settings, &["mcpServers", "fake"]).is_none(),
        "confirmed removal must delete the selected source entry"
    );

    assert_eq!(handle.join().unwrap(), 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn dashboard_server_policy_action_updates_workers_and_mode() {
    let _local_server_guard = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = temp_root();
    write_fake_upstream_config(&root);

    let listener = bind_loopback_test_listener();
    let addr = listener.local_addr().unwrap();
    let server_root = root.clone();
    let handle = thread::spawn(move || {
        let mut stderr = Vec::new();
        serve_listener(
            listener,
            test_config(server_root, Some(1), super::ServeSurface::Dashboard),
            &mut stderr,
        )
    });

    let body = r#"{"server":"fake","mode":"pool","maxWorkers":3,"maxInFlightPerWorker":2}"#;
    let mut response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "POST /api/actions/server-policy HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        body.len(),
        body
    )
    .unwrap();
    stream.read_to_string(&mut response).unwrap();
    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "policy response: {}",
        response
    );
    assert!(response.contains("\"action\": \"server-policy\""));

    let config = json_helpers::read_json_file(&root.join("mcpace.config.json")).unwrap();
    assert_eq!(
        json_helpers::string_at_path(&config, &["servers", "fake", "execution", "mode"]),
        Some("pool")
    );
    assert_eq!(
        json_helpers::value_at_path(&config, &["servers", "fake", "execution", "maxWorkers"])
            .and_then(JsonValue::as_i64),
        Some(3)
    );
    assert_eq!(
        json_helpers::value_at_path(
            &config,
            &["servers", "fake", "execution", "maxInFlightPerWorker"]
        )
        .and_then(JsonValue::as_i64),
        Some(2)
    );

    assert_eq!(handle.join().unwrap(), 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn dashboard_server_test_action_invokes_server_test_with_payload() {
    let _local_server_guard = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = temp_root();
    write_fake_upstream_config(&root);

    let listener = bind_loopback_test_listener();
    let addr = listener.local_addr().unwrap();
    let server_root = root.clone();
    let handle = thread::spawn(move || {
        let mut stderr = Vec::new();
        serve_listener(
            listener,
            test_config(server_root, Some(1), super::ServeSurface::Dashboard),
            &mut stderr,
        )
    });

    let body = r#"{"server":"fake","timeoutMs":5000}"#;
    let mut response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "POST /api/actions/server-test HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        body.len(),
        body
    )
    .unwrap();
    stream.read_to_string(&mut response).unwrap();
    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "server-test response: {}",
        response
    );
    assert!(response.contains("\"action\": \"server-test\""));

    assert_eq!(handle.join().unwrap(), 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn dashboard_server_autotune_action_batches_policy_updates() {
    let _local_server_guard = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = temp_root();
    write_fake_upstream_config(&root);

    let listener = bind_loopback_test_listener();
    let addr = listener.local_addr().unwrap();
    let server_root = root.clone();
    let handle = thread::spawn(move || {
        let mut stderr = Vec::new();
        serve_listener(
            listener,
            test_config(server_root, Some(1), super::ServeSurface::Dashboard),
            &mut stderr,
        )
    });

    let body = r#"{"changes":[{"server":"fake","mode":"serialized","maxWorkers":1,"maxInFlightPerWorker":1}]}"#;
    let mut response = String::new();
    let mut stream = connect_to_test_listener(addr);
    write!(
        stream,
        "POST /api/actions/server-autotune HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        body.len(),
        body
    )
    .unwrap();
    stream.read_to_string(&mut response).unwrap();
    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "autotune response: {}",
        response
    );
    assert!(response.contains("\"action\": \"server-autotune\""));
    assert!(response.contains("\"updated\": 1"));

    let config = json_helpers::read_json_file(&root.join("mcpace.config.json")).unwrap();
    assert_eq!(
        json_helpers::string_at_path(&config, &["servers", "fake", "execution", "mode"]),
        Some("serialized")
    );
    assert_eq!(
        json_helpers::value_at_path(&config, &["servers", "fake", "execution", "maxWorkers"])
            .and_then(JsonValue::as_i64),
        Some(1)
    );

    assert_eq!(handle.join().unwrap(), 0);
    let _ = fs::remove_dir_all(root);
}
