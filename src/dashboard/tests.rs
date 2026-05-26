use super::{
    build_overview_json, cached_health_json, cached_overview_json, is_allowed_local_host,
    is_allowed_local_origin, query_bool_flag, run_http_tool, run_json_command, runtime_status_json,
    serve_listener,
};
use crate::json::JsonValue;
use crate::json_helpers;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;

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

fn assert_dashboard_html_lacks(parts: &[&str]) {
    let marker = parts.concat();
    assert!(
        !super::DASHBOARD_HTML.contains(&marker),
        "dashboard HTML still contains removed marker: {}",
        marker
    );
}

#[test]
fn dashboard_ui_exposes_production_cockpit_without_probe_loop_copy() {
    assert!(super::DASHBOARD_HTML.contains("Production cockpit"));
    assert!(super::DASHBOARD_HTML.contains("backend-check-button"));
    assert!(super::DASHBOARD_HTML.contains("server-fleet-board"));
    assert!(super::DASHBOARD_HTML.contains("function serverVerdict"));
    assert!(super::DASHBOARD_HTML.contains("function serverDecision"));
    assert!(super::DASHBOARD_HTML.contains("function serverViewModel"));
    assert!(super::DASHBOARD_HTML.contains("function serverBucket"));
    assert!(super::DASHBOARD_HTML.contains("function serverSensitivity"));
    assert!(super::DASHBOARD_HTML.contains("reset group filter"));
    assert!(super::DASHBOARD_HTML.contains("Run Test first"));
    assert!(super::DASHBOARD_HTML.contains("server-guide"));
    assert!(super::DASHBOARD_HTML.contains("function serverToolEvidence"));
    assert!(super::DASHBOARD_HTML.contains("function normalizeProbeEvidence"));
    assert!(super::DASHBOARD_HTML.contains("function serverEvidenceSummary"));
    assert_dashboard_html_lacks(&["SERVER", "_USE", "_GUIDANCE"]);
    assert_dashboard_html_lacks(&["function server", "Use", "Guidance"]);
    assert_dashboard_html_lacks(&["Browser", " QA"]);
    assert_dashboard_html_lacks(&["Sentry", " issues"]);
    assert_dashboard_html_lacks(&["Example MCP", " sandbox"]);
    assert!(super::DASHBOARD_HTML.contains("function serverSettingProfile"));
    assert!(super::DASHBOARD_HTML.contains("function serverHumanSummary"));
    assert!(super::DASHBOARD_HTML.contains("server-human-card"));
    assert!(super::DASHBOARD_HTML.contains("Live evidence"));
    assert!(super::DASHBOARD_HTML.contains("Current state"));
    assert!(super::DASHBOARD_HTML.contains("Tools not checked"));
    assert!(super::DASHBOARD_HTML.contains("No live tools evidence"));
    assert!(super::DASHBOARD_HTML.contains("Tool evidence"));
    assert!(super::DASHBOARD_HTML.contains("Best setting"));
    assert!(super::DASHBOARD_HTML.contains("Routing and workers"));
    assert!(super::DASHBOARD_HTML.contains("Policy after on"));
    assert!(super::DASHBOARD_HTML.contains("Manual override: routing and workers"));
    assert!(super::DASHBOARD_HTML.contains("Recommended next step"));
    assert!(super::DASHBOARD_HTML.contains("allowHidden"));
    assert!(super::DASHBOARD_HTML
        .contains("refreshDashboard({ allowHidden: true, reason: \"initial\" })"));
    assert!(super::DASHBOARD_HTML.contains("const MAX_SERVER_ROWS = 64"));
    assert!(super::DASHBOARD_HTML.contains("server-autotune"));
    assert!(!super::DASHBOARD_HTML.contains("Auto</strong> ${tuned ? \"OK\""));
    assert_dashboard_html_lacks(&["mcpace lab", " probe"]);
    assert_dashboard_html_lacks(&["cannot safely", " prove"]);
    assert_dashboard_html_lacks(&["before increasing", " workers"]);
    assert_dashboard_html_lacks(&["auto", "-safe"]);
    assert_dashboard_html_lacks(&["Safety", " Automatic"]);
    assert_dashboard_html_lacks(&["Safe", " On"]);
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
        overview_cache: Mutex::new(None),
        health_cache: Mutex::new(None),
        http_session_store: Mutex::new(super::http_session::McpHttpSessionStore::default()),
        metrics: super::HttpRuntimeMetrics::default(),
        surface,
        upstream_session_pools: super::new_upstream_session_pools(),
        auth_token: None,
    }
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
    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_resources_response_reports_live_limits_and_pool_shards() {
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
        json_helpers::value_at_path(&response, &["runtime", "upstreamSessionPool", "shardCount"],)
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
fn origin_validation_allows_only_exact_loopback_hosts() {
    for origin in [
        "http://127.0.0.1",
        "http://127.0.0.1:39022",
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
    ] {
        assert!(
            !is_allowed_local_host(host),
            "host should be rejected: {host}"
        );
    }
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

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    let server_root = root.clone();
    let handle = thread::spawn(move || {
        let mut stderr = Vec::new();
        serve_listener(
            listener,
            test_config(server_root, Some(4), super::ServeSurface::Dashboard),
            &mut stderr,
        )
    });

    let mut root_response = String::new();
    let mut stream = TcpStream::connect(addr).unwrap();
    write!(
        stream,
        "GET / HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        addr
    )
    .unwrap();
    stream.read_to_string(&mut root_response).unwrap();
    assert!(root_response.contains("MCPace dashboard"));
    assert!(root_response.contains("/api/overview"));

    let mut favicon_response = String::new();
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
    write!(
        stream,
        "GET /api/resources HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        addr
    )
    .unwrap();
    stream.read_to_string(&mut resources_response).unwrap();
    assert!(resources_response.contains("\"upstreamSessionPool\""));
    assert!(resources_response.contains("\"activeConnections\""));

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

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    let server_root = root.clone();
    let handle = thread::spawn(move || {
        let mut stderr = Vec::new();
        serve_listener(
            listener,
            test_config(server_root, Some(8), super::ServeSurface::UnifiedServe),
            &mut stderr,
        )
    });

    let mut health_response = String::new();
    let mut stream = TcpStream::connect(addr).unwrap();
    write!(
        stream,
        "GET /healthz HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        addr
    )
    .unwrap();
    stream.read_to_string(&mut health_response).unwrap();
    assert!(health_response.contains("\"ok\""));
    assert!(health_response.contains("\"readiness\""));

    let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"serve-test","version":"0.1.0"}}}"#;
    let mut mcp_response = String::new();
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
        sse_get_response.contains("Allow: POST"),
        "sse GET response should advertise POST as the supported MCP method: {}",
        sse_get_response
    );

    let tools_list = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
    let mut tools_response = String::new();
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
        response.contains("not allowed for local MCPace serve mode"),
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

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
fn dashboard_server_policy_action_updates_workers_and_mode() {
    let _local_server_guard = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = temp_root();
    write_fake_upstream_config(&root);

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
fn dashboard_server_autotune_action_batches_policy_updates() {
    let _local_server_guard = crate::LOCAL_SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = temp_root();
    write_fake_upstream_config(&root);

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
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
    let mut stream = TcpStream::connect(addr).unwrap();
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
