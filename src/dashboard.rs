use crate::app;
use crate::json::{parse_str, JsonValue};
use crate::json_helpers;
use crate::runtimepaths;
use crate::upstream;
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
struct ParsedArgs {
    help: bool,
    root_override: Option<PathBuf>,
    host: String,
    port: u16,
    max_requests: Option<usize>,
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
            error: None,
        }
    }
}

struct DashboardConfig {
    root_path: PathBuf,
    max_requests: Option<usize>,
    surface: ServeSurface,
    upstream_session_pool: Mutex<upstream::UpstreamSessionPool>,
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
            surface,
            upstream_session_pool: Mutex::new(upstream::UpstreamSessionPool::default()),
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
                match value.parse::<usize>() {
                    Ok(limit) => parsed.max_requests = Some(limit),
                    Err(_) => {
                        parsed.error =
                            Some("dashboard --max-requests must be a valid integer".to_string());
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
        "Usage: mcpace dashboard [--root <path>] [--host <addr>] [--port <n>] [--max-requests <n>]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "dashboard serves a local browser UI for hub status, verification, servers, clients, and logs."
    );
    let _ = writeln!(
        stdout,
        "The UI stays local-only and reuses existing native JSON command surfaces."
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

fn serve_listener(listener: TcpListener, config: DashboardConfig, stderr: &mut dyn Write) -> i32 {
    for (index, incoming) in listener.incoming().enumerate() {
        let stream = match incoming {
            Ok(value) => value,
            Err(error) => {
                let _ = writeln!(stderr, "dashboard accept failed: {}", error);
                return 1;
            }
        };

        if let Err(error) = handle_connection(stream, &config) {
            let _ = writeln!(stderr, "dashboard request failed: {}", error);
        }
        let handled = index + 1;
        if config
            .max_requests
            .map(|limit| handled >= limit)
            .unwrap_or(false)
        {
            break;
        }
    }
    0
}

fn handle_connection(mut stream: TcpStream, config: &DashboardConfig) -> Result<(), String> {
    let mut reader = BufReader::new(
        stream
            .try_clone()
            .map_err(|error| format!("clone stream: {}", error))?,
    );
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|error| format!("read request line: {}", error))?;

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
    loop {
        let mut header = String::new();
        reader
            .read_line(&mut header)
            .map_err(|error| format!("read request header: {}", error))?;
        if header == "\r\n" || header == "\n" || header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            let key = name.trim().to_ascii_lowercase();
            let trimmed = value.trim().to_string();
            if key == "content-length" {
                content_length = trimmed.parse::<usize>().unwrap_or(0);
            }
            headers.push((key, trimmed));
        }
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

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => write_text_response(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            DASHBOARD_HTML,
        )?,
        ("GET", "/healthz") => {
            let readiness =
                run_json_command(&config.root_path, &["verify", "readiness", "--json"])?;
            let ok =
                json_helpers::bool_at_path(&readiness, &["readyForRuntimeOps"]).unwrap_or(false);
            let payload = JsonValue::object([
                ("ok", JsonValue::bool(ok)),
                ("generatedAtMs", JsonValue::number(now_ms())),
                ("readiness", readiness),
            ]);
            write_json_response(&mut stream, "200 OK", &payload)?;
        }
        ("GET", "/status") => {
            let payload = run_json_command(&config.root_path, &["hub", "status", "--json"])?;
            write_json_response(&mut stream, "200 OK", &payload)?;
        }
        ("GET", "/api/overview") => {
            let payload = build_overview_json(&config.root_path)?;
            write_json_response(&mut stream, "200 OK", &payload)?;
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
            write_json_response(&mut stream, "200 OK", &payload)?;
        }
        ("GET", "/mcp") => {
            if accepts(&request, "text/event-stream") {
                write_empty_response(&mut stream, "405 Method Not Allowed")?;
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
            write_json_response(&mut stream, "200 OK", &payload)?;
        }
        ("POST", "/mcp") => {
            let response = match handle_mcp_http_request(&request, config) {
                Ok(value) => value,
                Err(error) => McpHttpResponse::JsonStatus(
                    "400 Bad Request",
                    mcp_error_response(JsonValue::Null, -32700, error),
                ),
            };
            match response {
                McpHttpResponse::Json(payload) => {
                    write_json_response(&mut stream, "200 OK", &payload)?
                }
                McpHttpResponse::JsonStatus(status, payload) => {
                    write_json_response(&mut stream, status, &payload)?
                }
                McpHttpResponse::Accepted => write_empty_response(&mut stream, "202 Accepted")?,
            }
        }
        ("POST", "/api/actions/hub-up") => {
            if reject_forbidden_origin(&mut stream, &request)? {
                return Ok(());
            }
            let payload = action_response(
                "hub-up",
                run_json_command(&config.root_path, &["hub", "up", "--json"])?,
            );
            write_json_response(&mut stream, "200 OK", &payload)?;
        }
        ("POST", "/api/actions/hub-down") => {
            if reject_forbidden_origin(&mut stream, &request)? {
                return Ok(());
            }
            let payload = action_response(
                "hub-down",
                run_json_command(&config.root_path, &["hub", "down", "--json"])?,
            );
            write_json_response(&mut stream, "200 OK", &payload)?;
        }
        ("POST", "/api/actions/repair") => {
            if reject_forbidden_origin(&mut stream, &request)? {
                return Ok(());
            }
            let payload = action_response(
                "repair",
                run_json_command(&config.root_path, &["repair", "--json"])?,
            );
            write_json_response(&mut stream, "200 OK", &payload)?;
        }
        _ => write_text_response(
            &mut stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            "Not found",
        )?,
    }

    Ok(())
}

fn build_overview_json(root_path: &Path) -> Result<JsonValue, String> {
    let doctor = run_json_command(root_path, &["doctor", "--json"])?;
    let hub = run_json_command(root_path, &["hub", "status", "--json"])?;
    let readiness = run_json_command(root_path, &["verify", "readiness", "--json"])?;
    let servers = run_json_command(root_path, &["server", "list", "--json"])?;
    let clients = run_json_command(root_path, &["client", "list", "--json"])?;

    Ok(JsonValue::object([
        ("generatedAtMs", JsonValue::number(now_ms())),
        (
            "rootPath",
            JsonValue::string(sanitize_root_path(&root_path.display().to_string())),
        ),
        ("doctor", doctor),
        ("hub", hub),
        ("readiness", readiness),
        ("servers", servers),
        ("clients", clients),
    ]))
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

    match method {
        "initialize" => {
            let requested = json_helpers::string_at_path(&message, &["params", "protocolVersion"])
                .unwrap_or("2025-11-25");
            let negotiated = match requested {
                "2025-11-25" | "2025-06-18" | "2025-03-26" | "2024-11-05" => requested,
                _ => "2025-11-25",
            };

            Ok(McpHttpResponse::Json(JsonValue::object([
                ("jsonrpc", JsonValue::string("2.0")),
                ("id", id),
                (
                    "result",
                    JsonValue::object([
                        ("protocolVersion", JsonValue::string(negotiated)),
                        ("capabilities", JsonValue::object([("tools", empty_object())])),
                        (
                            "serverInfo",
                            JsonValue::object([
                                ("name", JsonValue::string("mcpace")),
                                ("version", JsonValue::string(env!("CARGO_PKG_VERSION"))),
                            ]),
                        ),
                        (
                            "instructions",
                            JsonValue::string(
                            "Use MCPace over HTTP on this local port. Management tools are native here; configured stdio upstreams are discovered from mcp_settings.json, cataloged with upstream_catalog, probed with upstream_probe, audited with upstream_policy_audit, suggested with upstream_policy_suggest, listed with upstream_tools, called once through upstream_call, or called as a single state-preserving sequence through upstream_batch. Call surface_manifest for the exact native-vs-upstream surface contract. HTTP-only upstreams remain explicit diagnostics until a real proxy is configured.",
                            ),
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
        "tools/list" => Ok(McpHttpResponse::Json(JsonValue::object([
            ("jsonrpc", JsonValue::string("2.0")),
            ("id", id),
            (
                "result",
                JsonValue::object([("tools", JsonValue::array(http_tool_definitions()))]),
            ),
        ]))),
        "tools/call" => {
            let tool_name = json_helpers::string_at_path(&message, &["params", "name"])
                .ok_or_else(|| "tools/call requires a tool name".to_string())?;
            let args = json_helpers::value_at_path(&message, &["params", "arguments"])
                .cloned()
                .unwrap_or_else(empty_object);
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
            Ok(mcp_tool_result(id, structured, is_error))
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

fn mcp_tool_result(id: JsonValue, structured: JsonValue, is_error: bool) -> McpHttpResponse {
    let text = structured.to_pretty_string();
    McpHttpResponse::Json(JsonValue::object([
        ("jsonrpc", JsonValue::string("2.0")),
        ("id", id),
        (
            "result",
            JsonValue::object([
                (
                    "content",
                    JsonValue::array([JsonValue::object([
                        ("type", JsonValue::string("text")),
                        ("text", JsonValue::string(text)),
                    ])]),
                ),
                ("structuredContent", structured),
                ("isError", JsonValue::bool(is_error)),
            ]),
        ),
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
                    "allowDesktopObservation",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "Explicitly allow windows-mcp observation tools such as Snapshot/Screenshot/Scrape for this call.",
                            ),
                        ),
                    ]),
                ),
                (
                    "allowDesktopControl",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "Explicitly allow windows-mcp desktop-control tools such as App/Click/Type/Shortcut for this call.",
                            ),
                        ),
                    ]),
                ),
                (
                    "allowSystemControl",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "Explicitly allow windows-mcp system tools such as PowerShell/FileSystem/Clipboard/Process/Registry for this call.",
                            ),
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
                                "Ordered upstream calls to execute after one initialize handshake. Use this for stateful servers such as browser automation.",
                            ),
                        ),
                        (
                            "items",
                            JsonValue::object([
                                ("type", JsonValue::string("object")),
                                (
                                    "properties",
                                    JsonValue::object([
                                        (
                                            "tool",
                                            JsonValue::object([
                                                ("type", JsonValue::string("string")),
                                                (
                                                    "description",
                                                    JsonValue::string("Upstream tool name."),
                                                ),
                                            ]),
                                        ),
                                        (
                                            "arguments",
                                            JsonValue::object([
                                                ("type", JsonValue::string("object")),
                                                (
                                                    "description",
                                                    JsonValue::string(
                                                        "Arguments to pass to the upstream tool.",
                                                    ),
                                                ),
                                                ("additionalProperties", JsonValue::bool(true)),
                                            ]),
                                        ),
                                    ]),
                                ),
                                (
                                    "required",
                                    JsonValue::array([JsonValue::string("tool")]),
                                ),
                                ("additionalProperties", JsonValue::bool(false)),
                            ]),
                        ),
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
                    "allowDesktopObservation",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "Explicitly allow windows-mcp observation tools such as Snapshot/Screenshot/Scrape for this batch.",
                            ),
                        ),
                    ]),
                ),
                (
                    "allowDesktopControl",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "Explicitly allow windows-mcp desktop-control tools such as App/Click/Type/Shortcut for this batch.",
                            ),
                        ),
                    ]),
                ),
                (
                    "allowSystemControl",
                    JsonValue::object([
                        ("type", JsonValue::string("boolean")),
                        (
                            "description",
                            JsonValue::string(
                                "Explicitly allow windows-mcp system tools such as PowerShell/FileSystem/Clipboard/Process/Registry for this batch.",
                            ),
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
        http_tool("browser_status", "Explain browser MCP bridge status"),
        http_tool("client_list", "List known client targets"),
    ]
}

fn http_tool(name: &str, description: &str) -> JsonValue {
    http_tool_with_schema(name, description, empty_object(), vec![])
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
        "runtime_diagnostics" => runtime_diagnostics(root_path),
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
                &config.upstream_session_pool,
            )
        }
        "upstream_batch" => {
            let server = json_helpers::string_at_path(args, &["server"])
                .ok_or_else(|| "upstream_batch requires a 'server' string".to_string())?;
            let raw_calls = json_helpers::array_at_path(args, &["calls"])
                .ok_or_else(|| "upstream_batch requires a 'calls' array".to_string())?;
            let mut calls = Vec::new();
            for (index, raw_call) in raw_calls.iter().enumerate() {
                let tool = json_helpers::string_at_path(raw_call, &["tool"])
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        format!("upstream_batch calls[{}] requires a non-empty 'tool'", index)
                    })?
                    .to_string();
                let arguments = json_helpers::value_at_path(raw_call, &["arguments"])
                    .cloned()
                    .unwrap_or_else(empty_object);
                calls.push(upstream::UpstreamToolCall { tool, arguments });
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
                &config.upstream_session_pool,
            )
        }
        "browser_status" => upstream::browser_status(root_path),
        "client_list" => run_json_command(root_path, &["client", "list", "--json"]),
        other => Err(format!(
            "unsupported MCPace HTTP tool '{}'. This HTTP endpoint exposes MCPace management tools and stdio upstream access through surface_manifest/upstream_catalog/upstream_probe/upstream_policy_audit/upstream_policy_suggest/upstream_tools/upstream_call/upstream_batch. Direct upstream tool names are not advertised as native MCPace tools; call surface_manifest for the exact contract, upstream_catalog for concise descriptions, upstream_policy_audit for policy review, upstream_policy_suggest for generated policy candidates, upstream_tools for one server's full schemas, then upstream_call or upstream_batch. Call runtime_diagnostics for exact status.",
            other
        )),
    }
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

fn runtime_diagnostics(root_path: &Path) -> Result<JsonValue, String> {
    let doctor = run_json_command(root_path, &["doctor", "--json"])?;
    let hub_status = run_json_command(root_path, &["hub", "status", "--json"])?;
    let server_capabilities = run_json_command(root_path, &["server", "capabilities", "--json"])?;
    let upstream_inventory = upstream::configured_inventory(root_path).unwrap_or_else(|error| {
        JsonValue::object([
            ("ok", JsonValue::bool(false)),
            ("error", JsonValue::string(error)),
            ("servers", JsonValue::array([])),
        ])
    });
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
                "MCPace HTTP MCP is reachable. This build exposes management tools plus explicit stdio upstream access through surface_manifest/upstream_catalog/upstream_probe/upstream_policy_audit/upstream_policy_suggest/upstream_tools/upstream_call/upstream_batch; direct upstream tool names are intentionally not advertised as native MCPace tools.",
            ),
        ),
        ("doctor", doctor),
        ("hub", hub_status),
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
                        "Only the names in managementTools.names are returned by this endpoint's tools/list.",
                    ),
                ),
                (
                    "upstreamProjection",
                    JsonValue::string(
                        "Configured upstream tool names remain upstream; use surface_manifest for this exact contract and upstream_catalog/upstream_tools for live discovery.",
                    ),
                ),
                ("directTopLevelProjectionEnabled", JsonValue::bool(false)),
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
                "Use surface_manifest to see the exact native-vs-upstream surface contract, upstream_probe to check all configured upstream MCP servers, upstream_policy_audit to review annotations/policy coverage, upstream_policy_suggest to generate policy candidates, then upstream_tools with a specific stdio server, then upstream_call for stateless calls or upstream_batch for stateful sequences. Use browser_status for browser bridge truth; direct upstream tool names are intentionally not advertised as native MCPace tools.",
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
    } else if name == "browser" || kind == "host-bridge" {
        (
            "blocked-preview-host-bridge",
            "browser/host-bridge policy is configured, but MCPace does not currently launch or proxy a real browser bridge through this HTTP adapter",
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

      async function refreshDashboard() {
        try {
          const [overview, logs] = await Promise.all([
            fetchJson("/api/overview"),
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
          await refreshDashboard();
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

      document.getElementById("refresh-button").addEventListener("click", refreshDashboard);
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
        build_overview_json, is_allowed_local_origin, run_http_tool, run_json_command,
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

    fn test_config(
        root_path: PathBuf,
        max_requests: Option<usize>,
        surface: super::ServeSurface,
    ) -> super::DashboardConfig {
        super::DashboardConfig {
            root_path,
            max_requests,
            surface,
            upstream_session_pool: Mutex::new(crate::upstream::UpstreamSessionPool::default()),
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
                ("allowDesktopObservation", JsonValue::bool(true)),
                ("allowDesktopControl", JsonValue::bool(true)),
                ("allowSystemControl", JsonValue::bool(true)),
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
            .allow_arguments
            .contains("allowDesktopObservation"));
        assert!(explicit_context
            .allow_arguments
            .contains("allowDesktopControl"));
        assert!(explicit_context
            .allow_arguments
            .contains("allowSystemControl"));
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
                test_config(server_root, Some(2), super::ServeSurface::Dashboard),
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
        assert!(tools_response.contains("\"doctor\""));
        assert!(tools_response.contains("\"hub_status\""));
        assert!(tools_response.contains("\"hub_repair\""));
        assert!(tools_response.contains("\"runtime_diagnostics\""));
        assert!(tools_response.contains("\"surface_manifest\""));
        assert!(tools_response.contains("\"upstream_tools\""));
        assert!(tools_response.contains("\"upstream_catalog\""));
        assert!(tools_response.contains("\"upstream_probe\""));
        assert!(tools_response.contains("\"upstream_policy_audit\""));
        assert!(tools_response.contains("\"upstream_policy_suggest\""));
        assert!(tools_response.contains("\"upstream_call\""));
        assert!(tools_response.contains("\"upstream_batch\""));
        assert!(tools_response.contains("\"browser_status\""));

        let unsupported_call = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"browser","arguments":{}}}"#;
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
