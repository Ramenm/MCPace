use crate::app;
use crate::diagnostics as stderr_diagnostics;
use crate::json::{parse_str, JsonValue};
use crate::resources;
use crate::runtimepaths;
use crate::text_utils;
use crate::upstream;
use clap::{error::ErrorKind, Parser};

mod admission;
mod diagnostics;
mod governor;
mod http_boundary;
mod http_headers;
mod http_session;
mod http_tools;
mod latency;
mod mcp_http;
mod operations;
mod overview;
mod rate_limit;
mod response;
mod tool_runtime;
mod trace;
use self::admission::{HttpAdmissionController, HttpAdmissionKind};
use self::diagnostics::runtime_diagnostics;
use self::governor::GlobalResourceGovernor;
use self::http_tools::{
    http_tool_definitions, http_tool_definitions_for_protocol, http_tool_names,
};
use self::latency::{RequestLatencyObservation, RequestLatencyTracker};
use self::mcp_http::{handle_mcp_http_route, write_json_error_response};
use self::overview::{
    action_response, cached_health_json, cached_overview_json, query_bool_flag,
    runtime_resources_response,
};
#[cfg(test)]
use self::overview::{build_overview_json, runtime_status_json};
use self::rate_limit::HttpRateLimiter;
use self::response::{
    empty_object, now_ms, query_parameter, split_target, write_empty_response,
    write_empty_response_with_headers, write_json_response, write_json_response_with_owned_headers,
    write_text_response,
};
#[cfg(test)]
use self::tool_runtime::http_upstream_lease_context;
use self::tool_runtime::run_http_tool;
use self::trace::{OperationTraceObservation, OperationTraceTracker};
#[cfg(test)]
use http_boundary::{is_allowed_local_host, is_allowed_local_origin};
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read, Write};
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
const DEFAULT_OPERATIONS_LIMIT: usize = 2000;
const MAX_OPERATIONS_LIMIT: usize = 5000;

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
    value: Arc<JsonValue>,
}

#[derive(Clone, Debug)]
struct CachedHealth {
    stored_at: Instant,
    value: JsonValue,
}

#[derive(Clone, Debug)]
struct CachedOverviewFailure {
    stored_at: Instant,
    error: String,
}

#[derive(Default)]
struct OverviewCacheState {
    entry: Option<CachedOverview>,
    cold_failure: Option<CachedOverviewFailure>,
    refreshing: bool,
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
    overview_cache: Mutex<OverviewCacheState>,
    health_cache: Mutex<Option<CachedHealth>>,
    request_latencies: Mutex<RequestLatencyTracker>,
    operation_traces: Mutex<OperationTraceTracker>,
    rate_limiter: Mutex<HttpRateLimiter>,
    admission: HttpAdmissionController,
    resource_governor: GlobalResourceGovernor,
    http_session_store: Mutex<http_session::McpHttpSessionStore>,
    metrics: HttpRuntimeMetrics,
    surface: ServeSurface,
    upstream_session_pool: upstream::UpstreamSessionPool,
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
    let parsed = parse_cli(args);
    if let Some(error) = parsed.error {
        stderr_diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 2;
    }
    if parsed.help {
        write_help(stdout);
        return 0;
    }
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        stderr_diagnostics::stderr_line(
            stderr,
            format_args!("mcpace root not found; expected mcpace.config.json"),
        );
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
            stderr_diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 2;
        }
    };
    if parsed.allow_nonlocal_bind || parsed.insecure_nonlocal_bind {
        stderr_diagnostics::stderr_line(
            stderr,
            format_args!(
                "direct non-loopback HTTP flags are no longer supported; keep MCPace on loopback and terminate HTTPS in a trusted reverse proxy or tunnel"
            ),
        );
        return 2;
    }
    if !is_loopback_bind_host(&host) {
        stderr_diagnostics::stderr_line(
            stderr,
            format_args!(
                "refusing to bind non-loopback host '{}'; the built-in HTTP server is loopback-only because it does not terminate TLS; use a trusted HTTPS reverse proxy or tunnel",
                host
            ),
        );
        return 2;
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
            stderr_diagnostics::stderr_line(
                stderr,
                format_args!("failed to bind local HTTP listener: {}", error),
            );
            return 1;
        }
    };

    let address = match listener.local_addr() {
        Ok(value) => value,
        Err(error) => {
            stderr_diagnostics::stderr_line(
                stderr,
                format_args!("failed to resolve local HTTP address: {}", error),
            );
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
            overview_cache: Mutex::new(OverviewCacheState::default()),
            health_cache: Mutex::new(None),
            request_latencies: Mutex::new(RequestLatencyTracker::default()),
            operation_traces: Mutex::new(OperationTraceTracker::default()),
            rate_limiter: Mutex::new(HttpRateLimiter::default()),
            admission: HttpAdmissionController::default(),
            resource_governor: GlobalResourceGovernor::for_http_connections(parsed.max_connections),
            http_session_store: Mutex::new(http_session::McpHttpSessionStore::default()),
            metrics: HttpRuntimeMetrics::default(),
            surface,
            upstream_session_pool: new_upstream_session_pool(),
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
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on" | "enabled"
            )
        })
        .unwrap_or(false)
}

fn is_loopback_bind_host(host: &str) -> bool {
    http_boundary::is_loopback_host(host)
}

#[derive(Debug, Parser)]
#[command(
    name = "mcpace advanced runtime foreground",
    disable_version_flag = true,
    about = "Serve the local MCPace dashboard UI"
)]
struct DashboardCli {
    #[arg(long = "root", value_name = "PATH")]
    root_override: Option<PathBuf>,

    #[arg(long = "host", value_name = "ADDR")]
    host: Option<String>,

    #[arg(long = "port", value_name = "N")]
    port: Option<u16>,

    #[arg(long = "max-requests", value_name = "N")]
    max_requests: Option<usize>,

    #[arg(long = "max-connections", value_name = "N")]
    max_connections: Option<usize>,

    #[arg(long = "io-timeout-ms", value_name = "MS")]
    io_timeout_ms: Option<u64>,

    #[arg(long = "max-body-bytes", value_name = "N")]
    max_body_bytes: Option<usize>,

    #[arg(long = "overview-cache-ms", value_name = "MS")]
    overview_cache_ms: Option<u64>,

    #[arg(long = "allow-nonlocal-bind", hide = true)]
    allow_nonlocal_bind: bool,

    #[arg(long = "insecure-nonlocal-bind", hide = true)]
    insecure_nonlocal_bind: bool,

    #[arg(long = "auth-token-env", value_name = "NAME")]
    auth_token_env: Option<String>,
}

fn parse_cli(args: &[String]) -> ParsedArgs {
    match DashboardCli::try_parse_from(argv(args)) {
        Ok(cli) => parsed_from_cli(cli),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            ParsedArgs {
                help: true,
                ..ParsedArgs::default()
            }
        }
        Err(error) => ParsedArgs {
            error: Some(error.to_string()),
            ..ParsedArgs::default()
        },
    }
}

fn parsed_from_cli(cli: DashboardCli) -> ParsedArgs {
    let mut parsed = ParsedArgs {
        root_override: cli.root_override,
        host: cli.host,
        port: cli.port.unwrap_or(0),
        max_requests: cli.max_requests,
        allow_nonlocal_bind: cli.allow_nonlocal_bind,
        insecure_nonlocal_bind: cli.insecure_nonlocal_bind,
        auth_token_env: cli.auth_token_env,
        ..ParsedArgs::default()
    };

    if parsed.max_requests == Some(0) {
        parsed.error = Some("dashboard --max-requests must be a positive integer".to_string());
        return parsed;
    }
    if let Some(limit) = cli.max_connections {
        if limit == 0 {
            parsed.error =
                Some("dashboard --max-connections must be a positive integer".to_string());
            return parsed;
        }
        if limit > resources::HTTP_CONNECTION_LIMIT_MAX {
            parsed.error = Some(format!(
                "dashboard --max-connections must not exceed {}",
                resources::HTTP_CONNECTION_LIMIT_MAX
            ));
            return parsed;
        }
        parsed.max_connections = limit;
    }
    if let Some(timeout_ms) = cli.io_timeout_ms {
        if timeout_ms == 0 {
            parsed.error = Some("dashboard --io-timeout-ms must be a positive integer".to_string());
            return parsed;
        }
        if timeout_ms > resources::HTTP_IO_TIMEOUT_MS_MAX {
            parsed.error = Some(format!(
                "dashboard --io-timeout-ms must not exceed {}",
                resources::HTTP_IO_TIMEOUT_MS_MAX
            ));
            return parsed;
        }
        parsed.io_timeout = Duration::from_millis(timeout_ms);
    }
    if let Some(limit) = cli.max_body_bytes {
        if limit == 0 {
            parsed.error =
                Some("dashboard --max-body-bytes must be a positive integer".to_string());
            return parsed;
        }
        if limit > resources::HTTP_BODY_BYTES_MAX {
            parsed.error = Some(format!(
                "dashboard --max-body-bytes must not exceed {}",
                resources::HTTP_BODY_BYTES_MAX
            ));
            return parsed;
        }
        parsed.max_body_bytes = limit;
    }
    if let Some(ttl_ms) = cli.overview_cache_ms {
        parsed.overview_cache_ttl = Duration::from_millis(ttl_ms);
    }
    if parsed
        .auth_token_env
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        parsed.error = Some("dashboard --auth-token-env must not be empty".to_string());
    }

    parsed
}

fn argv(args: &[String]) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(OsString::from("mcpace advanced runtime foreground"));
    argv.extend(args.iter().map(OsString::from));
    argv
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace advanced runtime foreground [--root <path>] [--host <loopback>] [--port <n>] [--max-requests <n>] [--max-connections <n>] [--io-timeout-ms <n>] [--max-body-bytes <n>] [--overview-cache-ms <n>] [--auth-token-env <NAME>]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "The foreground runtime serves the local web UI plus the MCP endpoint."
    );
    let _ = writeln!(
        stdout,
        "The UI stays local-only and reuses existing native JSON command surfaces."
    );
    let _ = writeln!(
        stdout,
        "The built-in HTTP server is loopback-only and does not terminate TLS. Use a trusted HTTPS reverse proxy or tunnel for remote access."
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

fn new_upstream_session_pool() -> upstream::UpstreamSessionPool {
    upstream::UpstreamSessionPool::with_max_sessions(
        resources::default_upstream_session_pool_limit(),
    )
}

fn serve_listener(listener: TcpListener, config: DashboardConfig, stderr: &mut dyn Write) -> i32 {
    let max_requests = config.max_requests;
    let worker_count = config.max_connections.max(1);
    let config = Arc::new(config);
    // Keep the listener accepting short-lived local HTTP connections even when
    // workers are briefly busy. A zero-capacity rendezvous channel makes the
    // accept loop wait for an idle worker on every connection; on Windows this
    // can overflow the TCP listen backlog during modest loopback load and show
    // up to clients as connect timeouts even though handled requests succeed.
    let (request_tx, request_rx) = mpsc::sync_channel::<TcpStream>(worker_count);
    let request_rx = Arc::new(Mutex::new(request_rx));
    let (log_tx, log_rx) = mpsc::channel::<String>();
    let mut handles = Vec::with_capacity(worker_count);

    for worker_index in 0..worker_count {
        let worker_rx = Arc::clone(&request_rx);
        let request_config = Arc::clone(&config);
        let worker_log_tx = log_tx.clone();
        let worker = thread::Builder::new()
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
            });
        match worker {
            Ok(handle) => handles.push(handle),
            Err(error) => {
                stderr_diagnostics::stderr_line(
                    stderr,
                    format_args!(
                        "dashboard failed to spawn HTTP worker {}: {}",
                        worker_index, error
                    ),
                );
                drop(request_tx);
                join_request_workers(handles, stderr);
                drain_request_worker_logs(&log_rx, stderr);
                return 1;
            }
        }
    }
    drop(log_tx);

    let mut accepted = 0usize;
    for incoming in listener.incoming() {
        drain_request_worker_logs(&log_rx, stderr);
        let stream = match incoming {
            Ok(value) => value,
            Err(error) => {
                stderr_diagnostics::stderr_line(
                    stderr,
                    format_args!("dashboard accept failed: {}", error),
                );
                drop(request_tx);
                join_request_workers(handles, stderr);
                drain_request_worker_logs(&log_rx, stderr);
                return 1;
            }
        };

        accepted = accepted.saturating_add(1);
        if request_tx.send(stream).is_err() {
            stderr_diagnostics::stderr_line(
                stderr,
                format_args!("dashboard request worker pool stopped unexpectedly"),
            );
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
            stderr_diagnostics::stderr_line(
                stderr,
                format_args!("dashboard request worker panicked"),
            );
        }
    }
}

fn drain_request_worker_logs(log_rx: &mpsc::Receiver<String>, stderr: &mut dyn Write) {
    while let Ok(message) = log_rx.try_recv() {
        stderr_diagnostics::stderr_line(stderr, format_args!("{}", message));
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
    deadline: Instant,
) -> Result<LimitedHttpLine, String> {
    let mut line = Vec::new();
    let max_with_sentinel = max_bytes.saturating_add(1);

    loop {
        set_remaining_http_read_timeout(reader, deadline, label)?;
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

fn set_remaining_http_read_timeout(
    reader: &mut BufReader<TcpStream>,
    deadline: Instant,
    label: &str,
) -> Result<(), String> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| format!("HTTP request deadline exceeded while reading {}", label))?;
    reader
        .get_mut()
        .set_read_timeout(Some(remaining.max(Duration::from_millis(1))))
        .map_err(|error| format!("set {} timeout: {}", label, error))
}

fn read_limited_http_body(
    reader: &mut BufReader<TcpStream>,
    content_length: usize,
    deadline: Instant,
) -> Result<Vec<u8>, String> {
    const BODY_READ_CHUNK_BYTES: usize = 8 * 1024;
    let mut body = Vec::with_capacity(content_length.min(BODY_READ_CHUNK_BYTES));
    let mut buffer = [0u8; BODY_READ_CHUNK_BYTES];

    while body.len() < content_length {
        set_remaining_http_read_timeout(reader, deadline, "request body")?;
        let remaining = content_length.saturating_sub(body.len());
        let read_len = remaining.min(buffer.len());
        reader
            .read_exact(&mut buffer[..read_len])
            .map_err(|error| format!("read request body: {}", error))?;
        body.extend_from_slice(&buffer[..read_len]);
    }

    Ok(body)
}

fn is_supported_http_version(version: &str) -> bool {
    matches!(version, "HTTP/1.1" | "HTTP/1.0")
}

fn record_http_request_latency(config: &DashboardConfig, observation: RequestLatencyObservation) {
    config
        .request_latencies
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .record(observation);
}

struct HttpLatencyProbe<'a> {
    config: &'a DashboardConfig,
    started_at: Instant,
    method: String,
    route: String,
    path: String,
    request_body_bytes: usize,
    request_header_bytes: usize,
    parse_duration: Duration,
    body_read_duration: Duration,
    dispatch_duration: Duration,
    failed: bool,
    recorded: bool,
}

impl<'a> HttpLatencyProbe<'a> {
    fn new(config: &'a DashboardConfig) -> Self {
        Self {
            config,
            started_at: Instant::now(),
            method: "UNKNOWN".to_string(),
            route: "http.unparsed".to_string(),
            path: String::new(),
            request_body_bytes: 0,
            request_header_bytes: 0,
            parse_duration: Duration::from_millis(0),
            body_read_duration: Duration::from_millis(0),
            dispatch_duration: Duration::from_millis(0),
            failed: true,
            recorded: false,
        }
    }

    fn mark_empty(&mut self) {
        self.route = "http.empty".to_string();
        self.failed = false;
        self.note_parse_elapsed();
    }

    fn mark_route(&mut self, route: &str) {
        self.route = route.to_string();
        self.note_parse_elapsed();
    }

    fn note_parse_elapsed(&mut self) {
        if self.parse_duration == Duration::from_millis(0) {
            self.parse_duration = self.started_at.elapsed();
        }
    }

    fn set_parsed_target(&mut self, method: &str, path: &str, query: &str) {
        self.method = method.to_string();
        self.path = path.to_string();
        self.route = latency_route_label(path, query, self.config);
    }

    fn set_parse_complete(&mut self, parse_duration: Duration, request_header_bytes: usize) {
        self.parse_duration = parse_duration;
        self.request_header_bytes = request_header_bytes;
    }

    fn set_body(&mut self, request_body_bytes: usize, body_read_duration: Duration) {
        self.request_body_bytes = request_body_bytes;
        self.body_read_duration = body_read_duration;
    }

    fn set_dispatch(&mut self, dispatch_duration: Duration, failed: bool) {
        self.dispatch_duration = dispatch_duration;
        self.failed = failed;
    }

    fn mark_ok(&mut self) {
        self.failed = false;
    }

    fn record_now(&mut self) {
        if self.recorded {
            return;
        }
        self.recorded = true;
        record_http_request_latency(
            self.config,
            RequestLatencyObservation {
                method: self.method.clone(),
                route: self.route.clone(),
                path: self.path.clone(),
                request_body_bytes: self.request_body_bytes,
                request_header_bytes: self.request_header_bytes,
                parse_duration: self.parse_duration,
                body_read_duration: self.body_read_duration,
                dispatch_duration: self.dispatch_duration,
                total_duration: self.started_at.elapsed(),
                failed: self.failed,
            },
        );
    }
}

impl Drop for HttpLatencyProbe<'_> {
    fn drop(&mut self) {
        self.record_now();
    }
}

fn record_operation_trace(config: &DashboardConfig, observation: OperationTraceObservation) {
    config
        .operation_traces
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .record(observation);
}

fn check_http_rate_limit(
    stream: &TcpStream,
    config: &DashboardConfig,
) -> Option<(String, Duration)> {
    let client_key = stream
        .peer_addr()
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|_| "unknown-peer".to_string());
    let decision = config
        .rate_limiter
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .check(&client_key, Instant::now());
    if decision.allowed {
        None
    } else {
        Some((decision.client_key, decision.retry_after))
    }
}

fn enter_http_heavy_action_or_write_busy<'a>(
    stream: &mut TcpStream,
    config: &'a DashboardConfig,
    label: &str,
) -> Result<Option<self::admission::HttpAdmissionPermit<'a>>, String> {
    let Some(permit) = config.admission.try_enter(HttpAdmissionKind::HeavyAction) else {
        record_operation_trace(
            config,
            OperationTraceObservation {
                name: label.to_string(),
                route: "http.admission_rejected".to_string(),
                duration: Duration::from_millis(0),
                failed: true,
                attributes: vec![("kind".to_string(), "heavyAction".to_string())],
            },
        );
        write_admission_rejected(stream, label)?;
        return Ok(None);
    };
    Ok(Some(permit))
}

fn handle_connection(mut stream: TcpStream, config: &DashboardConfig) -> Result<(), String> {
    let mut latency_probe = HttpLatencyProbe::new(config);
    let request_started_at = latency_probe.started_at;
    let request_deadline = request_started_at + config.io_timeout;
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
        request_deadline,
    )? {
        LimitedHttpLine::Empty => {
            latency_probe.mark_empty();
            return Ok(());
        }
        LimitedHttpLine::Line(value) => value,
        LimitedHttpLine::TooLong => {
            latency_probe.mark_route("http.request_line_too_large");
            write_text_response(
                &mut stream,
                "414 URI Too Long",
                "text/plain; charset=utf-8",
                "HTTP request line is too large",
            )?;
            latency_probe.mark_ok();
            return Ok(());
        }
    };

    if request_line.trim().is_empty() {
        latency_probe.mark_empty();
        return Ok(());
    }

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() != 3 || !is_supported_http_version(parts[2]) {
        latency_probe.mark_route("http.malformed_request");
        write_text_response(
            &mut stream,
            "400 Bad Request",
            "text/plain; charset=utf-8",
            "Malformed HTTP request",
        )?;
        latency_probe.mark_ok();
        return Ok(());
    }
    if !parts[1].starts_with('/') {
        latency_probe.mark_route("http.unsupported_target");
        write_text_response(
            &mut stream,
            "400 Bad Request",
            "text/plain; charset=utf-8",
            "Unsupported HTTP request target",
        )?;
        latency_probe.mark_ok();
        return Ok(());
    }
    let method = parts[0].to_string();
    let target = parts[1].to_string();
    let (early_path, early_query) = split_target(&target);
    latency_probe.set_parsed_target(&method, early_path, early_query);

    let mut headers = Vec::new();
    let mut content_length: Option<usize> = None;
    let mut header_bytes = 0usize;
    let mut header_count = 0usize;
    loop {
        let header = match read_limited_http_line(
            &mut reader,
            resources::MAX_HTTP_HEADER_LINE_BYTES,
            "request header",
            request_deadline,
        )? {
            LimitedHttpLine::Empty => break,
            LimitedHttpLine::Line(value) => value,
            LimitedHttpLine::TooLong => {
                latency_probe.mark_route("http.header_line_too_large");
                write_text_response(
                    &mut stream,
                    "431 Request Header Fields Too Large",
                    "text/plain; charset=utf-8",
                    "HTTP request headers are too large",
                )?;
                latency_probe.mark_ok();
                return Ok(());
            }
        };
        if header == "\r\n" || header == "\n" || header.is_empty() {
            break;
        }
        header_count = header_count.saturating_add(1);
        if header_count > resources::MAX_HTTP_HEADER_COUNT {
            latency_probe.mark_route("http.header_count_too_large");
            write_text_response(
                &mut stream,
                "431 Request Header Fields Too Large",
                "text/plain; charset=utf-8",
                "HTTP request headers are too large",
            )?;
            latency_probe.mark_ok();
            return Ok(());
        }
        header_bytes = header_bytes.saturating_add(header.len());
        if header_bytes > resources::MAX_HTTP_HEADER_BYTES {
            latency_probe.mark_route("http.headers_too_large");
            write_text_response(
                &mut stream,
                "431 Request Header Fields Too Large",
                "text/plain; charset=utf-8",
                "HTTP request headers are too large",
            )?;
            latency_probe.mark_ok();
            return Ok(());
        }
        let Some((name, value)) = header.split_once(':') else {
            latency_probe.mark_route("http.malformed_header");
            write_text_response(
                &mut stream,
                "400 Bad Request",
                "text/plain; charset=utf-8",
                "Malformed HTTP header",
            )?;
            latency_probe.mark_ok();
            return Ok(());
        };
        let raw_name = name.trim();
        if !http_boundary::is_valid_http_header_name(raw_name) {
            latency_probe.mark_route("http.invalid_header_name");
            write_text_response(
                &mut stream,
                "400 Bad Request",
                "text/plain; charset=utf-8",
                "Invalid HTTP header name",
            )?;
            latency_probe.mark_ok();
            return Ok(());
        }
        let key = raw_name.to_ascii_lowercase();
        let trimmed = value.trim().to_string();
        if !http_boundary::is_valid_http_header_value(&trimmed) {
            latency_probe.mark_route("http.invalid_header_value");
            write_text_response(
                &mut stream,
                "400 Bad Request",
                "text/plain; charset=utf-8",
                "Invalid HTTP header value",
            )?;
            latency_probe.mark_ok();
            return Ok(());
        }
        if key == "transfer-encoding" {
            latency_probe.mark_route("http.transfer_encoding_rejected");
            write_text_response(
                &mut stream,
                "400 Bad Request",
                "text/plain; charset=utf-8",
                "Transfer-Encoding is not supported",
            )?;
            latency_probe.mark_ok();
            return Ok(());
        }
        if key == "content-length" {
            let parsed_length = match trimmed.parse::<usize>() {
                Ok(value) => value,
                Err(_) => {
                    latency_probe.mark_route("http.invalid_content_length");
                    write_text_response(
                        &mut stream,
                        "400 Bad Request",
                        "text/plain; charset=utf-8",
                        "Invalid Content-Length",
                    )?;
                    latency_probe.mark_ok();
                    return Ok(());
                }
            };
            if content_length.is_some() {
                latency_probe.mark_route("http.duplicate_content_length");
                write_text_response(
                    &mut stream,
                    "400 Bad Request",
                    "text/plain; charset=utf-8",
                    "Duplicate Content-Length is not allowed",
                )?;
                latency_probe.mark_ok();
                return Ok(());
            }
            content_length = Some(parsed_length);
        }
        headers.push((key, trimmed));
    }
    let parse_duration = request_started_at.elapsed();
    latency_probe.set_parse_complete(parse_duration, header_bytes);

    let host_header_count = headers.iter().filter(|(key, _)| key == "host").count();
    if host_header_count != 1 {
        latency_probe.mark_route(if host_header_count == 0 {
            "http.missing_host"
        } else {
            "http.duplicate_host"
        });
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
        latency_probe.mark_ok();
        return Ok(());
    }

    let content_length = content_length.unwrap_or(0);
    let (path, query) = split_target(&target);
    latency_probe.set_parsed_target(&method, path, query);
    if content_length > config.max_body_bytes {
        let dispatch_started_at = Instant::now();
        write_text_response(
            &mut stream,
            "413 Payload Too Large",
            "text/plain; charset=utf-8",
            "HTTP request body is too large",
        )?;
        latency_probe.set_body(content_length, Duration::from_millis(0));
        latency_probe.set_dispatch(dispatch_started_at.elapsed(), true);
        latency_probe.mark_ok();
        return Ok(());
    }

    // Validate the local browser boundary, rate-limit every peer attempt, and
    // then authenticate after bounded header parsing, before consuming a
    // potentially slow request body. The absolute request deadline still
    // applies to every read that follows.
    let header_request = HttpRequest {
        method: method.clone(),
        path: path.to_string(),
        query: query.to_string(),
        headers: headers.clone(),
        body: Vec::new(),
    };
    if reject_forbidden_origin(&mut stream, &header_request)? {
        latency_probe.mark_route("http.forbidden_origin");
        latency_probe.mark_ok();
        return Ok(());
    }
    if let Some((_client_key, retry_after)) = check_http_rate_limit(&stream, config) {
        latency_probe.mark_route("http.rate_limited");
        let retry_after_secs = retry_after.as_secs().max(1).to_string();
        write_empty_response_with_headers(
            &mut stream,
            "429 Too Many Requests",
            &[("Retry-After", retry_after_secs.as_str())],
        )?;
        latency_probe.mark_ok();
        return Ok(());
    }
    if !is_authorized_http_request(&header_request, config) {
        latency_probe.mark_route("http.unauthorized");
        write_empty_response_with_headers(
            &mut stream,
            "401 Unauthorized",
            &[("WWW-Authenticate", "Bearer realm=\"mcpace\"")],
        )?;
        latency_probe.mark_ok();
        return Ok(());
    }

    let _resource_permit = match config.resource_governor.try_enter_request() {
        Ok(permit) => permit,
        Err(rejection) => {
            latency_probe.mark_route("http.resource_governor_rejected");
            let retry_after_secs = (rejection.retry_after_ms / 1000).max(1).to_string();
            record_operation_trace(
                config,
                OperationTraceObservation {
                    name: "http.resource_governor_rejected".to_string(),
                    route: "http.resource_governor_rejected".to_string(),
                    duration: Duration::from_millis(0),
                    failed: true,
                    attributes: vec![("reason".to_string(), rejection.reason.to_string())],
                },
            );
            write_empty_response_with_headers(
                &mut stream,
                "503 Service Unavailable",
                &[
                    ("Retry-After", retry_after_secs.as_str()),
                    ("X-MCPace-Resource-Rejection", rejection.reason),
                ],
            )?;
            latency_probe.mark_ok();
            return Ok(());
        }
    };

    let body_read_started_at = Instant::now();
    let body = read_limited_http_body(&mut reader, content_length, request_deadline)?;
    let body_read_duration = body_read_started_at.elapsed();
    latency_probe.set_body(content_length, body_read_duration);

    let request = HttpRequest {
        method: method.clone(),
        path: path.to_string(),
        query: query.to_string(),
        headers,
        body,
    };

    let dispatch_started_at = Instant::now();
    let result = handle_http_request(&mut stream, &request, config);
    let dispatch_duration = dispatch_started_at.elapsed();
    let failed = result.is_err();
    latency_probe.set_dispatch(dispatch_duration, failed);
    if !failed {
        latency_probe.mark_ok();
    }

    if let Err(error) = result {
        write_json_error_response(
            &mut stream,
            "500 Internal Server Error",
            "internal_error",
            "Request failed; see local MCPace diagnostics.",
        )
        .map_err(|response_error| {
            format!(
                "{}; failed to write HTTP error response: {}",
                error, response_error
            )
        })?;
        let _ = stream.shutdown(Shutdown::Write);
        return Err(error);
    }

    let _ = stream.shutdown(Shutdown::Write);

    Ok(())
}

fn handle_http_request(
    stream: &mut TcpStream,
    request: &HttpRequest,
    config: &DashboardConfig,
) -> Result<(), String> {
    if reject_forbidden_origin(stream, request)? {
        return Ok(());
    }
    if !is_authorized_http_request(request, config) {
        write_empty_response_with_headers(
            stream,
            "401 Unauthorized",
            &[("WWW-Authenticate", "Bearer realm=\"mcpace\"")],
        )?;
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
        ("GET", "/dashboard.css") => {
            write_text_response(stream, "200 OK", "text/css; charset=utf-8", DASHBOARD_CSS)?
        }
        ("GET", "/dashboard.product.css") => write_text_response(
            stream,
            "200 OK",
            "text/css; charset=utf-8",
            DASHBOARD_PRODUCT_CSS,
        )?,
        ("GET", "/dashboard.js") => write_text_response(
            stream,
            "200 OK",
            "application/javascript; charset=utf-8",
            DASHBOARD_JS,
        )?,
        ("GET", "/dashboard.runtime.js") => write_text_response(
            stream,
            "200 OK",
            "application/javascript; charset=utf-8",
            DASHBOARD_RUNTIME_JS,
        )?,
        ("GET", "/dashboard.model.js") => write_text_response(
            stream,
            "200 OK",
            "application/javascript; charset=utf-8",
            DASHBOARD_MODEL_JS,
        )?,
        ("GET", "/dashboard.render.js") => write_text_response(
            stream,
            "200 OK",
            "application/javascript; charset=utf-8",
            DASHBOARD_RENDER_JS,
        )?,
        ("GET", "/dashboard.render.details.js") => write_text_response(
            stream,
            "200 OK",
            "application/javascript; charset=utf-8",
            DASHBOARD_RENDER_DETAILS_JS,
        )?,
        ("GET", "/dashboard.actions.js") => write_text_response(
            stream,
            "200 OK",
            "application/javascript; charset=utf-8",
            DASHBOARD_ACTIONS_JS,
        )?,
        ("GET", "/dashboard.boot.js") => write_text_response(
            stream,
            "200 OK",
            "application/javascript; charset=utf-8",
            DASHBOARD_BOOT_JS,
        )?,
        ("GET", "/dashboard.product.js") => write_text_response(
            stream,
            "200 OK",
            "application/javascript; charset=utf-8",
            DASHBOARD_PRODUCT_JS,
        )?,
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
            let _permit = if refresh {
                let Some(permit) = config
                    .admission
                    .try_enter(HttpAdmissionKind::OverviewRefresh)
                else {
                    write_admission_rejected(stream, "overview-refresh")?;
                    return Ok(());
                };
                Some(permit)
            } else {
                None
            };
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
        ("GET", "/api/operations") => {
            let limit = bounded_query_usize(
                &request.query,
                "limit",
                DEFAULT_OPERATIONS_LIMIT,
                MAX_OPERATIONS_LIMIT,
            );
            let payload = operations::retained_operations_response(&config.root_path, limit);
            write_json_response(stream, "200 OK", &payload)?;
        }
        ("POST", "/api/actions/hub-up") => {
            let Some(_permit) = enter_http_heavy_action_or_write_busy(stream, config, "hub-up")?
            else {
                return Ok(());
            };
            let payload = action_response(
                "hub-up",
                run_json_command(&config.root_path, &["hub", "up", "--json"])?,
            );
            write_json_response(stream, "200 OK", &payload)?;
        }
        ("POST", "/api/actions/hub-down") => {
            let Some(_permit) = enter_http_heavy_action_or_write_busy(stream, config, "hub-down")?
            else {
                return Ok(());
            };
            let payload = action_response(
                "hub-down",
                run_json_command(&config.root_path, &["hub", "down", "--json"])?,
            );
            write_json_response(stream, "200 OK", &payload)?;
        }
        ("POST", "/api/actions/repair") => {
            let Some(_permit) = enter_http_heavy_action_or_write_busy(stream, config, "repair")?
            else {
                return Ok(());
            };
            let payload = action_response(
                "repair",
                run_json_command(&config.root_path, &["repair", "--json"])?,
            );
            write_json_response(stream, "200 OK", &payload)?;
        }
        ("POST", "/api/actions/update-check") => {
            let Some(_permit) =
                enter_http_heavy_action_or_write_busy(stream, config, "update-check")?
            else {
                return Ok(());
            };
            let payload =
                action_response("update-check", crate::update::dashboard_update_check_json());
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
        ("POST", "/api/actions/server-remove") => {
            write_server_remove_action(stream, request, config)?;
        }
        ("POST", "/api/actions/server-discover") => {
            write_server_discover_action(stream, request, config)?;
        }
        ("POST", "/api/actions/server-import-config") => {
            write_server_import_config_action(stream, request, config)?;
        }
        ("POST", "/api/actions/server-install-command") => {
            write_server_install_command_action(stream, request, config)?;
        }
        ("POST", "/api/actions/client-install") => {
            write_client_install_action(stream, request, config)?;
        }
        ("POST", "/api/actions/client-restore") => {
            write_client_restore_action(stream, request, config)?;
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

fn write_client_install_action(
    stream: &mut TcpStream,
    request: &HttpRequest,
    config: &DashboardConfig,
) -> Result<(), String> {
    let body = match parse_action_body(request) {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    };
    let client_id = match action_client_target_id(&body) {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    };
    let mut args = vec![
        "client".to_string(),
        "install".to_string(),
        client_id,
        "--json".to_string(),
    ];
    match action_bool(&body, "dryRun") {
        Ok(true) => args.push("--dry-run".to_string()),
        Ok(false) => {}
        Err(error) => return write_bad_action_request(stream, &error),
    }
    match action_bool(&body, "diff") {
        Ok(true) => args.push("--diff".to_string()),
        Ok(false) => {}
        Err(error) => return write_bad_action_request(stream, &error),
    }

    let Some(_permit) = enter_http_heavy_action_or_write_busy(stream, config, "client-install")?
    else {
        return Ok(());
    };
    let payload = action_response(
        "client-install",
        run_json_command_vec(&config.root_path, args)?,
    );
    Ok(write_json_response(stream, "200 OK", &payload)?)
}

fn write_client_restore_action(
    stream: &mut TcpStream,
    request: &HttpRequest,
    config: &DashboardConfig,
) -> Result<(), String> {
    let body = match parse_action_body(request) {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    };
    let client_id = match action_client_target_id(&body) {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    };
    let backup = match optional_action_string(&body, "backup") {
        Ok(Some(value)) => value,
        Ok(None) => "latest".to_string(),
        Err(error) => return write_bad_action_request(stream, &error),
    };
    if let Err(error) = validate_action_token_field("backup", &backup, true) {
        return write_bad_action_request(stream, &error);
    }
    let args = vec![
        "client".to_string(),
        "restore".to_string(),
        client_id,
        "--backup".to_string(),
        backup,
        "--json".to_string(),
    ];

    let Some(_permit) = enter_http_heavy_action_or_write_busy(stream, config, "client-restore")?
    else {
        return Ok(());
    };
    let payload = action_response(
        "client-restore",
        run_json_command_vec(&config.root_path, args)?,
    );
    Ok(write_json_response(stream, "200 OK", &payload)?)
}

fn action_client_target_id(body: &JsonValue) -> Result<String, String> {
    let client_id = action_string(body, "clientId")
        .or_else(|_| action_string(body, "client"))
        .map_err(|_| "client action requires a non-empty clientId field".to_string())?;
    validate_action_token_field("clientId", &client_id, true)?;
    Ok(client_id)
}

fn validate_action_token_field(label: &str, value: &str, allow_all: bool) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("{} cannot be empty", label));
    }
    if value.len() > 96 {
        return Err(format!("{} is too long", label));
    }
    if allow_all && value == "all" {
        return Ok(());
    }
    if value
        .chars()
        .any(|ch| ch == '\0' || ch == '\r' || ch == '\n' || ch.is_control())
    {
        return Err(format!(
            "{} cannot contain control characters or newlines",
            label
        ));
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.')
    {
        return Err(format!(
            "{} must contain only letters, numbers, dash, underscore, or dot",
            label
        ));
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
    let Some(_permit) = enter_http_heavy_action_or_write_busy(stream, config, action)? else {
        return Ok(());
    };
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
    Ok(write_json_response(stream, "200 OK", &payload)?)
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

    let Some(_permit) = enter_http_heavy_action_or_write_busy(stream, config, "server-policy")?
    else {
        return Ok(());
    };
    let payload = action_response(
        "server-policy",
        run_json_command_vec(&config.root_path, args)?,
    );
    Ok(write_json_response(stream, "200 OK", &payload)?)
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

    let Some(_permit) = enter_http_heavy_action_or_write_busy(stream, config, "server-autotune")?
    else {
        return Ok(());
    };

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
    Ok(write_json_response(stream, "200 OK", &payload)?)
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

    let Some(_permit) = enter_http_heavy_action_or_write_busy(stream, config, "server-test")?
    else {
        return Ok(());
    };
    let payload = action_response(
        "server-test",
        run_json_command_vec(&config.root_path, args)?,
    );
    Ok(write_json_response(stream, "200 OK", &payload)?)
}

fn write_server_remove_action(
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
    let dry_run = match action_bool(&body, "dryRun") {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    };

    let mut args = vec![
        "server".to_string(),
        "remove".to_string(),
        server,
        "--json".to_string(),
    ];
    match optional_action_string(&body, "settingsPath") {
        Ok(Some(settings_path)) => {
            if let Err(error) = validate_action_path_field("settingsPath", &settings_path) {
                return write_bad_action_request(stream, &error);
            }
            args.push("--settings".to_string());
            args.push(settings_path);
        }
        Ok(None) => {}
        Err(error) => return write_bad_action_request(stream, &error),
    }
    if dry_run {
        args.push("--dry-run".to_string());
    }

    let Some(_permit) = enter_http_heavy_action_or_write_busy(stream, config, "server-remove")?
    else {
        return Ok(());
    };
    let payload = action_response(
        "server-remove",
        run_json_command_vec(&config.root_path, args)?,
    );
    Ok(write_json_response(stream, "200 OK", &payload)?)
}

fn write_server_import_config_action(
    stream: &mut TcpStream,
    request: &HttpRequest,
    config: &DashboardConfig,
) -> Result<(), String> {
    let body = match parse_action_body(request) {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    };
    let source_path =
        match action_string(&body, "sourcePath").or_else(|_| action_string(&body, "from")) {
            Ok(value) => value,
            Err(_) => {
                return write_bad_action_request(
                    stream,
                    "server import requires a non-empty sourcePath field",
                );
            }
        };
    if let Err(error) = validate_action_path_field("sourcePath", &source_path) {
        return write_bad_action_request(stream, &error);
    }

    let mut args = vec![
        "server".to_string(),
        "import".to_string(),
        "--from".to_string(),
        source_path,
        "--json".to_string(),
    ];
    match optional_action_string(&body, "settingsPath") {
        Ok(Some(settings_path)) => {
            if let Err(error) = validate_action_path_field("settingsPath", &settings_path) {
                return write_bad_action_request(stream, &error);
            }
            args.push("--settings".to_string());
            args.push(settings_path);
        }
        Ok(None) => {}
        Err(error) => return write_bad_action_request(stream, &error),
    }
    match action_bool(&body, "dryRun") {
        Ok(true) => args.push("--dry-run".to_string()),
        Ok(false) => {}
        Err(error) => return write_bad_action_request(stream, &error),
    }
    match action_bool(&body, "force") {
        Ok(true) => args.push("--force".to_string()),
        Ok(false) => {}
        Err(error) => return write_bad_action_request(stream, &error),
    }
    match action_bool(&body, "disabled") {
        Ok(true) => args.push("--disabled".to_string()),
        Ok(false) => {}
        Err(error) => return write_bad_action_request(stream, &error),
    }

    let Some(_permit) =
        enter_http_heavy_action_or_write_busy(stream, config, "server-import-config")?
    else {
        return Ok(());
    };
    let payload = action_response(
        "server-import-config",
        run_json_command_vec(&config.root_path, args)?,
    );
    Ok(write_json_response(stream, "200 OK", &payload)?)
}

fn validate_action_path_field(label: &str, value: &str) -> Result<(), String> {
    if value.len() > 2048 {
        return Err(format!("{} is too long", label));
    }
    if value
        .chars()
        .any(|ch| ch == '\0' || ch == '\r' || ch == '\n' || ch.is_control())
    {
        return Err(format!(
            "{} cannot contain control characters or newlines",
            label
        ));
    }
    let trimmed = value.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("://") {
        return Err(format!("{} must be a local file path, not a URL", label));
    }
    if trimmed.starts_with("//") || trimmed.starts_with("\\\\") {
        return Err(format!("{} cannot use a UNC or device-network path", label));
    }
    let bytes = trimmed.as_bytes();
    if bytes.get(1) == Some(&b':') {
        if !matches!(bytes.get(2), Some(b'/') | Some(b'\\')) {
            return Err(format!("{} cannot use a drive-relative path", label));
        }
        if trimmed[2..].contains(':') {
            return Err(format!("{} cannot use an alternate data stream", label));
        }
    } else if trimmed.contains(':') {
        return Err(format!("{} contains an unsupported colon", label));
    }
    for component in trimmed.split(['/', '\\']) {
        if component == ".." {
            return Err(format!("{} cannot contain parent traversal", label));
        }
        let device_name = component
            .trim_end_matches([' ', '.'])
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        let device_bytes = device_name.as_bytes();
        let numbered_device = device_bytes.len() == 4
            && (&device_bytes[..3] == b"COM" || &device_bytes[..3] == b"LPT")
            && matches!(device_bytes[3], b'1'..=b'9');
        if matches!(device_name.as_str(), "CON" | "PRN" | "AUX" | "NUL") || numbered_device {
            return Err(format!("{} contains a reserved device name", label));
        }
    }
    Ok(())
}

fn write_server_discover_action(
    stream: &mut TcpStream,
    request: &HttpRequest,
    config: &DashboardConfig,
) -> Result<(), String> {
    let body = match parse_action_body(request) {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    };
    let query = match optional_action_string(&body, "query") {
        Ok(value) => value.unwrap_or_default(),
        Err(error) => return write_bad_action_request(stream, &error),
    };
    if query.len() > 256 {
        return write_bad_action_request(stream, "server discovery query is too long");
    }
    if query
        .chars()
        .any(|ch| ch == '\0' || ch == '\r' || ch == '\n' || ch.is_control())
    {
        return write_bad_action_request(
            stream,
            "server discovery query cannot contain control characters or newlines",
        );
    }
    let mode = optional_action_string(&body, "mode")
        .map(|value| value.unwrap_or_else(|| "preview".to_string()))
        .unwrap_or_else(|_| "preview".to_string())
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-");
    if !matches!(
        mode.as_str(),
        "preview" | "install" | "apply" | "auto-install" | "auto" | "auto-mode"
    ) {
        return write_bad_action_request(
            stream,
            "server discovery mode must be preview, install, apply, auto-install, auto, or auto-mode",
        );
    }
    let mode_apply = matches!(mode.as_str(), "install" | "apply" | "auto-install");
    let mode_auto = matches!(mode.as_str(), "auto" | "auto-mode");
    let auto_install = match action_bool(&body, "autoInstall") {
        Ok(value) => value || mode_apply,
        Err(error) => return write_bad_action_request(stream, &error),
    };
    let auto_mode = match action_bool(&body, "autoMode") {
        Ok(value) => value || mode_auto,
        Err(error) => return write_bad_action_request(stream, &error),
    };
    if query.trim().is_empty() && (auto_install || auto_mode) {
        return write_bad_action_request(
            stream,
            "dashboard server discovery install mode requires a query to avoid broad automatic sweeps",
        );
    }

    let mut args = vec!["server".to_string(), "discover".to_string()];
    if !query.trim().is_empty() {
        args.push(query);
    }
    args.push("--json".to_string());
    if auto_mode {
        args.push("--auto".to_string());
    } else if auto_install {
        args.push("--auto-install".to_string());
    }
    let allow_review = match action_bool(&body, "allowReview") {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    };
    let allow_review_install = match action_bool(&body, "allowReviewInstall") {
        Ok(value) => value,
        Err(error) => return write_bad_action_request(stream, &error),
    };
    if allow_review || allow_review_install {
        args.push("--allow-review".to_string());
    }
    match action_bool(&body, "refresh") {
        Ok(true) => args.push("--refresh".to_string()),
        Ok(false) => {}
        Err(error) => return write_bad_action_request(stream, &error),
    }
    match action_bool(&body, "force") {
        Ok(true) => args.push("--force".to_string()),
        Ok(false) => {}
        Err(error) => return write_bad_action_request(stream, &error),
    }
    match action_bool(&body, "disabled") {
        Ok(true) => args.push("--disabled".to_string()),
        Ok(false) => {}
        Err(error) => return write_bad_action_request(stream, &error),
    }
    match action_bool(&body, "dryRun") {
        Ok(true) => args.push("--dry-run".to_string()),
        Ok(false) => {}
        Err(error) => return write_bad_action_request(stream, &error),
    }

    let Some(_permit) = enter_http_heavy_action_or_write_busy(stream, config, "server-discover")?
    else {
        return Ok(());
    };
    let payload = action_response(
        "server-discover",
        run_json_command_vec(&config.root_path, args)?,
    );
    Ok(write_json_response(stream, "200 OK", &payload)?)
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
        validate_action_name_field("server", &server, 256)?;
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

    let Some(_permit) =
        enter_http_heavy_action_or_write_busy(stream, config, "server-install-command")?
    else {
        return Ok(());
    };
    let payload = action_response(
        "server-install-command",
        run_json_command_vec(&config.root_path, args)?,
    );
    Ok(write_json_response(stream, "200 OK", &payload)?)
}

fn command_line_uses_shell_composition(value: &str) -> bool {
    text_utils::uses_shell_composition(value)
}

fn write_admission_rejected(stream: &mut TcpStream, action: &str) -> Result<(), String> {
    let payload = JsonValue::object([
        ("ok", JsonValue::bool(false)),
        (
            "error",
            JsonValue::string("server is busy; retry this operation shortly"),
        ),
        ("action", JsonValue::string(action)),
        ("retryAfterMs", JsonValue::number(1000usize)),
    ]);
    Ok(write_json_response_with_owned_headers(
        stream,
        "429 Too Many Requests",
        &payload,
        &[("Retry-After".to_string(), "1".to_string())],
    )?)
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
    let server = action_string(body, "server")
        .or_else(|_| action_string(body, "name"))
        .map_err(|_| "server action requires a non-empty server name".to_string())?;
    validate_action_name_field("server", &server, 256)?;
    Ok(server)
}

fn validate_action_name_field(label: &str, value: &str, max_len: usize) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("{} cannot be empty", label));
    }
    if value.len() > max_len {
        return Err(format!("{} is too long", label));
    }
    if value.starts_with('-') {
        return Err(format!("{} cannot start with '-'", label));
    }
    if value
        .chars()
        .any(|ch| ch == '\0' || ch == '\r' || ch == '\n' || ch.is_control())
    {
        return Err(format!(
            "{} cannot contain control characters or newlines",
            label
        ));
    }
    Ok(())
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
    push_reuse_policy_arg(&mut args, body)?;
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

fn push_reuse_policy_arg(args: &mut Vec<String>, body: &JsonValue) -> Result<(), String> {
    let Some(value) = body.get("reusePolicy") else {
        return Ok(());
    };
    let raw = value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "'reusePolicy' must be a non-empty string".to_string())?;
    let normalized = raw.to_ascii_lowercase().replace('_', "-");
    let canonical = match normalized.as_str() {
        "sticky" | "ttl" | "never" => normalized,
        _ => return Err("'reusePolicy' must be one of sticky, ttl, or never".to_string()),
    };
    args.push("--reuse-policy".to_string());
    args.push(canonical);
    Ok(())
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
    if affinity.len() > 8 {
        return Err("'affinity' accepts at most 8 entries".to_string());
    }
    for item in &affinity {
        validate_action_token_field("affinity", item, false)?;
    }
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

fn latency_route_label(path: &str, query: &str, config: &DashboardConfig) -> String {
    let (mcp_path, health_path) = configured_http_paths(config);
    let refresh_requested = query_bool_flag(query, "refresh") || query_bool_flag(query, "noCache");
    if matches_configured_path(path, &health_path, runtimepaths::DEFAULT_LOCAL_HEALTH_PATH) {
        return if refresh_requested {
            "health.refresh".to_string()
        } else {
            "health".to_string()
        };
    }
    if matches_configured_path(path, &mcp_path, runtimepaths::DEFAULT_LOCAL_MCP_PATH) {
        return "mcp".to_string();
    }
    match path {
        "/" => "dashboard.index".to_string(),
        "/dashboard.css" => "dashboard.css".to_string(),
        "/dashboard.product.css" => "dashboard.product.css".to_string(),
        "/dashboard.js" => "dashboard.js".to_string(),
        "/dashboard.runtime.js" => "dashboard.runtime.js".to_string(),
        "/dashboard.model.js" => "dashboard.model.js".to_string(),
        "/dashboard.render.js" => "dashboard.render.js".to_string(),
        "/dashboard.render.details.js" => "dashboard.render.details.js".to_string(),
        "/dashboard.actions.js" => "dashboard.actions.js".to_string(),
        "/dashboard.boot.js" => "dashboard.boot.js".to_string(),
        "/dashboard.product.js" => "dashboard.product.js".to_string(),
        "/favicon.ico" => "dashboard.favicon".to_string(),
        "/status" => "status".to_string(),
        "/api/overview" => {
            if refresh_requested {
                "api.overview.refresh".to_string()
            } else {
                "api.overview.cached".to_string()
            }
        }
        "/api/resources" => "api.resources".to_string(),
        "/api/logs" => "api.logs".to_string(),
        "/api/operations" => "api.operations".to_string(),
        _ if path.starts_with("/api/actions/") => "api.actions".to_string(),
        _ if path.starts_with("/api/") => "api.other".to_string(),
        _ => "other".to_string(),
    }
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
    let Err(error) = http_boundary::validate_origin_for_bind(request) else {
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
    let exit_code = app::run_internal(args, &mut stdout_buffer, &mut stderr_buffer);
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
const DASHBOARD_CSS: &str = include_str!("dashboard/frontend/styles.css");
const DASHBOARD_PRODUCT_CSS: &str = include_str!("dashboard/frontend/product.css");
const DASHBOARD_JS: &str = include_str!("dashboard/frontend/app.js");
const DASHBOARD_RUNTIME_JS: &str = include_str!("dashboard/frontend/app.runtime.js");
const DASHBOARD_MODEL_JS: &str = include_str!("dashboard/frontend/app.model.js");
const DASHBOARD_RENDER_JS: &str = include_str!("dashboard/frontend/app.render.js");
const DASHBOARD_RENDER_DETAILS_JS: &str = include_str!("dashboard/frontend/app.render.details.js");
const DASHBOARD_ACTIONS_JS: &str = include_str!("dashboard/frontend/app.actions.js");
const DASHBOARD_BOOT_JS: &str = include_str!("dashboard/frontend/app.boot.js");
const DASHBOARD_PRODUCT_JS: &str = include_str!("dashboard/frontend/product.js");
const DASHBOARD_FAVICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" rx="14" fill="#111827"/><path d="M16 42V22h9l7 10 7-10h9v20h-8V30l-8 11-8-11v12h-8Z" fill="#7dd3fc"/><circle cx="51" cy="13" r="5" fill="#34d399"/></svg>"##;

#[cfg(test)]
mod tests;
