use crate::json::{parse_str, JsonValue};
use crate::{app, doctor, json_helpers, resources, runtimepaths};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

const HTTP_PROBE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug)]
struct ParsedArgs {
    help: bool,
    json_output: bool,
    root_override: Option<PathBuf>,
    host: String,
    port: u16,
    max_connections: Option<usize>,
    io_timeout_ms: Option<u64>,
    max_body_bytes: Option<usize>,
    overview_cache_ms: Option<u64>,
    skip_client_install: bool,
    install_service: bool,
    no_enable_service: bool,
    error: Option<String>,
}

impl Default for ParsedArgs {
    fn default() -> Self {
        Self {
            help: false,
            json_output: false,
            root_override: None,
            host: runtimepaths::DEFAULT_LOCAL_HOST.to_string(),
            port: runtimepaths::DEFAULT_LOCAL_MCP_PORT,
            max_connections: None,
            io_timeout_ms: None,
            max_body_bytes: None,
            overview_cache_ms: None,
            skip_client_install: false,
            install_service: false,
            no_enable_service: false,
            error: None,
        }
    }
}

struct CommandResult {
    ok: bool,
    exit_code: i32,
    json: Option<JsonValue>,
    stdout: String,
    stderr: String,
}

pub fn run(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let parsed = parse_args(args);
    if let Some(error) = parsed.error.clone() {
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

    let report = run_setup(parsed, root_path);
    let exit_code = if json_helpers::string_at_path(&report, &["status"]) == Some("ready") {
        0
    } else {
        1
    };

    if json_helpers::bool_at_path(&report, &["jsonOutput"]).unwrap_or(false) {
        let _ = writeln!(stdout, "{}", report.to_pretty_string());
    } else {
        write_text_report(&report, stdout);
    }

    exit_code
}

fn parse_args(args: &[String]) -> ParsedArgs {
    let mut parsed = ParsedArgs::default();
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "--json" | "-json" => {
                parsed.json_output = true;
                index += 1;
            }
            "--root" | "-root" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("setup requires a path after --root".to_string());
                    return parsed;
                };
                parsed.root_override = Some(PathBuf::from(value));
                index += 2;
            }
            "--host" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("setup requires a value after --host".to_string());
                    return parsed;
                };
                parsed.host = value.to_string();
                index += 2;
            }
            "--port" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("setup requires a value after --port".to_string());
                    return parsed;
                };
                match value.parse::<u16>() {
                    Ok(port) => parsed.port = port,
                    Err(_) => {
                        parsed.error = Some("setup --port must be a valid u16".to_string());
                        return parsed;
                    }
                }
                index += 2;
            }
            "--max-connections" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("setup requires a value after --max-connections".to_string());
                    return parsed;
                };
                match resources::parse_positive_usize(value, "setup --max-connections") {
                    Ok(limit) => parsed.max_connections = Some(limit),
                    Err(error) => {
                        parsed.error = Some(error);
                        return parsed;
                    }
                }
                index += 2;
            }
            "--io-timeout-ms" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("setup requires a value after --io-timeout-ms".to_string());
                    return parsed;
                };
                match resources::parse_positive_u64(value, "setup --io-timeout-ms") {
                    Ok(timeout_ms) => parsed.io_timeout_ms = Some(timeout_ms),
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
                        Some("setup requires a value after --max-body-bytes".to_string());
                    return parsed;
                };
                match resources::parse_positive_usize(value, "setup --max-body-bytes") {
                    Ok(limit) => parsed.max_body_bytes = Some(limit),
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
                        Some("setup requires a value after --overview-cache-ms".to_string());
                    return parsed;
                };
                match resources::parse_nonnegative_u64(value, "setup --overview-cache-ms") {
                    Ok(ttl_ms) => parsed.overview_cache_ms = Some(ttl_ms),
                    Err(error) => {
                        parsed.error = Some(error);
                        return parsed;
                    }
                }
                index += 2;
            }
            "--skip-client-install" => {
                parsed.skip_client_install = true;
                index += 1;
            }
            "--install-service" | "--install-autostart" => {
                parsed.install_service = true;
                index += 1;
            }
            "--no-enable" => {
                parsed.no_enable_service = true;
                index += 1;
            }
            "-h" | "--help" | "-?" => {
                parsed.help = true;
                return parsed;
            }
            other => {
                parsed.error = Some(format!("unsupported setup argument: {}", other));
                return parsed;
            }
        }
    }

    parsed
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace setup [--json] [--root <path>] [--host <addr>] [--port <n>] [--max-connections <n>] [--io-timeout-ms <n>] [--max-body-bytes <n>] [--overview-cache-ms <n>] [--skip-client-install] [--install-service] [--no-enable]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "Starts the local MCPace endpoint, installs supported local client config entries, and verifies /healthz plus /mcp."
    );
    let _ = writeln!(
        stdout,
        "Serve resource defaults: max connections={}, IO timeout={}ms, max body={} bytes, overview cache={}ms.",
        resources::default_http_connection_limit(),
        resources::DEFAULT_HTTP_IO_TIMEOUT_MS,
        resources::DEFAULT_MAX_HTTP_BODY_BYTES,
        resources::DEFAULT_DASHBOARD_OVERVIEW_CACHE_MS
    );
}

fn append_serve_resource_args(
    args: &mut Vec<String>,
    max_connections: Option<usize>,
    io_timeout_ms: Option<u64>,
    max_body_bytes: Option<usize>,
    overview_cache_ms: Option<u64>,
) {
    if let Some(value) = max_connections {
        args.push("--max-connections".to_string());
        args.push(value.to_string());
    }
    if let Some(value) = io_timeout_ms {
        args.push("--io-timeout-ms".to_string());
        args.push(value.to_string());
    }
    if let Some(value) = max_body_bytes {
        args.push("--max-body-bytes".to_string());
        args.push(value.to_string());
    }
    if let Some(value) = overview_cache_ms {
        args.push("--overview-cache-ms".to_string());
        args.push(value.to_string());
    }
}

fn run_setup(parsed: ParsedArgs, root_path: PathBuf) -> JsonValue {
    let root_text = root_path.display().to_string();
    let endpoint = http_url(&parsed.host, parsed.port, "/mcp");
    let mut warnings = Vec::new();
    let current_executable = std::env::current_exe()
        .ok()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let mcpace_in_path = doctor::command_available("mcpace");
    if !mcpace_in_path {
        warnings.push(
            "The global 'mcpace' command is not in PATH; local HTTP clients can still use the endpoint, but stdio launcher exports need a PATH install or an absolute binary path."
                .to_string(),
        );
    }

    let mut serve_args = vec![
        "serve".to_string(),
        "start".to_string(),
        "--json".to_string(),
        "--host".to_string(),
        parsed.host.clone(),
        "--port".to_string(),
        parsed.port.to_string(),
        "--root".to_string(),
        root_text.clone(),
    ];
    append_serve_resource_args(
        &mut serve_args,
        parsed.max_connections,
        parsed.io_timeout_ms,
        parsed.max_body_bytes,
        parsed.overview_cache_ms,
    );
    let serve = run_json_command(serve_args);

    let client_install = if parsed.skip_client_install {
        warnings.push(
            "Client install was skipped; run 'mcpace client install all' when ready.".to_string(),
        );
        None
    } else {
        Some(run_json_command(vec![
            "client".to_string(),
            "install".to_string(),
            "all".to_string(),
            "--json".to_string(),
            "--root".to_string(),
            root_text.clone(),
        ]))
    };

    let readiness = run_json_command(vec![
        "verify".to_string(),
        "readiness".to_string(),
        "--json".to_string(),
        "--root".to_string(),
        root_text.clone(),
    ]);

    let service_install = if parsed.install_service {
        let mut args = vec![
            "service".to_string(),
            "install".to_string(),
            "--json".to_string(),
            "--host".to_string(),
            parsed.host.clone(),
            "--port".to_string(),
            parsed.port.to_string(),
            "--root".to_string(),
            root_text.clone(),
        ];
        append_serve_resource_args(
            &mut args,
            parsed.max_connections,
            parsed.io_timeout_ms,
            parsed.max_body_bytes,
            parsed.overview_cache_ms,
        );
        if parsed.no_enable_service {
            args.push("--no-enable".to_string());
        }
        Some(run_json_command(args))
    } else {
        None
    };

    let probe_host = probe_host(&parsed.host);
    let health = http_json_get(&probe_host, parsed.port, "/healthz");
    let initialize = http_mcp_request(
        &probe_host,
        parsed.port,
        JsonValue::object([
            ("jsonrpc", JsonValue::string("2.0")),
            ("id", JsonValue::number(1)),
            ("method", JsonValue::string("initialize")),
            (
                "params",
                JsonValue::object([
                    ("protocolVersion", JsonValue::string("2025-11-25")),
                    ("capabilities", empty_object()),
                    (
                        "clientInfo",
                        JsonValue::object([
                            ("name", JsonValue::string("mcpace-setup")),
                            ("version", JsonValue::string(env!("CARGO_PKG_VERSION"))),
                        ]),
                    ),
                ]),
            ),
        ]),
    );
    let tools_list = http_mcp_request(
        &probe_host,
        parsed.port,
        JsonValue::object([
            ("jsonrpc", JsonValue::string("2.0")),
            ("id", JsonValue::number(2)),
            ("method", JsonValue::string("tools/list")),
            ("params", empty_object()),
        ]),
    );

    warnings.push(
        "Cloud/public connector surfaces still require a public relay and are not made ready by local setup."
            .to_string(),
    );

    let serve_ok = serve.ok
        && json_helpers::string_at_path(
            serve.json.as_ref().unwrap_or(&JsonValue::Null),
            &["status"],
        ) == Some("running");
    let install_ok = parsed.skip_client_install
        || client_install
            .as_ref()
            .map(|result| result.ok)
            .unwrap_or(false);
    let readiness_ok = readiness.ok
        && json_helpers::bool_at_path(
            readiness.json.as_ref().unwrap_or(&JsonValue::Null),
            &["readyForRuntimeOps"],
        )
        .unwrap_or(false);
    let service_ok = !parsed.install_service
        || service_install
            .as_ref()
            .map(|result| result.ok)
            .unwrap_or(false);
    let health_ok = health
        .as_ref()
        .ok()
        .and_then(|value| json_helpers::bool_at_path(value, &["ok"]))
        .unwrap_or(false);
    let initialize_ok = initialize
        .as_ref()
        .ok()
        .and_then(|value| json_helpers::string_at_path(value, &["result", "protocolVersion"]))
        .is_some();
    let tool_count = tools_list
        .as_ref()
        .ok()
        .and_then(|value| json_helpers::array_at_path(value, &["result", "tools"]))
        .map(|items| items.len())
        .unwrap_or(0);
    let tools_ok = tool_count > 0;
    let status = if serve_ok
        && install_ok
        && readiness_ok
        && service_ok
        && health_ok
        && initialize_ok
        && tools_ok
    {
        "ready"
    } else {
        "blocked"
    };

    JsonValue::object([
        ("status", JsonValue::string(status)),
        ("jsonOutput", JsonValue::bool(parsed.json_output)),
        ("rootPath", JsonValue::string(root_text)),
        ("endpoint", JsonValue::string(endpoint.clone())),
        ("host", JsonValue::string(parsed.host)),
        ("port", JsonValue::number(parsed.port)),
        (
            "serveResources",
            JsonValue::object([
                (
                    "maxConnections",
                    JsonValue::number(
                        parsed
                            .max_connections
                            .unwrap_or_else(resources::default_http_connection_limit),
                    ),
                ),
                (
                    "ioTimeoutMs",
                    JsonValue::number(
                        parsed
                            .io_timeout_ms
                            .unwrap_or(resources::DEFAULT_HTTP_IO_TIMEOUT_MS),
                    ),
                ),
                (
                    "maxBodyBytes",
                    JsonValue::number(
                        parsed
                            .max_body_bytes
                            .unwrap_or(resources::DEFAULT_MAX_HTTP_BODY_BYTES),
                    ),
                ),
            ]),
        ),
        ("serve", command_result_json(&serve)),
        (
            "launcher",
            JsonValue::object([
                ("mcpaceCommandInPath", JsonValue::bool(mcpace_in_path)),
                ("currentExecutable", JsonValue::string(current_executable)),
            ]),
        ),
        (
            "clientInstall",
            match client_install.as_ref() {
                Some(result) => command_result_json(result),
                None => JsonValue::object([
                    ("ok", JsonValue::bool(true)),
                    ("skipped", JsonValue::bool(true)),
                ]),
            },
        ),
        ("readiness", command_result_json(&readiness)),
        (
            "serviceInstall",
            match service_install.as_ref() {
                Some(result) => command_result_json(result),
                None => JsonValue::object([
                    ("ok", JsonValue::bool(true)),
                    ("skipped", JsonValue::bool(true)),
                ]),
            },
        ),
        ("health", result_json(health)),
        ("mcpInitialize", result_json(initialize)),
        (
            "mcpTools",
            JsonValue::object([
                ("ok", JsonValue::bool(tools_ok)),
                ("toolCount", JsonValue::number(tool_count)),
                ("response", result_json(tools_list)),
            ]),
        ),
        (
            "checks",
            JsonValue::object([
                ("serveRunning", JsonValue::bool(serve_ok)),
                ("clientInstallReady", JsonValue::bool(install_ok)),
                ("serviceInstallReady", JsonValue::bool(service_ok)),
                ("readinessReady", JsonValue::bool(readiness_ok)),
                ("healthOk", JsonValue::bool(health_ok)),
                ("mcpInitializeOk", JsonValue::bool(initialize_ok)),
                ("mcpToolsOk", JsonValue::bool(tools_ok)),
                ("mcpaceCommandInPath", JsonValue::bool(mcpace_in_path)),
            ]),
        ),
        (
            "warnings",
            JsonValue::array(warnings.into_iter().map(JsonValue::string)),
        ),
        (
            "nextActions",
            JsonValue::array([
                JsonValue::string(format!(
                    "Point local MCP clients at {} or use the configs installed by this command.",
                    endpoint
                )),
                JsonValue::string(
                    "Keep MCPace running with 'mcpace serve start' before opening local clients."
                        .to_string(),
                ),
            ]),
        ),
    ])
}

fn command_result_json(result: &CommandResult) -> JsonValue {
    JsonValue::object([
        ("ok", JsonValue::bool(result.ok)),
        ("exitCode", JsonValue::number(result.exit_code)),
        ("json", result.json.clone().unwrap_or(JsonValue::Null)),
        ("stdout", JsonValue::string(result.stdout.clone())),
        ("stderr", JsonValue::string(result.stderr.clone())),
    ])
}

fn empty_object() -> JsonValue {
    JsonValue::Object(BTreeMap::new())
}

fn result_json(result: Result<JsonValue, String>) -> JsonValue {
    match result {
        Ok(value) => JsonValue::object([("ok", JsonValue::bool(true)), ("json", value)]),
        Err(error) => JsonValue::object([
            ("ok", JsonValue::bool(false)),
            ("error", JsonValue::string(error)),
        ]),
    }
}

fn run_json_command(args: Vec<String>) -> CommandResult {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = app::run(args, &mut stdout, &mut stderr);
    let stdout_text = String::from_utf8(stdout).unwrap_or_default();
    let stderr_text = String::from_utf8(stderr).unwrap_or_default();
    let json = parse_str(stdout_text.trim()).ok();
    CommandResult {
        ok: exit_code == 0 && json.is_some(),
        exit_code,
        json,
        stdout: stdout_text,
        stderr: stderr_text,
    }
}

fn http_json_get(host: &str, port: u16, path: &str) -> Result<JsonValue, String> {
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, host
    );
    http_json_request(host, port, &request)
}

fn http_mcp_request(host: &str, port: u16, body: JsonValue) -> Result<JsonValue, String> {
    let body = body.to_compact_string();
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        host,
        body.len(),
        body
    );
    http_json_request(host, port, &request)
}

fn http_json_request(host: &str, port: u16, request: &str) -> Result<JsonValue, String> {
    let mut stream = TcpStream::connect((host, port))
        .map_err(|error| format!("connect {}:{}: {}", host, port, error))?;
    stream
        .set_read_timeout(Some(HTTP_PROBE_TIMEOUT))
        .map_err(|error| format!("set read timeout: {}", error))?;
    stream
        .set_write_timeout(Some(HTTP_PROBE_TIMEOUT))
        .map_err(|error| format!("set write timeout: {}", error))?;
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("write request: {}", error))?;
    let _ = stream.shutdown(Shutdown::Write);

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| format!("read response: {}", error))?;
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "HTTP response missing header/body separator".to_string())?;
    let status = headers.lines().next().unwrap_or_default();
    if !status.contains(" 200 ") {
        return Err(format!("HTTP request failed: {}", status));
    }
    parse_str(body.trim()).map_err(|error| format!("parse HTTP JSON response: {}", error))
}

fn http_url(host: &str, port: u16, path: &str) -> String {
    format!("http://{}:{}{}", host, port, path)
}

fn probe_host(host: &str) -> String {
    match host {
        "0.0.0.0" | "::" => runtimepaths::DEFAULT_LOCAL_HOST.to_string(),
        other => other.to_string(),
    }
}

fn write_text_report(report: &JsonValue, stdout: &mut dyn Write) {
    let status = json_helpers::string_at_path(report, &["status"]).unwrap_or("unknown");
    let endpoint = json_helpers::string_at_path(report, &["endpoint"]).unwrap_or("unknown");
    let _ = writeln!(stdout, "MCPace setup: {}", status);
    let _ = writeln!(stdout, "Endpoint: {}", endpoint);
    for (label, path) in [
        ("Serve running", "serveRunning"),
        ("Client install ready", "clientInstallReady"),
        ("Autostart install ready", "serviceInstallReady"),
        ("Readiness ready", "readinessReady"),
        ("Health OK", "healthOk"),
        ("MCP initialize OK", "mcpInitializeOk"),
        ("MCP tools OK", "mcpToolsOk"),
        ("mcpace command in PATH", "mcpaceCommandInPath"),
    ] {
        let value = json_helpers::bool_at_path(report, &["checks", path]).unwrap_or(false);
        let _ = writeln!(stdout, "- {}: {}", label, if value { "yes" } else { "no" });
    }
    if let Some(warnings) = json_helpers::array_at_path(report, &["warnings"]) {
        if !warnings.is_empty() {
            let _ = writeln!(stdout, "Warnings:");
            for warning in warnings.iter().filter_map(JsonValue::as_str) {
                let _ = writeln!(stdout, "- {}", warning);
            }
        }
    }
}
