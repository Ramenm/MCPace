use crate::app;
use crate::json::{parse_str, JsonValue};
use crate::resources;
use crate::runtimepaths;
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
use self::http_tools::{http_tool_definitions, http_tool_definitions_for_request, http_tool_names};
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
use std::net::{IpAddr, Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

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
}

#[derive(Clone, Debug)]
struct HttpRuntimeMetricsSnapshot {
    accepted_connections: usize,
    active_connections: usize,
    completed_connections: usize,
    failed_connections: usize,
    max_active_connections: usize,
}

struct HttpRuntimeMetricsGuard<'a> {
    metrics: &'a HttpRuntimeMetrics,
    failed: bool,
}

impl HttpRuntimeMetrics {
    fn begin(&self) -> HttpRuntimeMetricsGuard<'_> {
        self.accepted_connections.fetch_add(1, Ordering::Relaxed);
        let active = self.active_connections.fetch_add(1, Ordering::Relaxed) + 1;
        self.record_max_active(active);
        HttpRuntimeMetricsGuard {
            metrics: self,
            failed: false,
        }
    }

    fn snapshot(&self) -> HttpRuntimeMetricsSnapshot {
        HttpRuntimeMetricsSnapshot {
            accepted_connections: self.accepted_connections.load(Ordering::Relaxed),
            active_connections: self.active_connections.load(Ordering::Relaxed),
            completed_connections: self.completed_connections.load(Ordering::Relaxed),
            failed_connections: self.failed_connections.load(Ordering::Relaxed),
            max_active_connections: self.max_active_connections.load(Ordering::Relaxed),
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
}

impl HttpRuntimeMetricsGuard<'_> {
    fn mark_failed(&mut self) {
        self.failed = true;
    }
}

impl Drop for HttpRuntimeMetricsGuard<'_> {
    fn drop(&mut self) {
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
    if !parsed.allow_nonlocal_bind && !is_loopback_bind_host(&host) {
        let _ = writeln!(
            stderr,
            "refusing to bind non-loopback host '{}'; MCPace local HTTP mode is loopback-only unless --allow-nonlocal-bind is set intentionally",
            host
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
        },
        stderr,
    )
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
    let trimmed = host.trim();
    if trimmed.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let normalized = trimmed
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(trimmed);
    normalized
        .parse::<IpAddr>()
        .map(|address| address.is_loopback())
        .unwrap_or(false)
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
        "Usage: mcpace dashboard [--root <path>] [--host <addr>] [--port <n>] [--max-requests <n>] [--max-connections <n>] [--io-timeout-ms <n>] [--max-body-bytes <n>] [--overview-cache-ms <n>] [--allow-nonlocal-bind]"
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
        "Non-loopback bind hosts are rejected by default; --allow-nonlocal-bind is an explicit operator escape hatch, not a public auth mode."
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
        handles.push(thread::spawn(move || loop {
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
        }));
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
    if parts.len() < 2 {
        write_text_response(
            &mut stream,
            "400 Bad Request",
            "text/plain; charset=utf-8",
            "Malformed HTTP request",
        )?;
        return Ok(());
    }
    let method = parts[0].to_string();
    let target = parts[1].to_string();

    let mut headers = Vec::new();
    let mut content_length = 0usize;
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
        if let Some((name, value)) = header.split_once(':') {
            let key = name.trim().to_ascii_lowercase();
            let trimmed = value.trim().to_string();
            if key == "content-length" {
                content_length = match trimmed.parse::<usize>() {
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
            }
            headers.push((key, trimmed));
        }
    }

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
            let tail = query_parameter(&request.query, "tail")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(20);
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
            if reject_forbidden_origin(stream, request)? {
                return Ok(());
            }
            let payload = action_response(
                "hub-up",
                run_json_command(&config.root_path, &["hub", "up", "--json"])?,
            );
            write_json_response(stream, "200 OK", &payload)?;
        }
        ("POST", "/api/actions/hub-down") => {
            if reject_forbidden_origin(stream, request)? {
                return Ok(());
            }
            let payload = action_response(
                "hub-down",
                run_json_command(&config.root_path, &["hub", "down", "--json"])?,
            );
            write_json_response(stream, "200 OK", &payload)?;
        }
        ("POST", "/api/actions/repair") => {
            if reject_forbidden_origin(stream, request)? {
                return Ok(());
            }
            let payload = action_response(
                "repair",
                run_json_command(&config.root_path, &["repair", "--json"])?,
            );
            write_json_response(stream, "200 OK", &payload)?;
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

#[cfg(test)]
mod tests;
