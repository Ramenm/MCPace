use crate::adapter;
use crate::app;
use crate::json::{parse_str, JsonValue};
use crate::json_helpers;
use crate::mcp_protocol as mcp;
use crate::resources;
use crate::runtimepaths;
use crate::tool_result::{self, ToolResultOptions};
use crate::upstream;
use std::collections::{hash_map::DefaultHasher, BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug)]
struct ParsedArgs {
    help: bool,
    root_override: Option<PathBuf>,
    host: String,
    port: u16,
    max_requests: Option<usize>,
    max_connections: usize,
    io_timeout: Duration,
    max_body_bytes: usize,
    overview_cache_ttl: Duration,
    error: Option<String>,
}

impl Default for ParsedArgs {
    fn default() -> Self {
        Self {
            help: false,
            root_override: None,
            host: runtimepaths::DEFAULT_LOCAL_HOST.to_string(),
            port: 0,
            max_requests: None,
            max_connections: resources::default_http_connection_limit(),
            io_timeout: resources::default_http_io_timeout(),
            max_body_bytes: resources::DEFAULT_MAX_HTTP_BODY_BYTES,
            overview_cache_ttl: resources::default_dashboard_overview_cache_ttl(),
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
    let mut parsed = parse_args(args);
    if let Some(error) = parsed.error {
        let _ = writeln!(stderr, "{}", error);
        return 2;
    }
    if parsed.help {
        write_help(stdout);
        return 0;
    }
    if matches!(surface, ServeSurface::UnifiedServe) && parsed.port == 0 {
        parsed.port = runtimepaths::DEFAULT_LOCAL_MCP_PORT;
    }

    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };

    let listener = match TcpListener::bind((parsed.host.as_str(), parsed.port)) {
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

    let _ = match surface {
        ServeSurface::Dashboard => writeln!(stdout, "Dashboard running at http://{}", address),
        ServeSurface::UnifiedServe => writeln!(
            stdout,
            "Server running at http://{} (UI: /, MCP: /mcp, health: /healthz)",
            address
        ),
    };
    let _ = stdout.flush();

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
            metrics: HttpRuntimeMetrics::default(),
            surface,
            upstream_session_pools: new_upstream_session_pools(),
        },
        stderr,
    )
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
                parsed.host = value.to_string();
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
        "Usage: mcpace dashboard [--root <path>] [--host <addr>] [--port <n>] [--max-requests <n>] [--max-connections <n>] [--io-timeout-ms <n>] [--max-body-bytes <n>] [--overview-cache-ms <n>]"
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
        "Resource defaults: max connections={}, IO timeout={}ms, max body={} bytes, overview cache={}ms, health cache={}ms.",
        resources::default_http_connection_limit(),
        resources::DEFAULT_HTTP_IO_TIMEOUT_MS,
        resources::DEFAULT_MAX_HTTP_BODY_BYTES,
        resources::DEFAULT_DASHBOARD_OVERVIEW_CACHE_MS,
        resources::DEFAULT_DASHBOARD_HEALTH_CACHE_MS
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
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => {
            write_text_response(stream, "200 OK", "text/html; charset=utf-8", DASHBOARD_HTML)?
        }
        ("GET", "/healthz") => {
            let refresh = query_bool_flag(&request.query, "refresh")
                || query_bool_flag(&request.query, "noCache");
            let payload = cached_health_json(config, refresh)?;
            write_json_response(stream, "200 OK", &payload)?;
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
        ("GET", "/mcp") => {
            if accepts(request, "text/event-stream") {
                write_empty_response(stream, "405 Method Not Allowed")?;
                return Ok(());
            }
            let payload = JsonValue::object([
                ("ok", JsonValue::bool(true)),
                (
                    "surface",
                    JsonValue::string(match config.surface {
                        ServeSurface::Dashboard => "dashboard-http",
                        ServeSurface::UnifiedServe => "unified-serve-http",
                    }),
                ),
                (
                    "message",
                    JsonValue::string(
                        "Use HTTP POST with a single JSON-RPC request body at this endpoint.",
                    ),
                ),
            ]);
            write_json_response(stream, "200 OK", &payload)?;
        }
        ("POST", "/mcp") => {
            let response = match handle_mcp_http_request(request, config) {
                Ok(value) => value,
                Err(error) => McpHttpResponse::JsonStatus(
                    "400 Bad Request",
                    mcp_error_response(JsonValue::Null, -32700, error),
                ),
            };
            match response {
                McpHttpResponse::Json(payload) => write_json_response(stream, "200 OK", &payload)?,
                McpHttpResponse::JsonStatus(status, payload) => {
                    write_json_response(stream, status, &payload)?
                }
                McpHttpResponse::Accepted => write_empty_response(stream, "202 Accepted")?,
            }
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

fn write_json_error_response(
    stream: &mut TcpStream,
    status: &str,
    code: &str,
    message: &str,
) -> Result<(), String> {
    write_json_response(
        stream,
        status,
        &JsonValue::object([
            ("ok", JsonValue::bool(false)),
            ("generatedAtMs", JsonValue::number(now_ms())),
            (
                "error",
                JsonValue::object([
                    ("code", JsonValue::string(code)),
                    ("message", JsonValue::string(message)),
                ]),
            ),
        ]),
    )
}

fn cached_health_json(config: &DashboardConfig, refresh: bool) -> Result<JsonValue, String> {
    if config.health_cache_ttl.is_zero() {
        return build_health_json(config).map(|value| {
            with_runtime_cache_metadata(
                value,
                config,
                CacheMetadata {
                    hit: false,
                    bypassed: true,
                    stale: false,
                    ttl: config.health_cache_ttl,
                    age: None,
                    refresh_error: None,
                },
            )
        });
    }

    let mut guard = config
        .health_cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !refresh {
        if let Some(cached) = guard.as_ref() {
            let age = cached.stored_at.elapsed();
            if age <= config.health_cache_ttl {
                return Ok(with_runtime_cache_metadata(
                    cached.value.clone(),
                    config,
                    CacheMetadata {
                        hit: true,
                        bypassed: false,
                        stale: false,
                        ttl: config.health_cache_ttl,
                        age: Some(age),
                        refresh_error: None,
                    },
                ));
            }
        }
    }

    match build_health_json(config) {
        Ok(value) => {
            *guard = Some(CachedHealth {
                stored_at: Instant::now(),
                value: value.clone(),
            });
            Ok(with_runtime_cache_metadata(
                value,
                config,
                CacheMetadata {
                    hit: false,
                    bypassed: refresh,
                    stale: false,
                    ttl: config.health_cache_ttl,
                    age: Some(Duration::from_millis(0)),
                    refresh_error: None,
                },
            ))
        }
        Err(error) => {
            if let Some(cached) = guard.as_ref() {
                return Ok(with_runtime_cache_metadata(
                    cached.value.clone(),
                    config,
                    CacheMetadata {
                        hit: true,
                        bypassed: refresh,
                        stale: true,
                        ttl: config.health_cache_ttl,
                        age: Some(cached.stored_at.elapsed()),
                        refresh_error: Some(error),
                    },
                ));
            }
            Err(error)
        }
    }
}

fn build_health_json(config: &DashboardConfig) -> Result<JsonValue, String> {
    let readiness = run_json_command(&config.root_path, &["verify", "readiness", "--json"])?;
    let ok = json_helpers::bool_at_path(&readiness, &["readyForRuntimeOps"]).unwrap_or(false);
    Ok(JsonValue::object([
        ("ok", JsonValue::bool(ok)),
        ("generatedAtMs", JsonValue::number(now_ms())),
        ("readiness", readiness),
    ]))
}

fn cached_overview_json(config: &DashboardConfig, refresh: bool) -> Result<JsonValue, String> {
    if config.overview_cache_ttl.is_zero() {
        return build_overview_json(&config.root_path).map(|value| {
            with_runtime_cache_metadata(
                value,
                config,
                CacheMetadata {
                    hit: false,
                    bypassed: true,
                    stale: false,
                    ttl: config.overview_cache_ttl,
                    age: None,
                    refresh_error: None,
                },
            )
        });
    }

    let mut guard = config
        .overview_cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !refresh {
        if let Some(cached) = guard.as_ref() {
            let age = cached.stored_at.elapsed();
            if age <= config.overview_cache_ttl {
                return Ok(with_runtime_cache_metadata(
                    cached.value.clone(),
                    config,
                    CacheMetadata {
                        hit: true,
                        bypassed: false,
                        stale: false,
                        ttl: config.overview_cache_ttl,
                        age: Some(age),
                        refresh_error: None,
                    },
                ));
            }
        }
    }

    match build_overview_json(&config.root_path) {
        Ok(value) => {
            *guard = Some(CachedOverview {
                stored_at: Instant::now(),
                value: value.clone(),
            });
            Ok(with_runtime_cache_metadata(
                value,
                config,
                CacheMetadata {
                    hit: false,
                    bypassed: refresh,
                    stale: false,
                    ttl: config.overview_cache_ttl,
                    age: Some(Duration::from_millis(0)),
                    refresh_error: None,
                },
            ))
        }
        Err(error) => {
            if let Some(cached) = guard.as_ref() {
                return Ok(with_runtime_cache_metadata(
                    cached.value.clone(),
                    config,
                    CacheMetadata {
                        hit: true,
                        bypassed: refresh,
                        stale: true,
                        ttl: config.overview_cache_ttl,
                        age: Some(cached.stored_at.elapsed()),
                        refresh_error: Some(error),
                    },
                ));
            }
            Err(error)
        }
    }
}

struct CacheMetadata {
    hit: bool,
    bypassed: bool,
    stale: bool,
    ttl: Duration,
    age: Option<Duration>,
    refresh_error: Option<String>,
}

fn with_runtime_cache_metadata(
    mut value: JsonValue,
    config: &DashboardConfig,
    cache: CacheMetadata,
) -> JsonValue {
    if let JsonValue::Object(map) = &mut value {
        map.insert("cache".to_string(), cache_metadata_json(cache));
        map.insert("runtime".to_string(), runtime_status_json(config));
    }
    value
}

fn cache_metadata_json(cache: CacheMetadata) -> JsonValue {
    let mut entries = vec![
        ("hit".to_string(), JsonValue::bool(cache.hit)),
        ("bypassed".to_string(), JsonValue::bool(cache.bypassed)),
        ("stale".to_string(), JsonValue::bool(cache.stale)),
        (
            "ttlMs".to_string(),
            JsonValue::number(cache.ttl.as_millis()),
        ),
    ];
    if let Some(age) = cache.age {
        entries.push(("ageMs".to_string(), JsonValue::number(age.as_millis())));
    }
    if let Some(error) = cache.refresh_error {
        entries.push(("refreshError".to_string(), JsonValue::string(error)));
    }
    JsonValue::object(entries)
}

fn runtime_resources_response(config: &DashboardConfig) -> JsonValue {
    JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("generatedAtMs", JsonValue::number(now_ms())),
        ("runtime", runtime_status_json(config)),
    ])
}

fn runtime_status_json(config: &DashboardConfig) -> JsonValue {
    let metrics = config.metrics.snapshot();
    let mut upstream_pool_size = 0usize;
    let mut upstream_pool_max_size = 0usize;
    let mut upstream_pool_idle_ttl_ms = 0u128;
    let mut upstream_pool_locked_shards = 0usize;

    for pool_lock in &config.upstream_session_pools {
        if let Ok(pool) = pool_lock.lock() {
            upstream_pool_size = upstream_pool_size.saturating_add(pool.session_count());
            upstream_pool_max_size =
                upstream_pool_max_size.saturating_add(pool.max_session_count());
            upstream_pool_idle_ttl_ms = pool.idle_ttl_ms();
            upstream_pool_locked_shards = upstream_pool_locked_shards.saturating_add(1);
        }
    }

    JsonValue::object([
        (
            "surface",
            JsonValue::string(match config.surface {
                ServeSurface::Dashboard => "dashboard-http",
                ServeSurface::UnifiedServe => "unified-serve-http",
            }),
        ),
        (
            "availableParallelism",
            JsonValue::number(resources::available_parallelism()),
        ),
        (
            "http",
            JsonValue::object([
                ("maxConnections", JsonValue::number(config.max_connections)),
                (
                    "activeConnections",
                    JsonValue::number(metrics.active_connections),
                ),
                (
                    "acceptedConnections",
                    JsonValue::number(metrics.accepted_connections),
                ),
                (
                    "completedConnections",
                    JsonValue::number(metrics.completed_connections),
                ),
                (
                    "failedConnections",
                    JsonValue::number(metrics.failed_connections),
                ),
                (
                    "maxObservedActiveConnections",
                    JsonValue::number(metrics.max_active_connections),
                ),
                (
                    "ioTimeoutMs",
                    JsonValue::number(config.io_timeout.as_millis()),
                ),
                ("maxBodyBytes", JsonValue::number(config.max_body_bytes)),
                (
                    "maxRequestLineBytes",
                    JsonValue::number(resources::MAX_HTTP_REQUEST_LINE_BYTES),
                ),
                (
                    "maxHeaderLineBytes",
                    JsonValue::number(resources::MAX_HTTP_HEADER_LINE_BYTES),
                ),
                (
                    "maxHeaderBytes",
                    JsonValue::number(resources::MAX_HTTP_HEADER_BYTES),
                ),
                (
                    "maxHeaderCount",
                    JsonValue::number(resources::MAX_HTTP_HEADER_COUNT),
                ),
            ]),
        ),
        (
            "caches",
            JsonValue::object([
                (
                    "overviewTtlMs",
                    JsonValue::number(config.overview_cache_ttl.as_millis()),
                ),
                (
                    "healthTtlMs",
                    JsonValue::number(config.health_cache_ttl.as_millis()),
                ),
            ]),
        ),
        (
            "upstreamSessionPool",
            JsonValue::object([
                ("size", JsonValue::number(upstream_pool_size)),
                ("maxSize", JsonValue::number(upstream_pool_max_size)),
                ("idleTtlMs", JsonValue::number(upstream_pool_idle_ttl_ms)),
                (
                    "shardCount",
                    JsonValue::number(config.upstream_session_pools.len()),
                ),
                (
                    "lockedShardCount",
                    JsonValue::number(upstream_pool_locked_shards),
                ),
            ]),
        ),
    ])
}

fn query_bool_flag(query: &str, key: &str) -> bool {
    query.split('&').any(|part| {
        let (name, value) = part.split_once('=').unwrap_or((part, ""));
        if !name.eq_ignore_ascii_case(key) {
            return false;
        }
        value.is_empty()
            || value == "1"
            || value.eq_ignore_ascii_case("true")
            || value.eq_ignore_ascii_case("yes")
    })
}

fn build_overview_json(root_path: &Path) -> Result<JsonValue, String> {
    let mut results = run_json_commands_parallel(
        root_path,
        vec![
            ("doctor", vec!["doctor", "--json"]),
            ("hub", vec!["hub", "status", "--json"]),
            ("readiness", vec!["verify", "readiness", "--json"]),
            ("servers", vec!["server", "list", "--json"]),
            ("clients", vec!["client", "list", "--json"]),
        ],
    )?;

    Ok(JsonValue::object([
        ("generatedAtMs", JsonValue::number(now_ms())),
        (
            "rootPath",
            JsonValue::string(sanitize_root_path(&root_path.display().to_string())),
        ),
        ("doctor", take_parallel_result(&mut results, "doctor")?),
        ("hub", take_parallel_result(&mut results, "hub")?),
        (
            "readiness",
            take_parallel_result(&mut results, "readiness")?,
        ),
        ("servers", take_parallel_result(&mut results, "servers")?),
        ("clients", take_parallel_result(&mut results, "clients")?),
    ]))
}

fn run_json_commands_parallel(
    root_path: &Path,
    commands: Vec<(&'static str, Vec<&'static str>)>,
) -> Result<BTreeMap<&'static str, JsonValue>, String> {
    if commands.len() <= 1 {
        let mut results = BTreeMap::new();
        for (name, args) in commands {
            results.insert(
                name,
                run_json_command_vec(root_path, args.into_iter().map(str::to_string).collect())
                    .map_err(|error| format!("{}: {}", name, error))?,
            );
        }
        return Ok(results);
    }

    let handles = commands
        .into_iter()
        .map(|(name, args)| {
            let root_path = root_path.to_path_buf();
            (
                name,
                thread::spawn(move || {
                    run_json_command_vec(&root_path, args.into_iter().map(str::to_string).collect())
                }),
            )
        })
        .collect::<Vec<_>>();

    let mut results = BTreeMap::new();
    for (name, handle) in handles {
        match handle.join() {
            Ok(Ok(value)) => {
                results.insert(name, value);
            }
            Ok(Err(error)) => return Err(format!("{}: {}", name, error)),
            Err(_) => return Err(format!("{}: command worker panicked", name)),
        }
    }
    Ok(results)
}

fn take_parallel_result(
    results: &mut BTreeMap<&'static str, JsonValue>,
    name: &'static str,
) -> Result<JsonValue, String> {
    results
        .remove(name)
        .ok_or_else(|| format!("{}: command result missing", name))
}

fn action_response(action: &str, result: JsonValue) -> JsonValue {
    JsonValue::object([
        ("action", JsonValue::string(action)),
        ("generatedAtMs", JsonValue::number(now_ms())),
        ("result", result),
    ])
}

fn handle_mcp_http_request(
    request: &HttpRequest,
    config: &DashboardConfig,
) -> Result<McpHttpResponse, String> {
    validate_origin(request)?;
    let body_text = std::str::from_utf8(&request.body)
        .map_err(|error| format!("invalid UTF-8 request body: {}", error))?;
    let message =
        parse_str(body_text.trim()).map_err(|error| format!("invalid JSON-RPC body: {}", error))?;
    let id = json_helpers::value_at_path(&message, &["id"]).cloned();
    let method = json_helpers::string_at_path(&message, &["method"]);

    if id.is_none() {
        if method.is_some() || json_helpers::value_at_path(&message, &["result"]).is_some() {
            return Ok(McpHttpResponse::Accepted);
        }
        if json_helpers::value_at_path(&message, &["error"]).is_some() {
            return Ok(McpHttpResponse::Accepted);
        }
    }

    let id = id.unwrap_or(JsonValue::Null);
    let method = method.ok_or_else(|| "missing JSON-RPC method".to_string())?;
    if let Some(protocol_header) = request_header_string(Some(request), "mcp-protocol-version") {
        let protocol_header = protocol_header.trim();
        if !protocol_header.is_empty() && !mcp::is_supported_protocol_version(protocol_header) {
            return Ok(McpHttpResponse::JsonStatus(
                "400 Bad Request",
                mcp_error_response(
                    id,
                    mcp::ERROR_INVALID_REQUEST,
                    format!(
                        "unsupported MCP-Protocol-Version header: {}",
                        protocol_header
                    ),
                ),
            ));
        }
    }

    match method {
        "initialize" => {
            let requested = json_helpers::string_at_path(&message, &["params", "protocolVersion"])
                .unwrap_or(mcp::CURRENT_PROTOCOL_VERSION);
            let negotiated = mcp::negotiate_protocol_version(requested);

            Ok(McpHttpResponse::Json(JsonValue::object([
                ("jsonrpc", JsonValue::string("2.0")),
                ("id", id),
                (
                    "result",
                    JsonValue::object([
                        ("protocolVersion", JsonValue::string(negotiated)),
                        ("capabilities", adapter::adapter_capabilities()),
                        (
                            "serverInfo",
                            JsonValue::object([
                                ("name", JsonValue::string("mcpace")),
                                ("version", JsonValue::string(env!("CARGO_PKG_VERSION"))),
                            ]),
                        ),
                        (
                            "instructions",
                            JsonValue::string(adapter::adapter_instructions()),
                        ),
                    ]),
                ),
            ])))
        }
        "ping" => Ok(McpHttpResponse::Json(JsonValue::object([
            ("jsonrpc", JsonValue::string("2.0")),
            ("id", id),
            ("result", empty_object()),
        ]))),
        "tools/list" => {
            let cursor = json_helpers::string_at_path(&message, &["params", "cursor"]);
            let protocol = request_header_string(Some(request), "mcp-protocol-version")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| mcp::STREAMABLE_HTTP_DEFAULT_PROTOCOL_VERSION.to_string());
            let protocol_params =
                JsonValue::object([("protocolVersion", JsonValue::string(protocol))]);
            let result = adapter::tool_list_result(
                &config.root_path,
                http_tool_definitions_for_request(request),
                Some(&protocol_params),
                cursor,
            );
            Ok(McpHttpResponse::Json(JsonValue::object([
                ("jsonrpc", JsonValue::string("2.0")),
                ("id", id),
                ("result", result),
            ])))
        }
        "prompts/list" => {
            let cursor = json_helpers::string_at_path(&message, &["params", "cursor"]);
            Ok(McpHttpResponse::Json(JsonValue::object([
                ("jsonrpc", JsonValue::string("2.0")),
                ("id", id),
                (
                    "result",
                    adapter::list_prompts(&config.root_path, None, cursor),
                ),
            ])))
        }
        "prompts/get" => {
            let name = json_helpers::string_at_path(&message, &["params", "name"]);
            let args = json_helpers::value_at_path(&message, &["params", "arguments"])
                .cloned()
                .unwrap_or_else(empty_object);
            let result = match name {
                Some(name) => match adapter::get_prompt(&config.root_path, name, args, None) {
                    Ok(value) => JsonValue::object([
                        ("jsonrpc", JsonValue::string("2.0")),
                        ("id", id),
                        ("result", value),
                    ]),
                    Err(error) => JsonValue::object([
                        ("jsonrpc", JsonValue::string("2.0")),
                        ("id", id),
                        (
                            "error",
                            JsonValue::object([
                                ("code", JsonValue::number(-32602)),
                                ("message", JsonValue::string(error)),
                            ]),
                        ),
                    ]),
                },
                None => JsonValue::object([
                    ("jsonrpc", JsonValue::string("2.0")),
                    ("id", id),
                    (
                        "error",
                        JsonValue::object([
                            ("code", JsonValue::number(-32602)),
                            (
                                "message",
                                JsonValue::string("prompts/get requires a prompt name"),
                            ),
                        ]),
                    ),
                ]),
            };
            Ok(McpHttpResponse::Json(result))
        }
        "resources/list" => {
            let cursor = json_helpers::string_at_path(&message, &["params", "cursor"]);
            Ok(McpHttpResponse::Json(JsonValue::object([
                ("jsonrpc", JsonValue::string("2.0")),
                ("id", id),
                (
                    "result",
                    adapter::list_resources(&config.root_path, None, cursor),
                ),
            ])))
        }
        "resources/templates/list" => {
            let cursor = json_helpers::string_at_path(&message, &["params", "cursor"]);
            Ok(McpHttpResponse::Json(JsonValue::object([
                ("jsonrpc", JsonValue::string("2.0")),
                ("id", id),
                (
                    "result",
                    adapter::list_resource_templates(&config.root_path, None, cursor),
                ),
            ])))
        }
        "resources/read" => {
            let uri = json_helpers::string_at_path(&message, &["params", "uri"]);
            let result = match uri {
                Some(uri) => match adapter::read_resource(&config.root_path, uri, None) {
                    Ok(value) => JsonValue::object([
                        ("jsonrpc", JsonValue::string("2.0")),
                        ("id", id),
                        ("result", value),
                    ]),
                    Err(error) => JsonValue::object([
                        ("jsonrpc", JsonValue::string("2.0")),
                        ("id", id),
                        (
                            "error",
                            JsonValue::object([
                                ("code", JsonValue::number(-32602)),
                                ("message", JsonValue::string(error)),
                            ]),
                        ),
                    ]),
                },
                None => JsonValue::object([
                    ("jsonrpc", JsonValue::string("2.0")),
                    ("id", id),
                    (
                        "error",
                        JsonValue::object([
                            ("code", JsonValue::number(-32602)),
                            (
                                "message",
                                JsonValue::string("resources/read requires a uri"),
                            ),
                        ]),
                    ),
                ]),
            };
            Ok(McpHttpResponse::Json(result))
        }
        "tools/call" => {
            let tool_name = json_helpers::string_at_path(&message, &["params", "name"])
                .ok_or_else(|| "tools/call requires a tool name".to_string())?;
            let args = json_helpers::value_at_path(&message, &["params", "arguments"])
                .cloned()
                .unwrap_or_else(empty_object);
            let projected_call = tool_name.starts_with("u_");
            let option_arguments = if projected_call {
                adapter::projected_adapter_control_arguments(&args)
            } else {
                args.clone()
            };
            let result_options = match tool_result::options_from_arguments(&option_arguments) {
                Ok(options) => options,
                Err(error) => {
                    return Ok(mcp_tool_result(
                        id,
                        JsonValue::object([
                            ("ok", JsonValue::bool(false)),
                            ("tool", JsonValue::string(tool_name)),
                            ("error", JsonValue::string(error)),
                        ]),
                        true,
                        ToolResultOptions::default(),
                    ));
                }
            };
            let (structured, is_error) =
                match run_http_tool(config, tool_name, &args, Some(request)) {
                    Ok(value) => (value, false),
                    Err(error) => (
                        JsonValue::object([
                            ("ok", JsonValue::bool(false)),
                            ("tool", JsonValue::string(tool_name)),
                            ("error", JsonValue::string(error)),
                            (
                                "supportedTools",
                                JsonValue::array(http_tool_definitions().into_iter().filter_map(
                                    |tool| {
                                        tool.get("name")
                                            .and_then(JsonValue::as_str)
                                            .map(JsonValue::string)
                                    },
                                )),
                            ),
                        ]),
                        true,
                    ),
                };
            if !is_error
                && (matches!(tool_name, "upstream_call" | "upstream_batch") || projected_call)
            {
                Ok(mcp_tool_result_payload(
                    id,
                    tool_result::upstream_tool_result_payload(structured, false, result_options),
                ))
            } else {
                Ok(mcp_tool_result(id, structured, is_error, result_options))
            }
        }
        _ => Ok(McpHttpResponse::Json(JsonValue::object([
            ("jsonrpc", JsonValue::string("2.0")),
            ("id", id),
            (
                "error",
                JsonValue::object([
                    ("code", JsonValue::number(-32601)),
                    (
                        "message",
                        JsonValue::string(format!("unsupported MCP method '{}'", method)),
                    ),
                ]),
            ),
        ]))),
    }
}

fn mcp_tool_result(
    id: JsonValue,
    structured: JsonValue,
    is_error: bool,
    options: ToolResultOptions,
) -> McpHttpResponse {
    mcp_tool_result_payload(
        id,
        tool_result::tool_result_payload(structured, is_error, options),
    )
}

fn mcp_tool_result_payload(id: JsonValue, payload: JsonValue) -> McpHttpResponse {
    McpHttpResponse::Json(JsonValue::object([
        ("jsonrpc", JsonValue::string("2.0")),
        ("id", id),
        ("result", payload),
    ]))
}

fn mcp_error_response(id: JsonValue, code: i64, message: impl Into<String>) -> JsonValue {
    JsonValue::object([
        ("jsonrpc", JsonValue::string("2.0")),
        ("id", id),
        (
            "error",
            JsonValue::object([
                ("code", JsonValue::number(code)),
                ("message", JsonValue::string(message.into())),
            ]),
        ),
    ])
}

fn reject_forbidden_origin(stream: &mut TcpStream, request: &HttpRequest) -> Result<bool, String> {
    let Err(error) = validate_origin(request) else {
        return Ok(false);
    };
    let payload = JsonValue::object([
        ("ok", JsonValue::bool(false)),
        ("error", JsonValue::string(error)),
    ]);
    write_json_response(stream, "403 Forbidden", &payload)?;
    Ok(true)
}

fn validate_origin(request: &HttpRequest) -> Result<(), String> {
    if let Some((_, origin)) = request.headers.iter().find(|(key, _)| key == "origin") {
        if !is_allowed_local_origin(origin.trim()) {
            return Err(format!(
                "origin '{}' is not allowed for local MCPace serve mode",
                origin
            ));
        }
    }
    Ok(())
}

fn is_allowed_local_origin(origin: &str) -> bool {
    if origin == "null" {
        return true;
    }
    let Some(authority) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    if authority.is_empty() || authority.contains('/') || authority.contains('@') {
        return false;
    }

    let Some(host) = origin_host(authority) else {
        return false;
    };
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "[::1]"
}

fn origin_host(authority: &str) -> Option<&str> {
    if authority.starts_with('[') {
        let end = authority.find(']')?;
        let host = &authority[..=end];
        let suffix = &authority[end + 1..];
        if suffix.is_empty() || valid_port_suffix(suffix) {
            return Some(host);
        }
        return None;
    }

    if authority.matches(':').count() > 1 {
        return None;
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && valid_port(port) => Some(host),
        Some(_) => None,
        None => Some(authority),
    }
}

fn valid_port_suffix(value: &str) -> bool {
    value.strip_prefix(':').map(valid_port).unwrap_or(false)
}

fn valid_port(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|character| character.is_ascii_digit())
}

fn accepts(request: &HttpRequest, media_type: &str) -> bool {
    request
        .headers
        .iter()
        .filter(|(key, _)| key == "accept")
        .any(|(_, value)| {
            value.split(',').any(|item| {
                item.trim()
                    .split(';')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .eq_ignore_ascii_case(media_type)
            })
        })
}

fn http_tool_definitions() -> Vec<JsonValue> {
    vec![
        http_tool("doctor", "Inspect MCPace readiness"),
        http_tool("hub_status", "Inspect hub status"),
        http_tool("hub_up", "Start the hub"),
        http_tool("hub_down", "Stop the hub"),
        http_tool("hub_repair", "Repair stopped or stale hub state"),
        http_tool("hub_logs", "Read hub logs"),
        http_tool("server_list", "List configured servers"),
        http_tool(
            "runtime_diagnostics",
            "Explain MCPace runtime and upstream tool availability",
        ),
        http_tool_with_schema(
            "adapter_profile",
            "Infer current client/protocol/transport and server coordination profile dynamically",
            JsonValue::object([
                (
                    "includeLiveCatalog",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "When true, include a live upstream catalog/projection sample. Defaults to false.",
                            ),
                        ),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional live upstream catalog timeout in milliseconds."),
                        ),
                    ]),
                ),
                (
                    "refresh",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string("Bypass the short successful upstream tools/list cache."),
                        ),
                    ]),
                ),
            ]),
            vec![],
        ),
        http_tool_with_schema(
            "adapter_route",
            "Plan upstream call routing, batching, serialization, and parallel-safe lanes dynamically",
            JsonValue::object([
                (
                    "calls",
                    JsonValue::object([
                        ("type", JsonValue::string("array")),
                        (
                            "description",
                            JsonValue::string(
                                "Optional calls to plan. Each item may be [server, tool, arguments], {server, name/tool, arguments}, an upstream_search result, or an upstream_call object.",
                            ),
                        ),
                        ("items", JsonValue::object([(
                            "oneOf",
                            JsonValue::array([
                                JsonValue::object([
                                    ("type", JsonValue::string("array")),
                                    ("minItems", JsonValue::number(2)),
                                    ("maxItems", JsonValue::number(3)),
                                ]),
                                JsonValue::object([("type", JsonValue::string("object"))]),
                                JsonValue::object([("type", JsonValue::string("string"))]),
                            ]),
                        )])),
                    ]),
                ),
                (
                    "includeLiveCatalog",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string("When true, include live upstream catalog context in the route plan."),
                        ),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional live upstream catalog timeout in milliseconds."),
                        ),
                    ]),
                ),
                (
                    "refresh",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string("Bypass the short tools/list cache when includeLiveCatalog=true."),
                        ),
                    ]),
                ),
            ]),
            vec![],
        ),
        http_tool_with_schema(
            "upstream_search",
            "Search upstream tools without exposing every upstream schema as a top-level tool",
            JsonValue::object([
                (
                    "query",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "description",
                            JsonValue::string(
                                "Optional keyword query over server, tool name, title, and description.",
                            ),
                        ),
                    ]),
                ),
                (
                    "server",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "description",
                            JsonValue::string("Optional configured upstream server name to search inside."),
                        ),
                    ]),
                ),
                (
                    "limit",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Maximum results to return, clamped to 1..100."),
                        ),
                    ]),
                ),
                (
                    "includeSchema",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "When true, include compact input schemas in search results.",
                            ),
                        ),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional per-server catalog timeout from 1000 to 300000 ms."),
                        ),
                    ]),
                ),
                (
                    "refresh",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string("Bypass the short tools/list cache before searching."),
                        ),
                    ]),
                ),
            ]),
            vec![],
        ),
        http_tool_with_schema(
            "surface_manifest",
            "Explain exact native MCPace tools versus configured upstream MCP tools",
            JsonValue::object([
                (
                    "includeLiveCatalog",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "When true, launch/probe configured callable upstreams and include the live upstream_catalog output.",
                            ),
                        ),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string(
                                "Optional per-server catalog timeout from 1000 to 300000 ms when includeLiveCatalog=true.",
                            ),
                        ),
                    ]),
                ),
                (
                    "refresh",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "Bypass the short tools/list cache when includeLiveCatalog=true.",
                            ),
                        ),
                    ]),
                ),
            ]),
            vec![],
        ),
        http_tool_with_schema(
            "upstream_tools",
            "List callable tools for one configured stdio upstream MCP server",
            JsonValue::object([
                (
                    "server",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "description",
                            JsonValue::string(
                                "Configured upstream server name from mcp_settings.json. Omit to return inventory without launching anything.",
                            ),
                        ),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional per-call timeout from 1000 to 300000 ms."),
                        ),
                    ]),
                ),
                (
                    "refresh",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "Bypass the short in-process tools/list cache and refresh from the upstream server.",
                            ),
                        ),
                    ]),
                ),
            ]),
            vec![],
        ),
        http_tool_with_schema(
            "upstream_catalog",
            "List configured upstream MCP tools as a flat server-qualified catalog with concise descriptions",
            JsonValue::object([
                (
                    "server",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "description",
                            JsonValue::string(
                                "Optional configured upstream server name. Omit to discover all configured upstream tool summaries.",
                            ),
                        ),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional per-server catalog timeout from 1000 to 300000 ms."),
                        ),
                    ]),
                ),
                (
                    "refresh",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "Bypass the short in-process tools/list cache and refresh from the upstream server.",
                            ),
                        ),
                    ]),
                ),
            ]),
            vec![],
        ),
        http_tool_with_schema(
            "upstream_probe",
            "Probe configured upstream MCP servers with short successful tools/list cache reuse",
            JsonValue::object([
                (
                    "server",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "description",
                            JsonValue::string(
                                "Optional configured upstream server name. Omit to probe all configured servers.",
                            ),
                        ),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string(
                                "Optional per-server probe timeout from 1000 to 300000 ms. Default probe timeout is capped so one broken future server cannot stall the whole check.",
                            ),
                        ),
                    ]),
                ),
                (
                    "refresh",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "Bypass the short successful tools/list cache and force a fresh upstream probe.",
                            ),
                        ),
                    ]),
                ),
            ]),
            vec![],
        ),
        http_tool_with_schema(
            "upstream_policy_audit",
            "Audit configured upstream MCP tool annotations and declarative MCPace toolPolicies",
            JsonValue::object([
                (
                    "server",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "description",
                            JsonValue::string(
                                "Optional configured upstream server name. Omit to audit all configured upstream tools.",
                            ),
                        ),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional per-server audit timeout from 1000 to 300000 ms."),
                        ),
                    ]),
                ),
                (
                    "refresh",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "Bypass the short tools/list cache and force a fresh upstream audit.",
                            ),
                        ),
                    ]),
                ),
            ]),
            vec![],
        ),
        http_tool_with_schema(
            "upstream_policy_suggest",
            "Generate declarative MCPace toolPolicies suggestions from live upstream MCP risk signals",
            JsonValue::object([
                (
                    "server",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "description",
                            JsonValue::string(
                                "Optional configured upstream server name. Omit to suggest policies for all configured upstream tools.",
                            ),
                        ),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional per-server suggestion timeout from 1000 to 300000 ms."),
                        ),
                    ]),
                ),
                (
                    "refresh",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "Bypass the short tools/list cache and force fresh upstream suggestions.",
                            ),
                        ),
                    ]),
                ),
            ]),
            vec![],
        ),
        http_tool_with_schema(
            "upstream_call",
            "Call a tool on a configured stdio upstream server",
            JsonValue::object([
                (
                    "server",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        ("description", JsonValue::string("Configured upstream server name.")),
                    ]),
                ),
                (
                    "tool",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        ("description", JsonValue::string("Upstream tool name.")),
                    ]),
                ),
                (
                    "arguments",
                    JsonValue::object([
                        ("type", JsonValue::string("object")),
                        (
                            "description",
                            JsonValue::string("Arguments to pass to the upstream tool."),
                        ),
                        ("additionalProperties", JsonValue::bool(true)),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional per-call timeout from 1000 to 300000 ms."),
                        ),
                    ]),
                ),
                (
                    "clientId",
                    JsonValue::object([("type", JsonValue::string("string"))]),
                ),
                (
                    "sessionId",
                    JsonValue::object([("type", JsonValue::string("string"))]),
                ),
                (
                    "projectRoot",
                    JsonValue::object([("type", JsonValue::string("string"))]),
                ),
                (
                    "transport",
                    JsonValue::object([("type", JsonValue::string("string"))]),
                ),
                (
                    "ttlMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional runtime lease TTL in milliseconds."),
                        ),
                    ]),
                ),
                (
                    "metadata",
                    JsonValue::object([("type", JsonValue::string("object"))]),
                ),
                (
                    "resultMode",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "enum",
                            JsonValue::array([
                                JsonValue::string("native"),
                                JsonValue::string("compat"),
                                JsonValue::string("compact"),
                                JsonValue::string("summary"),
                            ]),
                        ),
                        (
                            "description",
                            JsonValue::string(
                                "Tool-result content mode: native, compat, compact, or summary.",
                            ),
                        ),
                    ]),
                ),
                (
                    "diagnostics",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "enum",
                            JsonValue::array([
                                JsonValue::string("full"),
                                JsonValue::string("summary"),
                                JsonValue::string("none"),
                            ]),
                        ),
                        (
                            "description",
                            JsonValue::string("MCPace lease/session diagnostics to retain."),
                        ),
                    ]),
                ),
                (
                    "nestedContent",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "enum",
                            JsonValue::array([
                                JsonValue::string("full"),
                                JsonValue::string("compact"),
                            ]),
                        ),
                        (
                            "description",
                            JsonValue::string("Use compact to dedupe nested upstream content text."),
                        ),
                    ]),
                ),
                (
                    "tokenReducerPlugins",
                    JsonValue::object([
                        ("type", JsonValue::string("array")),
                        (
                            "description",
                            JsonValue::string(
                                "Optional built-in token reducers, e.g. mcpace.native-content.v1.",
                            ),
                        ),
                        (
                            "items",
                            JsonValue::object([("type", JsonValue::string("string"))]),
                        ),
                    ]),
                ),
                (
                    "allowToolRiskClasses",
                    JsonValue::object([
                        ("type", JsonValue::string("array")),
                        (
                            "description",
                            JsonValue::string(
                                "Generic risk-class opt-in for config-declared upstream tool policies, for example ['desktop-observation'].",
                            ),
                        ),
                        (
                            "items",
                            JsonValue::object([("type", JsonValue::string("string"))]),
                        ),
                    ]),
                ),
                (
                    "allowArguments",
                    JsonValue::object([
                        ("type", JsonValue::string("array")),
                        (
                            "description",
                            JsonValue::string(
                                "Generic allow-argument opt-in names for config-declared upstream tool policies, for example ['allowCustomRisk'].",
                            ),
                        ),
                        (
                            "items",
                            JsonValue::object([("type", JsonValue::string("string"))]),
                        ),
                    ]),
                ),
            ]),
            vec!["server", "tool"],
        ),
        http_tool_with_schema(
            "upstream_batch",
            "Call multiple tools on one configured stdio upstream server in a single state-preserving session",
            JsonValue::object([
                (
                    "server",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        ("description", JsonValue::string("Configured upstream server name.")),
                    ]),
                ),
                (
                    "calls",
                    JsonValue::object([
                        ("type", JsonValue::string("array")),
                        (
                            "description",
                            JsonValue::string(
                                "Ordered upstream calls to execute after one initialize handshake. Use this for any stateful upstream server that needs a shared session.",
                            ),
                        ),
                        ("items", http_upstream_batch_call_item_schema()),
                    ]),
                ),
                (
                    "timeoutMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional total batch timeout from 1000 to 300000 ms."),
                        ),
                    ]),
                ),
                (
                    "clientId",
                    JsonValue::object([("type", JsonValue::string("string"))]),
                ),
                (
                    "sessionId",
                    JsonValue::object([("type", JsonValue::string("string"))]),
                ),
                (
                    "projectRoot",
                    JsonValue::object([("type", JsonValue::string("string"))]),
                ),
                (
                    "transport",
                    JsonValue::object([("type", JsonValue::string("string"))]),
                ),
                (
                    "ttlMs",
                    JsonValue::object([
                        ("type", JsonValue::string("integer")),
                        (
                            "description",
                            JsonValue::string("Optional runtime lease TTL in milliseconds."),
                        ),
                    ]),
                ),
                (
                    "metadata",
                    JsonValue::object([("type", JsonValue::string("object"))]),
                ),
                (
                    "resultMode",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "enum",
                            JsonValue::array([
                                JsonValue::string("native"),
                                JsonValue::string("compat"),
                                JsonValue::string("compact"),
                                JsonValue::string("summary"),
                            ]),
                        ),
                        (
                            "description",
                            JsonValue::string(
                                "Tool-result content mode: native, compat, compact, or summary.",
                            ),
                        ),
                    ]),
                ),
                (
                    "diagnostics",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "enum",
                            JsonValue::array([
                                JsonValue::string("full"),
                                JsonValue::string("summary"),
                                JsonValue::string("none"),
                            ]),
                        ),
                        (
                            "description",
                            JsonValue::string("MCPace lease/session diagnostics to retain."),
                        ),
                    ]),
                ),
                (
                    "nestedContent",
                    JsonValue::object([
                        ("type", JsonValue::string("string")),
                        (
                            "enum",
                            JsonValue::array([
                                JsonValue::string("full"),
                                JsonValue::string("compact"),
                            ]),
                        ),
                        (
                            "description",
                            JsonValue::string("Use compact to dedupe nested upstream content text."),
                        ),
                    ]),
                ),
                (
                    "tokenReducerPlugins",
                    JsonValue::object([
                        ("type", JsonValue::string("array")),
                        (
                            "description",
                            JsonValue::string(
                                "Optional built-in token reducers, e.g. mcpace.native-content.v1.",
                            ),
                        ),
                        (
                            "items",
                            JsonValue::object([("type", JsonValue::string("string"))]),
                        ),
                    ]),
                ),
                (
                    "allowToolRiskClasses",
                    JsonValue::object([
                        ("type", JsonValue::string("array")),
                        (
                            "description",
                            JsonValue::string(
                                "Generic risk-class opt-in for config-declared upstream tool policies, for example ['desktop-observation'].",
                            ),
                        ),
                        (
                            "items",
                            JsonValue::object([("type", JsonValue::string("string"))]),
                        ),
                    ]),
                ),
                (
                    "allowArguments",
                    JsonValue::object([
                        ("type", JsonValue::string("array")),
                        (
                            "description",
                            JsonValue::string(
                                "Generic allow-argument opt-in names for config-declared upstream tool policies, for example ['allowCustomRisk'].",
                            ),
                        ),
                        (
                            "items",
                            JsonValue::object([("type", JsonValue::string("string"))]),
                        ),
                    ]),
                ),
            ]),
            vec!["server", "calls"],
        ),
        http_tool("client_list", "List known client targets"),
    ]
}

fn http_tool_definitions_for_request(request: &HttpRequest) -> Vec<JsonValue> {
    let protocol = request_header_string(Some(request), "mcp-protocol-version");
    let options = adapter::tool_surface_options_from_http_header(protocol.as_deref());
    let names = http_tool_definitions()
        .iter()
        .filter_map(|tool| json_helpers::string_at_path(tool, &["name"]).map(str::to_string))
        .collect::<Vec<_>>();
    let visible_names = adapter::visible_tool_names(&names, None);
    let visible = visible_names
        .iter()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    http_tool_definitions()
        .into_iter()
        .filter(|tool| {
            json_helpers::string_at_path(tool, &["name"])
                .map(|name| visible.contains(name))
                .unwrap_or(false)
        })
        .map(|tool| shape_http_tool_for_client(tool, options))
        .collect()
}

fn shape_http_tool_for_client(tool: JsonValue, options: adapter::ToolSurfaceOptions) -> JsonValue {
    let JsonValue::Object(mut map) = tool else {
        return tool;
    };
    if !options.include_title {
        map.remove("title");
    }
    if !options.include_annotations {
        map.remove("annotations");
    }
    JsonValue::Object(map)
}

fn http_tool(name: &str, description: &str) -> JsonValue {
    http_tool_with_schema(name, description, empty_object(), vec![])
}

fn http_tool_annotations(name: &str) -> JsonValue {
    let read_only = matches!(
        name,
        "doctor"
            | "hub_status"
            | "hub_logs"
            | "runtime_leases"
            | "server_list"
            | "server_capabilities"
            | "client_list"
            | "client_plan"
            | "client_export"
            | "adapter_profile"
            | "adapter_route"
            | "upstream_search"
            | "surface_manifest"
            | "upstream_tools"
            | "upstream_catalog"
            | "upstream_probe"
            | "upstream_policy_audit"
            | "upstream_policy_suggest"
    );
    let open_world = matches!(
        name,
        "adapter_route"
            | "upstream_search"
            | "upstream_tools"
            | "upstream_catalog"
            | "upstream_probe"
            | "upstream_policy_audit"
            | "upstream_policy_suggest"
            | "upstream_call"
            | "upstream_batch"
    );
    let destructive = matches!(name, "hub_down" | "upstream_call" | "upstream_batch");
    let idempotent = matches!(
        name,
        "doctor"
            | "hub_status"
            | "hub_logs"
            | "runtime_leases"
            | "server_list"
            | "server_capabilities"
            | "client_list"
            | "client_plan"
            | "client_export"
            | "adapter_profile"
            | "adapter_route"
            | "upstream_search"
            | "surface_manifest"
            | "upstream_tools"
            | "upstream_catalog"
            | "upstream_probe"
            | "upstream_policy_audit"
            | "upstream_policy_suggest"
    );

    JsonValue::object([
        ("readOnlyHint", JsonValue::bool(read_only)),
        ("destructiveHint", JsonValue::bool(destructive)),
        ("idempotentHint", JsonValue::bool(idempotent)),
        ("openWorldHint", JsonValue::bool(open_world)),
    ])
}

fn http_tool_with_schema(
    name: &str,
    description: &str,
    properties: JsonValue,
    required: Vec<&str>,
) -> JsonValue {
    JsonValue::object([
        ("name", JsonValue::string(name)),
        ("title", JsonValue::string(description)),
        ("description", JsonValue::string(description)),
        ("annotations", http_tool_annotations(name)),
        (
            "inputSchema",
            JsonValue::object([
                ("type", JsonValue::string("object")),
                ("properties", properties),
                (
                    "required",
                    JsonValue::array(required.into_iter().map(JsonValue::string)),
                ),
                ("additionalProperties", JsonValue::bool(false)),
            ]),
        ),
    ])
}

fn http_upstream_batch_call_item_schema() -> JsonValue {
    JsonValue::object([(
        "oneOf",
        JsonValue::array([
            JsonValue::object([
                ("type", JsonValue::string("object")),
                (
                    "properties",
                    JsonValue::object([
                        (
                            "tool",
                            JsonValue::object([
                                ("type", JsonValue::string("string")),
                                ("description", JsonValue::string("Upstream tool name.")),
                            ]),
                        ),
                        (
                            "arguments",
                            JsonValue::object([
                                ("type", JsonValue::string("object")),
                                (
                                    "description",
                                    JsonValue::string("Arguments to pass to the upstream tool."),
                                ),
                                ("additionalProperties", JsonValue::bool(true)),
                            ]),
                        ),
                    ]),
                ),
                ("required", JsonValue::array([JsonValue::string("tool")])),
                ("additionalProperties", JsonValue::bool(false)),
            ]),
            JsonValue::object([
                ("type", JsonValue::string("array")),
                (
                    "description",
                    JsonValue::string("Compact tuple form: [tool] or [tool, arguments]."),
                ),
                ("minItems", JsonValue::number(1)),
                ("maxItems", JsonValue::number(2)),
                (
                    "prefixItems",
                    JsonValue::array([
                        JsonValue::object([
                            ("type", JsonValue::string("string")),
                            ("description", JsonValue::string("Upstream tool name.")),
                        ]),
                        JsonValue::object([
                            ("type", JsonValue::string("object")),
                            (
                                "description",
                                JsonValue::string("Arguments to pass to the upstream tool."),
                            ),
                            ("additionalProperties", JsonValue::bool(true)),
                        ]),
                    ]),
                ),
                ("items", JsonValue::bool(false)),
            ]),
        ]),
    )])
}

fn http_tool_names() -> Vec<String> {
    http_tool_definitions()
        .into_iter()
        .filter_map(|tool| {
            tool.get("name")
                .and_then(JsonValue::as_str)
                .map(str::to_string)
        })
        .collect()
}

fn run_http_tool(
    config: &DashboardConfig,
    name: &str,
    args: &JsonValue,
    request: Option<&HttpRequest>,
) -> Result<JsonValue, String> {
    let root_path = &config.root_path;
    if name.starts_with("u_") {
        let reserved = http_tool_names().into_iter().collect::<BTreeSet<_>>();
        let target = adapter::resolve_projected_tool(
            root_path,
            name,
            &reserved,
            &adapter::ToolExposureOptions::for_call_resolution(),
        )?;
        if let Some(target) = target {
            let control_arguments = adapter::projected_adapter_control_arguments(args);
            let context = http_upstream_lease_context(&control_arguments, request)?;
            let upstream_arguments = adapter::strip_projected_adapter_arguments(args);
            let timeout_ms = json_helpers::value_at_path(&control_arguments, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            return upstream::call_tool_with_pooled_context(
                root_path,
                &target.server,
                &target.tool,
                &upstream_arguments,
                timeout_ms,
                Some(&context),
                upstream_pool_for_context(config, &target.server, &context),
            );
        }
    }
    match name {
        "doctor" => run_json_command(root_path, &["doctor", "--json"]),
        "hub_status" => run_json_command(root_path, &["hub", "status", "--json"]),
        "hub_up" => run_json_command(root_path, &["hub", "up", "--json"]),
        "hub_down" => run_json_command(root_path, &["hub", "down", "--json"]),
        "hub_repair" => run_json_command(root_path, &["hub", "repair", "--json"]),
        "hub_logs" => {
            let tail = json_helpers::value_at_path(args, &["tail"])
                .and_then(JsonValue::as_i64)
                .unwrap_or(20);
            run_json_command_vec(
                root_path,
                vec![
                    "hub".to_string(),
                    "logs".to_string(),
                    "--json".to_string(),
                    "--tail".to_string(),
                    tail.to_string(),
                ],
            )
        }
        "server_list" => run_json_command(root_path, &["server", "list", "--json"]),
        "runtime_diagnostics" => runtime_diagnostics(config),
        "adapter_profile" => {
            let include_live_catalog =
                json_helpers::bool_at_path(args, &["includeLiveCatalog"]).unwrap_or(false);
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let refresh = json_helpers::bool_at_path(args, &["refresh"]).unwrap_or(false);
            let context = http_upstream_lease_context(args, request)?;
            let visible_tools = adapter::visible_tool_names(&http_tool_names(), context.metadata.as_ref());
            adapter::adapter_profile(
                root_path,
                context.metadata.as_ref(),
                context.transport.as_deref().unwrap_or("streamable-http"),
                &visible_tools,
                include_live_catalog,
                timeout_ms,
                refresh,
            )
        }
        "adapter_route" => {
            let include_live_catalog =
                json_helpers::bool_at_path(args, &["includeLiveCatalog"]).unwrap_or(false);
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let refresh = json_helpers::bool_at_path(args, &["refresh"]).unwrap_or(false);
            let calls = json_helpers::value_at_path(args, &["calls"]);
            adapter::adapter_route_plan(root_path, calls, include_live_catalog, timeout_ms, refresh)
        }
        "upstream_search" => {
            let server = json_helpers::string_at_path(args, &["server"]);
            let query = json_helpers::string_at_path(args, &["query"]);
            let limit = json_helpers::value_at_path(args, &["limit"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as usize)
                .unwrap_or(20);
            let include_schema =
                json_helpers::bool_at_path(args, &["includeSchema"]).unwrap_or(false);
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let refresh = json_helpers::bool_at_path(args, &["refresh"]).unwrap_or(false);
            adapter::upstream_search(
                root_path,
                server,
                query,
                limit,
                include_schema,
                timeout_ms,
                refresh,
            )
        }
        "surface_manifest" => {
            let include_live_catalog =
                json_helpers::bool_at_path(args, &["includeLiveCatalog"]).unwrap_or(false);
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let refresh = json_helpers::bool_at_path(args, &["refresh"]).unwrap_or(false);
            upstream::surface_manifest(
                root_path,
                "streamable-http",
                http_tool_names(),
                include_live_catalog,
                timeout_ms,
                refresh,
            )
        }
        "upstream_tools" => {
            let server = json_helpers::string_at_path(args, &["server"]);
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let refresh = json_helpers::bool_at_path(args, &["refresh"]).unwrap_or(false);
            upstream::list_tools(root_path, server, timeout_ms, refresh)
        }
        "upstream_probe" => {
            let server = json_helpers::string_at_path(args, &["server"]);
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let refresh = json_helpers::bool_at_path(args, &["refresh"]).unwrap_or(false);
            upstream::probe_servers(root_path, server, timeout_ms, refresh)
        }
        "upstream_catalog" => {
            let server = json_helpers::string_at_path(args, &["server"]);
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let refresh = json_helpers::bool_at_path(args, &["refresh"]).unwrap_or(false);
            upstream::catalog_tools(root_path, server, timeout_ms, refresh)
        }
        "upstream_policy_audit" => {
            let server = json_helpers::string_at_path(args, &["server"]);
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let refresh = json_helpers::bool_at_path(args, &["refresh"]).unwrap_or(false);
            upstream::audit_tool_policies(root_path, server, timeout_ms, refresh)
        }
        "upstream_policy_suggest" => {
            let server = json_helpers::string_at_path(args, &["server"]);
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let refresh = json_helpers::bool_at_path(args, &["refresh"]).unwrap_or(false);
            upstream::suggest_tool_policies(root_path, server, timeout_ms, refresh)
        }
        "upstream_call" => {
            let server = json_helpers::string_at_path(args, &["server"])
                .ok_or_else(|| "upstream_call requires a 'server' string".to_string())?;
            let tool = json_helpers::string_at_path(args, &["tool"])
                .ok_or_else(|| "upstream_call requires a 'tool' string".to_string())?;
            let arguments = json_helpers::value_at_path(args, &["arguments"])
                .cloned()
                .unwrap_or_else(empty_object);
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let context = http_upstream_lease_context(args, request)?;
            upstream::call_tool_with_pooled_context(
                root_path,
                server,
                tool,
                &arguments,
                timeout_ms,
                Some(&context),
                upstream_pool_for_context(config, server, &context),
            )
        }
        "upstream_batch" => {
            let server = json_helpers::string_at_path(args, &["server"])
                .ok_or_else(|| "upstream_batch requires a 'server' string".to_string())?;
            let raw_calls = json_helpers::array_at_path(args, &["calls"])
                .ok_or_else(|| "upstream_batch requires a 'calls' array".to_string())?;
            let mut calls = Vec::new();
            for (index, raw_call) in raw_calls.iter().enumerate() {
                calls.push(parse_http_upstream_batch_call(raw_call, index)?);
            }
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let context = http_upstream_lease_context(args, request)?;
            upstream::call_tools_with_pooled_context(
                root_path,
                server,
                &calls,
                timeout_ms,
                Some(&context),
                upstream_pool_for_context(config, server, &context),
            )
        }
        "client_list" => run_json_command(root_path, &["client", "list", "--json"]),
        other => Err(format!(
            "unsupported MCPace HTTP tool '{}'. This HTTP endpoint exposes MCPace management tools plus adapter_profile/upstream_search and stdio upstream access through surface_manifest/upstream_catalog/upstream_probe/upstream_policy_audit/upstream_policy_suggest/upstream_tools/upstream_call/upstream_batch. In auto/native exposure mode, upstream tools may also appear as projected u_<server>_<tool>_<hash> names in tools/list; call adapter_profile for the current routing plan, upstream_search for concise discovery, upstream_tools for one server's full schemas, then upstream_call or upstream_batch when brokered routing is better. Call runtime_diagnostics for exact status.",
            other
        )),
    }
}

fn parse_http_upstream_batch_call(
    raw_call: &JsonValue,
    index: usize,
) -> Result<upstream::UpstreamToolCall, String> {
    if let Some(items) = raw_call.as_array() {
        if items.is_empty() || items.len() > 2 {
            return Err(format!(
                "upstream_batch calls[{}] tuple form must be [tool] or [tool, arguments]",
                index
            ));
        }
        let tool = items[0]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!(
                    "upstream_batch calls[{}][0] must be a non-empty tool string",
                    index
                )
            })?
            .to_string();
        let arguments = items.get(1).cloned().unwrap_or_else(empty_object);
        return Ok(upstream::UpstreamToolCall { tool, arguments });
    }

    let tool = json_helpers::string_at_path(raw_call, &["tool"])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "upstream_batch calls[{}] requires a non-empty 'tool'",
                index
            )
        })?
        .to_string();
    let arguments = json_helpers::value_at_path(raw_call, &["arguments"])
        .cloned()
        .unwrap_or_else(empty_object);
    Ok(upstream::UpstreamToolCall { tool, arguments })
}

fn http_upstream_lease_context(
    args: &JsonValue,
    request: Option<&HttpRequest>,
) -> Result<upstream::UpstreamLeaseContext, String> {
    Ok(upstream::UpstreamLeaseContext {
        client_id: Some(
            optional_http_string(args, "clientId")?
                .or_else(|| first_http_metadata_string(args, CLIENT_ID_METADATA_PATHS))
                .or_else(|| request_header_string(request, "x-mcpace-client-id"))
                .or_else(|| request_header_string(request, "x-codex-client-id"))
                .unwrap_or_else(|| "local-http".to_string()),
        ),
        session_id: optional_http_string(args, "sessionId")?
            .or_else(|| first_http_metadata_string(args, SESSION_ID_METADATA_PATHS))
            .or_else(|| request_header_string(request, "mcp-session-id"))
            .or_else(|| request_header_string(request, "x-mcpace-session-id"))
            .or_else(|| request_header_string(request, "x-codex-session-id")),
        project_root: optional_http_string(args, "projectRoot")?
            .or_else(|| first_http_metadata_string(args, PROJECT_ROOT_METADATA_PATHS)),
        transport: Some(
            optional_http_string(args, "transport")?
                .or_else(|| first_http_metadata_string(args, TRANSPORT_METADATA_PATHS))
                .unwrap_or_else(|| "streamable-http".to_string()),
        ),
        metadata: json_helpers::value_at_path(args, &["metadata"]).cloned(),
        ttl_ms: json_helpers::value_at_path(args, &["ttlMs"])
            .and_then(JsonValue::as_i64)
            .filter(|value| *value > 0)
            .map(|value| value as u128),
        allow_arguments: http_allow_arguments(args)?,
        allowed_tool_risk_classes: http_allowed_tool_risk_classes(args)?,
    })
}

const CLIENT_ID_METADATA_PATHS: &[&[&str]] = &[
    &["metadata", "client", "id"],
    &["metadata", "clientId"],
    &["metadata", "clientProfileId"],
    &["metadata", "context", "clientId"],
];

const SESSION_ID_METADATA_PATHS: &[&[&str]] = &[
    &["metadata", "session", "id"],
    &["metadata", "sessionId"],
    &["metadata", "externalSessionId"],
    &["metadata", "conversationId"],
    &["metadata", "context", "sessionId"],
    &["metadata", "context", "externalSessionId"],
    &["metadata", "headers", "Mcp-Session-Id"],
    &["metadata", "headers", "mcp-session-id"],
];

const PROJECT_ROOT_METADATA_PATHS: &[&[&str]] = &[
    &["metadata", "projectRoot"],
    &["metadata", "workspaceRoot"],
    &["metadata", "workspace", "root"],
    &["metadata", "context", "projectRoot"],
    &["metadata", "context", "cwd"],
    &["metadata", "cwd"],
];

const TRANSPORT_METADATA_PATHS: &[&[&str]] = &[
    &["metadata", "transport"],
    &["metadata", "ingress"],
    &["metadata", "context", "transport"],
];

fn first_http_metadata_string(args: &JsonValue, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| {
        json_helpers::string_at_path(args, path)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn request_header_string(request: Option<&HttpRequest>, key: &str) -> Option<String> {
    let key = key.to_ascii_lowercase();
    request.and_then(|request| {
        request
            .headers
            .iter()
            .find(|(candidate, _)| candidate == &key)
            .map(|(_, value)| value.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn optional_http_string(args: &JsonValue, key: &str) -> Result<Option<String>, String> {
    match json_helpers::value_at_path(args, &[key]) {
        Some(JsonValue::String(value)) => Ok(Some(value.clone())),
        Some(JsonValue::Null) | None => Ok(None),
        Some(_) => Err(format!("{} must be a string when provided", key)),
    }
}

fn http_allow_arguments(args: &JsonValue) -> Result<BTreeSet<String>, String> {
    upstream::collect_allow_arguments(args).map_err(|error| format!("{} when provided", error))
}

fn http_allowed_tool_risk_classes(args: &JsonValue) -> Result<BTreeSet<String>, String> {
    upstream::collect_allowed_tool_risk_classes(args)
        .map_err(|error| format!("{} when provided", error))
}

fn runtime_diagnostics(config: &DashboardConfig) -> Result<JsonValue, String> {
    let root_path = &config.root_path;
    let inventory_root = root_path.to_path_buf();
    let inventory_handle = thread::spawn(move || upstream::configured_inventory(&inventory_root));
    let mut command_results = run_json_commands_parallel(
        root_path,
        vec![
            ("doctor", vec!["doctor", "--json"]),
            ("hub", vec!["hub", "status", "--json"]),
            (
                "serverCapabilities",
                vec!["server", "capabilities", "--json"],
            ),
        ],
    )?;
    let doctor = take_parallel_result(&mut command_results, "doctor")?;
    let hub_status = take_parallel_result(&mut command_results, "hub")?;
    let server_capabilities = take_parallel_result(&mut command_results, "serverCapabilities")?;
    let upstream_inventory = match inventory_handle.join() {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => JsonValue::object([
            ("ok", JsonValue::bool(false)),
            ("error", JsonValue::string(error)),
            ("servers", JsonValue::array([])),
        ]),
        Err(_) => JsonValue::object([
            ("ok", JsonValue::bool(false)),
            (
                "error",
                JsonValue::string("upstream inventory worker panicked"),
            ),
            ("servers", JsonValue::array([])),
        ]),
    };
    let server_items = server_capabilities.as_array().unwrap_or(&[]);
    let server_diagnostics = server_items
        .iter()
        .map(server_runtime_diagnostic)
        .collect::<Vec<_>>();
    let effective_enabled_count = server_items
        .iter()
        .filter(|server| json_helpers::bool_at_path(server, &["effectiveEnabled"]).unwrap_or(false))
        .count();
    let exposed_tools = http_tool_names();

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(true)),
        (
            "surface",
            JsonValue::string("mcpace-management-http-mcp"),
        ),
        (
            "summary",
            JsonValue::string(
                "MCPace HTTP MCP is reachable. This build exposes management tools plus dynamic adapter_profile/upstream_search and explicit stdio upstream access through surface_manifest/upstream_catalog/upstream_probe/upstream_policy_audit/upstream_policy_suggest/upstream_tools/upstream_call/upstream_batch; in auto/native exposure mode, upstream tools may also be advertised as projected u_<server>_<tool>_<hash> names when the live catalog fits the token budget.",
            ),
        ),
        ("doctor", doctor),
        ("hub", hub_status),
        ("runtime", runtime_status_json(config)),
        (
            "upstreamForwarding",
            JsonValue::object([
                ("implemented", JsonValue::bool(true)),
                ("stdioBridgeImplemented", JsonValue::bool(true)),
                (
                    "callableConfiguredStdioServerCount",
                    json_helpers::value_at_path(
                        &upstream_inventory,
                        &["callableConfiguredStdioServerCount"],
                    )
                    .cloned()
                    .unwrap_or_else(|| JsonValue::number(0)),
                ),
                (
                    "reason",
                    JsonValue::string(
                        "MCPace forwards resolvable configured stdio upstreams through upstream_tools/upstream_call and uses upstream_batch for stateful multi-call sessions. upstream_catalog lists concise tool descriptions, upstream_probe checks configured servers, upstream_policy_audit compares MCP annotations with declarative toolPolicies, and upstream_policy_suggest generates reviewable policy candidates without hardcoded server names. Non-stdio HTTP upstream fan-out remains explicit blocked diagnostics.",
                    ),
                ),
            ]),
        ),
        (
            "surfaceContract",
            JsonValue::object([
                (
                    "nativeTopLevelClaim",
                    JsonValue::string(
                        "tools/list returns adapter management tools plus budgeted projected upstream tools when MCPACE_TOOL_EXPOSURE allows them.",
                    ),
                ),
                (
                    "upstreamProjection",
                    JsonValue::string(
                        "Configured upstream tools can be exposed as stable u_<server>_<tool>_<hash> names when the live catalog fits the token budget; broker discovery remains available through upstream_search/upstream_catalog/upstream_tools.",
                    ),
                ),
                ("directTopLevelProjectionEnabled", JsonValue::bool(true)),
            ]),
        ),
        ("upstreamInventory", upstream_inventory),
        (
            "managementTools",
            JsonValue::object([
                ("count", JsonValue::number(exposed_tools.len())),
                (
                    "names",
                    JsonValue::array(exposed_tools.into_iter().map(JsonValue::string)),
                ),
            ]),
        ),
        (
            "configuredServers",
            JsonValue::object([
                ("count", JsonValue::number(server_items.len())),
                ("effectiveEnabledCount", JsonValue::number(effective_enabled_count)),
                ("items", JsonValue::array(server_diagnostics)),
            ]),
        ),
        (
            "nextSafeAction",
            JsonValue::string(
                "Use adapter_profile to see whether the current tools/list projected upstream tools natively or fell back to broker mode. Use upstream_search/upstream_catalog/upstream_tools for discovery, upstream_call for stateless calls, and upstream_batch for stateful same-server sequences.",
            ),
        ),
    ]))
}

fn server_runtime_diagnostic(server: &JsonValue) -> JsonValue {
    let name = json_helpers::string_at_path(server, &["name"]).unwrap_or("unknown");
    let kind = json_helpers::string_at_path(server, &["kind"]).unwrap_or("unknown");
    let source_type = json_helpers::string_at_path(server, &["sourceType"]).unwrap_or("");
    let effective_enabled =
        json_helpers::bool_at_path(server, &["effectiveEnabled"]).unwrap_or(false);
    let required = json_helpers::bool_at_path(server, &["required"]).unwrap_or(false);
    let auto_start = json_helpers::bool_at_path(server, &["autoStart"]).unwrap_or(false);
    let runtime_callable = effective_enabled && source_type == "stdio";
    let (status, reason) = if !effective_enabled {
        (
            "disabled",
            "server is disabled by source/profile/default configuration",
        )
    } else if runtime_callable {
        (
            "callable-stdio-bridge",
            "enabled stdio upstream can be listed with upstream_tools and called with upstream_call",
        )
    } else if kind == "host-bridge" {
        (
            "blocked-preview-host-bridge",
            "host-bridge policy is configured, but MCPace does not currently launch or proxy non-stdio bridges through this HTTP adapter",
        )
    } else if kind == "container-stdio" {
        (
            "blocked-nonstdio-or-missing-command",
            "this entry is not currently callable through the stdio bridge; check upstreamInventory for command/source details",
        )
    } else if kind == "external-http" || kind == "remote-http" {
        (
            "blocked-preview-http-upstream",
            "external/remote HTTP server policy is configured, but live HTTP upstream fan-out is not implemented in this HTTP adapter",
        )
    } else {
        (
            "blocked-preview-unknown-upstream",
            "this upstream kind is configured as inventory only and is not exposed as a callable MCP tool by this HTTP adapter",
        )
    };

    JsonValue::object([
        ("name", JsonValue::string(name)),
        ("kind", JsonValue::string(kind)),
        ("effectiveEnabled", JsonValue::bool(effective_enabled)),
        ("required", JsonValue::bool(required)),
        ("autoStart", JsonValue::bool(auto_start)),
        ("runtimeCallable", JsonValue::bool(runtime_callable)),
        ("exposedAsMcpTool", JsonValue::bool(false)),
        ("status", JsonValue::string(status)),
        ("reason", JsonValue::string(reason)),
        ("sourceType", JsonValue::string(source_type)),
        (
            "healthUrl",
            JsonValue::string(json_helpers::string_at_path(server, &["healthUrl"]).unwrap_or("")),
        ),
        (
            "requiredCommands",
            json_helpers::value_at_path(server, &["requiredCommands"])
                .cloned()
                .unwrap_or_else(|| JsonValue::array([])),
        ),
    ])
}

fn run_json_command(root_path: &Path, args: &[&str]) -> Result<JsonValue, String> {
    run_json_command_vec(
        root_path,
        args.iter().map(|value| (*value).to_string()).collect(),
    )
}

fn run_json_command_vec(root_path: &Path, mut args: Vec<String>) -> Result<JsonValue, String> {
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

fn write_json_response(
    stream: &mut TcpStream,
    status: &str,
    payload: &JsonValue,
) -> Result<(), String> {
    write_response(
        stream,
        status,
        "application/json; charset=utf-8",
        payload.to_pretty_string().as_bytes(),
    )
}

fn write_text_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) -> Result<(), String> {
    write_response(stream, status, content_type, body.as_bytes())
}

fn write_empty_response(stream: &mut TcpStream, status: &str) -> Result<(), String> {
    write_response(stream, status, "text/plain; charset=utf-8", &[])
}

fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> Result<(), String> {
    let header = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        status,
        content_type,
        body.len()
    );
    stream
        .write_all(header.as_bytes())
        .map_err(|error| format!("write response header: {}", error))?;
    stream
        .write_all(body)
        .map_err(|error| format!("write response body: {}", error))?;
    stream
        .flush()
        .map_err(|error| format!("flush response: {}", error))?;
    let _ = stream.shutdown(Shutdown::Both);
    Ok(())
}

fn split_target(target: &str) -> (&str, &str) {
    match target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (target, ""),
    }
}

fn query_parameter<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query
        .split('&')
        .find_map(|pair| match pair.split_once('=') {
            Some((candidate, value)) if candidate == key => Some(value),
            _ => None,
        })
}

fn empty_object() -> JsonValue {
    JsonValue::object::<String, Vec<(String, JsonValue)>>(Vec::new())
}

fn sanitize_root_path(root_path: &str) -> String {
    root_path
        .strip_prefix(r"\\?\")
        .unwrap_or(root_path)
        .to_string()
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

const DASHBOARD_HTML: &str = r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>MCPace dashboard</title>
    <style>
      :root {
        color-scheme: dark;
        --bg: #09111d;
        --panel: rgba(18, 27, 43, 0.88);
        --panel-strong: rgba(10, 18, 31, 0.96);
        --line: rgba(148, 163, 184, 0.14);
        --text: #ebf2ff;
        --muted: #97a6bf;
        --accent: #6ee7f9;
        --accent-2: #7c3aed;
        --good: #34d399;
        --warn: #fbbf24;
        --bad: #fb7185;
        --shadow: 0 22px 60px rgba(0, 0, 0, 0.34);
        --radius: 22px;
        --mono: "SFMono-Regular", Consolas, "Liberation Mono", Menlo, monospace;
        --sans: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont,
          "Segoe UI", sans-serif;
      }

      * { box-sizing: border-box; }
      html, body { margin: 0; padding: 0; background:
        radial-gradient(circle at top left, rgba(124, 58, 237, 0.18), transparent 34%),
        radial-gradient(circle at top right, rgba(110, 231, 249, 0.16), transparent 28%),
        var(--bg); color: var(--text); font-family: var(--sans); }

      body { min-height: 100vh; }
      .shell {
        max-width: 1400px;
        margin: 0 auto;
        padding: 36px 24px 80px;
      }

      .hero {
        position: relative;
        overflow: hidden;
        background: linear-gradient(135deg, rgba(14, 24, 38, 0.98), rgba(20, 29, 46, 0.92));
        border: 1px solid var(--line);
        border-radius: 28px;
        box-shadow: var(--shadow);
        padding: 28px 28px 24px;
        margin-bottom: 22px;
      }

      .hero::after {
        content: "";
        position: absolute;
        inset: auto -10% -60% auto;
        width: 420px;
        height: 420px;
        background: radial-gradient(circle, rgba(110, 231, 249, 0.18), transparent 62%);
        pointer-events: none;
      }

      .eyebrow {
        color: var(--accent);
        text-transform: uppercase;
        letter-spacing: 0.24em;
        font-size: 12px;
        margin-bottom: 12px;
      }

      h1 {
        margin: 0;
        font-size: clamp(32px, 5vw, 52px);
        line-height: 0.95;
        letter-spacing: -0.04em;
      }

      .subhead {
        margin: 14px 0 0;
        max-width: 780px;
        color: var(--muted);
        font-size: 15px;
        line-height: 1.65;
      }

      .hero-meta,
      .action-row,
      .metric-grid,
      .layout {
        display: grid;
        gap: 16px;
      }

      .hero-meta {
        grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
        margin-top: 22px;
      }

      .meta-card,
      .card {
        background: var(--panel);
        border: 1px solid var(--line);
        border-radius: var(--radius);
        box-shadow: var(--shadow);
      }

      .meta-card {
        padding: 16px 18px;
      }

      .meta-label {
        color: var(--muted);
        font-size: 12px;
        text-transform: uppercase;
        letter-spacing: 0.18em;
        margin-bottom: 10px;
      }

      .meta-value {
        font-size: 20px;
        font-weight: 650;
        letter-spacing: -0.03em;
      }

      .meta-value code {
        font-family: var(--mono);
        font-size: 14px;
        color: var(--accent);
      }

      .action-row {
        grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
        margin: 18px 0 22px;
      }

      button,
      input {
        font: inherit;
      }

      button {
        appearance: none;
        border: 1px solid rgba(110, 231, 249, 0.28);
        background: linear-gradient(135deg, rgba(33, 48, 72, 0.96), rgba(20, 29, 46, 0.96));
        color: var(--text);
        border-radius: 16px;
        padding: 14px 16px;
        cursor: pointer;
        transition: transform 140ms ease, border-color 140ms ease, background 140ms ease;
      }

      button:hover {
        transform: translateY(-1px);
        border-color: rgba(110, 231, 249, 0.56);
      }

      button:active { transform: translateY(0); }
      button.accent {
        background: linear-gradient(135deg, rgba(14, 165, 233, 0.28), rgba(124, 58, 237, 0.34));
      }

      .metric-grid {
        grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
        margin-bottom: 22px;
      }

      .metric {
        padding: 18px;
      }

      .metric h2 {
        margin: 0 0 10px;
        color: var(--muted);
        font-size: 12px;
        text-transform: uppercase;
        letter-spacing: 0.16em;
      }

      .metric-value {
        font-size: 34px;
        font-weight: 700;
        letter-spacing: -0.05em;
      }

      .metric-note {
        margin-top: 10px;
        color: var(--muted);
        font-size: 13px;
      }

      .layout {
        grid-template-columns: 1.25fr 0.95fr;
        align-items: start;
      }

      .stack {
        display: grid;
        gap: 16px;
      }

      .card {
        padding: 22px;
      }

      .card h2 {
        margin: 0;
        font-size: 22px;
        letter-spacing: -0.03em;
      }

      .card p.section-note {
        margin: 10px 0 0;
        color: var(--muted);
        font-size: 14px;
        line-height: 1.6;
      }

      .toolbar {
        display: flex;
        flex-wrap: wrap;
        gap: 10px;
        margin: 18px 0 16px;
      }

      .toolbar input {
        flex: 1 1 220px;
        min-width: 200px;
        border-radius: 14px;
        border: 1px solid var(--line);
        background: rgba(10, 18, 31, 0.88);
        color: var(--text);
        padding: 12px 14px;
      }

      .chip-row {
        display: flex;
        flex-wrap: wrap;
        gap: 10px;
      }

      .chip {
        display: inline-flex;
        align-items: center;
        gap: 8px;
        padding: 8px 12px;
        border-radius: 999px;
        background: rgba(12, 19, 31, 0.9);
        border: 1px solid var(--line);
        color: var(--muted);
        font-size: 13px;
      }

      .dot {
        width: 9px;
        height: 9px;
        border-radius: 50%;
        background: var(--muted);
      }

      .dot.good { background: var(--good); }
      .dot.warn { background: var(--warn); }
      .dot.bad { background: var(--bad); }

      .server-list,
      .warning-list,
      .client-list,
      .log-list {
        display: grid;
        gap: 12px;
      }

      .server-item,
      .warning-item,
      .client-item,
      .log-item {
        border-radius: 18px;
        border: 1px solid var(--line);
        background: rgba(10, 18, 31, 0.88);
        padding: 16px;
      }

      .server-top,
      .client-top,
      .log-top {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
      }

      .server-name,
      .client-name,
      .log-name {
        font-size: 16px;
        font-weight: 650;
        letter-spacing: -0.02em;
      }

      .server-meta,
      .client-meta,
      .log-meta,
      .warning-item {
        color: var(--muted);
        font-size: 13px;
        line-height: 1.55;
      }

      .server-tags,
      .client-tags {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
        margin-top: 12px;
      }

      .tag {
        border-radius: 999px;
        padding: 6px 10px;
        font-size: 12px;
        color: var(--text);
        background: rgba(110, 231, 249, 0.09);
        border: 1px solid rgba(110, 231, 249, 0.16);
      }

      details {
        margin-top: 14px;
        border-top: 1px solid rgba(148, 163, 184, 0.12);
        padding-top: 12px;
      }

      summary {
        cursor: pointer;
        color: var(--muted);
      }

      pre {
        white-space: pre-wrap;
        word-break: break-word;
        margin: 12px 0 0;
        font-family: var(--mono);
        font-size: 12px;
        line-height: 1.65;
        color: #d7e6ff;
      }

      .empty {
        color: var(--muted);
        padding: 14px 0;
      }

      .footer-note {
        margin-top: 18px;
        color: var(--muted);
        font-size: 13px;
      }

      @media (max-width: 1040px) {
        .layout { grid-template-columns: 1fr; }
      }
    </style>
  </head>
  <body>
    <main class="shell">
      <section class="hero">
        <div class="eyebrow">MCPace local control surface</div>
        <h1>Operational visibility without<br>leaving the terminal-era stack.</h1>
        <p class="subhead">
          This dashboard stays local-only. It reuses MCPace's native JSON read
          paths and a few safe lifecycle actions, so you can inspect runtime
          health, server coverage, and client surfaces from one polished panel.
        </p>
        <div class="hero-meta">
          <div class="meta-card">
            <div class="meta-label">Workspace root</div>
            <div class="meta-value"><code id="root-path">—</code></div>
          </div>
          <div class="meta-card">
            <div class="meta-label">Last refresh</div>
            <div class="meta-value" id="last-refresh">—</div>
          </div>
          <div class="meta-card">
            <div class="meta-label">Hub state</div>
            <div class="meta-value" id="hero-hub-state">—</div>
          </div>
          <div class="meta-card">
            <div class="meta-label">Warnings</div>
            <div class="meta-value" id="warning-count">—</div>
          </div>
          <div class="meta-card">
            <div class="meta-label">Overview cache</div>
            <div class="meta-value" id="overview-cache">—</div>
          </div>
          <div class="meta-card">
            <div class="meta-label">HTTP workers</div>
            <div class="meta-value" id="runtime-workers">—</div>
          </div>
          <div class="meta-card">
            <div class="meta-label">Upstream pool</div>
            <div class="meta-value" id="upstream-pool">—</div>
          </div>
        </div>
      </section>

      <section class="action-row">
        <button class="accent" id="refresh-button">Refresh dashboard</button>
        <button id="hub-up-button">Hub up</button>
        <button id="hub-down-button">Hub down</button>
        <button id="repair-button">Repair runtime</button>
      </section>

      <section class="metric-grid">
        <article class="card metric">
          <h2>Runtime readiness</h2>
          <div class="metric-value" id="runtime-ready">—</div>
          <div class="metric-note" id="runtime-ready-note">—</div>
        </article>
        <article class="card metric">
          <h2>Enabled servers</h2>
          <div class="metric-value" id="enabled-servers">—</div>
          <div class="metric-note" id="enabled-servers-note">—</div>
        </article>
        <article class="card metric">
          <h2>Required servers</h2>
          <div class="metric-value" id="required-servers">—</div>
          <div class="metric-note" id="required-servers-note">—</div>
        </article>
        <article class="card metric">
          <h2>Client surfaces</h2>
          <div class="metric-value" id="client-surfaces">—</div>
          <div class="metric-note" id="client-surfaces-note">—</div>
        </article>
      </section>

      <section class="layout">
        <div class="stack">
          <article class="card">
            <h2>Servers</h2>
            <p class="section-note">
              Filter by name and focus on the currently effective set. Each
              entry keeps the transport, policy, and scope data visible.
            </p>
            <div class="toolbar">
              <input id="server-search" type="search" placeholder="Search servers by name, kind, or scope">
              <button id="toggle-enabled-filter">Show enabled only</button>
            </div>
            <div class="chip-row" id="server-summary-chips"></div>
            <div class="server-list" id="server-list"></div>
          </article>

          <article class="card">
            <h2>Hub logs</h2>
            <p class="section-note">
              Recent lifecycle events from the local runtime log.
            </p>
            <div class="log-list" id="log-list"></div>
          </article>
        </div>

        <div class="stack">
          <article class="card">
            <h2>Runtime posture</h2>
            <p class="section-note">
              A compact operational read across doctor, readiness, and hub
              status.
            </p>
            <div class="chip-row" id="runtime-chip-row"></div>
            <div class="footer-note" id="runtime-summary">—</div>
          </article>

          <article class="card">
            <h2>Warnings</h2>
            <p class="section-note">
              Lifecycle or readiness warnings that deserve attention before a
              client session starts routing through the hub.
            </p>
            <div class="warning-list" id="warning-list"></div>
          </article>

          <article class="card">
            <h2>Client surfaces</h2>
            <p class="section-note">
              The documented client catalog, grouped by family and surface
              class. This helps you see where MCPace already has a believable
              connection story.
            </p>
            <div class="chip-row" id="client-summary-chips"></div>
            <div class="client-list" id="client-list"></div>
          </article>
        </div>
      </section>
    </main>

    <script>
      const state = {
        overview: null,
        logs: [],
        enabledOnly: true,
        serverQuery: ""
      };

      const els = {
        rootPath: document.getElementById("root-path"),
        lastRefresh: document.getElementById("last-refresh"),
        heroHubState: document.getElementById("hero-hub-state"),
        warningCount: document.getElementById("warning-count"),
        overviewCache: document.getElementById("overview-cache"),
        runtimeWorkers: document.getElementById("runtime-workers"),
        upstreamPool: document.getElementById("upstream-pool"),
        runtimeReady: document.getElementById("runtime-ready"),
        runtimeReadyNote: document.getElementById("runtime-ready-note"),
        enabledServers: document.getElementById("enabled-servers"),
        enabledServersNote: document.getElementById("enabled-servers-note"),
        requiredServers: document.getElementById("required-servers"),
        requiredServersNote: document.getElementById("required-servers-note"),
        clientSurfaces: document.getElementById("client-surfaces"),
        clientSurfacesNote: document.getElementById("client-surfaces-note"),
        serverList: document.getElementById("server-list"),
        warningList: document.getElementById("warning-list"),
        clientList: document.getElementById("client-list"),
        logList: document.getElementById("log-list"),
        serverSearch: document.getElementById("server-search"),
        toggleEnabledFilter: document.getElementById("toggle-enabled-filter"),
        runtimeChipRow: document.getElementById("runtime-chip-row"),
        runtimeSummary: document.getElementById("runtime-summary"),
        serverSummaryChips: document.getElementById("server-summary-chips"),
        clientSummaryChips: document.getElementById("client-summary-chips")
      };

      async function fetchJson(url, options) {
        const response = await fetch(url, options);
        if (!response.ok) {
          throw new Error(`${response.status} ${response.statusText}`);
        }
        return response.json();
      }

      async function refreshDashboard(options = {}) {
        try {
          const overviewUrl = options.force ? "/api/overview?refresh=1" : "/api/overview";
          const [overview, logs] = await Promise.all([
            fetchJson(overviewUrl),
            fetchJson("/api/logs?tail=18")
          ]);
          state.overview = overview;
          state.logs = Array.isArray(logs) ? logs : [];
          render();
        } catch (error) {
          console.error(error);
          els.runtimeSummary.textContent = `Dashboard refresh failed: ${error.message}`;
        }
      }

      async function runAction(path) {
        try {
          await fetchJson(path, { method: "POST" });
          await refreshDashboard({ force: true });
        } catch (error) {
          console.error(error);
          els.runtimeSummary.textContent = `Action failed: ${error.message}`;
        }
      }

      function chip(label, tone) {
        const dotClass = tone || "warn";
        return `<span class="chip"><span class="dot ${dotClass}"></span>${escapeHtml(label)}</span>`;
      }

      function toneForBoolean(value) {
        return value ? "good" : "bad";
      }

      function toneForHub(status) {
        if (status === "running" || status === "healthy") return "good";
        if (status === "stopped" || status === "stopped-ready") return "warn";
        return "bad";
      }

      function escapeHtml(value) {
        return String(value ?? "")
          .replaceAll("&", "&amp;")
          .replaceAll("<", "&lt;")
          .replaceAll(">", "&gt;")
          .replaceAll('"', "&quot;");
      }

      function formatDate(value) {
        if (value === null || value === undefined || value === "") return "—";
        if (typeof value === "number") return new Date(value).toLocaleString();
        return new Date(Number(value)).toLocaleString();
      }

      function render() {
        const overview = state.overview;
        if (!overview) return;

        const doctor = overview.doctor || {};
        const project = doctor.project || {};
        const hub = overview.hub || {};
        const readiness = overview.readiness || {};
        const servers = Array.isArray(overview.servers) ? overview.servers : [];
        const clientCatalog = overview.clients || {};
        const clients = Array.isArray(clientCatalog.targets) ? clientCatalog.targets : [];
        const warnings = [
          ...(Array.isArray(hub.warnings) ? hub.warnings : []),
          ...(Array.isArray(readiness.missingRequiredSourceEnablement)
            ? readiness.missingRequiredSourceEnablement.map(value => `Missing required source enablement: ${value}`)
            : []),
          ...(Array.isArray(readiness.missingRequiredCommands)
            ? readiness.missingRequiredCommands.map(value => `Missing required command: ${value}`)
            : [])
        ];

        els.rootPath.textContent = overview.rootPath || "—";
        els.lastRefresh.textContent = formatDate(overview.generatedAtMs);
        els.heroHubState.textContent = hub.status || hub.health || "unknown";
        els.warningCount.textContent = String(warnings.length);
        const cache = overview.cache || {};
        const runtime = overview.runtime || {};
        const http = runtime.http || {};
        const upstreamPool = runtime.upstreamSessionPool || {};
        els.overviewCache.textContent = cache.hit ? `hit · ${cache.ttlMs ?? "?"}ms` : cache.bypassed ? "fresh bypass" : "fresh";
        els.runtimeWorkers.textContent = `${http.activeConnections ?? 0}/${http.maxConnections ?? "?"} active`;
        els.upstreamPool.textContent = `${upstreamPool.size ?? 0}/${upstreamPool.maxSize ?? "?"} · ${upstreamPool.shardCount ?? 1} shard${(upstreamPool.shardCount ?? 1) === 1 ? "" : "s"}`;

        els.runtimeReady.textContent = readiness.runtimePrerequisitesReady ? "Ready" : "Blocked";
        els.runtimeReadyNote.textContent =
          `Rust ${project.rustSourceReady ? "ok" : "missing"} · npm ${project.npmSurfaceReady ? "ok" : "missing"} · Docker ${project.containerToolingReady ? "ok" : "missing"}`;
        els.enabledServers.textContent = String(readiness.effectiveEnabledServerCount ?? hub.effectiveEnabledServerCount ?? 0);
        els.enabledServersNote.textContent =
          `${readiness.sourceEnabledServerCount ?? 0} source-enabled · ${readiness.profileEnabledServerCount ?? 0} profile-enabled`;
        els.requiredServers.textContent = String(readiness.requiredServerCount ?? hub.requiredServerCount ?? 0);
        els.requiredServersNote.textContent =
          `${readiness.requiredSourceEnabledCount ?? 0} required source-enabled`;
        els.clientSurfaces.textContent = String(clients.length);
        els.clientSurfacesNote.textContent =
          `${Object.keys(clientCatalog.familyCounts || {}).length} client families`;

        els.runtimeChipRow.innerHTML = [
          chip(`Hub ${hub.status || "unknown"}`, toneForHub(hub.status || hub.health)),
          chip(`Read-only ops ${hub.readyForReadOnlyOps ? "ready" : "blocked"}`, toneForBoolean(Boolean(hub.readyForReadOnlyOps))),
          chip(`Runtime ops ${hub.readyForRuntimeOps ? "ready" : "blocked"}`, toneForBoolean(Boolean(hub.readyForRuntimeOps))),
          chip(`HTTP ${http.activeConnections ?? 0}/${http.maxConnections ?? "?"}`, (http.activeConnections ?? 0) < (http.maxConnections ?? 1) ? "good" : "warn"),
          chip(`Pool shards ${upstreamPool.shardCount ?? 1}`, "warn"),
          chip(`Profile ${hub.activeProfile || readiness.activeProfile || "unknown"}`, "warn"),
          chip(`Client key ${clientCatalog.configuredClientKeyName || "none"}`, "warn")
        ].join("");

        els.runtimeSummary.textContent =
          `Hub ${hub.status || "unknown"} · ${readiness.serverCount ?? 0} configured servers · generated ${formatDate(overview.generatedAtMs)}`;

        const enabledCount = servers.filter(server => server.effectiveEnabled).length;
        const requiredCount = servers.filter(server => server.required).length;
        const projectLocalCount = servers.filter(server => server.scopeClass === "project-local").length;
        els.serverSummaryChips.innerHTML = [
          chip(`${enabledCount} enabled`, enabledCount > 0 ? "good" : "warn"),
          chip(`${requiredCount} required`, requiredCount > 0 ? "warn" : "good"),
          chip(`${projectLocalCount} project-local`, projectLocalCount > 0 ? "warn" : "good"),
          chip(`${servers.length} total`, "warn")
        ].join("");

        const familyCounts = clientCatalog.familyCounts || {};
        const surfaceCounts = clientCatalog.surfaceClassCounts || {};
        els.clientSummaryChips.innerHTML = [
          ...Object.entries(surfaceCounts).map(([key, value]) => chip(`${value} ${key}`, key === "local" ? "good" : "warn")),
          ...Object.entries(familyCounts).slice(0, 4).map(([key, value]) => chip(`${value} ${key}`, "warn"))
        ].join("");

        renderServers(servers);
        renderWarnings(warnings);
        renderClients(clients);
        renderLogs(state.logs);
      }

      function renderServers(servers) {
        const query = state.serverQuery.trim().toLowerCase();
        const filtered = servers.filter(server => {
          if (state.enabledOnly && !server.effectiveEnabled) return false;
          if (!query) return true;
          return [
            server.name,
            server.kind,
            server.scopeClass,
            server.transportPreference
          ].join(" ").toLowerCase().includes(query);
        });

        if (!filtered.length) {
          els.serverList.innerHTML = `<div class="empty">No servers match the current filter.</div>`;
          return;
        }

        els.serverList.innerHTML = filtered.map(server => {
          const tone = server.effectiveEnabled ? "good" : server.required ? "warn" : "bad";
          return `
            <article class="server-item">
              <div class="server-top">
                <div class="server-name">${escapeHtml(server.name)}</div>
                <span class="chip"><span class="dot ${tone}"></span>${server.effectiveEnabled ? "enabled" : "disabled"}</span>
              </div>
              <div class="server-meta">
                ${escapeHtml(server.kind)} · scope ${escapeHtml(server.scopeClass)} · concurrency ${escapeHtml(server.concurrencyPolicy)}
              </div>
              <div class="server-tags">
                <span class="tag">${server.required ? "required" : "optional"}</span>
                <span class="tag">${escapeHtml(server.transportPreference || "stdio-default")}</span>
                <span class="tag">${escapeHtml(server.stateBinding)}</span>
                <span class="tag">${escapeHtml(server.credentialBinding)}</span>
              </div>
            </article>
          `;
        }).join("");
      }

      function renderWarnings(warnings) {
        if (!warnings.length) {
          els.warningList.innerHTML = `<div class="empty">No active readiness warnings.</div>`;
          return;
        }
        els.warningList.innerHTML = warnings.map(text => `
          <article class="warning-item">${escapeHtml(text)}</article>
        `).join("");
      }

      function renderClients(clients) {
        const preview = clients.slice(0, 10);
        if (!preview.length) {
          els.clientList.innerHTML = `<div class="empty">No documented client surfaces were returned.</div>`;
          return;
        }
        els.clientList.innerHTML = preview.map(client => `
          <article class="client-item">
            <div class="client-top">
              <div class="client-name">${escapeHtml(client.displayName)}</div>
              <span class="chip"><span class="dot ${client.surfaceClass === "local" ? "good" : "warn"}"></span>${escapeHtml(client.surfaceClass)}</span>
            </div>
            <div class="client-meta">
              ${escapeHtml(client.id)} · ${escapeHtml(client.surfaceKind)} · ingress ${escapeHtml((client.supportedIngresses || []).join(", "))}
            </div>
            <div class="client-tags">
              ${(client.nativeScopes || []).slice(0, 4).map(scope => `<span class="tag">${escapeHtml(scope)}</span>`).join("")}
            </div>
          </article>
        `).join("");
      }

      function renderLogs(logs) {
        if (!Array.isArray(logs) || !logs.length) {
          els.logList.innerHTML = `<div class="empty">No recent log entries.</div>`;
          return;
        }
        els.logList.innerHTML = logs.slice().reverse().map(entry => `
          <article class="log-item">
            <div class="log-top">
              <div class="log-name">${escapeHtml(entry.event || "event")}</div>
              <span class="chip"><span class="dot ${entry.level === "warn" ? "warn" : entry.level === "error" ? "bad" : "good"}"></span>${escapeHtml(entry.level || "info")}</span>
            </div>
            <div class="log-meta">${formatDate(entry.tsMs)}</div>
            <details>
              <summary>View raw event payload</summary>
              <pre>${escapeHtml(JSON.stringify(entry, null, 2))}</pre>
            </details>
          </article>
        `).join("");
      }

      document.getElementById("refresh-button").addEventListener("click", () => refreshDashboard({ force: true }));
      document.getElementById("hub-up-button").addEventListener("click", () => runAction("/api/actions/hub-up"));
      document.getElementById("hub-down-button").addEventListener("click", () => runAction("/api/actions/hub-down"));
      document.getElementById("repair-button").addEventListener("click", () => runAction("/api/actions/repair"));
      els.serverSearch.addEventListener("input", event => {
        state.serverQuery = event.target.value;
        render();
      });
      els.toggleEnabledFilter.addEventListener("click", () => {
        state.enabledOnly = !state.enabledOnly;
        els.toggleEnabledFilter.textContent = state.enabledOnly ? "Show all servers" : "Show enabled only";
        render();
      });

      refreshDashboard();
      window.setInterval(refreshDashboard, 15000);
    </script>
  </body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::{
        build_overview_json, cached_health_json, cached_overview_json, is_allowed_local_origin,
        query_bool_flag, run_http_tool, run_json_command, runtime_status_json, serve_listener,
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
            metrics: super::HttpRuntimeMetrics::default(),
            surface,
            upstream_session_pools: super::new_upstream_session_pools(),
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
            json_helpers::value_at_path(
                &response,
                &["runtime", "upstreamSessionPool", "shardCount"],
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
    fn http_upstream_call_attaches_and_releases_runtime_lease() {
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

        let header_context =
            super::http_upstream_lease_context(&super::empty_object(), Some(&request))
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
            "null",
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
                test_config(server_root, Some(3), super::ServeSurface::Dashboard),
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

        let initialized = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        let mut initialized_response = String::new();
        let mut stream = TcpStream::connect(addr).unwrap();
        write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            addr,
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

        let tools_list = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
        let mut tools_response = String::new();
        let mut stream = TcpStream::connect(addr).unwrap();
        write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            addr,
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
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            addr,
            unsupported_call.len(),
            unsupported_call
        )
        .unwrap();
        stream.read_to_string(&mut unsupported_response).unwrap();
        assert!(
            unsupported_response.contains("\"isError\": true"),
            "unsupported response: {}",
            unsupported_response
        );
        assert!(
            unsupported_response.contains(
                "surface_manifest/upstream_catalog/upstream_probe/upstream_policy_audit/upstream_policy_suggest/upstream_tools/upstream_call/upstream_batch"
            ),
            "unsupported response: {}",
            unsupported_response
        );

        let diagnostics_call = r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"runtime_diagnostics","arguments":{}}}"#;
        let mut diagnostics_response = String::new();
        let mut stream = TcpStream::connect(addr).unwrap();
        write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            addr,
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
}
