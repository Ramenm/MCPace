use crate::http_probe::HttpJsonResponse;
use crate::json::{parse_str, JsonValue};
use crate::{
    app, client_catalog, diagnostics, doctor, http_probe, json_helpers, mcp_protocol as mcp,
    mcp_sources, resources, runtimepaths, server, text_utils,
};
use clap::{error::ErrorKind as ClapErrorKind, Parser};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

const HTTP_PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_HTTP_SETUP_RESPONSE_BYTES: usize = http_probe::DEFAULT_MAX_RESPONSE_BYTES;

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
    let parsed = parse_cli(args);
    if let Some(error) = parsed.error.clone() {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 2;
    }
    if parsed.help {
        write_help(stdout);
        return 0;
    }

    let root_path = match resolve_setup_root(parsed.root_override.clone(), default_root) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    let bootstrap = match ensure_setup_root_layout(&root_path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
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

#[derive(Debug, Parser)]
#[command(
    name = "mcpace up",
    disable_version_flag = true,
    about = "Home-first onboarding for MCPace"
)]
struct SetupCli {
    #[arg(value_name = "SERVER_SPEC_OR_PATH")]
    values: Vec<String>,

    #[arg(long = "json")]
    json_output: bool,

    #[arg(long = "root", value_name = "PATH")]
    root_override: Option<PathBuf>,

    #[arg(long = "host", value_name = "ADDR")]
    host: Option<String>,

    #[arg(long = "port", value_name = "N")]
    port: Option<u16>,

    #[arg(long = "max-connections", value_name = "N")]
    max_connections: Option<usize>,

    #[arg(long = "io-timeout-ms", value_name = "MS")]
    io_timeout_ms: Option<u64>,

    #[arg(long = "max-body-bytes", value_name = "N")]
    max_body_bytes: Option<usize>,

    #[arg(long = "overview-cache-ms", value_name = "MS")]
    overview_cache_ms: Option<u64>,

    #[arg(
        long = "skip-client-install",
        alias = "no-client",
        alias = "skip-client"
    )]
    skip_client_install: bool,

    #[arg(long = "client", alias = "for", value_name = "auto|all|none|ID")]
    client_selector: Option<String>,

    #[arg(long = "all-clients")]
    all_clients: bool,

    #[arg(long = "auto-client", alias = "auto-clients")]
    auto_client: bool,

    #[arg(
        long = "server",
        alias = "with-server",
        alias = "install-server",
        value_name = "SPEC"
    )]
    server_spec: Option<String>,

    #[arg(long = "as", alias = "server-name", value_name = "NAME")]
    server_name: Option<String>,

    #[arg(long = "path", alias = "server-path", value_name = "PATH")]
    server_paths: Vec<String>,

    #[arg(long = "force")]
    server_force: bool,

    #[arg(long = "no-default-server", alias = "no-server")]
    no_default_server: bool,

    #[arg(
        long = "install-service",
        alias = "install-autostart",
        alias = "autostart"
    )]
    install_service: bool,

    #[arg(long = "no-enable")]
    no_enable_service: bool,
}

fn parse_cli(args: &[String]) -> ParsedArgs {
    match SetupCli::try_parse_from(argv(args)) {
        Ok(cli) => parsed_from_cli(cli),
        Err(error)
            if matches!(
                error.kind(),
                ClapErrorKind::DisplayHelp | ClapErrorKind::DisplayVersion
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

fn parsed_from_cli(cli: SetupCli) -> ParsedArgs {
    let (skip_client_selector, client_selector) = normalized_client_selector(cli.client_selector);
    let mut parsed = ParsedArgs {
        help: false,
        json_output: cli.json_output,
        root_override: cli.root_override,
        host: cli.host,
        port: cli.port.unwrap_or(0),
        max_connections: cli.max_connections,
        io_timeout_ms: cli.io_timeout_ms,
        max_body_bytes: cli.max_body_bytes,
        overview_cache_ms: cli.overview_cache_ms,
        skip_client_install: cli.skip_client_install || skip_client_selector,
        client_selector,
        install_service: cli.install_service,
        no_enable_service: cli.no_enable_service,
        server_spec: cli.server_spec,
        server_name: cli.server_name,
        server_paths: cli.server_paths,
        server_force: cli.server_force,
        no_default_server: cli.no_default_server,
        error: None,
    };

    if cli.all_clients {
        parsed.skip_client_install = false;
        parsed.client_selector = Some("all".to_string());
    }
    if cli.auto_client {
        parsed.skip_client_install = false;
        parsed.client_selector = Some("auto".to_string());
    }
    if parsed.max_connections == Some(0) {
        parsed.error = Some("setup --max-connections must be a positive integer".to_string());
        return parsed;
    }
    if parsed.io_timeout_ms == Some(0) {
        parsed.error = Some("setup --io-timeout-ms must be a positive integer".to_string());
        return parsed;
    }
    if parsed.max_body_bytes == Some(0) {
        parsed.error = Some("setup --max-body-bytes must be a positive integer".to_string());
        return parsed;
    }

    apply_setup_positionals(&mut parsed, cli.values);
    parsed
}

fn normalized_client_selector(value: Option<String>) -> (bool, Option<String>) {
    let Some(value) = value else {
        return (false, None);
    };
    let value = value.trim().to_ascii_lowercase();
    if matches!(value.as_str(), "none" | "skip" | "off") {
        (true, None)
    } else {
        (false, Some(value))
    }
}

fn apply_setup_positionals(parsed: &mut ParsedArgs, values: Vec<String>) {
    for value in values {
        if parsed
            .server_spec
            .as_deref()
            .map(looks_like_multiword_server_command)
            .unwrap_or(false)
        {
            let mut spec = parsed.server_spec.take().unwrap_or_default();
            if !spec.trim().is_empty() {
                spec.push(' ');
            }
            spec.push_str(&value);
            parsed.server_spec = Some(spec);
        } else if parsed.server_spec.is_none() {
            parsed.server_spec = Some(value);
        } else {
            parsed.server_paths.push(value);
        }
    }
}

fn argv(args: &[String]) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(OsString::from("mcpace up"));
    argv.extend(
        args.iter()
            .map(|arg| OsString::from(normalize_compat_arg(arg))),
    );
    argv
}

fn normalize_compat_arg(arg: &str) -> String {
    match arg {
        "-json" => "--json".to_string(),
        "-root" => "--root".to_string(),
        "-host" => "--host".to_string(),
        "-port" => "--port".to_string(),
        "-max-connections" => "--max-connections".to_string(),
        "-io-timeout-ms" => "--io-timeout-ms".to_string(),
        "-max-body-bytes" => "--max-body-bytes".to_string(),
        "-overview-cache-ms" => "--overview-cache-ms".to_string(),
        "-skip-client-install" => "--skip-client-install".to_string(),
        "-no-client" => "--no-client".to_string(),
        "-skip-client" => "--skip-client".to_string(),
        "-client" => "--client".to_string(),
        "-for" => "--for".to_string(),
        "-all-clients" => "--all-clients".to_string(),
        "-auto-client" => "--auto-client".to_string(),
        "-auto-clients" => "--auto-clients".to_string(),
        "-server" => "--server".to_string(),
        "-with-server" => "--with-server".to_string(),
        "-install-server" => "--install-server".to_string(),
        "-as" => "--as".to_string(),
        "-server-name" => "--server-name".to_string(),
        "-path" => "--path".to_string(),
        "-server-path" => "--server-path".to_string(),
        "-force" => "--force".to_string(),
        "-no-default-server" => "--no-default-server".to_string(),
        "-no-server" => "--no-server".to_string(),
        "-install-service" => "--install-service".to_string(),
        "-install-autostart" => "--install-autostart".to_string(),
        "-autostart" => "--autostart".to_string(),
        "-no-enable" => "--no-enable".to_string(),
        "-?" => "--help".to_string(),
        _ => arg.to_string(),
    }
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace up [server-spec] [--as <name>] [--path <path>...] [--client auto|all|<id>|none] [--json] [--root <path>] [--host <addr>] [--port <n>] [--autostart]"
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

pub(crate) fn bootstrap_root_layout(root_path: &Path) -> Result<(), String> {
    ensure_setup_root_layout(root_path).map(|_| ())
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

fn persist_setup_endpoint_overrides(
    root_path: &Path,
    host_override: Option<&str>,
    port_override: u16,
) -> Result<bool, String> {
    if host_override.is_none() && port_override == 0 {
        return Ok(false);
    }

    let config_path = root_path.join("mcpace.config.json");
    let _lock = runtimepaths::acquire_exclusive_file_lock(
        &config_path,
        "setup endpoint configuration update",
    )
    .map_err(|error| error.to_string())?;
    let config = json_helpers::read_json_file(&config_path)
        .map_err(|error| format!("failed to read {}: {}", config_path.display(), error))?;
    let JsonValue::Object(mut config) = config else {
        return Err(format!(
            "{} must contain a JSON object",
            config_path.display()
        ));
    };

    let serve = config
        .entry("serve".to_string())
        .or_insert_with(|| JsonValue::Object(BTreeMap::new()));
    let JsonValue::Object(serve) = serve else {
        return Err(format!(
            "{} field 'serve' must contain a JSON object",
            config_path.display()
        ));
    };
    if let Some(host) = host_override {
        serve.insert("host".to_string(), JsonValue::string(host.trim()));
    }
    if port_override != 0 {
        serve.insert("port".to_string(), JsonValue::number(port_override));
    }

    if port_override != 0 {
        let ports = config
            .entry("ports".to_string())
            .or_insert_with(|| JsonValue::Object(BTreeMap::new()));
        let JsonValue::Object(ports) = ports else {
            return Err(format!(
                "{} field 'ports' must contain a JSON object",
                config_path.display()
            ));
        };
        ports.insert("serve".to_string(), JsonValue::number(port_override));
    }

    runtimepaths::write_text_atomic(&config_path, &JsonValue::Object(config).to_pretty_string())
        .map_err(|error| format!("failed to write {}: {}", config_path.display(), error))?;
    Ok(true)
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
    let endpoint_override_requested = parsed.host.is_some() || parsed.port != 0;
    let endpoint_config_persisted = if endpoint_override_requested && serve.ok {
        match persist_setup_endpoint_overrides(&root_path, parsed.host.as_deref(), parsed.port) {
            Ok(_) => true,
            Err(error) => {
                warnings.push(format!(
                    "The running endpoint override could not be persisted; client installation was blocked to avoid writing a stale URL: {}",
                    error
                ));
                false
            }
        }
    } else if endpoint_override_requested {
        warnings.push(
            "The endpoint override was not persisted because the serve process did not start successfully."
                .to_string(),
        );
        false
    } else {
        true
    };

    let client_install = if parsed.skip_client_install {
        warnings.push(
            "Client install was skipped; run 'mcpace client install <client-id>' when ready."
                .to_string(),
        );
        None
    } else if !endpoint_config_persisted {
        Some(CommandResult {
            ok: false,
            exit_code: 1,
            json: Some(JsonValue::object([
                ("mode", JsonValue::string("blocked-endpoint-config")),
                ("ok", JsonValue::bool(false)),
                (
                    "error",
                    JsonValue::string(
                        "client install was blocked because the active endpoint could not be persisted",
                    ),
                ),
            ])),
            stdout: String::new(),
            stderr: "client install was blocked because the active endpoint could not be persisted"
                .to_string(),
        })
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
            "autostart".to_string(),
            "enable".to_string(),
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

    let probe_host = http_probe::probe_host(&host);
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
        && endpoint_config_persisted
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
                (
                    "endpointConfigPersisted",
                    JsonValue::bool(endpoint_config_persisted),
                ),
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
                    ("error", JsonValue::string(error.to_string())),
                ])),
                stdout: String::new(),
                stderr: error.to_string(),
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
                        ("error", JsonValue::string(error.to_string())),
                    ])),
                    stdout: String::new(),
                    stderr: error.to_string(),
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
        .any(|arg| {
            matches!(
                arg.as_str(),
                "mcp-server" | "stdio" | "stdio-shim" | "serve"
            )
        })
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
    let path = PathBuf::from(trimmed);
    if trimmed.starts_with("~/") || trimmed.starts_with("~\\") || path.is_absolute() {
        runtimepaths::resolve_user_config_path_expression(trimmed)
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
    http_probe::json_get(
        host,
        port,
        path,
        HTTP_PROBE_TIMEOUT,
        MAX_HTTP_SETUP_RESPONSE_BYTES,
    )
    .map_err(|error| error.to_string())
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
    let session_header = mcp_session_header(session_id)?;
    let request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMCP-Protocol-Version: {}\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        path,
        host,
        mcp::CURRENT_PROTOCOL_VERSION,
        session_header,
        body.len(),
        body
    );
    http_probe::json_response(
        host,
        port,
        &request,
        HTTP_PROBE_TIMEOUT,
        MAX_HTTP_SETUP_RESPONSE_BYTES,
    )
    .map_err(|error| error.to_string())
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
    let session_header = mcp_session_header(session_id)?;
    let request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMCP-Protocol-Version: {}\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        path,
        host,
        mcp::CURRENT_PROTOCOL_VERSION,
        session_header,
        body.len(),
        body
    );
    let response = http_probe::raw_response(
        host,
        port,
        &request,
        HTTP_PROBE_TIMEOUT,
        MAX_HTTP_SETUP_RESPONSE_BYTES,
    )
    .map_err(|error| error.to_string())?;
    let parsed = http_probe::parse_response(&response).map_err(|error| error.to_string())?;
    if matches!(parsed.status, 200 | 202 | 204) {
        Ok(parsed.status)
    } else {
        Err(format!("HTTP notification failed: {}", parsed.status_line))
    }
}

fn mcp_session_header(session_id: Option<&str>) -> Result<String, String> {
    let Some(value) = session_id else {
        return Ok(String::new());
    };
    if !text_utils::valid_http_header_value(value) {
        return Err("setup probe received an invalid MCP session id header".to_string());
    }
    Ok(format!("Mcp-Session-Id: {}\r\n", value))
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
mod tests;
