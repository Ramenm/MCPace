use crate::app;
use crate::json::{parse_str, JsonValue};
use crate::resources;
use crate::runtimepaths;
use crate::text_utils;
use crate::upstream;

mod diagnostics;
mod http_boundary;
mod http_headers;
mod http_session;
mod http_tools;
mod mcp_http;
mod overview;
mod response;
mod tool_runtime;
use self::diagnostics::runtime_diagnostics;
use self::http_tools::{
    http_tool_definitions, http_tool_definitions_for_protocol, http_tool_names,
};
use self::mcp_http::{handle_mcp_http_route, write_json_error_response};
use self::overview::{
    action_response, cached_health_json, cached_overview_json, query_bool_flag,
    runtime_resources_response,
};
#[cfg(test)]
use self::overview::{build_overview_json, runtime_status_json};
use self::response::{
    empty_object, now_ms, query_parameter, split_target, write_empty_response,
    write_empty_response_with_headers, write_json_response, write_json_response_with_owned_headers,
    write_text_response,
};
#[cfg(test)]
use self::tool_runtime::http_upstream_lease_context;
use self::tool_runtime::run_http_tool;
#[cfg(test)]
use http_boundary::{is_allowed_local_host, is_allowed_local_origin};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

const HTTP_WORKER_STACK_BYTES: usize = 4 * 1024 * 1024;
const DEFAULT_LOG_TAIL: usize = 20;
const MAX_LOG_TAIL: usize = 500;

#[derive(Debug)]
struct ParsedArgs {
    help: bool,
    root_override: Option<PathBuf>,
    host: Option<String>,
    port: u16,
    max_requests: Option<usize>,
    max_connections: usize,
    io_timeout: Duration,
    max_body_bytes: usize,
    overview_cache_ttl: Duration,
    allow_nonlocal_bind: bool,
    insecure_nonlocal_bind: bool,
    auth_token_env: Option<String>,
    error: Option<String>,
}

impl Default for ParsedArgs {
    fn default() -> Self {
        Self {
            help: false,
            root_override: None,
            host: None,
            port: 0,
            max_requests: None,
            max_connections: resources::default_http_connection_limit(),
            io_timeout: resources::default_http_io_timeout(),
            max_body_bytes: resources::default_max_http_body_bytes(),
            overview_cache_ttl: resources::default_dashboard_overview_cache_ttl(),
            allow_nonlocal_bind: false,
            insecure_nonlocal_bind: false,
            auth_token_env: None,
            error: None,
        }
    }
}

#[derive(Clone, Debug)]
struct CachedOverview {
    stored_at: Instant,
    value: JsonValue,
}

#[derive(Clone, Debug)]
struct CachedHealth {
    stored_at: Instant,
    value: JsonValue,
}

#[derive(Default)]
struct HttpRuntimeMetrics {
    accepted_connections: AtomicUsize,
    active_connections: AtomicUsize,
    completed_connections: AtomicUsize,
    failed_connections: AtomicUsize,
    max_active_connections: AtomicUsize,
    total_request_duration_ms: AtomicUsize,
    max_request_duration_ms: AtomicUsize,
}

#[derive(Clone, Debug)]
struct HttpRuntimeMetricsSnapshot {
    accepted_connections: usize,
    active_connections: usize,
    completed_connections: usize,
    failed_connections: usize,
    max_active_connections: usize,
    total_request_duration_ms: usize,
    average_request_duration_ms: usize,
    max_request_duration_ms: usize,
}

struct HttpRuntimeMetricsGuard<'a> {
    metrics: &'a HttpRuntimeMetrics,
    started_at: Instant,
    failed: bool,
}

impl HttpRuntimeMetrics {
    fn begin(&self) -> HttpRuntimeMetricsGuard<'_> {
        self.accepted_connections.fetch_add(1, Ordering::Relaxed);
        let active = self.active_connections.fetch_add(1, Ordering::Relaxed) + 1;
        self.record_max_active(active);
        HttpRuntimeMetricsGuard {
            metrics: self,
            started_at: Instant::now(),
            failed: false,
        }
    }

    fn snapshot(&self) -> HttpRuntimeMetricsSnapshot {
        let completed_connections = self.completed_connections.load(Ordering::Relaxed);
        let total_request_duration_ms = self.total_request_duration_ms.load(Ordering::Relaxed);
        HttpRuntimeMetricsSnapshot {
            accepted_connections: self.accepted_connections.load(Ordering::Relaxed),
            active_connections: self.active_connections.load(Ordering::Relaxed),
            completed_connections,
            failed_connections: self.failed_connections.load(Ordering::Relaxed),
            max_active_connections: self.max_active_connections.load(Ordering::Relaxed),
            total_request_duration_ms,
            average_request_duration_ms: total_request_duration_ms
                .checked_div(completed_connections)
                .unwrap_or(0),
            max_request_duration_ms: self.max_request_duration_ms.load(Ordering::Relaxed),
        }
    }

    fn record_max_active(&self, active: usize) {
        let mut observed = self.max_active_connections.load(Ordering::Relaxed);
        while active > observed {
            match self.max_active_connections.compare_exchange_weak(
                observed,
                active,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(value) => observed = value,
            }
        }
    }

    fn record_request_duration(&self, duration_ms: usize) {
        self.total_request_duration_ms
            .fetch_add(duration_ms, Ordering::Relaxed);
        let mut observed = self.max_request_duration_ms.load(Ordering::Relaxed);
        while duration_ms > observed {
            match self.max_request_duration_ms.compare_exchange_weak(
                observed,
                duration_ms,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(value) => observed = value,
            }
        }
    }
}

impl HttpRuntimeMetricsGuard<'_> {
    fn mark_failed(&mut self) {
        self.failed = true;
    }
}

impl Drop for HttpRuntimeMetricsGuard<'_> {
    fn drop(&mut self) {
        let duration_ms =
            usize::try_from(self.started_at.elapsed().as_millis()).unwrap_or(usize::MAX);
        self.metrics.record_request_duration(duration_ms);
        self.metrics
            .active_connections
            .fetch_sub(1, Ordering::Relaxed);
        self.metrics
            .completed_connections
            .fetch_add(1, Ordering::Relaxed);
        if self.failed {
            self.metrics
                .failed_connections
                .fetch_add(1, Ordering::Relaxed);
        }
    }
}

struct DashboardConfig {
    root_path: PathBuf,
    max_requests: Option<usize>,
    max_connections: usize,
    io_timeout: Duration,
    max_body_bytes: usize,
    overview_cache_ttl: Duration,
    health_cache_ttl: Duration,
    overview_cache: Mutex<Option<CachedOverview>>,
    health_cache: Mutex<Option<CachedHealth>>,
    http_session_store: Mutex<http_session::McpHttpSessionStore>,
    metrics: HttpRuntimeMetrics,
    surface: ServeSurface,
    upstream_session_pools: Vec<Mutex<upstream::UpstreamSessionPool>>,
    auth_token: Option<String>,
}

#[derive(Clone, Copy)]
enum ServeSurface {
    Dashboard,
    UnifiedServe,
}

pub fn run(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    run_internal(args, default_root, stdout, stderr, ServeSurface::Dashboard)
}

pub fn run_serve(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    run_internal(
        args,
        default_root,
        stdout,
        stderr,
        ServeSurface::UnifiedServe,
    )
}

fn run_internal(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    surface: ServeSurface,
) -> i32 {
    let parsed = parse_args(args);
    if let Some(error) = parsed.error {
        let _ = writeln!(stderr, "{}", error);
        return 2;
    }
    if parsed.help {
        write_help(stdout);
        return 0;
    }
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };

    let endpoint = if matches!(surface, ServeSurface::UnifiedServe) {
        runtimepaths::resolve_serve_endpoint(Some(&root_path))
    } else {
        runtimepaths::ServeEndpoint::default()
    };
    let host = parsed.host.clone().unwrap_or_else(|| endpoint.host.clone());
    let auth_token = match resolve_http_auth_token(&parsed) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 2;
        }
    };
    let non_loopback_bind = !is_loopback_bind_host(&host);
    if non_loopback_bind && !parsed.allow_nonlocal_bind {
        let _ = writeln!(
            stderr,
            "refusing to bind non-loopback host '{}'; MCPace local HTTP mode is loopback-only unless --allow-nonlocal-bind is set intentionally",
            host
        );
        return 2;
    }
    if non_loopback_bind && auth_token.is_none() && !parsed.insecure_nonlocal_bind {
        let _ = writeln!(
            stderr,
            "refusing unauthenticated non-loopback bind '{}'; set MCPACE_HTTP_AUTH_TOKEN or --auth-token-env <NAME>, or pass --insecure-nonlocal-bind for an explicit unauthenticated lab-only bind",
            host
        );
        return 2;
    }
    if non_loopback_bind && parsed.insecure_nonlocal_bind {
        let _ = writeln!(
            stderr,
            "warning: non-loopback bind '{}' is unauthenticated because --insecure-nonlocal-bind was provided",
            host
        );
    }
    let port = if parsed.port == 0 {
        if matches!(surface, ServeSurface::UnifiedServe) {
            endpoint.port
        } else {
            0
        }
    } else {
        parsed.port
    };

    let listener = match TcpListener::bind((host.as_str(), port)) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "failed to bind local HTTP listener: {}", error);
            return 1;
        }
    };

    let address = match listener.local_addr() {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "failed to resolve local HTTP address: {}", error);
            return 1;
        }
    };

    let mcp_path = if matches!(surface, ServeSurface::UnifiedServe) {
        endpoint.mcp_path.clone()
    } else {
        runtimepaths::DEFAULT_LOCAL_MCP_PATH.to_string()
    };
    let health_path = if matches!(surface, ServeSurface::UnifiedServe) {
        endpoint.health_path.clone()
    } else {
        runtimepaths::DEFAULT_LOCAL_HEALTH_PATH.to_string()
    };
    let _ = match surface {
        ServeSurface::Dashboard => writeln!(stdout, "Dashboard running at http://{}", address),
        ServeSurface::UnifiedServe => writeln!(
            stdout,
            "Server running at http://{} (UI: /, MCP: {}, health: {})",
            address, mcp_path, health_path
        ),
    };
    let _ = stdout.flush();

    maybe_start_tool_list_cache_warmup(&root_path, surface);

    serve_listener(
        listener,
        DashboardConfig {
            root_path,
            max_requests: parsed.max_requests,
            max_connections: parsed.max_connections,
            io_timeout: parsed.io_timeout,
            max_body_bytes: parsed.max_body_bytes,
            overview_cache_ttl: parsed.overview_cache_ttl,
            health_cache_ttl: resources::default_dashboard_health_cache_ttl(),
            overview_cache: Mutex::new(None),
            health_cache: Mutex::new(None),
            http_session_store: Mutex::new(http_session::McpHttpSessionStore::default()),
            metrics: HttpRuntimeMetrics::default(),
            surface,
            upstream_session_pools: new_upstream_session_pools(),
            auth_token,
        },
        stderr,
    )
}

fn resolve_http_auth_token(parsed: &ParsedArgs) -> Result<Option<String>, String> {
    let env_name = parsed
        .auth_token_env
        .as_deref()
        .unwrap_or("MCPACE_HTTP_AUTH_TOKEN");
    match std::env::var(env_name) {
        Ok(value) => {
            let token = value.trim();
            if token.is_empty() {
                if parsed.auth_token_env.is_some() {
                    Err(format!(
                        "HTTP auth token environment variable '{}' is empty",
                        env_name
                    ))
                } else {
                    Ok(None)
                }
            } else {
                Ok(Some(token.to_string()))
            }
        }
        Err(_) if parsed.auth_token_env.is_some() => Err(format!(
            "HTTP auth token environment variable '{}' is not set",
            env_name
        )),
        Err(_) => Ok(None),
    }
}

fn maybe_start_tool_list_cache_warmup(root_path: &Path, surface: ServeSurface) {
    if !matches!(surface, ServeSurface::UnifiedServe) || !tool_list_cache_warmup_enabled() {
        return;
    }
    let timeout_ms = crate::adapter::ToolExposureOptions::from_env().timeout_ms;
    upstream::warm_tool_list_cache_background(root_path.to_path_buf(), timeout_ms, true);
}

fn tool_list_cache_warmup_enabled() -> bool {
    std::env::var("MCPACE_TOOL_LIST_WARMUP")
        .ok()
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off" | "disabled"
            )
        })
        .unwrap_or(true)
}

fn is_loopback_bind_host(host: &str) -> bool {
    http_boundary::is_loopback_host(host)
}

fn parse_args(args: &[String]) -> ParsedArgs {
    let mut parsed = ParsedArgs::default();
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("dashboard requires a path after --root".to_string());
                    return parsed;
                };
                parsed.root_override = Some(PathBuf::from(value));
                index += 2;
            }
            "--host" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("dashboard requires a value after --host".to_string());
                    return parsed;
                };
                parsed.host = Some(value.to_string());
                index += 2;
            }
            "--port" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("dashboard requires a value after --port".to_string());
                    return parsed;
                };
                match value.parse::<u16>() {
                    Ok(port) => parsed.port = port,
                    Err(_) => {
                        parsed.error = Some("dashboard --port must be a valid u16".to_string());
                        return parsed;
                    }
                }
                index += 2;
            }
            "--max-requests" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("dashboard requires a value after --max-requests".to_string());
                    return parsed;
                };
                match resources::parse_positive_usize(value, "dashboard --max-requests") {
                    Ok(limit) => parsed.max_requests = Some(limit),
                    Err(error) => {
                        parsed.error = Some(error);
                        return parsed;
                    }
                }
                index += 2;
            }
            "--max-connections" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("dashboard requires a value after --max-connections".to_string());
                    return parsed;
                };
                match resources::parse_positive_usize(value, "dashboard --max-connections") {
                    Ok(limit) => parsed.max_connections = limit,
                    Err(error) => {
                        parsed.error = Some(error);
                        return parsed;
                    }
                }
                index += 2;
            }
            "--io-timeout-ms" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("dashboard requires a value after --io-timeout-ms".to_string());
                    return parsed;
                };
                match resources::parse_positive_u64(value, "dashboard --io-timeout-ms") {
                    Ok(timeout_ms) => parsed.io_timeout = Duration::from_millis(timeout_ms),
                    Err(error) => {
                        parsed.error = Some(error);
                        return parsed;
                    }
                }
                index += 2;
            }
            "--max-body-bytes" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("dashboard requires a value after --max-body-bytes".to_string());
                    return parsed;
                };
                match resources::parse_positive_usize(value, "dashboard --max-body-bytes") {
                    Ok(limit) => parsed.max_body_bytes = limit,
                    Err(error) => {
                        parsed.error = Some(error);
                        return parsed;
                    }
                }
                index += 2;
            }
            "--overview-cache-ms" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("dashboard requires a value after --overview-cache-ms".to_string());
                    return parsed;
                };
                match resources::parse_nonnegative_u64(value, "dashboard --overview-cache-ms") {
                    Ok(ttl_ms) => parsed.overview_cache_ttl = Duration::from_millis(ttl_ms),
                    Err(error) => {
                        parsed.error = Some(error);
                        return parsed;
                    }
                }
                index += 2;
            }
            "--allow-nonlocal-bind" => {
                parsed.allow_nonlocal_bind = true;
                index += 1;
            }
            "--insecure-nonlocal-bind" => {
                parsed.insecure_nonlocal_bind = true;
                index += 1;
            }
            "--auth-token-env" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some(
                        "dashboard requires an environment variable name after --auth-token-env"
                            .to_string(),
                    );
                    return parsed;
                };
                if value.trim().is_empty() {
                    parsed.error = Some("dashboard --auth-token-env must not be empty".to_string());
                    return parsed;
                }
                parsed.auth_token_env = Some(value.to_string());
                index += 2;
            }
            "-h" | "--help" | "-?" => {
                parsed.help = true;
                return parsed;
            }
            other => {
                parsed.error = Some(format!("unsupported dashboard argument: {}", other));
                return parsed;
            }
        }
    }

    parsed
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace dashboard [--root <path>] [--host <addr>] [--port <n>] [--max-requests <n>] [--max-connections <n>] [--io-timeout-ms <n>] [--max-body-bytes <n>] [--overview-cache-ms <n>] [--allow-nonlocal-bind] [--auth-token-env <NAME>] [--insecure-nonlocal-bind]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "dashboard serves a local web UI for hub status, verification, servers, clients, and logs."
    );
    let _ = writeln!(
        stdout,
        "The UI stays local-only and reuses existing native JSON command surfaces."
    );
    let _ = writeln!(
        stdout,
        "Non-loopback bind hosts are rejected by default; --allow-nonlocal-bind also requires MCPACE_HTTP_AUTH_TOKEN, --auth-token-env <NAME>, or explicit --insecure-nonlocal-bind."
    );
    let _ = writeln!(
        stdout,
        "Resource defaults: max connections={}, IO timeout={}ms, max body={} bytes, overview cache={}ms, health cache={}ms.",
        resources::default_http_connection_limit(),
        resources::default_http_io_timeout_ms(),
        resources::default_max_http_body_bytes(),
        resources::default_dashboard_overview_cache_ms(),
        resources::default_dashboard_health_cache_ms()
    );
}

struct HttpRequest {
    method: String,
    path: String,
    query: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

enum McpHttpResponse {
    Json(JsonValue),
    JsonWithHeaders(JsonValue, Vec<(String, String)>),
    JsonStatus(&'static str, JsonValue),
    Accepted,
}

fn new_upstream_session_pools() -> Vec<Mutex<upstream::UpstreamSessionPool>> {
    let shard_count = resources::default_upstream_session_pool_shard_count();
    let total_limit = resources::default_upstream_session_pool_limit().max(shard_count);
    let base_limit = total_limit / shard_count;
    let remainder = total_limit % shard_count;

    (0..shard_count)
        .map(|index| {
            let shard_limit = base_limit + if index < remainder { 1 } else { 0 };
            Mutex::new(upstream::UpstreamSessionPool::with_max_sessions(
                shard_limit,
            ))
        })
        .collect()
}

fn upstream_pool_for_context<'a>(
    config: &'a DashboardConfig,
    server: &str,
    context: &upstream::UpstreamLeaseContext,
) -> &'a Mutex<upstream::UpstreamSessionPool> {
    let mut hasher = DefaultHasher::new();
    server.hash(&mut hasher);
    context.client_id.hash(&mut hasher);
    context.session_id.hash(&mut hasher);
    context.project_root.hash(&mut hasher);
    context.transport.hash(&mut hasher);
    let index = (hasher.finish() as usize) % config.upstream_session_pools.len().max(1);
    &config.upstream_session_pools[index]
}

fn serve_listener(listener: TcpListener, config: DashboardConfig, stderr: &mut dyn Write) -> i32 {
    let max_requests = config.max_requests;
    let worker_count = config.max_connections.max(1);
    let config = Arc::new(config);
    let (request_tx, request_rx) = mpsc::sync_channel::<TcpStream>(0);
    let request_rx = Arc::new(Mutex::new(request_rx));
    let (log_tx, log_rx) = mpsc::channel::<String>();
    let mut handles = Vec::with_capacity(worker_count);

    for worker_index in 0..worker_count {
        let worker_rx = Arc::clone(&request_rx);
        let request_config = Arc::clone(&config);
        let worker_log_tx = log_tx.clone();
        handles.push(
            thread::Builder::new()
                .name(format!("mcpace-http-{worker_index}"))
                .stack_size(HTTP_WORKER_STACK_BYTES)
                .spawn(move || loop {
                    let stream = {
                        let rx_guard = worker_rx
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        rx_guard.recv()
                    };
                    let Ok(stream) = stream else {
                        break;
                    };
                    let mut metrics_guard = request_config.metrics.begin();
                    if let Err(error) = handle_connection(stream, request_config.as_ref()) {
                        metrics_guard.mark_failed();
                        let _ = worker_log_tx.send(format!(
                            "dashboard worker {} request failed: {}",
                            worker_index, error
                        ));
                    }
                })
                .expect("failed to spawn MCPace HTTP worker"),
        );
    }
    drop(log_tx);

    let mut accepted = 0usize;
    for incoming in listener.incoming() {
        drain_request_worker_logs(&log_rx, stderr);
        let stream = match incoming {
            Ok(value) => value,
            Err(error) => {
                let _ = writeln!(stderr, "dashboard accept failed: {}", error);
                drop(request_tx);
                join_request_workers(handles, stderr);
                drain_request_worker_logs(&log_rx, stderr);
                return 1;
            }
        };

        accepted = accepted.saturating_add(1);
        if request_tx.send(stream).is_err() {
            let _ = writeln!(stderr, "dashboard request worker pool stopped unexpectedly");
            break;
        }
        drain_request_worker_logs(&log_rx, stderr);

        if max_requests.map(|limit| accepted >= limit).unwrap_or(false) {
            break;
        }
    }

    drop(request_tx);
    join_request_workers(handles, stderr);
    drain_request_worker_logs(&log_rx, stderr);
    0
}

fn join_request_workers(handles: Vec<thread::JoinHandle<()>>, stderr: &mut dyn Write) {
    for handle in handles {
        if handle.join().is_err() {
            let _ = writeln!(stderr, "dashboard request worker panicked");
        }
    }
}

fn drain_request_worker_logs(log_rx: &mpsc::Receiver<String>, stderr: &mut dyn Write) {
    while let Ok(message) = log_rx.try_recv() {
        let _ = writeln!(stderr, "{}", message);
    }
}

enum LimitedHttpLine {
    Empty,
    Line(String),
    TooLong,
}

fn read_limited_http_line(
    reader: &mut BufReader<TcpStream>,
    max_bytes: usize,
    label: &str,
) -> Result<LimitedHttpLine, String> {
    let mut line = Vec::new();
    let max_with_sentinel = max_bytes.saturating_add(1);

    loop {
        let (take_len, newline_seen) = {
            let available = reader
                .fill_buf()
                .map_err(|error| format!("read {}: {}", label, error))?;
            if available.is_empty() {
                if line.is_empty() {
                    return Ok(LimitedHttpLine::Empty);
                }
                let rendered = String::from_utf8_lossy(&line).into_owned();
                return Ok(LimitedHttpLine::Line(rendered));
            }

            let until_newline = available
                .iter()
                .position(|byte| *byte == b'\n')
                .map(|position| position + 1)
                .unwrap_or(available.len());
            let remaining = max_with_sentinel.saturating_sub(line.len());
            let take_len = until_newline.min(remaining);
            line.extend_from_slice(&available[..take_len]);
            (
                take_len,
                take_len == until_newline && available[..take_len].contains(&b'\n'),
            )
        };

        reader.consume(take_len);

        if line.len() > max_bytes {
            return Ok(LimitedHttpLine::TooLong);
        }
        if newline_seen {
            let rendered = String::from_utf8_lossy(&line).into_owned();
            return Ok(LimitedHttpLine::Line(rendered));
        }
        if take_len == 0 {
            return Ok(LimitedHttpLine::TooLong);
        }
    }
}

fn is_supported_http_version(version: &str) -> bool {
    matches!(version, "HTTP/1.1" | "HTTP/1.0")
}

fn handle_connection(mut stream: TcpStream, config: &DashboardConfig) -> Result<(), String> {
    let _ = stream.set_read_timeout(Some(config.io_timeout));
    let _ = stream.set_write_timeout(Some(config.io_timeout));
    let mut reader = BufReader::new(
        stream
            .try_clone()
            .map_err(|error| format!("clone stream: {}", error))?,
    );
    let request_line = match read_limited_http_line(
        &mut reader,
        resources::MAX_HTTP_REQUEST_LINE_BYTES,
        "request line",
    )? {
        LimitedHttpLine::Empty => return Ok(()),
        LimitedHttpLine::Line(value) => value,
        LimitedHttpLine::TooLong => {
            write_text_response(
                &mut stream,
                "414 URI Too Long",
                "text/plain; charset=utf-8",
                "HTTP request line is too large",
            )?;
            return Ok(());
        }
    };

    if request_line.trim().is_empty() {
        return Ok(());
    }

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() != 3 || !is_supported_http_version(parts[2]) {
        write_text_response(
            &mut stream,
            "400 Bad Request",
            "text/plain; charset=utf-8",
            "Malformed HTTP request",
        )?;
        return Ok(());
    }
    if !parts[1].starts_with('/') {
        write_text_response(
            &mut stream,
            "400 Bad Request",
            "text/plain; charset=utf-8",
            "Unsupported HTTP request target",
        )?;
        return Ok(());
    }
    let method = parts[0].to_string();
    let target = parts[1].to_string();

    let mut headers = Vec::new();
    let mut content_length: Option<usize> = None;
    let mut header_bytes = 0usize;
    let mut header_count = 0usize;
    loop {
        let header = match read_limited_http_line(
            &mut reader,
            resources::MAX_HTTP_HEADER_LINE_BYTES,
            "request header",
        )? {
            LimitedHttpLine::Empty => break,
            LimitedHttpLine::Line(value) => value,
            LimitedHttpLine::TooLong => {
                write_text_response(
                    &mut stream,
                    "431 Request Header Fields Too Large",
                    "text/plain; charset=utf-8",
                    "HTTP request headers are too large",
                )?;
                return Ok(());
            }
        };
        if header == "\r\n" || header == "\n" || header.is_empty() {
            break;
        }
        header_count = header_count.saturating_add(1);
        if header_count > resources::MAX_HTTP_HEADER_COUNT {
            write_text_response(
                &mut stream,
                "431 Request Header Fields Too Large",
                "text/plain; charset=utf-8",
                "HTTP request headers are too large",
            )?;
            return Ok(());
        }
        header_bytes = header_bytes.saturating_add(header.len());
        if header_bytes > resources::MAX_HTTP_HEADER_BYTES {
            write_text_response(
                &mut stream,
                "431 Request Header Fields Too Large",
                "text/plain; charset=utf-8",
                "HTTP request headers are too large",
            )?;
            return Ok(());
        }
        let Some((name, value)) = header.split_once(':') else {
            write_text_response(
                &mut stream,
                "400 Bad Request",
                "text/plain; charset=utf-8",
                "Malformed HTTP header",
            )?;
            return Ok(());
        };
        let raw_name = name.trim();
        if !http_boundary::is_valid_http_header_name(raw_name) {
            write_text_response(
                &mut stream,
                "400 Bad Request",
                "text/plain; charset=utf-8",
                "Invalid HTTP header name",
            )?;
            return Ok(());
        }
        let key = raw_name.to_ascii_lowercase();
        let trimmed = value.trim().to_string();
        if key == "transfer-encoding" {
            write_text_response(
                &mut stream,
                "400 Bad Request",
                "text/plain; charset=utf-8",
                "Transfer-Encoding is not supported",
            )?;
            return Ok(());
        }
        if key == "content-length" {
            let parsed_length = match trimmed.parse::<usize>() {
                Ok(value) => value,
                Err(_) => {
                    write_text_response(
                        &mut stream,
                        "400 Bad Request",
                        "text/plain; charset=utf-8",
                        "Invalid Content-Length",
                    )?;
                    return Ok(());
                }
            };
            if content_length.is_some() {
                write_text_response(
                    &mut stream,
                    "400 Bad Request",
                    "text/plain; charset=utf-8",
                    "Duplicate Content-Length is not allowed",
                )?;
                return Ok(());
            }
            content_length = Some(parsed_length);
        }
        headers.push((key, trimmed));
    }

    let host_header_count = headers.iter().filter(|(key, _)| key == "host").count();
    if host_header_count != 1 {
        let message = if host_header_count == 0 {
            "Missing Host header"
        } else {
            "Duplicate Host headers are not allowed"
        };
        write_text_response(
            &mut stream,
            "400 Bad Request",
            "text/plain; charset=utf-8",
            message,
        )?;
        return Ok(());
    }

    let content_length = content_length.unwrap_or(0);
    if content_length > config.max_body_bytes {
        write_text_response(
            &mut stream,
            "413 Payload Too Large",
            "text/plain; charset=utf-8",
            "HTTP request body is too large",
        )?;
        return Ok(());
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        use std::io::Read;
        reader
            .read_exact(&mut body)
            .map_err(|error| format!("read request body: {}", error))?;
    }

    let (path, query) = split_target(&target);
    let request = HttpRequest {
        method,
        path: path.to_string(),
        query: query.to_string(),
        headers,
        body,
    };

    if let Err(error) = handle_http_request(&mut stream, &request, config) {
        write_json_error_response(
            &mut stream,
            "500 Internal Server Error",
            "internal_error",
            &error,
        )
        .map_err(|response_error| {
            format!(
                "{}; failed to write HTTP error response: {}",
                error, response_error
            )
        })?;
        let _ = stream.shutdown(Shutdown::Both);
        return Err(error);
    }

    let _ = stream.shutdown(Shutdown::Both);

    Ok(())
}

fn handle_http_request(
    stream: &mut TcpStream,
    request: &HttpRequest,
    config: &DashboardConfig,
) -> Result<(), String> {
    if !is_authorized_http_request(request, config) {
        write_empty_response_with_headers(
            stream,
            "401 Unauthorized",
            &[("WWW-Authenticate", "Bearer realm=\"mcpace\"")],
        )?;
        return Ok(());
    }
    if reject_forbidden_origin(stream, request)? {
        return Ok(());
    }

    let (mcp_path, health_path) = configured_http_paths(config);
    if request.method == "GET"
        && matches_configured_path(
            &request.path,
            &health_path,
            runtimepaths::DEFAULT_LOCAL_HEALTH_PATH,
        )
    {
        let refresh = query_bool_flag(&request.query, "refresh")
            || query_bool_flag(&request.query, "noCache");
        let payload = cached_health_json(config, refresh)?;
        write_json_response(stream, "200 OK", &payload)?;
        return Ok(());
    }
    if matches_configured_path(
        &request.path,
        &mcp_path,
        runtimepaths::DEFAULT_LOCAL_MCP_PATH,
    ) {
        return handle_mcp_http_route(stream, request, config);
    }

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => {
            write_text_response(stream, "200 OK", "text/html; charset=utf-8", DASHBOARD_HTML)?
        }
        ("GET", "/favicon.ico") => write_text_response(
            stream,
            "200 OK",
            "image/svg+xml; charset=utf-8",
            DASHBOARD_FAVICON_SVG,
        )?,
        ("GET", "/status") => {
            let payload = run_json_command(&config.root_path, &["hub", "status", "--json"])?;
            write_json_response(stream, "200 OK", &payload)?;
        }
        ("GET", "/api/overview") => {
            let refresh = query_bool_flag(&request.query, "refresh")
                || query_bool_flag(&request.query, "noCache");
            let payload = cached_overview_json(config, refresh)?;
            write_json_response(stream, "200 OK", &payload)?;
        }
        ("GET", "/api/resources") => {
            let payload = runtime_resources_response(config);
            write_json_response(stream, "200 OK", &payload)?;
        }
        ("GET", "/api/logs") => {
            let tail = bounded_query_usize(&request.query, "tail", DEFAULT_LOG_TAIL, MAX_LOG_TAIL);
            let payload = run_json_command_vec(
                &config.root_path,
                vec![
                    "hub".to_string(),
                    "logs".to_string(),
                    "--json".to_string(),
                    "--tail".to_string(),
                    tail.to_string(),
                ],
            )?;
            write_json_response(stream, "200 OK", &payload)?;
        }
        ("POST", "/api/actions/hub-up") => {
            let payload = action_response(
                "hub-up",
                run_json_command(&config.root_path, &["hub", "up", "--json"])?,
            );
            write_json_response(stream, "200 OK", &payload)?;
        }
        ("POST", "/api/actions/hub-down") => {
            let payload = action_response(
                "hub-down",
                run_json_command(&config.root_path, &["hub", "down", "--json"])?,
            );
            write_json_response(stream, "200 OK", &payload)?;
        }
        ("POST", "/api/actions/repair") => {
            let payload = action_response(
                "repair",
                run_json_command(&config.root_path, &["repair", "--json"])?,
            );
            write_json_response(stream, "200 OK", &payload)?;
        }
        ("POST", "/api/actions/ping") => {
            let payload = action_response(
                "ping",
                JsonValue::object([
                    ("status", JsonValue::string("ok")),
                    (
                        "surface",
                        JsonValue::string(match config.surface {
                            ServeSurface::Dashboard => "dashboard-http",
                            ServeSurface::UnifiedServe => "unified-serve-http",
                        }),
                    ),
                ]),
            );
            write_json_response(stream, "200 OK", &payload)?;
        }
        ("POST", "/api/actions/server-enable") => {
            write_server_toggle_action(stream, request, config, true)?;
        }
        ("POST", "/api/actions/server-disable") => {
            write_server_toggle_action(stream, request, config, false)?;
        }
        ("POST", "/api/actions/server-policy") => {
            write_server_policy_action(stream, request, config)?;
        }
        ("POST", "/api/actions/server-autotune") => {
            write_server_autotune_action(stream, request, config)?;
        }
        ("POST", "/api/actions/server-test") => {
            write_server_test_action(stream, request, config)?;
        }
        ("POST", "/api/actions/server-install-command") => {
            write_server_install_command_action(stream, request, config)?;
        }
        _ => write_text_response(
            stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            "Not found",
        )?,
    }

    Ok(())
}

fn write_server_toggle_action(
    stream: &mut TcpStream,
    request: &HttpRequest,
    config: &DashboardConfig,
    enabled: bool,
) -> Result<(), String> {
    let body = match parse_action_body(request) {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    };
    let server = match action_server_name(&body) {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    };

    let action = if enabled {
        "server-enable"
    } else {
        "server-disable"
    };
    let command = if enabled { "enable" } else { "disable" };
    let payload = action_response(
        action,
        run_json_command_vec(
            &config.root_path,
            vec![
                "server".to_string(),
                command.to_string(),
                server,
                "--json".to_string(),
            ],
        )?,
    );
    write_json_response(stream, "200 OK", &payload)
}

fn write_server_policy_action(
    stream: &mut TcpStream,
    request: &HttpRequest,
    config: &DashboardConfig,
) -> Result<(), String> {
    let body = match parse_action_body(request) {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    };
    let args = match server_policy_command_args(&body) {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    };

    let payload = action_response(
        "server-policy",
        run_json_command_vec(&config.root_path, args)?,
    );
    write_json_response(stream, "200 OK", &payload)
}

fn write_server_autotune_action(
    stream: &mut TcpStream,
    request: &HttpRequest,
    config: &DashboardConfig,
) -> Result<(), String> {
    let body = match parse_action_body(request) {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    };
    let changes = match body.get("changes").and_then(JsonValue::as_array) {
        Some(value) => value,
        None => {
            return write_bad_action_request(stream, "server autotune requires a 'changes' array");
        }
    };
    if changes.len() > 100 {
        return write_bad_action_request(
            stream,
            "server autotune accepts at most 100 changes per request",
        );
    }

    let mut results = Vec::new();
    for change in changes {
        if !matches!(change, JsonValue::Object(_)) {
            return write_bad_action_request(
                stream,
                "server autotune changes entries must be objects",
            );
        }
        let args = match server_policy_command_args(change) {
            Ok(value) => value,
            Err(error) => return write_bad_action_request(stream, &error),
        };
        results.push(run_json_command_vec(&config.root_path, args)?);
    }

    let payload = action_response(
        "server-autotune",
        JsonValue::object([
            ("status", JsonValue::string("updated")),
            ("updated", JsonValue::number(results.len())),
            ("results", JsonValue::array(results)),
        ]),
    );
    write_json_response(stream, "200 OK", &payload)
}

fn write_server_test_action(
    stream: &mut TcpStream,
    request: &HttpRequest,
    config: &DashboardConfig,
) -> Result<(), String> {
    let body = match parse_action_body(request) {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    };
    let server = match action_server_name(&body) {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    };
    let mut args = vec![
        "server".to_string(),
        "test".to_string(),
        server,
        "--json".to_string(),
    ];
    if let Some(timeout_ms) = match action_positive_usize(&body, "timeoutMs") {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    } {
        args.push("--timeout-ms".to_string());
        args.push(timeout_ms.to_string());
    }

    let payload = action_response(
        "server-test",
        run_json_command_vec(&config.root_path, args)?,
    );
    write_json_response(stream, "200 OK", &payload)
}

fn write_server_install_command_action(
    stream: &mut TcpStream,
    request: &HttpRequest,
    config: &DashboardConfig,
) -> Result<(), String> {
    let body = match parse_action_body(request) {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    };
    let command_line = match action_string(&body, "commandLine")
        .or_else(|_| action_string(&body, "command"))
        .or_else(|_| action_string(&body, "spec"))
    {
        Ok(value) => value,
        Err(_) => {
            return write_bad_action_request(
                stream,
                "server install requires a non-empty commandLine field",
            );
        }
    };
    if command_line.len() > 4096 {
        return write_bad_action_request(stream, "server install commandLine is too long");
    }
    if command_line
        .chars()
        .any(|ch| ch == '\0' || ch == '\r' || ch == '\n' || ch.is_control())
    {
        return write_bad_action_request(
            stream,
            "server install commandLine cannot contain control characters or newlines",
        );
    }
    if command_line_uses_shell_composition(&command_line) {
        return write_bad_action_request(
            stream,
            "server install commandLine must be one launcher, URL, or path; remove shell chaining, pipes, redirects, backticks, or command substitutions",
        );
    }

    let mut args = vec![
        "server".to_string(),
        "install".to_string(),
        command_line,
        "--json".to_string(),
    ];
    let server_name = match optional_action_string(&body, "server") {
        Ok(Some(value)) => Some(value),
        Ok(None) => match optional_action_string(&body, "name") {
            Ok(value) => value,
            Err(error) => return write_bad_action_request(stream, &error),
        },
        Err(error) => return write_bad_action_request(stream, &error),
    };
    if let Some(server) = server_name {
        args.push("--as".to_string());
        args.push(server);
    }
    let force = match action_bool(&body, "force") {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    };
    if force {
        args.push("--force".to_string());
    }
    let disabled = match action_bool(&body, "disabled") {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    };
    if disabled {
        args.push("--disabled".to_string());
    }
    let dry_run = match action_bool(&body, "dryRun") {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    };
    if dry_run {
        args.push("--dry-run".to_string());
    }

    let payload = action_response(
        "server-install-command",
        run_json_command_vec(&config.root_path, args)?,
    );
    write_json_response(stream, "200 OK", &payload)
}

fn command_line_uses_shell_composition(value: &str) -> bool {
    text_utils::uses_shell_composition(value)
}

fn write_bad_action_request(stream: &mut TcpStream, error: &str) -> Result<(), String> {
    write_json_error_response(stream, "400 Bad Request", "bad_request", error)
}

fn parse_action_body(request: &HttpRequest) -> Result<JsonValue, String> {
    if request.body.is_empty() {
        return Ok(empty_object());
    }
    if !http_boundary::content_type_is(request, "application/json") {
        return Err("JSON action bodies require Content-Type: application/json".to_string());
    }
    let body = std::str::from_utf8(&request.body)
        .map_err(|error| format!("request body is not UTF-8: {}", error))?;
    let parsed =
        parse_str(body.trim()).map_err(|error| format!("invalid JSON request body: {}", error))?;
    if !matches!(parsed, JsonValue::Object(_)) {
        return Err("action body must be a JSON object".to_string());
    }
    Ok(parsed)
}

fn action_server_name(body: &JsonValue) -> Result<String, String> {
    action_string(body, "server")
        .or_else(|_| action_string(body, "name"))
        .map_err(|_| "server action requires a non-empty server name".to_string())
}

fn action_policy_mode(body: &JsonValue) -> Result<String, String> {
    let mode = action_string(body, "mode")?
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-");
    let canonical = match mode.as_str() {
        "shared" | "parallel" | "parallel-safe" | "multi-reader" => "shared",
        "serialized" | "serial" | "single-writer" | "queue" | "queued" => "serialized",
        "session" | "session-isolated" | "per-session" | "single-session" => "session-isolated",
        "project" | "project-isolated" | "per-project" | "isolated-per-project" => {
            "project-isolated"
        }
        "pool" | "process-pool" | "worker-pool" => "pool",
        "disabled" | "off" => "disabled",
        _ => {
            return Err(
                "server policy mode must be one of shared, serialized, session-isolated, project-isolated, pool, or disabled"
                    .to_string(),
            );
        }
    };
    Ok(canonical.to_string())
}

fn server_policy_command_args(body: &JsonValue) -> Result<Vec<String>, String> {
    let server = action_server_name(body)?;
    let mode = action_policy_mode(body)?;
    let mut args = vec![
        "server".to_string(),
        "set-policy".to_string(),
        server,
        "--mode".to_string(),
        mode.clone(),
        "--json".to_string(),
    ];
    if mode != "disabled" {
        push_positive_usize_arg(&mut args, body, "maxWorkers", "--max-workers")?;
        push_positive_usize_arg(
            &mut args,
            body,
            "maxInFlightPerWorker",
            "--max-in-flight-per-worker",
        )?;
    }
    push_positive_usize_arg(&mut args, body, "queueTimeoutMs", "--queue-timeout-ms")?;
    push_string_arg(&mut args, body, "reusePolicy", "--reuse-policy")?;
    push_affinity_arg(&mut args, body)?;
    Ok(args)
}

fn push_positive_usize_arg(
    args: &mut Vec<String>,
    body: &JsonValue,
    key: &str,
    arg_name: &str,
) -> Result<(), String> {
    if let Some(value) = action_positive_usize(body, key)? {
        args.push(arg_name.to_string());
        args.push(value.to_string());
    }
    Ok(())
}

fn push_string_arg(
    args: &mut Vec<String>,
    body: &JsonValue,
    key: &str,
    arg_name: &str,
) -> Result<(), String> {
    let Some(value) = body.get(key) else {
        return Ok(());
    };
    match value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => {
            args.push(arg_name.to_string());
            args.push(value.to_string());
            Ok(())
        }
        None => Err(format!("'{}' must be a non-empty string", key)),
    }
}

fn push_affinity_arg(args: &mut Vec<String>, body: &JsonValue) -> Result<(), String> {
    let Some(value) = body.get("affinity") else {
        return Ok(());
    };
    let affinity = match value {
        JsonValue::String(raw) => raw
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>(),
        JsonValue::Array(items) => {
            let mut values = Vec::new();
            for item in items {
                let Some(raw) = item
                    .as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    return Err("'affinity' array items must be non-empty strings".to_string());
                };
                values.push(raw.to_string());
            }
            values
        }
        _ => return Err("'affinity' must be a string or array of strings".to_string()),
    };
    if !affinity.is_empty() {
        args.push("--affinity".to_string());
        args.push(affinity.join(","));
    }
    Ok(())
}

fn action_string(body: &JsonValue, key: &str) -> Result<String, String> {
    optional_action_string(body, key)?
        .ok_or_else(|| format!("request body requires a non-empty '{}' field", key))
}

fn optional_action_string(body: &JsonValue, key: &str) -> Result<Option<String>, String> {
    let Some(value) = body.get(key) else {
        return Ok(None);
    };
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| Ok(Some(value.to_string())))
        .unwrap_or_else(|| {
            Err(format!(
                "'{}' must be a non-empty string when provided",
                key
            ))
        })
}

fn action_bool(body: &JsonValue, key: &str) -> Result<bool, String> {
    let Some(value) = body.get(key) else {
        return Ok(false);
    };
    match value {
        JsonValue::Bool(value) => Ok(*value),
        JsonValue::String(value) if value.eq_ignore_ascii_case("true") => Ok(true),
        JsonValue::String(value) if value.eq_ignore_ascii_case("false") => Ok(false),
        _ => Err(format!("'{}' must be a boolean when provided", key)),
    }
}

fn action_positive_usize(body: &JsonValue, key: &str) -> Result<Option<usize>, String> {
    let Some(value) = body.get(key) else {
        return Ok(None);
    };
    let parsed = match value {
        JsonValue::Number(raw) | JsonValue::String(raw) => raw.trim().parse::<usize>().ok(),
        _ => None,
    };
    match parsed {
        Some(number) if number > 0 => Ok(Some(number)),
        _ => Err(format!("'{}' must be a positive integer", key)),
    }
}

fn is_authorized_http_request(request: &HttpRequest, config: &DashboardConfig) -> bool {
    let Some(expected) = config.auth_token.as_deref() else {
        return true;
    };
    let mut authorization_headers = request
        .headers
        .iter()
        .filter(|(key, _)| key == "authorization");
    let Some((_, value)) = authorization_headers.next() else {
        return false;
    };
    if authorization_headers.next().is_some() {
        return false;
    }
    let Some(token) = authorization_bearer_token(value) else {
        return false;
    };
    constant_time_eq(token.as_bytes(), expected.as_bytes())
}

fn authorization_bearer_token(value: &str) -> Option<&str> {
    let mut parts = value.split_whitespace();
    let scheme = parts.next()?;
    let token = parts.next()?;
    if parts.next().is_some() || token.is_empty() || !scheme.eq_ignore_ascii_case("Bearer") {
        return None;
    }
    Some(token)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let max_len = left.len().max(right.len());
    let mut diff = left.len() ^ right.len();
    for index in 0..max_len {
        let left_byte = left.get(index).copied().unwrap_or(0);
        let right_byte = right.get(index).copied().unwrap_or(0);
        diff |= (left_byte ^ right_byte) as usize;
    }
    diff == 0
}

fn configured_http_paths(config: &DashboardConfig) -> (String, String) {
    if matches!(config.surface, ServeSurface::UnifiedServe) {
        let endpoint = runtimepaths::resolve_serve_endpoint(Some(&config.root_path));
        (endpoint.mcp_path, endpoint.health_path)
    } else {
        (
            runtimepaths::DEFAULT_LOCAL_MCP_PATH.to_string(),
            runtimepaths::DEFAULT_LOCAL_HEALTH_PATH.to_string(),
        )
    }
}

fn matches_configured_path(path: &str, configured: &str, fallback: &str) -> bool {
    path == configured || path == fallback
}

fn bounded_query_usize(query: &str, key: &str, default: usize, max: usize) -> usize {
    query_parameter(query, key)
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .map(|value| value.min(max))
        .unwrap_or(default)
}

fn reject_forbidden_origin(stream: &mut TcpStream, request: &HttpRequest) -> Result<bool, String> {
    let Err(error) = http_boundary::validate_origin(request) else {
        return Ok(false);
    };
    let payload = JsonValue::object([
        ("ok", JsonValue::bool(false)),
        ("error", JsonValue::string(error)),
    ]);
    write_json_response(stream, "403 Forbidden", &payload)?;
    Ok(true)
}

pub(super) fn run_json_command(root_path: &Path, args: &[&str]) -> Result<JsonValue, String> {
    run_json_command_vec(
        root_path,
        args.iter().map(|value| (*value).to_string()).collect(),
    )
}

pub(super) fn run_json_command_vec(
    root_path: &Path,
    mut args: Vec<String>,
) -> Result<JsonValue, String> {
    args.push("--root".to_string());
    args.push(root_path.display().to_string());

    let mut stdout_buffer = Vec::new();
    let mut stderr_buffer = Vec::new();
    let exit_code = app::run(args, &mut stdout_buffer, &mut stderr_buffer);
    if exit_code != 0 {
        let stderr_text = String::from_utf8(stderr_buffer).unwrap_or_default();
        let stdout_text = String::from_utf8(stdout_buffer).unwrap_or_default();
        return Err(if !stderr_text.trim().is_empty() {
            stderr_text.trim().to_string()
        } else if !stdout_text.trim().is_empty() {
            stdout_text.trim().to_string()
        } else {
            format!("dashboard command failed with exit code {}", exit_code)
        });
    }

    let stdout_text =
        String::from_utf8(stdout_buffer).map_err(|error| format!("non-UTF8 output: {}", error))?;
    parse_str(stdout_text.trim()).map_err(|error| format!("invalid JSON output: {}", error))
}

const DASHBOARD_HTML: &str = include_str!("dashboard/index.html");
const DASHBOARD_FAVICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" rx="14" fill="#111827"/><path d="M16 42V22h9l7 10 7-10h9v20h-8V30l-8 11-8-11v12h-8Z" fill="#7dd3fc"/><circle cx="51" cy="13" r="5" fill="#34d399"/></svg>"##;

#[cfg(test)]
mod tests;
