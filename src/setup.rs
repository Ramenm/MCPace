use crate::json::{parse_str, JsonValue};
use crate::{
    app, client_catalog, doctor, json_helpers, mcp_protocol as mcp, mcp_sources, resources,
    runtimepaths, server, text_utils,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

const HTTP_PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_HTTP_SETUP_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug)]
struct HttpJsonResponse {
    headers: Vec<(String, String)>,
    json: JsonValue,
}

struct ParsedHttpJsonResponse<'a> {
    status_line: String,
    status: u16,
    headers: Vec<(String, String)>,
    content_type: String,
    content_length: Option<usize>,
    transfer_encoding: String,
    body: &'a str,
}

impl HttpJsonResponse {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug, Default)]
struct ParsedArgs {
    help: bool,
    json_output: bool,
    root_override: Option<PathBuf>,
    host: Option<String>,
    port: u16,
    max_connections: Option<usize>,
    io_timeout_ms: Option<u64>,
    max_body_bytes: Option<usize>,
    overview_cache_ms: Option<u64>,
    skip_client_install: bool,
    client_selector: Option<String>,
    install_service: bool,
    no_enable_service: bool,
    server_spec: Option<String>,
    server_name: Option<String>,
    server_paths: Vec<String>,
    server_force: bool,
    no_default_server: bool,
    error: Option<String>,
}

struct CommandResult {
    ok: bool,
    exit_code: i32,
    json: Option<JsonValue>,
    stdout: String,
    stderr: String,
}

#[derive(Clone, Debug)]
struct RootBootstrap {
    root_path: PathBuf,
    created_root_dir: bool,
    created_config: bool,
    created_settings: bool,
    created_settings_dir: bool,
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

    let root_path = match resolve_setup_root(parsed.root_override.clone(), default_root) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };
    let bootstrap = match ensure_setup_root_layout(&root_path) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    let report = run_setup(parsed, bootstrap);
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
            "--" => {
                if index + 1 >= args.len() {
                    parsed.error = Some("setup -- requires a command after it".to_string());
                    return parsed;
                }
                let mut spec = parsed.server_spec.take().unwrap_or_default();
                if !spec.trim().is_empty() {
                    spec.push(' ');
                }
                spec.push_str(&args[index + 1..].join(" "));
                parsed.server_spec = Some(spec);
                break;
            }
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
                parsed.host = Some(value.to_string());
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
            "--skip-client-install" | "--no-client" | "--skip-client" => {
                parsed.skip_client_install = true;
                index += 1;
            }
            "--client" | "--for" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some(
                        "setup requires a client id, 'auto', 'all', or 'none' after --client"
                            .to_string(),
                    );
                    return parsed;
                };
                let value = value.trim().to_ascii_lowercase();
                if value == "none" || value == "skip" || value == "off" {
                    parsed.skip_client_install = true;
                    parsed.client_selector = None;
                } else {
                    parsed.client_selector = Some(value);
                }
                index += 2;
            }
            "--all-clients" => {
                parsed.client_selector = Some("all".to_string());
                index += 1;
            }
            "--auto-client" | "--auto-clients" => {
                parsed.client_selector = Some("auto".to_string());
                index += 1;
            }
            "--server" | "--with-server" | "--install-server" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("setup requires a server spec after --server".to_string());
                    return parsed;
                };
                parsed.server_spec = Some(value.to_string());
                index += 2;
            }
            "--as" | "--server-name" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("setup requires a server name after --as".to_string());
                    return parsed;
                };
                parsed.server_name = Some(value.to_string());
                index += 2;
            }
            "--path" | "--server-path" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("setup requires a filesystem path after --path".to_string());
                    return parsed;
                };
                parsed.server_paths.push(value.to_string());
                index += 2;
            }
            "--force" => {
                parsed.server_force = true;
                index += 1;
            }
            "--no-default-server" | "--no-server" => {
                parsed.no_default_server = true;
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
                if parsed
                    .server_spec
                    .as_deref()
                    .map(looks_like_multiword_server_command)
                    .unwrap_or(false)
                {
                    let mut spec = parsed.server_spec.take().unwrap_or_default();
                    spec.push(' ');
                    spec.push_str(other);
                    parsed.server_spec = Some(spec);
                    index += 1;
                    continue;
                }
                if !other.starts_with('-') {
                    if parsed.server_spec.is_none() {
                        parsed.server_spec = Some(other.to_string());
                    } else {
                        let spec = parsed.server_spec.take().unwrap_or_default();
                        parsed.server_paths.push(other.to_string());
                        parsed.server_spec = Some(spec);
                    }
                    index += 1;
                    continue;
                }
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
        "Usage: mcpace up [server-spec] [--as <name>] [--path <path>...] [--client auto|all|<id>|none] [--json] [--root <path>] [--host <addr>] [--port <n>] [--install-service]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "Home-first onboarding: creates a user-level MCPace home when needed, starts the local endpoint, upserts only the MCPace entry into detected local clients, preserves existing client MCP servers, and verifies health plus MCP routes. It does not invent or install a default upstream server."
    );
    let _ = writeln!(stdout, "Examples:");
    let _ = writeln!(stdout, "  mcpace up                              # start endpoint, wire detected clients, keep existing client servers");
    let _ = writeln!(
        stdout,
        "  mcpace up --client cursor-local        # explicit client, no new upstream server"
    );
    let _ = writeln!(
        stdout,
        "  mcpace up npm:@modelcontextprotocol/server-memory --as memory --client none"
    );
    let _ = writeln!(
        stdout,
        "  mcpace up http://127.0.0.1:8010/mcp --as local-gateway --client all"
    );
    let _ = writeln!(
        stdout,
        "  mcpace up --server \"npx -y @modelcontextprotocol/server-memory\" --as memory"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Use 'mcpace install <path|package|url|command...>' when you actually want to add a new upstream server.");
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "Serve resource defaults: max connections={}, IO timeout={}ms, max body={} bytes, overview cache={}ms.",
        resources::default_http_connection_limit(),
        resources::default_http_io_timeout_ms(),
        resources::default_max_http_body_bytes(),
        resources::default_dashboard_overview_cache_ms()
    );
}

fn resolve_setup_root(
    root_override: Option<PathBuf>,
    discovered_root: Option<PathBuf>,
) -> Result<PathBuf, String> {
    if let Some(path) = root_override.or(discovered_root) {
        return Ok(runtimepaths::canonicalize_or_original(&path));
    }
    if let Some(home) = runtimepaths::user_home_dir() {
        return Ok(home.join(".mcpace"));
    }
    std::env::current_dir()
        .map(|path| path.join(".mcpace"))
        .map_err(|error| {
            format!(
                "failed to resolve current directory for MCPace setup: {}",
                error
            )
        })
}

fn ensure_setup_root_layout(root_path: &Path) -> Result<RootBootstrap, String> {
    let existed_before = root_path.is_dir();
    fs::create_dir_all(root_path).map_err(|error| {
        format!(
            "failed to create MCPace root {}: {}",
            root_path.display(),
            error
        )
    })?;
    let root_path = runtimepaths::canonicalize_or_original(root_path);

    let config_path = root_path.join("mcpace.config.json");
    let created_config = !config_path.is_file();
    if created_config {
        runtimepaths::write_text_atomic(&config_path, &default_config_json().to_pretty_string())
            .map_err(|error| format!("failed to write {}: {}", config_path.display(), error))?;
    }

    let settings_path = root_path.join("mcp_settings.json");
    let created_settings = !settings_path.is_file();
    if created_settings {
        let settings = JsonValue::object([("mcpServers", empty_object())]);
        runtimepaths::write_text_atomic(&settings_path, &settings.to_pretty_string())
            .map_err(|error| format!("failed to write {}: {}", settings_path.display(), error))?;
    }

    let settings_dir = root_path.join("mcp_settings.d");
    let created_settings_dir = !settings_dir.is_dir();
    if created_settings_dir {
        fs::create_dir_all(&settings_dir)
            .map_err(|error| format!("failed to create {}: {}", settings_dir.display(), error))?;
    }

    Ok(RootBootstrap {
        root_path,
        created_root_dir: !existed_before,
        created_config,
        created_settings,
        created_settings_dir,
    })
}

fn default_config_json() -> JsonValue {
    JsonValue::object([
        ("name", JsonValue::string("mcpace")),
        ("version", JsonValue::string(env!("CARGO_PKG_VERSION"))),
        (
            "ports",
            JsonValue::object([(
                "serve",
                JsonValue::number(runtimepaths::DEFAULT_LOCAL_MCP_PORT),
            )]),
        ),
        (
            "profiles",
            JsonValue::object([(
                "runtime",
                JsonValue::object([
                    ("default", JsonValue::string("safe")),
                    (
                        "profiles",
                        JsonValue::object([(
                            "safe",
                            JsonValue::object([
                                (
                                    "description",
                                    JsonValue::string("Default safe local runtime profile."),
                                ),
                                ("serverOverrides", empty_object()),
                            ]),
                        )]),
                    ),
                ]),
            )]),
        ),
        ("servers", empty_object()),
        (
            "client",
            JsonValue::object([("keyName", JsonValue::string("MCPace"))]),
        ),
        (
            "serve",
            JsonValue::object([
                ("host", JsonValue::string(runtimepaths::DEFAULT_LOCAL_HOST)),
                (
                    "port",
                    JsonValue::number(runtimepaths::DEFAULT_LOCAL_MCP_PORT),
                ),
                (
                    "mcpPath",
                    JsonValue::string(runtimepaths::DEFAULT_LOCAL_MCP_PATH),
                ),
                ("publicUrl", JsonValue::string("")),
            ]),
        ),
        (
            "mcpSettings",
            JsonValue::object([(
                "includeDirs",
                JsonValue::array([JsonValue::string("mcp_settings.d")]),
            )]),
        ),
        (
            "clientCatalog",
            JsonValue::object([
                ("paths", JsonValue::array(std::iter::empty::<JsonValue>())),
                ("targets", JsonValue::array(std::iter::empty::<JsonValue>())),
            ]),
        ),
    ])
}

fn run_setup(parsed: ParsedArgs, bootstrap: RootBootstrap) -> JsonValue {
    let root_path = bootstrap.root_path.clone();
    let resolved_endpoint = runtimepaths::resolve_serve_endpoint(Some(&root_path));
    let host = parsed
        .host
        .clone()
        .unwrap_or_else(|| resolved_endpoint.host.clone());
    let port = if parsed.port == 0 {
        resolved_endpoint.port
    } else {
        parsed.port
    };
    let mcp_path = resolved_endpoint.mcp_path.clone();
    let health_path = resolved_endpoint.health_path.clone();
    let root_text = root_path.display().to_string();
    let endpoint = runtimepaths::http_url(&host, port, &mcp_path);
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

    let init = run_json_command(vec![
        "init".to_string(),
        "--json".to_string(),
        "--root".to_string(),
        root_text.clone(),
    ]);

    let server_counts_before = setup_server_counts(&root_path);
    let server_count_before = server_counts_before.source_enabled;
    let requested_server_spec = parsed
        .server_spec
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    if parsed.no_default_server {
        warnings.push(
            "--no-default-server is now the default behavior; MCPace will not add an upstream server unless you pass one explicitly."
                .to_string(),
        );
    }

    let home_import = if server_count_before == 0 && requested_server_spec.is_none() {
        Some(import_existing_home_mcp_servers(
            &root_path,
            &endpoint,
            &mut warnings,
        ))
    } else {
        None
    };
    let server_spec_to_install = requested_server_spec.clone();
    let server_install = server_spec_to_install.as_ref().map(|spec| {
        let mut args = vec![
            "install".to_string(),
            spec.clone(),
            "--json".to_string(),
            "--root".to_string(),
            root_text.clone(),
        ];
        if let Some(name) = parsed
            .server_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            args.push("--as".to_string());
            args.push(name.to_string());
        }
        for path in &parsed.server_paths {
            args.push("--path".to_string());
            args.push(path.clone());
        }
        if parsed.server_force {
            args.push("--force".to_string());
        }
        run_json_command(args)
    });

    let server_counts_after = setup_server_counts(&root_path);
    let server_count_after = server_counts_after.source_enabled;
    let effective_server_count_after = server_counts_after.effective_enabled;
    if server_count_after == 0 {
        warnings.push(
            "No upstream MCP servers are configured yet, and MCPace did not add a default filesystem server. Add one explicitly with 'mcpace install <path|package|url|command>' or import an existing MCP settings file when you want tools behind the hub."
                .to_string(),
        );
    }

    let mut serve_args = vec![
        "serve".to_string(),
        "start".to_string(),
        "--json".to_string(),
        "--host".to_string(),
        host.clone(),
        "--port".to_string(),
        port.to_string(),
        "--root".to_string(),
        root_text.clone(),
    ];
    resources::append_serve_resource_args(
        &mut serve_args,
        parsed.max_connections,
        parsed.io_timeout_ms,
        parsed.max_body_bytes,
        parsed.overview_cache_ms,
    );
    let serve = run_json_command(serve_args);

    let client_install = if parsed.skip_client_install {
        warnings.push(
            "Client install was skipped; run 'mcpace client install <client-id>' when ready."
                .to_string(),
        );
        None
    } else {
        Some(run_client_install(
            &root_path,
            &root_text,
            parsed.client_selector.as_deref(),
            &mut warnings,
        ))
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
            host.clone(),
            "--port".to_string(),
            port.to_string(),
            "--root".to_string(),
            root_text.clone(),
        ];
        resources::append_serve_resource_args(
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

    let probe_host = probe_host(&host);
    let health = http_json_get(&probe_host, port, &health_path);
    let initialize_response = http_mcp_request(
        &probe_host,
        port,
        &mcp_path,
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
        None,
    );
    let setup_session_id = initialize_response
        .as_ref()
        .ok()
        .and_then(|response| response.header("Mcp-Session-Id"))
        .map(ToOwned::to_owned);
    let initialize = initialize_response.map(|response| response.json);
    let initialize_ok_for_notification = initialize
        .as_ref()
        .ok()
        .and_then(|value| json_helpers::string_at_path(value, &["result", "protocolVersion"]))
        .is_some();
    let initialized_notification = if initialize_ok_for_notification {
        http_mcp_notification(
            &probe_host,
            port,
            &mcp_path,
            JsonValue::object([
                ("jsonrpc", JsonValue::string("2.0")),
                ("method", JsonValue::string("notifications/initialized")),
            ]),
            setup_session_id.as_deref(),
        )
    } else {
        Err("skipped notifications/initialized because initialize did not complete".to_string())
    };
    let initialized_ok = initialized_notification.is_ok();
    let tools_list = if initialized_ok {
        http_mcp_request(
            &probe_host,
            port,
            &mcp_path,
            JsonValue::object([
                ("jsonrpc", JsonValue::string("2.0")),
                ("id", JsonValue::number(2)),
                ("method", JsonValue::string("tools/list")),
                ("params", empty_object()),
            ]),
            setup_session_id.as_deref(),
        )
        .map(|response| response.json)
    } else {
        Err("skipped tools/list because notifications/initialized did not complete".to_string())
    };

    warnings.push(
        "Cloud/public connector surfaces still require a public relay and are not made ready by local setup."
            .to_string(),
    );

    let init_ok = init.ok;
    let home_import_ok = home_import.as_ref().map(|result| result.ok).unwrap_or(true);
    let server_install_ok = server_install
        .as_ref()
        .map(|result| result.ok)
        .unwrap_or(true);
    let server_configured = server_count_after > 0;
    let effective_server_configured = effective_server_count_after > 0;
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
    let initialize_ok = initialize_ok_for_notification;
    let tool_count = tools_list
        .as_ref()
        .ok()
        .and_then(|value| json_helpers::array_at_path(value, &["result", "tools"]))
        .map(|items| items.len())
        .unwrap_or(0);
    let tools_ok = tool_count > 0;
    let tools_expected = effective_server_configured;
    let tools_ready = !tools_expected || tools_ok;
    if !tools_ok {
        if !tools_expected {
            warnings.push(
                "MCPace responded to initialize. No upstream tools are expected until you add a server."
                    .to_string(),
            );
        } else {
            warnings.push(
                "MCPace responded to initialize, but no tools were discovered yet; run 'mcpace server test <name> --refresh' if the first client still shows no tools."
                    .to_string(),
            );
        }
    }
    let status = if init_ok
        && home_import_ok
        && server_install_ok
        && serve_ok
        && install_ok
        && readiness_ok
        && service_ok
        && health_ok
        && initialize_ok
        && initialized_ok
    {
        "ready"
    } else {
        "blocked"
    };

    JsonValue::object([
        ("status", JsonValue::string(status)),
        ("jsonOutput", JsonValue::bool(parsed.json_output)),
        ("rootPath", JsonValue::string(root_text)),
        ("rootBootstrap", root_bootstrap_json(&bootstrap)),
        ("endpoint", JsonValue::string(endpoint.clone())),
        ("host", JsonValue::string(host.clone())),
        ("port", JsonValue::number(port)),
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
                            .unwrap_or_else(resources::default_http_io_timeout_ms),
                    ),
                ),
                (
                    "maxBodyBytes",
                    JsonValue::number(
                        parsed
                            .max_body_bytes
                            .unwrap_or_else(resources::default_max_http_body_bytes),
                    ),
                ),
            ]),
        ),
        ("init", command_result_json(&init)),
        (
            "homeImport",
            match home_import.as_ref() {
                Some(result) => command_result_json(result),
                None => JsonValue::object([
                    ("ok", JsonValue::bool(true)),
                    ("skipped", JsonValue::bool(true)),
                    ("serversBefore", JsonValue::number(server_count_before)),
                ]),
            },
        ),
        (
            "serverInstall",
            match server_install.as_ref() {
                Some(result) => command_result_json(result),
                None => JsonValue::object([
                    ("ok", JsonValue::bool(true)),
                    ("skipped", JsonValue::bool(true)),
                    ("serversBefore", JsonValue::number(server_count_before)),
                ]),
            },
        ),
        (
            "serversKnown",
            JsonValue::object([
                ("policyCount", JsonValue::number(server_counts_after.policy_records)),
                ("sourceEnabledCount", JsonValue::number(server_count_after)),
                ("effectiveEnabledCount", JsonValue::number(effective_server_count_after)),
            ]),
        ),
        ("serversConfigured", JsonValue::number(server_count_after)),
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
            "mcpInitialized",
            result_json(initialized_notification.map(|status| {
                JsonValue::object([
                    ("ok", JsonValue::bool(true)),
                    ("httpStatus", JsonValue::number(status)),
                ])
            })),
        ),
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
                ("initReady", JsonValue::bool(init_ok)),
                ("homeImportReady", JsonValue::bool(home_import_ok)),
                ("serverInstallReady", JsonValue::bool(server_install_ok)),
                ("serverConfigured", JsonValue::bool(server_configured)),
                (
                    "effectiveServerConfigured",
                    JsonValue::bool(effective_server_configured),
                ),
                ("mcpToolsExpected", JsonValue::bool(tools_expected)),
                ("serveRunning", JsonValue::bool(serve_ok)),
                ("clientInstallReady", JsonValue::bool(install_ok)),
                ("serviceInstallReady", JsonValue::bool(service_ok)),
                ("readinessReady", JsonValue::bool(readiness_ok)),
                ("healthOk", JsonValue::bool(health_ok)),
                ("mcpInitializeOk", JsonValue::bool(initialize_ok)),
                ("mcpInitializedOk", JsonValue::bool(initialized_ok)),
                ("mcpToolsOk", JsonValue::bool(tools_ok)),
                ("mcpToolsReady", JsonValue::bool(tools_ready)),
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
                    "Open Cursor, VS Code, Claude Code, or another local MCP client and approve/trust the MCPace entry if the client asks. Endpoint: {}.",
                    endpoint
                )),
                JsonValue::string(
                    "Add upstream servers only when you choose with 'mcpace install <package|url|command>' or 'mcpace server import <mcp-settings-file>'; MCPace keeps one stable client endpoint and preserves existing client config entries."
                        .to_string(),
                ),
            ]),
        ),
    ])
}

#[derive(Clone, Copy, Debug, Default)]
struct SetupServerCounts {
    policy_records: usize,
    source_enabled: usize,
    effective_enabled: usize,
}

fn setup_server_counts(root_path: &Path) -> SetupServerCounts {
    match server::load_server_records(root_path) {
        Ok(records) => SetupServerCounts {
            policy_records: records.len(),
            source_enabled: records
                .iter()
                .filter(|record| record.source_enabled)
                .count(),
            effective_enabled: records
                .iter()
                .filter(|record| record.effective_enabled)
                .count(),
        },
        Err(_) => SetupServerCounts::default(),
    }
}

#[derive(Clone, Debug)]
struct HomeMcpSource {
    client_id: String,
    path: PathBuf,
}

fn import_existing_home_mcp_servers(
    root_path: &Path,
    endpoint: &str,
    warnings: &mut Vec<String>,
) -> CommandResult {
    let target_path = root_path
        .join("mcp_settings.d")
        .join("auto-imported-home.json");
    let target_text = target_path.display().to_string();
    let _namespace_lock = match mcp_sources::acquire_mcp_settings_namespace_lock(root_path) {
        Ok(lock) => lock,
        Err(error) => {
            return CommandResult {
                ok: false,
                exit_code: 1,
                json: Some(JsonValue::object([
                    ("mode", JsonValue::string("home-import")),
                    ("ok", JsonValue::bool(false)),
                    ("targetPath", JsonValue::string(target_text)),
                    ("error", JsonValue::string(error.clone())),
                ])),
                stdout: String::new(),
                stderr: error,
            };
        }
    };
    let _target_lock =
        match runtimepaths::acquire_exclusive_file_lock(&target_path, "home MCP import") {
            Ok(lock) => lock,
            Err(error) => {
                return CommandResult {
                    ok: false,
                    exit_code: 1,
                    json: Some(JsonValue::object([
                        ("mode", JsonValue::string("home-import")),
                        ("ok", JsonValue::bool(false)),
                        ("targetPath", JsonValue::string(target_text)),
                        ("error", JsonValue::string(error.clone())),
                    ])),
                    stdout: String::new(),
                    stderr: error,
                };
            }
        };
    let sources = collect_existing_home_mcp_sources(root_path, warnings);

    let mut imported = BTreeMap::<String, (String, JsonValue, String, String)>::new();
    let mut skipped = Vec::new();
    for source in &sources {
        let source_text = source.path.display().to_string();
        let source_value = match json_helpers::read_json_file(&source.path) {
            Ok(value) => value,
            Err(error) => {
                warnings.push(format!(
                    "Home MCP import could not read '{}': {}",
                    source.path.display(),
                    error
                ));
                continue;
            }
        };
        let (shape, servers) = match json_helpers::mcp_servers_object_with_key(&source_value) {
            Some(value) => value,
            None => continue,
        };
        for (name, server_value) in servers {
            let normalized_name = mcp_sources::normalize_server_name(name);
            if normalized_name.is_empty() {
                skipped.push(JsonValue::object([
                    ("name", JsonValue::string(name.clone())),
                    ("source", JsonValue::string(source_text.clone())),
                    ("reason", JsonValue::string("empty-normalized-name")),
                ]));
                continue;
            }
            let normalized_value = normalize_home_imported_server_value(server_value);
            if is_mcpace_self_entry(name, &normalized_value, endpoint) {
                skipped.push(JsonValue::object([
                    ("name", JsonValue::string(name.clone())),
                    ("source", JsonValue::string(source_text.clone())),
                    ("reason", JsonValue::string("mcpace-self-entry")),
                ]));
                continue;
            }
            if imported.contains_key(&normalized_name) {
                warnings.push(format!(
                    "Home MCP import skipped duplicate server '{}' from '{}'; first matching normalized name wins",
                    name,
                    source.path.display()
                ));
                skipped.push(JsonValue::object([
                    ("name", JsonValue::string(name.clone())),
                    ("source", JsonValue::string(source_text.clone())),
                    ("reason", JsonValue::string("duplicate-normalized-name")),
                ]));
                continue;
            }
            imported.insert(
                normalized_name.clone(),
                (
                    name.clone(),
                    normalized_value,
                    source.client_id.clone(),
                    format!("{}:{}", shape, source_text),
                ),
            );
        }
    }

    if imported.is_empty() {
        return CommandResult {
            ok: true,
            exit_code: 0,
            json: Some(JsonValue::object([
                ("mode", JsonValue::string("home-import")),
                ("ok", JsonValue::bool(true)),
                ("targetPath", JsonValue::string(target_text)),
                ("sourceFileCount", JsonValue::number(sources.len())),
                ("importedCount", JsonValue::number(0)),
                ("entries", JsonValue::array(std::iter::empty::<JsonValue>())),
                ("skipped", JsonValue::array(skipped)),
            ])),
            stdout: String::new(),
            stderr: String::new(),
        };
    }

    let mut mcp_servers = BTreeMap::new();
    let mut entries = Vec::new();
    for (normalized_name, (name, value, client_id, source)) in imported {
        mcp_servers.insert(name.clone(), value);
        entries.push(JsonValue::object([
            ("name", JsonValue::string(name)),
            ("normalizedName", JsonValue::string(normalized_name)),
            ("clientId", JsonValue::string(client_id)),
            ("source", JsonValue::string(source)),
        ]));
    }

    let output = JsonValue::object([
        ("mcpServers", JsonValue::Object(mcp_servers)),
        (
            "_meta",
            JsonValue::object([
                ("managedBy", JsonValue::string("mcpace up")),
                (
                    "note",
                    JsonValue::string(
                        "Generated from existing local MCP client configs. Edit the original client config or add explicit MCPace sources for permanent custom entries."
                    ),
                ),
            ]),
        ),
    ]);

    if let Some(parent) = target_path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            let message = format!(
                "failed to create home MCP import directory '{}': {}",
                parent.display(),
                error
            );
            return CommandResult {
                ok: false,
                exit_code: 1,
                json: Some(JsonValue::object([
                    ("mode", JsonValue::string("home-import")),
                    ("ok", JsonValue::bool(false)),
                    ("targetPath", JsonValue::string(target_text)),
                    ("error", JsonValue::string(message.clone())),
                ])),
                stdout: String::new(),
                stderr: message,
            };
        }
    }

    let mut serialized = output.to_pretty_string();
    serialized.push('\n');
    if let Err(error) = runtimepaths::write_private_text_atomic(&target_path, &serialized) {
        let message = format!(
            "failed to write home MCP import file '{}': {}",
            target_path.display(),
            error
        );
        return CommandResult {
            ok: false,
            exit_code: 1,
            json: Some(JsonValue::object([
                ("mode", JsonValue::string("home-import")),
                ("ok", JsonValue::bool(false)),
                ("targetPath", JsonValue::string(target_text)),
                ("error", JsonValue::string(message.clone())),
            ])),
            stdout: String::new(),
            stderr: message,
        };
    }

    let imported_count = entries.len();
    CommandResult {
        ok: true,
        exit_code: 0,
        json: Some(JsonValue::object([
            ("mode", JsonValue::string("home-import")),
            ("ok", JsonValue::bool(true)),
            ("targetPath", JsonValue::string(target_text)),
            ("sourceFileCount", JsonValue::number(sources.len())),
            ("importedCount", JsonValue::number(imported_count)),
            ("entries", JsonValue::array(entries)),
            ("skipped", JsonValue::array(skipped)),
        ])),
        stdout: String::new(),
        stderr: String::new(),
    }
}

fn collect_existing_home_mcp_sources(
    root_path: &Path,
    warnings: &mut Vec<String>,
) -> Vec<HomeMcpSource> {
    let mut sources = Vec::new();
    let mut seen = BTreeSet::new();

    match client_catalog::load_registry(Some(root_path)) {
        Ok(registry) => {
            for warning in registry.warnings {
                warnings.push(format!("Client catalog warning: {}", warning));
            }
            for target in registry.targets {
                if target.surface_class != "local" {
                    continue;
                }
                let target_id = target.id.clone();
                for expr in &target.config_paths {
                    if !looks_like_json_mcp_config_path(expr) {
                        continue;
                    }
                    push_existing_home_source_expr(
                        &mut sources,
                        &mut seen,
                        &target_id,
                        expr,
                        root_path,
                    );
                }
            }
        }
        Err(error) => warnings.push(format!(
            "Home MCP import could not load client catalog: {}",
            error
        )),
    }

    for (client_id, path) in standard_home_mcp_config_paths(root_path) {
        push_existing_home_source_path(&mut sources, &mut seen, &client_id, path);
    }

    sources.sort_by(|left, right| left.path.cmp(&right.path));
    sources
}

fn looks_like_json_mcp_config_path(expr: &str) -> bool {
    let lower = expr.trim().to_ascii_lowercase();
    !lower.contains('*')
        && !lower.contains('<')
        && !lower.contains('>')
        && (lower.ends_with(".json") || lower.ends_with("/mcp") || lower.ends_with("\\mcp"))
}

fn push_existing_home_source_expr(
    sources: &mut Vec<HomeMcpSource>,
    seen: &mut BTreeSet<PathBuf>,
    client_id: &str,
    expr: &str,
    root_path: &Path,
) {
    if let Some(path) = expand_user_or_root_path(expr, root_path) {
        push_existing_home_source_path(sources, seen, client_id, path);
    }
    let trimmed = expr.trim();
    if trimmed.starts_with("~/")
        || trimmed.starts_with("~\\")
        || PathBuf::from(trimmed).is_absolute()
    {
        return;
    }
    if let Ok(current_dir) = std::env::current_dir() {
        push_existing_home_source_path(sources, seen, client_id, current_dir.join(trimmed));
    }
}

fn push_existing_home_source_path(
    sources: &mut Vec<HomeMcpSource>,
    seen: &mut BTreeSet<PathBuf>,
    client_id: &str,
    path: PathBuf,
) {
    if !path.is_file() {
        return;
    }
    let path = runtimepaths::canonicalize_or_original(&path);
    if seen.insert(path.clone()) {
        sources.push(HomeMcpSource {
            client_id: client_id.to_string(),
            path,
        });
    }
}

fn standard_home_mcp_config_paths(root_path: &Path) -> Vec<(String, PathBuf)> {
    let mut paths = Vec::new();
    if let Ok(current_dir) = std::env::current_dir() {
        paths.push(("vscode".to_string(), current_dir.join(".vscode/mcp.json")));
        paths.push(("claude-code".to_string(), current_dir.join(".mcp.json")));
    }
    if let Some(home) = runtimepaths::user_home_dir() {
        paths.push((
            "claude-desktop".to_string(),
            home.join("Library/Application Support/Claude/claude_desktop_config.json"),
        ));
        paths.push((
            "claude-desktop".to_string(),
            home.join(".config/Claude/claude_desktop_config.json"),
        ));
        paths.push((
            "vscode".to_string(),
            home.join(".config/Code/User/mcp.json"),
        ));
        paths.push((
            "vscode-insiders".to_string(),
            home.join(".config/Code - Insiders/User/mcp.json"),
        ));
        paths.push((
            "vscode".to_string(),
            home.join("Library/Application Support/Code/User/mcp.json"),
        ));
        paths.push((
            "vscode-insiders".to_string(),
            home.join("Library/Application Support/Code - Insiders/User/mcp.json"),
        ));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let appdata = PathBuf::from(appdata);
        paths.push((
            "claude-desktop".to_string(),
            appdata.join("Claude/claude_desktop_config.json"),
        ));
        paths.push(("vscode".to_string(), appdata.join("Code/User/mcp.json")));
        paths.push((
            "vscode-insiders".to_string(),
            appdata.join("Code - Insiders/User/mcp.json"),
        ));
    }
    paths.push(("mcpace".to_string(), root_path.join("mcp_settings.json")));
    paths
}

fn normalize_home_import_type(raw_type: &str, has_command: bool, has_url: bool) -> String {
    if raw_type.trim().is_empty() && !has_command && !has_url {
        return String::new();
    }
    crate::source_type::infer_public_source_type(
        raw_type,
        if has_command { "command" } else { "" },
        if has_url {
            "https://example.invalid/mcp"
        } else {
            ""
        },
    )
}

fn normalize_home_imported_server_value(value: &JsonValue) -> JsonValue {
    let Some(object) = value.as_object() else {
        return value.clone();
    };
    let mut normalized = object.clone();

    if !normalized.contains_key("url") {
        for key in ["serverUrl", "httpUrl", "endpoint"] {
            if let Some(url) = object.get(key).and_then(JsonValue::as_str) {
                if !url.trim().is_empty() {
                    normalized.insert("url".to_string(), JsonValue::string(url.trim()));
                    break;
                }
            }
        }
    }

    let raw_type = normalized
        .get("type")
        .or_else(|| object.get("transport"))
        .and_then(JsonValue::as_str)
        .unwrap_or("");
    let has_command = normalized
        .get("command")
        .and_then(JsonValue::as_str)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let has_url = normalized
        .get("url")
        .and_then(JsonValue::as_str)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let normalized_type = normalize_home_import_type(raw_type, has_command, has_url);
    if !normalized_type.is_empty() {
        normalized.insert("type".to_string(), JsonValue::string(normalized_type));
    }

    if !normalized.contains_key("enabled") {
        if let Some(disabled) = object.get("disabled").and_then(JsonValue::as_bool) {
            normalized.insert("enabled".to_string(), JsonValue::bool(!disabled));
        }
    }

    JsonValue::Object(normalized)
}

fn is_mcpace_self_entry(name: &str, value: &JsonValue, endpoint: &str) -> bool {
    let normalized_name = mcp_sources::normalize_server_name(name);
    if normalized_name == "mcpace" || normalized_name == "mcp-pace" {
        return true;
    }
    let Some(object) = value.as_object() else {
        return false;
    };
    if object
        .get("url")
        .and_then(JsonValue::as_str)
        .map(|url| normalized_endpoint_matches(url, endpoint))
        .unwrap_or(false)
    {
        return true;
    }
    let command = object
        .get("command")
        .and_then(JsonValue::as_str)
        .unwrap_or("")
        .trim();
    if command.is_empty() {
        return false;
    }
    let command_base = command
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(command)
        .trim_end_matches(".exe")
        .to_ascii_lowercase();
    if command_base != "mcpace" {
        return false;
    }
    json_helpers::strings_from_array(object.get("args").and_then(JsonValue::as_array))
        .iter()
        .any(|arg| matches!(arg.as_str(), "mcp-server" | "stdio-shim" | "serve"))
}

fn normalized_endpoint_matches(url: &str, endpoint: &str) -> bool {
    let left = url.trim().trim_end_matches('/');
    let right = endpoint.trim().trim_end_matches('/');
    if left.eq_ignore_ascii_case(right) {
        return true;
    }
    let lower = left.to_ascii_lowercase();
    lower.contains("127.0.0.1:39022") || lower.contains("localhost:39022")
}

fn run_client_install(
    root_path: &Path,
    root_text: &str,
    selector: Option<&str>,
    warnings: &mut Vec<String>,
) -> CommandResult {
    let selector = selector.unwrap_or("auto").trim().to_ascii_lowercase();
    if selector == "all" {
        return run_json_command(vec![
            "client".to_string(),
            "install".to_string(),
            "all".to_string(),
            "--json".to_string(),
            "--root".to_string(),
            root_text.to_string(),
        ]);
    }
    if selector != "auto" {
        return run_json_command(vec![
            "client".to_string(),
            "install".to_string(),
            selector,
            "--json".to_string(),
            "--root".to_string(),
            root_text.to_string(),
        ]);
    }

    let detected = detect_local_clients(root_path, warnings);
    if detected.is_empty() {
        warnings.push(
            "No supported local client was auto-detected, so MCPace did not create new app config files. Run 'mcpace client list' or 'mcpace client install cursor-local' after installing a client.".to_string(),
        );
        let json = JsonValue::object([
            ("mode", JsonValue::string("auto-detected-none")),
            ("ok", JsonValue::bool(true)),
            (
                "detected",
                JsonValue::array(std::iter::empty::<JsonValue>()),
            ),
        ]);
        return CommandResult {
            ok: true,
            exit_code: 0,
            json: Some(json),
            stdout: "".to_string(),
            stderr: "".to_string(),
        };
    }

    let mut results = Vec::new();
    let mut combined_stdout = String::new();
    let mut combined_stderr = String::new();
    let mut ok = true;
    for id in detected {
        let result = run_json_command(vec![
            "client".to_string(),
            "install".to_string(),
            id.clone(),
            "--json".to_string(),
            "--root".to_string(),
            root_text.to_string(),
        ]);
        ok &= result.ok;
        combined_stdout.push_str(&result.stdout);
        combined_stderr.push_str(&result.stderr);
        results.push(JsonValue::object([
            ("clientTargetId", JsonValue::string(id)),
            ("ok", JsonValue::bool(result.ok)),
            ("exitCode", JsonValue::number(result.exit_code)),
            ("json", result.json.unwrap_or(JsonValue::Null)),
            ("stderr", JsonValue::string(result.stderr)),
        ]));
    }

    CommandResult {
        ok,
        exit_code: if ok { 0 } else { 1 },
        json: Some(JsonValue::object([
            ("mode", JsonValue::string("auto-detected")),
            ("ok", JsonValue::bool(ok)),
            ("installed", JsonValue::array(results)),
        ])),
        stdout: combined_stdout,
        stderr: combined_stderr,
    }
}

fn detect_local_clients(root_path: &Path, warnings: &mut Vec<String>) -> Vec<String> {
    let registry = match client_catalog::load_registry(Some(root_path)) {
        Ok(value) => value,
        Err(error) => {
            warnings.push(format!(
                "Client auto-detect could not load client catalog: {}",
                error
            ));
            return Vec::new();
        }
    };
    for warning in registry.warnings {
        warnings.push(format!("Client catalog warning: {}", warning));
    }

    let mut detected = Vec::new();
    for target in registry.targets {
        if target.surface_class != "local" || !target.supports_client_install() {
            continue;
        }
        if client_target_detected(&target, root_path) {
            detected.push(target.id);
        }
    }
    detected.sort();
    detected.dedup();
    detected
}

fn client_target_detected(target: &client_catalog::ClientTargetRecord, root_path: &Path) -> bool {
    if target
        .config_paths
        .iter()
        .any(|path| config_path_exists_for_target(path, root_path))
    {
        return true;
    }
    command_candidates_for_client(&target.id)
        .iter()
        .any(|command| doctor::command_available(command))
}

fn config_path_exists_for_target(expr: &str, root_path: &Path) -> bool {
    if expand_user_or_root_path(expr, root_path)
        .map(|path| path.is_file())
        .unwrap_or(false)
    {
        return true;
    }
    let trimmed = expr.trim();
    if trimmed.starts_with("~/")
        || trimmed.starts_with("~\\")
        || PathBuf::from(trimmed).is_absolute()
    {
        return false;
    }
    std::env::current_dir()
        .map(|current_dir| current_dir.join(trimmed).is_file())
        .unwrap_or(false)
}

fn command_candidates_for_client(id: &str) -> &'static [&'static str] {
    match id {
        "codex" => &["codex"],
        "claude-code" => &["claude"],
        "cursor-local" => &["cursor"],
        "kiro-ide" | "kiro-cli" => &["kiro"],
        "windsurf" => &["windsurf"],
        "gemini-cli" => &["gemini"],
        "github-copilot-cli" => &[],
        "hermes-agent" => &["hermes"],
        _ => &[],
    }
}

fn expand_user_or_root_path(expr: &str, root_path: &Path) -> Option<PathBuf> {
    if expr.contains('*') || expr.contains('<') || expr.contains('>') {
        return None;
    }
    let trimmed = expr.trim();
    if let Some(rest) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
    {
        let mut path = runtimepaths::user_home_dir()?;
        for segment in rest.split(['/', '\\']) {
            if !segment.is_empty() {
                path.push(segment);
            }
        }
        return Some(path);
    }
    let path = PathBuf::from(trimmed);
    if path.is_absolute() {
        Some(path)
    } else {
        Some(root_path.join(path))
    }
}

fn root_bootstrap_json(bootstrap: &RootBootstrap) -> JsonValue {
    JsonValue::object([
        (
            "createdRootDir",
            JsonValue::bool(bootstrap.created_root_dir),
        ),
        ("createdConfig", JsonValue::bool(bootstrap.created_config)),
        (
            "createdSettings",
            JsonValue::bool(bootstrap.created_settings),
        ),
        (
            "createdSettingsDir",
            JsonValue::bool(bootstrap.created_settings_dir),
        ),
    ])
}

fn looks_like_multiword_server_command(spec: &str) -> bool {
    let base = spec
        .split_whitespace()
        .next()
        .unwrap_or("")
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .trim_end_matches(".exe")
        .to_ascii_lowercase();
    matches!(
        base.as_str(),
        "npx" | "bunx" | "pnpm" | "yarn" | "uvx" | "docker"
    )
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
    json_helpers::empty_object()
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

fn http_mcp_request(
    host: &str,
    port: u16,
    path: &str,
    body: JsonValue,
    session_id: Option<&str>,
) -> Result<HttpJsonResponse, String> {
    let body = body.to_compact_string();
    let path = runtimepaths::normalize_http_path(path, runtimepaths::DEFAULT_LOCAL_MCP_PATH);
    let session_header = if let Some(value) = session_id {
        if !text_utils::valid_http_header_value(value) {
            return Err("setup probe received an invalid MCP session id header".to_string());
        }
        format!("Mcp-Session-Id: {}\r\n", value)
    } else {
        String::new()
    };
    let request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMCP-Protocol-Version: {}\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        path,
        host,
        mcp::CURRENT_PROTOCOL_VERSION,
        session_header,
        body.len(),
        body
    );
    http_json_response(host, port, &request)
}

fn http_json_request(host: &str, port: u16, request: &str) -> Result<JsonValue, String> {
    http_json_response(host, port, request).map(|response| response.json)
}

fn http_mcp_notification(
    host: &str,
    port: u16,
    path: &str,
    body: JsonValue,
    session_id: Option<&str>,
) -> Result<u16, String> {
    let body = body.to_compact_string();
    let path = runtimepaths::normalize_http_path(path, runtimepaths::DEFAULT_LOCAL_MCP_PATH);
    let session_header = if let Some(value) = session_id {
        if !text_utils::valid_http_header_value(value) {
            return Err("setup probe received an invalid MCP session id header".to_string());
        }
        format!("Mcp-Session-Id: {}\r\n", value)
    } else {
        String::new()
    };
    let request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMCP-Protocol-Version: {}\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        path,
        host,
        mcp::CURRENT_PROTOCOL_VERSION,
        session_header,
        body.len(),
        body
    );
    let response = http_raw_response(host, port, &request)?;
    let parsed = parse_http_setup_response(&response)?;
    if matches!(parsed.status, 200 | 202 | 204) {
        Ok(parsed.status)
    } else {
        Err(format!("HTTP notification failed: {}", parsed.status_line))
    }
}

fn http_json_response(host: &str, port: u16, request: &str) -> Result<HttpJsonResponse, String> {
    let response = http_raw_response(host, port, request)?;
    parse_http_json_response(&response)
}

fn http_raw_response(host: &str, port: u16, request: &str) -> Result<String, String> {
    let probe_host = probe_host(host);
    let addrs = (probe_host.as_str(), port)
        .to_socket_addrs()
        .map_err(|error| format!("resolve {}:{}: {}", probe_host, port, error))?
        .collect::<Vec<_>>();
    if addrs.is_empty() {
        return Err(format!("{}:{} resolved to no addresses", probe_host, port));
    }

    let mut last_error = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, HTTP_PROBE_TIMEOUT) {
            Ok(mut stream) => {
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
                return read_http_setup_response(&mut stream);
            }
            Err(error) => last_error = Some(error),
        }
    }

    Err(format!(
        "connect {}:{}: {}",
        probe_host,
        port,
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "no resolved address accepted the connection".to_string())
    ))
}

fn read_http_setup_response(stream: &mut TcpStream) -> Result<String, String> {
    let mut raw = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                raw.extend_from_slice(&buffer[..count]);
                if raw.len() > MAX_HTTP_SETUP_RESPONSE_BYTES {
                    return Err(
                        "HTTP setup probe response exceeded the maximum supported size".to_string(),
                    );
                }
                if let Ok(text) = std::str::from_utf8(&raw) {
                    if http_setup_response_ready(text) {
                        return Ok(text.to_string());
                    }
                }
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if let Ok(text) = std::str::from_utf8(&raw) {
                    if http_setup_response_ready(text) {
                        return Ok(text.to_string());
                    }
                }
                return Err("timed out while reading HTTP setup probe response".to_string());
            }
            Err(error) => return Err(format!("read response: {}", error)),
        }
    }

    String::from_utf8(raw).map_err(|_| "HTTP setup probe returned a non-UTF-8 response".to_string())
}

fn http_setup_response_ready(raw: &str) -> bool {
    let Ok(parsed) = parse_http_setup_response(raw) else {
        return false;
    };
    if parsed.status != 200 {
        return true;
    }
    let transfer_encoding = parsed.transfer_encoding.to_ascii_lowercase();
    let content_type = parsed.content_type.to_ascii_lowercase();
    if content_type.contains("text/event-stream") {
        return sse_json_body(parsed.body).is_some();
    }
    if transfer_encoding.contains("chunked") {
        return decode_chunked_body(parsed.body.as_bytes()).is_ok();
    }
    if let Some(content_length) = parsed.content_length {
        return parsed.body.len() >= content_length;
    }
    parse_str(parsed.body.trim()).is_ok()
}

fn parse_http_json_response(response: &str) -> Result<HttpJsonResponse, String> {
    let parsed = parse_http_setup_response(response)?;
    if parsed.status != 200 {
        return Err(format!("HTTP request failed: {}", parsed.status_line));
    }
    let body_bytes = if parsed
        .transfer_encoding
        .to_ascii_lowercase()
        .contains("chunked")
    {
        decode_chunked_body(parsed.body.as_bytes())?
    } else if let Some(content_length) = parsed.content_length {
        parsed
            .body
            .as_bytes()
            .get(..content_length.min(parsed.body.len()))
            .unwrap_or(parsed.body.as_bytes())
            .to_vec()
    } else {
        parsed.body.as_bytes().to_vec()
    };
    let body = String::from_utf8(body_bytes)
        .map_err(|_| "HTTP setup probe returned a non-UTF-8 body".to_string())?;
    let json_body = if parsed
        .content_type
        .to_ascii_lowercase()
        .contains("text/event-stream")
    {
        sse_json_body(&body).ok_or_else(|| {
            "HTTP setup probe SSE response did not contain a JSON-RPC response".to_string()
        })?
    } else {
        body.trim().to_string()
    };
    let json =
        parse_str(&json_body).map_err(|error| format!("parse HTTP JSON response: {}", error))?;
    Ok(HttpJsonResponse {
        headers: parsed.headers,
        json,
    })
}

fn parse_http_setup_response(raw: &str) -> Result<ParsedHttpJsonResponse<'_>, String> {
    let (headers_text, body) = raw
        .split_once("\r\n\r\n")
        .ok_or_else(|| "HTTP response missing header/body separator".to_string())?;
    let mut lines = headers_text.lines();
    let status_line = lines.next().unwrap_or_default().to_string();
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| format!("HTTP response has malformed status line: {}", status_line))?;
    let mut headers = Vec::new();
    let mut content_type = String::new();
    let mut content_length = None;
    let mut transfer_encoding = String::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_string();
        let value = value.trim().to_string();
        match name.to_ascii_lowercase().as_str() {
            "content-type" => content_type = value.clone(),
            "content-length" => content_length = value.parse::<usize>().ok(),
            "transfer-encoding" => transfer_encoding = value.clone(),
            _ => {}
        }
        headers.push((name, value));
    }
    Ok(ParsedHttpJsonResponse {
        status_line,
        status,
        headers,
        content_type,
        content_length,
        transfer_encoding,
        body,
    })
}

fn decode_chunked_body(body: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoded = Vec::new();
    let mut offset = 0usize;
    loop {
        let Some(line_end) = find_crlf(body, offset) else {
            return Err("chunked HTTP body is incomplete".to_string());
        };
        let size_line = std::str::from_utf8(&body[offset..line_end])
            .map_err(|_| "chunked HTTP body has a non-UTF-8 size line".to_string())?;
        let size_hex = size_line.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_hex, 16)
            .map_err(|_| "chunked HTTP body has an invalid chunk size".to_string())?;
        offset = line_end + 2;
        if size == 0 {
            return Ok(decoded);
        }
        if body.len() < offset + size {
            return Err("chunked HTTP body is incomplete".to_string());
        }
        decoded.extend_from_slice(&body[offset..offset + size]);
        offset += size;
        if body.get(offset..offset + 2) == Some(b"\r\n") {
            offset += 2;
        } else if offset < body.len() {
            return Err("chunked HTTP body is missing a chunk terminator".to_string());
        }
    }
}

fn find_crlf(body: &[u8], start: usize) -> Option<usize> {
    body.get(start..)?
        .windows(2)
        .position(|window| window == b"\r\n")
        .map(|index| start + index)
}

fn sse_json_body(body: &str) -> Option<String> {
    let normalized = body.replace("\r\n", "\n");
    for event in normalized.split("\n\n") {
        let data = event
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim)
            .collect::<Vec<_>>()
            .join("\n");
        let data = data.trim();
        if data.is_empty() {
            continue;
        }
        if parse_str(data).is_ok() {
            return Some(data.to_string());
        }
    }
    None
}

fn probe_host(host: &str) -> String {
    match host {
        "0.0.0.0" | "::" => runtimepaths::DEFAULT_LOCAL_HOST.to_string(),
        other => other.to_string(),
    }
}

fn usize_at_path(value: &JsonValue, path: &[&str]) -> Option<usize> {
    json_helpers::value_at_path(value, path)?
        .as_i64()
        .and_then(|number| usize::try_from(number).ok())
}

fn write_text_report(report: &JsonValue, stdout: &mut dyn Write) {
    let status = json_helpers::string_at_path(report, &["status"]).unwrap_or("unknown");
    let endpoint = json_helpers::string_at_path(report, &["endpoint"]).unwrap_or("unknown");
    let _ = writeln!(stdout, "MCPace setup: {}", status);
    let _ = writeln!(stdout, "Endpoint: {}", endpoint);
    for (label, path) in [
        ("Init ready", "initReady"),
        ("Server install ready", "serverInstallReady"),
        ("Serve running", "serveRunning"),
        ("Client install ready", "clientInstallReady"),
        ("Autostart install ready", "serviceInstallReady"),
        ("Readiness ready", "readinessReady"),
        ("Health OK", "healthOk"),
        ("MCP initialize OK", "mcpInitializeOk"),
        ("MCP initialized notification OK", "mcpInitializedOk"),
        ("mcpace command in PATH", "mcpaceCommandInPath"),
    ] {
        let value = json_helpers::bool_at_path(report, &["checks", path]).unwrap_or(false);
        let _ = writeln!(stdout, "- {}: {}", label, if value { "yes" } else { "no" });
    }
    let source_count = usize_at_path(report, &["serversKnown", "sourceEnabledCount"])
        .unwrap_or_else(|| usize_at_path(report, &["serversConfigured"]).unwrap_or(0));
    let effective_count =
        usize_at_path(report, &["serversKnown", "effectiveEnabledCount"]).unwrap_or(source_count);
    let tools_expected = json_helpers::bool_at_path(report, &["checks", "mcpToolsExpected"])
        .unwrap_or(effective_count > 0);
    let tools_ready = json_helpers::bool_at_path(report, &["checks", "mcpToolsReady"])
        .unwrap_or_else(|| {
            json_helpers::bool_at_path(report, &["checks", "mcpToolsOk"]).unwrap_or(false)
        });
    let _ = writeln!(
        stdout,
        "- Upstream servers: {} source-enabled, {} effective-enabled{}",
        source_count,
        effective_count,
        if source_count == 0 {
            " (optional; empty setup is OK)"
        } else {
            ""
        }
    );
    let _ = writeln!(
        stdout,
        "- MCP tools ready: {}",
        if !tools_expected {
            "not expected yet"
        } else if tools_ready {
            "yes"
        } else {
            "no"
        }
    );
    if let Some(warnings) = json_helpers::array_at_path(report, &["warnings"]) {
        if !warnings.is_empty() {
            let _ = writeln!(stdout, "Warnings:");
            for warning in warnings.iter().filter_map(JsonValue::as_str) {
                let _ = writeln!(stdout, "- {}", warning);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_import_normalizes_url_alias_type_and_disabled() {
        let value = JsonValue::object([
            ("serverUrl", JsonValue::string("https://example.com/mcp")),
            ("transport", JsonValue::string("http")),
            ("disabled", JsonValue::bool(true)),
        ]);
        let normalized = normalize_home_imported_server_value(&value);
        assert_eq!(
            normalized.get("url").and_then(JsonValue::as_str),
            Some("https://example.com/mcp")
        );
        assert_eq!(
            normalized.get("type").and_then(JsonValue::as_str),
            Some("streamable-http")
        );
        assert_eq!(
            normalized.get("enabled").and_then(JsonValue::as_bool),
            Some(false)
        );
    }

    #[test]
    fn home_import_skips_mcp_pace_self_name_and_endpoint() {
        let value = JsonValue::object([("url", JsonValue::string("http://127.0.0.1:39022/mcp"))]);
        assert!(is_mcpace_self_entry(
            "mcp pace",
            &value,
            "http://127.0.0.1:39022/mcp"
        ));
    }
}
