use crate::dashboard;
use crate::diagnostics;
use crate::http_probe;
use crate::json::{parse_str, JsonValue};
use crate::json_helpers;
use crate::process_identity::{self, ProcessMatch};
use crate::resources;
use crate::runtimepaths;
use clap::{error::ErrorKind, Parser};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const HEALTH_PROBE_IO_TIMEOUT: Duration = Duration::from_secs(5);
const HEALTH_PROBE_MAX_RESPONSE_BYTES: usize = 256 * 1024;

#[derive(Debug, Default)]
struct ParsedArgs {
    action: Option<String>,
    help: bool,
    json_output: bool,
    root_override: Option<PathBuf>,
    host: Option<String>,
    port: Option<u16>,
    max_connections: Option<usize>,
    io_timeout_ms: Option<u64>,
    max_body_bytes: Option<usize>,
    overview_cache_ms: Option<u64>,
    passthrough: Vec<String>,
    managed_service: bool,
    stop_token: Option<String>,
    error: Option<String>,
}

#[derive(Clone, Debug)]
struct ServeState {
    root_path: String,
    state_root: String,
    host: String,
    port: u16,
    max_connections: Option<usize>,
    io_timeout_ms: Option<u64>,
    max_body_bytes: Option<usize>,
    overview_cache_ms: Option<u64>,
    url: String,
    pid: u32,
    process_identity: Option<String>,
    stop_token: Option<String>,
    supervisor_managed: Option<bool>,
    started_at_ms: u128,
    runner_path: String,
    stdout_log_path: String,
    stderr_log_path: String,
}

pub fn run(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let parsed = parse_cli(args);
    if let Some(error) = parsed.error {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 2;
    }
    if parsed.help {
        write_help(stdout);
        return 0;
    }
    if parsed.action.is_none() && !parsed.managed_service {
        if let Err(error) = start_direct_stop_watcher(&parsed) {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    }

    match parsed.action.as_deref() {
        Some("start") => run_start(parsed, default_root, stdout, stderr),
        Some("stop") => run_stop(parsed, default_root, stdout, stderr),
        Some("status") => run_status(parsed, default_root, stdout, stderr),
        Some("restart") => run_restart(parsed, default_root, stdout, stderr),
        _ if parsed.managed_service => run_managed_foreground(parsed, default_root, stdout, stderr),
        _ => dashboard::run_serve(&parsed.passthrough, default_root, stdout, stderr),
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "mcpace serve",
    disable_version_flag = true,
    about = "Run the public MCPace local HTTP surface"
)]
struct ServeCli {
    #[arg(value_name = "start|restart|stop|status")]
    action: Option<String>,

    #[arg(
        value_name = "EXTRA",
        trailing_var_arg = true,
        allow_hyphen_values = true
    )]
    extra: Vec<String>,

    #[arg(long = "managed-service", hide = true)]
    managed_service: bool,

    #[arg(long = "stop-token", value_name = "TOKEN", hide = true)]
    stop_token: Option<String>,

    #[arg(long = "json")]
    json_output: bool,

    #[arg(long = "root", value_name = "PATH")]
    root_override: Option<PathBuf>,

    #[arg(long = "host", value_name = "ADDR")]
    host: Option<String>,

    #[arg(long = "port", value_name = "N")]
    port: Option<String>,

    #[arg(long = "max-requests", value_name = "N")]
    max_requests: Option<String>,

    #[arg(long = "max-connections", value_name = "N")]
    max_connections: Option<String>,

    #[arg(long = "io-timeout-ms", value_name = "MS")]
    io_timeout_ms: Option<String>,

    #[arg(long = "max-body-bytes", value_name = "N")]
    max_body_bytes: Option<String>,

    #[arg(long = "overview-cache-ms", value_name = "MS")]
    overview_cache_ms: Option<String>,

    #[arg(long = "allow-nonlocal-bind", hide = true)]
    allow_nonlocal_bind: bool,

    #[arg(long = "insecure-nonlocal-bind", hide = true)]
    insecure_nonlocal_bind: bool,

    #[arg(long = "auth-token-env", value_name = "NAME")]
    auth_token_env: Option<String>,
}

fn parse_cli(args: &[String]) -> ParsedArgs {
    match ServeCli::try_parse_from(argv(args)) {
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

fn parsed_from_cli(cli: ServeCli) -> ParsedArgs {
    let mut passthrough = serve_passthrough_args(&cli);
    let raw_action = cli
        .action
        .as_ref()
        .map(|value| value.trim().to_ascii_lowercase());
    let action = match raw_action.as_deref() {
        Some("start" | "stop" | "status" | "restart") => raw_action.clone(),
        Some(other) if !other.is_empty() => {
            passthrough.insert(0, cli.action.clone().unwrap_or_default());
            None
        }
        _ => None,
    };

    let mut parsed = ParsedArgs {
        action,
        help: false,
        json_output: cli.json_output,
        root_override: cli.root_override,
        host: cli.host,
        port: None,
        max_connections: None,
        io_timeout_ms: None,
        max_body_bytes: None,
        overview_cache_ms: None,
        passthrough,
        managed_service: cli.managed_service,
        stop_token: cli.stop_token,
        error: None,
    };

    if parsed
        .stop_token
        .as_deref()
        .is_some_and(|token| !valid_stop_token(token))
    {
        parsed.error = Some("serve --stop-token is invalid".to_string());
        return parsed;
    }

    if cli.allow_nonlocal_bind || cli.insecure_nonlocal_bind {
        parsed.error = Some(
            "direct non-loopback HTTP flags are no longer supported; use a trusted HTTPS reverse proxy or tunnel"
                .to_string(),
        );
        return parsed;
    }

    if parsed.action.is_some()
        && cli
            .extra
            .iter()
            .any(|value| matches!(value.as_str(), "start" | "stop" | "status" | "restart"))
    {
        parsed.error = Some("serve accepts only one action".to_string());
        return parsed;
    }

    if let Some(value) = cli.port.as_deref() {
        match value.parse::<u16>() {
            Ok(port) => parsed.port = Some(port),
            Err(_) => {
                parsed.error = Some("serve --port must be a valid u16".to_string());
                return parsed;
            }
        }
    }
    if let Some(value) = cli.max_connections.as_deref() {
        match resources::parse_http_connection_limit(value, "serve --max-connections") {
            Ok(limit) => parsed.max_connections = Some(limit),
            Err(error) => {
                parsed.error = Some(error.to_string());
                return parsed;
            }
        }
    }
    if let Some(value) = cli.io_timeout_ms.as_deref() {
        match resources::parse_http_io_timeout_ms(value, "serve --io-timeout-ms") {
            Ok(timeout_ms) => parsed.io_timeout_ms = Some(timeout_ms),
            Err(error) => {
                parsed.error = Some(error.to_string());
                return parsed;
            }
        }
    }
    if let Some(value) = cli.max_body_bytes.as_deref() {
        match resources::parse_http_body_limit(value, "serve --max-body-bytes") {
            Ok(limit) => parsed.max_body_bytes = Some(limit),
            Err(error) => {
                parsed.error = Some(error.to_string());
                return parsed;
            }
        }
    }
    if let Some(value) = cli.overview_cache_ms.as_deref() {
        match resources::parse_nonnegative_u64(value, "serve --overview-cache-ms") {
            Ok(ttl_ms) => parsed.overview_cache_ms = Some(ttl_ms),
            Err(error) => {
                parsed.error = Some(error.to_string());
                return parsed;
            }
        }
    }

    parsed
}

fn serve_passthrough_args(cli: &ServeCli) -> Vec<String> {
    let mut passthrough = Vec::new();
    if cli.json_output {
        passthrough.push("--json".to_string());
    }
    push_path_arg(&mut passthrough, "--root", cli.root_override.as_ref());
    push_string_arg(&mut passthrough, "--host", cli.host.as_deref());
    push_string_arg(&mut passthrough, "--port", cli.port.as_deref());
    push_string_arg(
        &mut passthrough,
        "--max-requests",
        cli.max_requests.as_deref(),
    );
    push_string_arg(
        &mut passthrough,
        "--max-connections",
        cli.max_connections.as_deref(),
    );
    push_string_arg(
        &mut passthrough,
        "--io-timeout-ms",
        cli.io_timeout_ms.as_deref(),
    );
    push_string_arg(
        &mut passthrough,
        "--max-body-bytes",
        cli.max_body_bytes.as_deref(),
    );
    push_string_arg(
        &mut passthrough,
        "--overview-cache-ms",
        cli.overview_cache_ms.as_deref(),
    );
    if cli.allow_nonlocal_bind {
        passthrough.push("--allow-nonlocal-bind".to_string());
    }
    if cli.insecure_nonlocal_bind {
        passthrough.push("--insecure-nonlocal-bind".to_string());
    }
    push_string_arg(
        &mut passthrough,
        "--auth-token-env",
        cli.auth_token_env.as_deref(),
    );
    passthrough.extend(cli.extra.iter().cloned());
    passthrough
}

fn push_string_arg(args: &mut Vec<String>, flag: &str, value: Option<&str>) {
    if let Some(value) = value {
        args.push(flag.to_string());
        args.push(value.to_string());
    }
}

fn push_path_arg(args: &mut Vec<String>, flag: &str, value: Option<&PathBuf>) {
    if let Some(value) = value {
        args.push(flag.to_string());
        args.push(value.display().to_string());
    }
}

fn argv(args: &[String]) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(OsString::from("mcpace serve"));
    argv.extend(args.iter().map(OsString::from));
    argv
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "`mcpace serve` is a hidden compatibility entrypoint."
    );
    let _ = writeln!(stdout, "Use the public lifecycle commands instead:");
    let _ = writeln!(stdout, "  mcpace start [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace stop [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace restart [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace status [--json] [--root <path>]");
    let _ = writeln!(
        stdout,
        "  mcpace advanced runtime foreground [--root <path>] [serve options]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "Default endpoint: {}. Internal service-only flags are intentionally omitted.",
        runtimepaths::default_local_mcp_url()
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

fn run_managed_foreground(
    parsed: ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        diagnostics::stderr_line(
            stderr,
            format_args!("mcpace root not found; expected mcpace.config.json"),
        );
        return 1;
    };

    let canonical_root = runtimepaths::canonicalize_or_original(&root_path);
    let endpoint = runtimepaths::resolve_serve_endpoint(Some(&canonical_root));
    let host = parsed.host.clone().unwrap_or_else(|| endpoint.host.clone());
    let port = parsed.port.unwrap_or(endpoint.port);
    let state_root = runtimepaths::resolve_state_root(&canonical_root);
    if let Err(error) = runtimepaths::ensure_runtime_dir(&state_root) {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }
    if let Err(error) = runtimepaths::ensure_serve_dir(&state_root) {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }
    let start_lock = match acquire_serve_start_lock(&state_root) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };

    let state_path = runtimepaths::serve_state_path(&state_root);
    let restart_guard_path = runtimepaths::serve_restart_guard_path(&state_root);
    if let Ok(existing_state) = read_state(&state_path) {
        let existing_healthy = health_check(
            &existing_state.host,
            existing_state.port,
            &endpoint.health_path,
        )
        .unwrap_or(false);
        if existing_healthy {
            let settings_match = state_matches_start_request(
                &existing_state,
                &host,
                port,
                parsed.max_connections,
                parsed.io_timeout_ms,
                parsed.max_body_bytes,
                parsed.overview_cache_ms,
            );
            let detail = if settings_match {
                "already healthy"
            } else {
                "already healthy with different settings; refusing to start a duplicate runtime"
            };
            let _ = writeln!(
                stdout,
                "MCPace managed service is {} at {}",
                detail, existing_state.url
            );
            return 0;
        }
        if !existing_healthy {
            remove_managed_serve_runner_copy(&state_root, &existing_state);
            let _ = fs::remove_file(&state_path);
            crate::restart_guard::clear(&restart_guard_path);
        }
    }

    if let Err(error) = crate::restart_guard::check_and_record(&restart_guard_path, "serve-managed")
    {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }

    let runner_path = resolve_runner_source()
        .unwrap_or_else(|_| std::env::current_exe().unwrap_or_else(|_| PathBuf::from("mcpace")));
    let process_identity = match process_identity_token(std::process::id()) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    let stop_token = match random_stop_token() {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    let state = ServeState {
        root_path: sanitize_display(&canonical_root),
        state_root: sanitize_display(&state_root),
        host: host.clone(),
        port,
        max_connections: parsed.max_connections,
        io_timeout_ms: parsed.io_timeout_ms,
        max_body_bytes: parsed.max_body_bytes,
        overview_cache_ms: parsed.overview_cache_ms,
        url: runtimepaths::http_url(&host, port, &endpoint.mcp_path),
        pid: std::process::id(),
        process_identity: Some(process_identity),
        stop_token: Some(stop_token.clone()),
        supervisor_managed: Some(true),
        started_at_ms: now_ms(),
        runner_path: sanitize_display(&runner_path),
        stdout_log_path: "service-manager-stdout".to_string(),
        stderr_log_path: "service-manager-stderr".to_string(),
    };
    if let Err(error) = write_state(&state_path, &state) {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }
    let _ = fs::remove_file(serve_stop_request_path(&state_root));
    if let Err(error) = start_stop_watcher(state_root.clone(), stop_token) {
        let _ = fs::remove_file(&state_path);
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }
    drop(start_lock);

    let exit_code = dashboard::run_serve(&parsed.passthrough, Some(canonical_root), stdout, stderr);
    remove_state_if_current_pid(&state_path, std::process::id());
    exit_code
}

fn run_start(
    parsed: ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    run_start_impl(parsed, default_root, stdout, stderr, true)
}

fn run_start_after_supervisor_stop(
    parsed: ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    run_start_impl(parsed, default_root, stdout, stderr, false)
}

fn run_start_impl(
    parsed: ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    clear_stop_request: bool,
) -> i32 {
    let json_output = parsed.json_output;
    let mut resource_args = Vec::new();
    resources::append_serve_resource_args(
        &mut resource_args,
        parsed.max_connections,
        parsed.io_timeout_ms,
        parsed.max_body_bytes,
        parsed.overview_cache_ms,
    );
    let max_connections = parsed.max_connections;
    let io_timeout_ms = parsed.io_timeout_ms;
    let max_body_bytes = parsed.max_body_bytes;
    let overview_cache_ms = parsed.overview_cache_ms;
    let root_path = parsed.root_override.or(default_root);
    let Some(root_path) = root_path else {
        diagnostics::stderr_line(
            stderr,
            format_args!("mcpace root not found; expected mcpace.config.json"),
        );
        return 1;
    };

    let canonical_root = runtimepaths::canonicalize_or_original(&root_path);
    if clear_stop_request {
        clear_agent_supervisor_stop_request(&canonical_root);
    }
    let endpoint = runtimepaths::resolve_serve_endpoint(Some(&canonical_root));
    let host = parsed.host.unwrap_or_else(|| endpoint.host.clone());
    let port = parsed.port.unwrap_or(endpoint.port);
    let state_root = runtimepaths::resolve_state_root(&canonical_root);
    if let Err(error) = runtimepaths::ensure_runtime_dir(&state_root) {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }
    if let Err(error) = runtimepaths::ensure_serve_dir(&state_root) {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }
    if let Err(error) = runtimepaths::ensure_runtime_bin_dir(&state_root) {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }
    let stop_token = match random_stop_token() {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    resource_args.push("--stop-token".to_string());
    resource_args.push(stop_token.clone());
    let _start_lock = match acquire_serve_start_lock(&state_root) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    let _ = fs::remove_file(serve_stop_request_path(&state_root));

    let state_path = runtimepaths::serve_state_path(&state_root);
    let restart_guard_path = runtimepaths::serve_restart_guard_path(&state_root);
    if let Ok(existing_state) = read_state(&state_path) {
        let existing_healthy = health_check(
            &existing_state.host,
            existing_state.port,
            &endpoint.health_path,
        )
        .unwrap_or(false);
        if existing_healthy {
            if !state_matches_start_request(
                &existing_state,
                &host,
                port,
                max_connections,
                io_timeout_ms,
                max_body_bytes,
                overview_cache_ms,
            ) {
                if let Err(error) = stop_existing_serve_locked(&canonical_root) {
                    diagnostics::stderr_line(stderr, format_args!("{}", error));
                    return 1;
                }
            }
        } else {
            remove_managed_serve_runner_copy(&state_root, &existing_state);
            let _ = fs::remove_file(&state_path);
            crate::restart_guard::clear(&restart_guard_path);
        }
    }

    if let Ok(status) = collect_status(&canonical_root, Some(host.clone()), Some(port)) {
        if status.status == "running" {
            return write_status_response(&status, json_output, stdout);
        }
    }

    cleanup_stale_serve_runner_copies(&state_root, None);
    if let Err(error) = crate::restart_guard::check_and_record(&restart_guard_path, "serve") {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }

    let current_exe = match resolve_runner_source() {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(
                stderr,
                format_args!("failed to resolve mcpace binary path: {}", error),
            );
            return 1;
        }
    };
    let runner_path = runtimepaths::serve_runner_path_for_start(&state_root);
    if let Err(error) = fs::copy(&current_exe, &runner_path) {
        diagnostics::stderr_line(
            stderr,
            format_args!(
                "failed to copy mcpace serve runner to '{}': {}",
                runner_path.display(),
                error
            ),
        );
        return 1;
    }

    let stdout_log_path = runtimepaths::serve_stdout_log_path(&state_root);
    let stderr_log_path = runtimepaths::serve_stderr_log_path(&state_root);
    let stdout_file = match File::create(&stdout_log_path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(
                stderr,
                format_args!(
                    "failed to open serve stdout log '{}': {}",
                    stdout_log_path.display(),
                    error
                ),
            );
            return 1;
        }
    };
    let stderr_file = match File::create(&stderr_log_path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(
                stderr,
                format_args!(
                    "failed to open serve stderr log '{}': {}",
                    stderr_log_path.display(),
                    error
                ),
            );
            return 1;
        }
    };

    let mut background_process = match spawn_background(
        &runner_path,
        &canonical_root,
        &host,
        port,
        &resource_args,
        stdout_file,
        stderr_file,
    ) {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_file(&runner_path);
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };

    let process_identity = match process_identity_token(background_process.pid()) {
        Ok(value) => value,
        Err(error) => {
            let _ = request_unverified_cooperative_stop(
                &state_root,
                background_process.pid(),
                &stop_token,
            );
            let _ = fs::remove_file(&runner_path);
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    let state = ServeState {
        root_path: sanitize_display(&canonical_root),
        state_root: sanitize_display(&state_root),
        host: host.clone(),
        port,
        max_connections,
        io_timeout_ms,
        max_body_bytes,
        overview_cache_ms,
        url: runtimepaths::http_url(&host, port, &endpoint.mcp_path),
        pid: background_process.pid(),
        process_identity: Some(process_identity),
        stop_token: Some(stop_token),
        supervisor_managed: Some(false),
        started_at_ms: now_ms(),
        runner_path: sanitize_display(&runner_path),
        stdout_log_path: sanitize_display(&stdout_log_path),
        stderr_log_path: sanitize_display(&stderr_log_path),
    };
    if let Err(error) = write_state(&state_path, &state) {
        if let Some(token) = state.stop_token.as_deref() {
            let _ = request_cooperative_serve_stop(&state_root, &state, token);
        }
        let _ = fs::remove_file(&runner_path);
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }

    if let Err(error) = wait_for_health(
        &host,
        port,
        &endpoint.health_path,
        60,
        Duration::from_millis(100),
    ) {
        let child_status = background_process.status_detail();
        if let Some(token) = state.stop_token.as_deref() {
            let _ = request_cooperative_serve_stop(&state_root, &state, token);
        }
        let _ = fs::remove_file(&state_path);
        remove_managed_serve_runner_copy(&state_root, &state);
        diagnostics::stderr_line(
            stderr,
            format_args!("{}; child process {}", error, child_status),
        );
        return 1;
    }

    let status = match collect_status(&canonical_root, Some(host), Some(port)) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    write_status_response(&status, json_output, stdout)
}

fn run_stop(
    parsed: ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let json_output = parsed.json_output;
    let root_path = parsed.root_override.or(default_root);
    let Some(root_path) = root_path else {
        diagnostics::stderr_line(
            stderr,
            format_args!("mcpace root not found; expected mcpace.config.json"),
        );
        return 1;
    };
    let canonical_root = runtimepaths::canonicalize_or_original(&root_path);
    if let Err(error) = request_agent_supervisor_stop(&canonical_root) {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }
    if let Err(error) = stop_existing_serve(&canonical_root) {
        let _ = wait_for_agent_supervisor_stop(&canonical_root);
        clear_agent_supervisor_stop_request(&canonical_root);
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }
    if let Err(error) = wait_for_agent_supervisor_stop(&canonical_root) {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }
    clear_agent_supervisor_stop_request(&canonical_root);

    let status = match collect_status(&canonical_root, None, None) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    write_status_response(&status, json_output, stdout)
}

fn run_restart(
    parsed: ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.clone().or(default_root.clone());
    let Some(root_path) = root_path else {
        diagnostics::stderr_line(
            stderr,
            format_args!("mcpace root not found; expected mcpace.config.json"),
        );
        return 1;
    };
    let canonical_root = runtimepaths::canonicalize_or_original(&root_path);
    let endpoint = runtimepaths::resolve_serve_endpoint(Some(&canonical_root));
    let requested_host = parsed.host.as_deref().unwrap_or(&endpoint.host);
    let requested_port = parsed.port.unwrap_or(endpoint.port);
    let state_path =
        runtimepaths::serve_state_path(&runtimepaths::resolve_state_root(&canonical_root));
    let restart_with_supervisor = agent_supervisor_is_active(&canonical_root)
        && read_state(&state_path).ok().is_some_and(|state| {
            state_matches_start_request(
                &state,
                requested_host,
                requested_port,
                parsed.max_connections,
                parsed.io_timeout_ms,
                parsed.max_body_bytes,
                parsed.overview_cache_ms,
            )
        });
    if let Err(error) = request_agent_supervisor_stop(&canonical_root) {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }
    if let Err(error) = stop_existing_serve(&canonical_root) {
        let _ = wait_for_agent_supervisor_stop(&canonical_root);
        clear_agent_supervisor_stop_request(&canonical_root);
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }
    if let Err(error) = wait_for_agent_supervisor_stop(&canonical_root) {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 1;
    }
    if restart_with_supervisor {
        clear_agent_supervisor_stop_request(&canonical_root);
        if let Err(error) = start_agent_supervisor(&canonical_root) {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
        if let Err(error) = wait_for_health(
            requested_host,
            requested_port,
            &endpoint.health_path,
            100,
            Duration::from_millis(100),
        ) {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
        let status = match collect_status(
            &canonical_root,
            Some(requested_host.to_string()),
            Some(requested_port),
        ) {
            Ok(value) => value,
            Err(error) => {
                diagnostics::stderr_line(stderr, format_args!("{}", error));
                return 1;
            }
        };
        return write_status_response(&status, parsed.json_output, stdout);
    }
    let exit_code = run_start_after_supervisor_stop(parsed, default_root, stdout, stderr);
    clear_agent_supervisor_stop_request(&canonical_root);
    exit_code
}

fn state_matches_start_request(
    state: &ServeState,
    host: &str,
    port: u16,
    max_connections: Option<usize>,
    io_timeout_ms: Option<u64>,
    max_body_bytes: Option<usize>,
    overview_cache_ms: Option<u64>,
) -> bool {
    state.host == host
        && state.port == port
        && state.max_connections == max_connections
        && state.io_timeout_ms == io_timeout_ms
        && state.max_body_bytes == max_body_bytes
        && state.overview_cache_ms == overview_cache_ms
}

#[cfg(windows)]
fn agent_supervisor_is_active(root: &Path) -> bool {
    fs::read_to_string(agent_supervisor_pid_path(root))
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .is_some_and(|pid| {
            crate::windows_process::process_image_is(pid, "mcpace-agent-launcher.exe")
        })
}

#[cfg(target_os = "linux")]
fn agent_supervisor_is_active(_root: &Path) -> bool {
    std::process::Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", "mcpace-agent.service"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn agent_supervisor_is_active(_root: &Path) -> bool {
    crate::macos_launch_agent::is_loaded(crate::service::APP_NAME).unwrap_or(false)
}

#[cfg(all(not(windows), not(target_os = "linux"), not(target_os = "macos")))]
fn agent_supervisor_is_active(_root: &Path) -> bool {
    false
}

#[cfg(windows)]
fn start_agent_supervisor(root: &Path) -> Result<(), String> {
    let current_exe = std::env::current_exe()
        .map_err(|error| format!("failed to resolve mcpace executable: {}", error))?;
    let launcher = current_exe.with_file_name("mcpace-agent-launcher.exe");
    if !launcher.is_file() {
        return Err(format!(
            "Windows MCPace Agent launcher is missing: {}",
            launcher.display()
        ));
    }
    crate::windows_process::spawn_detached_no_window(
        &launcher,
        &[OsString::from("--from-login")],
        Some(root),
    )
    .map(|_| ())
    .map_err(|error| format!("failed to restart MCPace Agent supervisor: {}", error))
}

#[cfg(target_os = "linux")]
fn start_agent_supervisor(_root: &Path) -> Result<(), String> {
    let output = std::process::Command::new("systemctl")
        .args(["--user", "start", "mcpace-agent.service"])
        .output()
        .map_err(|error| format!("failed to restart systemd user service: {}", error))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "failed to restart systemd user service: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

#[cfg(target_os = "macos")]
fn start_agent_supervisor(_root: &Path) -> Result<(), String> {
    crate::macos_launch_agent::start(crate::service::APP_NAME).map_err(|error| error.to_string())
}

#[cfg(all(not(windows), not(target_os = "linux"), not(target_os = "macos")))]
fn start_agent_supervisor(_root: &Path) -> Result<(), String> {
    Err("user supervisor restart is not supported on this platform".to_string())
}

#[cfg(windows)]
fn agent_supervisor_runtime_path(root: &Path, name: &str) -> PathBuf {
    root.join("data").join("runtime").join("agent").join(name)
}

#[cfg(windows)]
fn agent_supervisor_stop_request_path(root: &Path) -> PathBuf {
    agent_supervisor_runtime_path(root, "stop-requested")
}

#[cfg(windows)]
fn agent_supervisor_pid_path(root: &Path) -> PathBuf {
    agent_supervisor_runtime_path(root, "supervisor.pid")
}

#[cfg(windows)]
fn request_agent_supervisor_stop(root: &Path) -> Result<(), String> {
    if !recorded_runtime_is_explicitly_managed(root) && !agent_supervisor_is_active(root) {
        return Ok(());
    }
    let path = agent_supervisor_stop_request_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create MCPace Agent supervisor control directory '{}': {}",
                parent.display(),
                error
            )
        })?;
    }
    fs::write(&path, format!("{}\n", std::process::id())).map_err(|error| {
        format!(
            "failed to request MCPace Agent supervisor stop at '{}': {}",
            path.display(),
            error
        )
    })
}

#[cfg(target_os = "linux")]
fn request_agent_supervisor_stop(root: &Path) -> Result<(), String> {
    if !recorded_runtime_is_explicitly_managed(root) {
        return Ok(());
    }
    let explicitly_direct = false;
    let output = match std::process::Command::new("systemctl")
        .env("LC_ALL", "C")
        .args(["--user", "stop", "mcpace-agent.service"])
        .output()
    {
        Ok(output) => output,
        Err(error)
            if systemd_command_launch_failure_is_ignorable(error.kind(), explicitly_direct) =>
        {
            return Ok(())
        }
        Err(error) => return Err(format!("failed to stop systemd user service: {}", error)),
    };
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if systemd_stop_failure_is_ignorable(&detail, explicitly_direct) {
        return Ok(());
    }
    Err(format!("failed to stop systemd user service: {}", detail))
}

#[cfg(any(test, target_os = "linux"))]
fn systemd_service_absent(detail: &str) -> bool {
    let detail = detail.to_ascii_lowercase();
    detail.contains("not loaded")
        || detail.contains("not found")
        || detail.contains("could not be found")
        || detail.contains("does not exist")
}

#[cfg(any(test, target_os = "linux"))]
fn systemd_user_manager_unavailable(detail: &str) -> bool {
    let detail = detail.to_ascii_lowercase();
    if detail.contains("access denied") || detail.contains("permission denied") {
        return false;
    }
    detail.contains("failed to connect to bus: no medium found")
        || detail.contains("failed to connect to bus: no such file or directory")
        || detail.contains("failed to connect to bus: host is down")
        || detail.contains("system has not been booted with systemd as init system")
}

#[cfg(any(test, target_os = "linux"))]
fn systemd_command_launch_failure_is_ignorable(
    kind: std::io::ErrorKind,
    explicitly_direct: bool,
) -> bool {
    explicitly_direct && kind == std::io::ErrorKind::NotFound
}

#[cfg(any(test, target_os = "linux"))]
fn systemd_stop_failure_is_ignorable(detail: &str, explicitly_direct: bool) -> bool {
    let normalized = detail.to_ascii_lowercase();
    if normalized.contains("access denied") || normalized.contains("permission denied") {
        return false;
    }
    systemd_service_absent(detail)
        || (explicitly_direct && systemd_user_manager_unavailable(detail))
}

#[cfg(any(test, windows, target_os = "linux", target_os = "macos"))]
fn recorded_runtime_supervision(root: &Path) -> Option<bool> {
    let state_root = runtimepaths::resolve_state_root(root);
    read_state(&runtimepaths::serve_state_path(&state_root))
        .ok()
        .and_then(|state| state.supervisor_managed)
}

#[cfg(any(test, target_os = "linux", target_os = "macos"))]
fn recorded_runtime_is_explicitly_direct(root: &Path) -> bool {
    recorded_runtime_supervision(root) == Some(false)
}

#[cfg(any(test, windows, target_os = "linux", target_os = "macos"))]
fn recorded_runtime_is_explicitly_managed(root: &Path) -> bool {
    recorded_runtime_supervision(root) == Some(true)
}

#[cfg(target_os = "macos")]
fn request_agent_supervisor_stop(root: &Path) -> Result<(), String> {
    if !recorded_runtime_is_explicitly_managed(root) {
        return Ok(());
    }
    crate::macos_launch_agent::stop(crate::service::APP_NAME).map_err(|error| error.to_string())
}

#[cfg(all(not(windows), not(target_os = "linux"), not(target_os = "macos")))]
fn request_agent_supervisor_stop(_root: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn wait_for_agent_supervisor_stop(root: &Path) -> Result<(), String> {
    const ACK_TIMEOUT: Duration = Duration::from_secs(5);
    const POLL_INTERVAL: Duration = Duration::from_millis(25);

    let marker = agent_supervisor_stop_request_path(root);
    let pid_path = agent_supervisor_pid_path(root);
    let supervisor_pid = fs::read_to_string(&pid_path)
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok());
    let Some(supervisor_pid) = supervisor_pid else {
        let _ = fs::remove_file(&pid_path);
        return Ok(());
    };
    if !crate::windows_process::process_image_is(supervisor_pid, "mcpace-agent-launcher.exe") {
        let _ = fs::remove_file(&pid_path);
        return Ok(());
    }

    let started = std::time::Instant::now();
    while started.elapsed() < ACK_TIMEOUT {
        if !marker.is_file() {
            return Ok(());
        }
        thread::sleep(POLL_INTERVAL);
    }
    if !crate::windows_process::process_image_is(supervisor_pid, "mcpace-agent-launcher.exe") {
        let _ = fs::remove_file(&marker);
        let _ = fs::remove_file(&pid_path);
        return Ok(());
    }
    Err(format!(
        "timed out waiting for MCPace Agent supervisor pid {} to acknowledge stop; refusing to start a duplicate runtime",
        supervisor_pid
    ))
}

#[cfg(target_os = "macos")]
fn wait_for_agent_supervisor_stop(_root: &Path) -> Result<(), String> {
    if crate::macos_launch_agent::is_loaded(crate::service::APP_NAME)
        .map_err(|error| error.to_string())?
    {
        Err("macOS LaunchAgent remained loaded after the stop request".to_string())
    } else {
        Ok(())
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn wait_for_agent_supervisor_stop(_root: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn clear_agent_supervisor_stop_request(root: &Path) {
    let _ = fs::remove_file(agent_supervisor_stop_request_path(root));
}

#[cfg(not(windows))]
fn clear_agent_supervisor_stop_request(_root: &Path) {}

pub(crate) fn managed_runtime_is_live(canonical_root: &Path) -> Result<bool, String> {
    let state_root = runtimepaths::resolve_state_root(canonical_root);
    let state_path = runtimepaths::serve_state_path(&state_root);
    if !state_path.exists() {
        return Ok(false);
    }
    let state = read_state(&state_path)?;
    match process_identity::match_process(
        state.pid,
        state.process_identity.as_deref(),
        Some(Path::new(&state.runner_path)),
    )
    .map_err(|error| {
        format!(
            "failed to verify managed serve pid {} before cleanup: {}",
            state.pid, error
        )
    })? {
        ProcessMatch::Match => Ok(true),
        ProcessMatch::NotFound | ProcessMatch::Mismatch => Ok(false),
    }
}

fn stop_existing_serve(canonical_root: &Path) -> Result<(), String> {
    let state_root = runtimepaths::resolve_state_root(canonical_root);
    let _coordination = acquire_lifecycle_coordination(&state_root)?;
    stop_existing_serve_locked(canonical_root)
}

fn stop_existing_serve_locked(canonical_root: &Path) -> Result<(), String> {
    let state_root = runtimepaths::resolve_state_root(canonical_root);
    let state_path = runtimepaths::serve_state_path(&state_root);
    let existing = if state_path.exists() {
        Some(read_state(&state_path)?)
    } else {
        None
    };
    if let Some(state) = &existing {
        let expected_runner = Path::new(&state.runner_path);
        match process_identity::match_process(
            state.pid,
            state.process_identity.as_deref(),
            Some(expected_runner),
        )
        .map_err(|error| {
            format!(
                "failed to verify serve process identity for pid {}: {}",
                state.pid, error
            )
        })? {
            ProcessMatch::Match => {
                let token = state.stop_token.as_deref().ok_or_else(|| {
                    format!(
                        "serve state for pid {} predates cooperative stop tokens; refusing an unsafe raw-PID signal. Verify and stop that older process through the OS supervisor or process manager, then run 'mcpace advanced runtime cleanup runtime' before retrying",
                        state.pid
                    )
                })?;
                request_cooperative_serve_stop(&state_root, state, token)?;
                remove_managed_serve_runner_copy(&state_root, state);
            }
            ProcessMatch::NotFound => {
                remove_managed_serve_runner_copy(&state_root, state);
            }
            ProcessMatch::Mismatch => {
                return Err(format!(
                    "serve state pid {} no longer identifies the recorded MCPace runner '{}'; refusing to signal a reused or unrelated process",
                    state.pid, state.runner_path
                ));
            }
        }
    }
    cleanup_stale_serve_runner_copies(&state_root, existing.as_ref());
    crate::restart_guard::clear(&runtimepaths::serve_restart_guard_path(&state_root));
    let _ = fs::remove_file(&state_path);
    Ok(())
}

fn request_unverified_cooperative_stop(
    state_root: &Path,
    pid: u32,
    token: &str,
) -> Result<(), String> {
    let marker = serve_stop_request_path(state_root);
    runtimepaths::write_private_text_atomic(&marker, &format!("{}\n", token))
        .map_err(String::from)?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        match process_identity::capture(pid) {
            Ok(None) => {
                let _ = fs::remove_file(&marker);
                return Ok(());
            }
            Ok(Some(_)) => thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                let _ = fs::remove_file(&marker);
                return Err(format!(
                    "failed to observe serve pid {} during cooperative stop: {}",
                    pid, error
                ));
            }
        }
    }
    let _ = fs::remove_file(&marker);
    Err(format!(
        "serve pid {} did not acknowledge an unverified cooperative stop request within 5 seconds",
        pid
    ))
}

fn request_cooperative_serve_stop(
    state_root: &Path,
    state: &ServeState,
    token: &str,
) -> Result<(), String> {
    let marker = serve_stop_request_path(state_root);
    runtimepaths::write_private_text_atomic(&marker, &format!("{}\n", token))
        .map_err(String::from)?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        match process_identity::match_process(
            state.pid,
            state.process_identity.as_deref(),
            Some(Path::new(&state.runner_path)),
        )
        .map_err(|error| {
            format!(
                "failed to verify serve pid {} while waiting for cooperative stop: {}",
                state.pid, error
            )
        })? {
            ProcessMatch::Match => thread::sleep(Duration::from_millis(25)),
            ProcessMatch::NotFound | ProcessMatch::Mismatch => {
                let _ = fs::remove_file(&marker);
                return Ok(());
            }
        }
    }
    let _ = fs::remove_file(&marker);
    Err(format!(
        "serve pid {} did not acknowledge the cooperative stop request within 5 seconds; refusing an unsafe raw-PID signal",
        state.pid
    ))
}

fn remove_managed_serve_runner_copy(state_root: &Path, state: &ServeState) {
    if state.runner_path.trim().is_empty() {
        return;
    }
    let runner_path = PathBuf::from(&state.runner_path);
    let runtime_bin_dir = runtimepaths::runtime_bin_dir(state_root);
    let canonical_runner = runtimepaths::canonicalize_or_original(&runner_path);
    let canonical_runtime_bin = runtimepaths::canonicalize_or_original(&runtime_bin_dir);
    if canonical_runner.starts_with(&canonical_runtime_bin) {
        let _ = fs::remove_file(runner_path);
    }
}

fn cleanup_stale_serve_runner_copies(state_root: &Path, active_state: Option<&ServeState>) {
    let runtime_bin_dir = runtimepaths::runtime_bin_dir(state_root);
    let Ok(entries) = fs::read_dir(&runtime_bin_dir) else {
        return;
    };
    let active_runner = active_state
        .map(|state| PathBuf::from(&state.runner_path))
        .map(|path| runtimepaths::canonicalize_or_original(&path));
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !is_managed_serve_runner_name(file_name) {
            continue;
        }
        let canonical = runtimepaths::canonicalize_or_original(&path);
        if active_runner
            .as_ref()
            .is_some_and(|active| active == &canonical)
        {
            continue;
        }
        let _ = fs::remove_file(path);
    }
}

fn is_managed_serve_runner_name(file_name: &str) -> bool {
    let stem = file_name.strip_suffix(".exe").unwrap_or(file_name);
    stem.starts_with("mcpace-serve-")
}

fn run_status(
    parsed: ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let json_output = parsed.json_output;
    let root_path = parsed.root_override.or(default_root);
    let Some(root_path) = root_path else {
        diagnostics::stderr_line(
            stderr,
            format_args!("mcpace root not found; expected mcpace.config.json"),
        );
        return 1;
    };
    let canonical_root = runtimepaths::canonicalize_or_original(&root_path);
    let status = match collect_status(&canonical_root, parsed.host, parsed.port) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    write_status_response(&status, json_output, stdout)
}

#[derive(Debug)]
struct ServeStatus {
    root_path: String,
    state_root: String,
    host: String,
    port: u16,
    max_connections: Option<usize>,
    io_timeout_ms: Option<u64>,
    max_body_bytes: Option<usize>,
    overview_cache_ms: Option<u64>,
    url: String,
    status: String,
    identity_verified: bool,
    pid: Option<u32>,
    started_at_ms: Option<u128>,
    stdout_log_path: String,
    stderr_log_path: String,
    restart_guard_path: String,
    warnings: Vec<String>,
}

fn serve_state_process_identity_matches(state: &ServeState) -> bool {
    let expected_executable =
        (!state.runner_path.trim().is_empty()).then(|| Path::new(&state.runner_path));
    matches!(
        process_identity::match_process(
            state.pid,
            state.process_identity.as_deref(),
            expected_executable,
        ),
        Ok(ProcessMatch::Match)
    )
}

fn collect_status(
    root_path: &Path,
    host_override: Option<String>,
    port_override: Option<u16>,
) -> Result<ServeStatus, String> {
    let endpoint = runtimepaths::resolve_serve_endpoint(Some(root_path));
    let state_root = runtimepaths::resolve_state_root(root_path);
    let state_path = runtimepaths::serve_state_path(&state_root);
    let state = read_state(&state_path).ok();
    let host = host_override
        .or_else(|| state.as_ref().map(|value| value.host.clone()))
        .unwrap_or_else(|| endpoint.host.clone());
    let port = port_override
        .or_else(|| state.as_ref().map(|value| value.port))
        .unwrap_or(endpoint.port);
    let endpoint_healthy = health_check(&host, port, &endpoint.health_path).unwrap_or(false);
    let identity_verified = state
        .as_ref()
        .is_some_and(serve_state_process_identity_matches);
    let running = endpoint_healthy && identity_verified;
    let mut warnings = Vec::new();
    let status = if running {
        if let Some(state) = &state {
            if let Some(warning) = stale_runner_warning(state) {
                warnings.push(warning);
            }
        }
        "running".to_string()
    } else if endpoint_healthy {
        warnings.push(if state.is_some() {
            "serve endpoint answered, but the recorded PID/process identity does not match; refusing to trust an unrelated or reused process"
                .to_string()
        } else {
            "serve endpoint answered, but no managed state file exists; refusing to trust endpoint health without process identity"
                .to_string()
        });
        "unverified".to_string()
    } else if state.is_some() {
        warnings.push(
            "serve state exists but the MCPace HTTP endpoint did not answer the local health probe"
                .to_string(),
        );
        "stopped".to_string()
    } else {
        "stopped".to_string()
    };

    Ok(ServeStatus {
        root_path: sanitize_display(root_path),
        state_root: sanitize_display(&state_root),
        host: host.clone(),
        port,
        max_connections: state.as_ref().and_then(|value| value.max_connections),
        io_timeout_ms: state.as_ref().and_then(|value| value.io_timeout_ms),
        max_body_bytes: state.as_ref().and_then(|value| value.max_body_bytes),
        overview_cache_ms: state.as_ref().and_then(|value| value.overview_cache_ms),
        url: runtimepaths::http_url(&host, port, &endpoint.mcp_path),
        status,
        identity_verified,
        pid: state.as_ref().map(|value| value.pid).filter(|_| running),
        started_at_ms: state
            .as_ref()
            .map(|value| value.started_at_ms)
            .filter(|_| running),
        stdout_log_path: state
            .as_ref()
            .map(|value| value.stdout_log_path.clone())
            .unwrap_or_else(|| sanitize_display(&runtimepaths::serve_stdout_log_path(&state_root))),
        stderr_log_path: state
            .as_ref()
            .map(|value| value.stderr_log_path.clone())
            .unwrap_or_else(|| sanitize_display(&runtimepaths::serve_stderr_log_path(&state_root))),
        restart_guard_path: sanitize_display(&runtimepaths::serve_restart_guard_path(&state_root)),
        warnings,
    })
}

fn write_status_response(status: &ServeStatus, json_output: bool, stdout: &mut dyn Write) -> i32 {
    if json_output {
        let mut map = BTreeMap::new();
        map.insert(
            "rootPath".to_string(),
            JsonValue::string(status.root_path.clone()),
        );
        map.insert(
            "stateRoot".to_string(),
            JsonValue::string(status.state_root.clone()),
        );
        map.insert("host".to_string(), JsonValue::string(status.host.clone()));
        map.insert("port".to_string(), JsonValue::number(status.port));
        map.insert(
            "maxConnections".to_string(),
            json_helpers::optional_number(status.max_connections),
        );
        map.insert(
            "ioTimeoutMs".to_string(),
            json_helpers::optional_number(status.io_timeout_ms),
        );
        map.insert(
            "maxBodyBytes".to_string(),
            json_helpers::optional_number(status.max_body_bytes),
        );
        map.insert(
            "overviewCacheMs".to_string(),
            json_helpers::optional_number(status.overview_cache_ms),
        );
        map.insert("url".to_string(), JsonValue::string(status.url.clone()));
        map.insert(
            "status".to_string(),
            JsonValue::string(status.status.clone()),
        );
        map.insert(
            "identityVerified".to_string(),
            JsonValue::bool(status.identity_verified),
        );
        map.insert(
            "pid".to_string(),
            match status.pid {
                Some(value) => JsonValue::number(value),
                None => JsonValue::Null,
            },
        );
        map.insert(
            "startedAtMs".to_string(),
            match status.started_at_ms {
                Some(value) => JsonValue::number(value),
                None => JsonValue::Null,
            },
        );
        map.insert(
            "stdoutLogPath".to_string(),
            JsonValue::string(status.stdout_log_path.clone()),
        );
        map.insert(
            "stderrLogPath".to_string(),
            JsonValue::string(status.stderr_log_path.clone()),
        );
        map.insert(
            "restartGuardPath".to_string(),
            JsonValue::string(status.restart_guard_path.clone()),
        );
        map.insert(
            "warnings".to_string(),
            JsonValue::array(status.warnings.iter().cloned().map(JsonValue::string)),
        );
        let _ = writeln!(stdout, "{}", JsonValue::Object(map).to_pretty_string());
        return 0;
    }

    let _ = writeln!(stdout, "Serve status: {}", status.status);
    let _ = writeln!(stdout, "URL: {}", status.url);
    let _ = writeln!(stdout, "Host: {}", status.host);
    let _ = writeln!(stdout, "Port: {}", status.port);
    let _ = writeln!(
        stdout,
        "Process identity verified: {}",
        if status.identity_verified {
            "yes"
        } else {
            "no"
        }
    );
    if let Some(value) = status.max_connections {
        let _ = writeln!(stdout, "Max connections: {}", value);
    }
    if let Some(value) = status.io_timeout_ms {
        let _ = writeln!(stdout, "IO timeout ms: {}", value);
    }
    if let Some(value) = status.max_body_bytes {
        let _ = writeln!(stdout, "Max body bytes: {}", value);
    }
    if let Some(value) = status.overview_cache_ms {
        let _ = writeln!(stdout, "Overview cache ms: {}", value);
    }
    let _ = writeln!(stdout, "Root path: {}", status.root_path);
    let _ = writeln!(stdout, "State root: {}", status.state_root);
    let _ = writeln!(stdout, "Stdout log: {}", status.stdout_log_path);
    let _ = writeln!(stdout, "Stderr log: {}", status.stderr_log_path);
    let _ = writeln!(stdout, "Restart guard: {}", status.restart_guard_path);
    if let Some(pid) = status.pid {
        let _ = writeln!(stdout, "PID: {}", pid);
    }
    if let Some(started_at_ms) = status.started_at_ms {
        let _ = writeln!(stdout, "Started at ms: {}", started_at_ms);
    }
    if !status.warnings.is_empty() {
        let _ = writeln!(stdout, "Warnings: {}", status.warnings.join(" | "));
    }
    0
}

const MALFORMED_START_LOCK_STALE_AFTER: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub(crate) struct ServeLifecycleCoordinationGuard {
    _file: File,
}

pub(crate) fn acquire_lifecycle_coordination(
    state_root: &Path,
) -> Result<ServeLifecycleCoordinationGuard, String> {
    runtimepaths::ensure_serve_dir(state_root).map_err(String::from)?;
    let path = runtimepaths::serve_dir(state_root).join("lifecycle.lock");
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| {
            format!(
                "failed to open serve lifecycle coordination lock '{}': {}",
                path.display(),
                error
            )
        })?;
    file.lock().map_err(|error| {
        format!(
            "failed to acquire serve lifecycle coordination lock '{}': {}",
            path.display(),
            error
        )
    })?;
    Ok(ServeLifecycleCoordinationGuard { _file: file })
}

#[derive(Debug)]
struct ServeStartLockGuard {
    path: PathBuf,
    owner_pid: u32,
    process_identity: String,
    _coordination: ServeLifecycleCoordinationGuard,
}

impl Drop for ServeStartLockGuard {
    fn drop(&mut self) {
        let owns_lock =
            read_serve_start_lock_owner(&self.path)
                .ok()
                .is_some_and(|(pid, identity)| {
                    pid == self.owner_pid && identity.as_deref() == Some(&self.process_identity)
                });
        if owns_lock {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn acquire_serve_start_lock(state_root: &Path) -> Result<ServeStartLockGuard, String> {
    let coordination = acquire_lifecycle_coordination(state_root)?;
    let lock_path = runtimepaths::serve_start_lock_path(state_root);
    let owner_pid = std::process::id();
    let process_identity = process_identity_token(owner_pid)?;
    let payload = JsonValue::object([
        ("pid", JsonValue::number(owner_pid)),
        (
            "processIdentity",
            JsonValue::string(process_identity.clone()),
        ),
        ("startedAtMs", JsonValue::number(now_ms())),
    ])
    .to_pretty_string();

    for attempt in 0..2 {
        match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                if let Err(error) = file.write_all(payload.as_bytes()) {
                    let _ = fs::remove_file(&lock_path);
                    return Err(format!(
                        "failed to write serve start lock '{}': {}",
                        lock_path.display(),
                        error
                    ));
                }
                let _ = file.sync_all();
                return Ok(ServeStartLockGuard {
                    path: lock_path,
                    owner_pid,
                    process_identity,
                    _coordination: coordination,
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let (existing_pid, existing_identity) =
                    match read_serve_start_lock_owner(&lock_path) {
                        Ok(owner) => owner,
                        Err(parse_error) => {
                            if attempt == 0 && malformed_start_lock_is_stale(&lock_path)? {
                                fs::remove_file(&lock_path).map_err(|error| {
                                    format!(
                                    "failed to remove malformed stale serve start lock '{}': {}",
                                    lock_path.display(),
                                    error
                                )
                                })?;
                                continue;
                            }
                            return Err(format!(
                            "{}; refusing to reclaim a malformed start lock newer than {} seconds",
                            parse_error,
                            MALFORMED_START_LOCK_STALE_AFTER.as_secs()
                        ));
                        }
                    };
                match process_identity::match_process(
                    existing_pid,
                    existing_identity.as_deref(),
                    None,
                )
                .map_err(|error| {
                    format!(
                        "failed to verify serve start lock owner pid {} at {}: {}",
                        existing_pid,
                        lock_path.display(),
                        error
                    )
                })? {
                    ProcessMatch::Match => {
                        return Err(format!(
                            "mcpace start is already in progress at {} (owner pid {})",
                            lock_path.display(),
                            existing_pid
                        ));
                    }
                    ProcessMatch::NotFound | ProcessMatch::Mismatch if attempt == 0 => {
                        fs::remove_file(&lock_path).map_err(|error| {
                            format!(
                                "failed to remove stale serve start lock '{}': {}",
                                lock_path.display(),
                                error
                            )
                        })?;
                    }
                    ProcessMatch::NotFound | ProcessMatch::Mismatch => {
                        return Err(format!(
                            "serve start lock at {} changed while stale ownership was being reclaimed",
                            lock_path.display()
                        ));
                    }
                }
            }
            Err(error) => {
                return Err(format!(
                    "failed to acquire serve start lock at {}: {}",
                    lock_path.display(),
                    error
                ));
            }
        }
    }
    Err(format!(
        "failed to acquire serve start lock at {}",
        lock_path.display()
    ))
}

fn malformed_start_lock_is_stale(path: &Path) -> Result<bool, String> {
    let modified = fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .map_err(|error| {
            format!(
                "failed to inspect malformed serve start lock '{}': {}",
                path.display(),
                error
            )
        })?;
    Ok(std::time::SystemTime::now()
        .duration_since(modified)
        .map(|age| age >= MALFORMED_START_LOCK_STALE_AFTER)
        .unwrap_or(false))
}

fn read_serve_start_lock_owner(path: &Path) -> Result<(u32, Option<String>), String> {
    let raw = fs::read_to_string(path).map_err(|error| {
        format!(
            "failed to read serve start lock '{}': {}",
            path.display(),
            error
        )
    })?;
    let value = parse_str(&raw).map_err(|error| {
        format!(
            "failed to parse serve start lock '{}': {}",
            path.display(),
            error
        )
    })?;
    let pid = value
        .get("pid")
        .and_then(JsonValue::as_i64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            format!(
                "serve start lock '{}' has no valid owner pid",
                path.display()
            )
        })?;
    let process_identity = value
        .get("processIdentity")
        .and_then(JsonValue::as_str)
        .map(str::to_string);
    Ok((pid, process_identity))
}

fn write_state(path: &Path, state: &ServeState) -> Result<(), String> {
    let payload = JsonValue::object([
        ("rootPath", JsonValue::string(state.root_path.clone())),
        ("stateRoot", JsonValue::string(state.state_root.clone())),
        ("host", JsonValue::string(state.host.clone())),
        ("port", JsonValue::number(state.port)),
        (
            "maxConnections",
            json_helpers::optional_number(state.max_connections),
        ),
        (
            "ioTimeoutMs",
            json_helpers::optional_number(state.io_timeout_ms),
        ),
        (
            "maxBodyBytes",
            json_helpers::optional_number(state.max_body_bytes),
        ),
        (
            "overviewCacheMs",
            json_helpers::optional_number(state.overview_cache_ms),
        ),
        ("url", JsonValue::string(state.url.clone())),
        ("pid", JsonValue::number(state.pid)),
        (
            "processIdentity",
            state
                .process_identity
                .as_ref()
                .map_or(JsonValue::Null, |value| JsonValue::string(value.clone())),
        ),
        (
            "stopToken",
            state
                .stop_token
                .as_ref()
                .map_or(JsonValue::Null, |value| JsonValue::string(value.clone())),
        ),
        (
            "supervisorManaged",
            state
                .supervisor_managed
                .map_or(JsonValue::Null, JsonValue::bool),
        ),
        ("startedAtMs", JsonValue::number(state.started_at_ms)),
        ("runnerPath", JsonValue::string(state.runner_path.clone())),
        (
            "stdoutLogPath",
            JsonValue::string(state.stdout_log_path.clone()),
        ),
        (
            "stderrLogPath",
            JsonValue::string(state.stderr_log_path.clone()),
        ),
    ])
    .to_pretty_string();
    Ok(runtimepaths::write_text_atomic(path, &payload)?)
}

fn remove_state_if_current_pid(path: &Path, pid: u32) {
    let should_remove = match read_state(path) {
        Ok(state) => state.pid == pid,
        Err(_) => false,
    };
    if should_remove {
        let _ = fs::remove_file(path);
    }
}

fn read_state(path: &Path) -> Result<ServeState, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("failed to read serve state '{}': {}", path.display(), error))?;
    let json = parse_str(&raw).map_err(|error| {
        format!(
            "failed to parse serve state '{}': {}",
            path.display(),
            error
        )
    })?;
    let host = json
        .get("host")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| format!("serve state '{}' is missing host", path.display()))?
        .to_string();
    let port = json
        .get("port")
        .and_then(JsonValue::as_i64)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| format!("serve state '{}' is missing port", path.display()))?;
    let pid = json
        .get("pid")
        .and_then(JsonValue::as_i64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| format!("serve state '{}' is missing pid", path.display()))?;
    let started_at_ms = json
        .get("startedAtMs")
        .and_then(JsonValue::as_i64)
        .and_then(|value| u128::try_from(value).ok())
        .ok_or_else(|| format!("serve state '{}' is missing startedAtMs", path.display()))?;

    Ok(ServeState {
        root_path: json
            .get("rootPath")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_string(),
        state_root: json
            .get("stateRoot")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_string(),
        host: host.clone(),
        port,
        max_connections: json
            .get("maxConnections")
            .and_then(JsonValue::as_i64)
            .and_then(|value| usize::try_from(value).ok()),
        io_timeout_ms: json
            .get("ioTimeoutMs")
            .and_then(JsonValue::as_i64)
            .and_then(|value| u64::try_from(value).ok()),
        max_body_bytes: json
            .get("maxBodyBytes")
            .and_then(JsonValue::as_i64)
            .and_then(|value| usize::try_from(value).ok()),
        overview_cache_ms: json
            .get("overviewCacheMs")
            .and_then(JsonValue::as_i64)
            .and_then(|value| u64::try_from(value).ok()),
        url: json
            .get("url")
            .and_then(JsonValue::as_str)
            .map(|value| value.to_string())
            .unwrap_or_else(|| {
                runtimepaths::http_url(&host, port, runtimepaths::DEFAULT_LOCAL_MCP_PATH)
            }),
        pid,
        process_identity: json
            .get("processIdentity")
            .and_then(JsonValue::as_str)
            .map(str::to_string),
        stop_token: json
            .get("stopToken")
            .and_then(JsonValue::as_str)
            .filter(|token| valid_stop_token(token))
            .map(str::to_string),
        supervisor_managed: json.get("supervisorManaged").and_then(JsonValue::as_bool),
        started_at_ms,
        runner_path: json
            .get("runnerPath")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_string(),
        stdout_log_path: json
            .get("stdoutLogPath")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_string(),
        stderr_log_path: json
            .get("stderrLogPath")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

fn health_probe_detail(host: &str, port: u16, path: &str) -> Result<(), String> {
    let probe_host = http_probe::probe_host(host);
    let path = runtimepaths::normalize_http_path(path, runtimepaths::DEFAULT_LOCAL_HEALTH_PATH);
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, probe_host
    );
    let response = http_probe::raw_response(
        &probe_host,
        port,
        &request,
        HEALTH_PROBE_IO_TIMEOUT,
        HEALTH_PROBE_MAX_RESPONSE_BYTES,
    )
    .map_err(|error| error.to_string())?;
    let parsed = http_probe::parse_response(&response)
        .map_err(|_| "parse HTTP health response: malformed response".to_string())?;
    if parsed.status != 200 {
        return Err(format!(
            "health endpoint returned HTTP status {}",
            parsed.status
        ));
    }
    let body_bytes = parsed
        .body_bytes()
        .map_err(|error| format!("decode HTTP health body: {}", error))?;
    let body = String::from_utf8(body_bytes)
        .map_err(|_| "health endpoint returned a non-UTF-8 body".to_string())?;
    let payload =
        parse_str(body.trim()).map_err(|error| format!("parse health response JSON: {}", error))?;
    if !matches!(payload.get("readiness"), Some(JsonValue::Object(_))) {
        return Err("health response JSON did not contain a readiness object".to_string());
    }
    Ok(())
}

fn health_check(host: &str, port: u16, path: &str) -> Result<bool, String> {
    match health_probe_detail(host, port, path) {
        Ok(()) => Ok(true),
        Err(error) if error.starts_with("resolve ") => Err(error),
        Err(_) => Ok(false),
    }
}

fn wait_for_health(
    host: &str,
    port: u16,
    path: &str,
    attempts: usize,
    delay: Duration,
) -> Result<(), String> {
    let path = runtimepaths::normalize_http_path(path, runtimepaths::DEFAULT_LOCAL_HEALTH_PATH);
    let started = std::time::Instant::now();
    let mut failures = Vec::<(String, usize)>::new();
    let mut omitted_distinct_failures = 0usize;
    for _ in 0..attempts {
        match health_probe_detail(host, port, &path) {
            Ok(()) => return Ok(()),
            Err(error) if error.starts_with("resolve ") => return Err(error),
            Err(error) => {
                if let Some((_, count)) = failures.iter_mut().find(|(message, _)| message == &error)
                {
                    *count = count.saturating_add(1);
                } else if failures.len() < 4 {
                    failures.push((error, 1));
                } else {
                    omitted_distinct_failures = omitted_distinct_failures.saturating_add(1);
                }
            }
        }
        thread::sleep(delay);
    }
    let failure_summary = if failures.is_empty() {
        "no probe attempts were made".to_string()
    } else {
        failures
            .iter()
            .map(|(message, count)| format!("{}x {}", count, message))
            .collect::<Vec<_>>()
            .join("; ")
    };
    let omitted_summary = if omitted_distinct_failures == 0 {
        String::new()
    } else {
        format!(
            "; {} additional distinct failures omitted",
            omitted_distinct_failures
        )
    };
    Err(format!(
        "serve did not become healthy on {} after {} ms; probe failures: {}{}",
        http_url(host, port, &path),
        started.elapsed().as_millis(),
        failure_summary,
        omitted_summary
    ))
}

fn http_url(host: &str, port: u16, path: &str) -> String {
    runtimepaths::http_url(host, port, path)
}

#[allow(dead_code)]
fn host_for_url(host: &str) -> String {
    let trimmed = host.trim();
    let unbracketed = trimmed.trim_start_matches('[').trim_end_matches(']');
    let connectable = match unbracketed {
        "" | "0.0.0.0" | "::" => runtimepaths::DEFAULT_LOCAL_HOST,
        other => other,
    };
    if connectable.contains(':') {
        return format!("[{}]", connectable);
    }
    connectable.to_string()
}

fn stale_runner_warning(state: &ServeState) -> Option<String> {
    let current_runner_path = resolve_runner_source().ok()?;
    let runner_path = PathBuf::from(&state.runner_path);
    stale_runner_warning_for_paths(&runner_path, &current_runner_path)
}

fn stale_runner_warning_for_paths(
    runner_path: &Path,
    current_runner_path: &Path,
) -> Option<String> {
    let runner_path = runtimepaths::canonicalize_or_original(runner_path);
    let current_runner_path = runtimepaths::canonicalize_or_original(current_runner_path);
    if runner_path == current_runner_path {
        return None;
    }
    match files_have_same_contents(&runner_path, &current_runner_path) {
        Ok(true) => None,
        Ok(false) => Some(format!(
            "running MCPace serve runner '{}' differs from current binary '{}'; run 'mcpace restart --root <project>' after rebuilding or upgrading",
            sanitize_display(&runner_path),
            sanitize_display(&current_runner_path)
        )),
        Err(_) => None,
    }
}

fn files_have_same_contents(left: &Path, right: &Path) -> Result<bool, std::io::Error> {
    let left_meta = fs::metadata(left)?;
    let right_meta = fs::metadata(right)?;
    if left_meta.len() != right_meta.len() {
        return Ok(false);
    }
    Ok(fs::read(left)? == fs::read(right)?)
}

fn resolve_runner_source() -> Result<PathBuf, std::io::Error> {
    if let Some(path) = std::env::var_os("MCPACE_RUNNER_PATH") {
        let explicit = PathBuf::from(path);
        if explicit.is_file() {
            return Ok(explicit);
        }
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "MCPACE_RUNNER_PATH does not point to a file: {}",
                explicit.display()
            ),
        ));
    }

    let current = std::env::current_exe()?;
    let current_is_dependency_test_binary = current
        .parent()
        .and_then(|parent| parent.file_name())
        .map(|value| value == "deps")
        .unwrap_or(false);

    if current
        .file_stem()
        .and_then(|value| value.to_str())
        .map(|value| value.starts_with("mcpace"))
        .unwrap_or(false)
        && !current_is_dependency_test_binary
    {
        return Ok(current);
    }

    let fallback = current
        .parent()
        .and_then(|parent| parent.parent())
        .map(|parent| {
            parent.join(if cfg!(windows) {
                "mcpace.exe"
            } else {
                "mcpace"
            })
        });
    match fallback {
        Some(path) if path.is_file() => Ok(path),
        _ if current_is_dependency_test_binary => Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "mcpace serve runner binary is not available; build target/debug/mcpace or set MCPACE_RUNNER_PATH",
        )),
        _ => Ok(current),
    }
}

fn sanitize_display(path: &Path) -> String {
    let rendered = path.display().to_string();
    runtimepaths::strip_windows_extended_path_prefix(&rendered)
}

fn valid_stop_token(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn random_stop_token() -> Result<String, String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("failed to generate serve stop token: {}", error))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn serve_stop_request_path(state_root: &Path) -> PathBuf {
    runtimepaths::serve_dir(state_root).join("stop-requested")
}

fn start_direct_stop_watcher(parsed: &ParsedArgs) -> Result<(), String> {
    let Some(token) = parsed.stop_token.as_deref() else {
        return Ok(());
    };
    let root = parsed
        .root_override
        .as_deref()
        .ok_or_else(|| "serve --stop-token requires --root".to_string())?;
    let state_root =
        runtimepaths::resolve_state_root(&runtimepaths::canonicalize_or_original(root));
    start_stop_watcher(state_root, token.to_string())
}

fn start_stop_watcher(state_root: PathBuf, token: String) -> Result<(), String> {
    let marker = serve_stop_request_path(&state_root);
    thread::Builder::new()
        .name("mcpace-serve-stop".to_string())
        .spawn(move || loop {
            let requested = fs::read_to_string(&marker)
                .ok()
                .is_some_and(|value| value.trim() == token);
            if requested {
                let _ = fs::remove_file(&marker);
                std::process::exit(0);
            }
            thread::sleep(Duration::from_millis(50));
        })
        .map(|_| ())
        .map_err(|error| format!("failed to start serve stop watcher: {}", error))
}

fn process_identity_token(pid: u32) -> Result<String, String> {
    process_identity::capture(pid)
        .map_err(|error| {
            format!(
                "failed to capture process identity for pid {}: {}",
                pid, error
            )
        })?
        .map(|identity| identity.start_token)
        .ok_or_else(|| {
            format!(
                "process pid {} exited before its identity could be captured",
                pid
            )
        })
}

fn now_ms() -> u128 {
    runtimepaths::unix_time_ms()
}

struct SpawnedBackground {
    pid: u32,
    #[cfg(unix)]
    child: std::process::Child,
}

impl SpawnedBackground {
    fn pid(&self) -> u32 {
        self.pid
    }

    #[cfg(unix)]
    fn status_detail(&mut self) -> String {
        match self.child.try_wait() {
            Ok(Some(status)) => format!("exited before startup completed ({})", status),
            Ok(None) => "was still running when startup timed out".to_string(),
            Err(error) => format!("status check failed: {}", error),
        }
    }

    #[cfg(not(unix))]
    fn status_detail(&mut self) -> String {
        format!("status is unavailable for pid {}", self.pid)
    }
}

fn spawn_background(
    runner_path: &Path,
    root_path: &Path,
    host: &str,
    port: u16,
    extra_args: &[String],
    stdout_file: File,
    stderr_file: File,
) -> Result<SpawnedBackground, String> {
    #[cfg(windows)]
    {
        drop(stdout_file);
        drop(stderr_file);
        return spawn_background_windows(runner_path, root_path, host, port, extra_args)
            .map(|pid| SpawnedBackground { pid });
    }

    #[cfg(unix)]
    {
        use std::process::{Command, Stdio};

        let mut command = Command::new(runner_path);
        command
            .arg("serve")
            .arg("--root")
            .arg(root_path)
            .arg("--host")
            .arg(host)
            .arg("--port")
            .arg(port.to_string());
        command.args(extra_args);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file));

        crate::process_detach::configure_unix_new_session(&mut command);

        return command
            .spawn()
            .map(|child| SpawnedBackground {
                pid: child.id(),
                child,
            })
            .map_err(|error| format!("failed to start MCPace serve runtime: {}", error));
    }

    #[allow(unreachable_code)]
    Err("background serve launch is not implemented for this platform".to_string())
}

#[cfg(windows)]
fn spawn_background_windows(
    runner_path: &Path,
    root_path: &Path,
    host: &str,
    port: u16,
    extra_args: &[String],
) -> Result<u32, String> {
    use std::ffi::OsString;

    let mut args = vec![
        OsString::from("serve"),
        OsString::from("--root"),
        root_path.as_os_str().to_os_string(),
        OsString::from("--host"),
        OsString::from(host),
        OsString::from("--port"),
        OsString::from(port.to_string()),
    ];
    args.extend(
        extra_args
            .iter()
            .map(|value| OsString::from(value.as_str())),
    );
    crate::windows_process::spawn_detached_no_window(runner_path, &args, Some(root_path))
        .map_err(|error| format!("failed to start MCPace serve runtime: {}", error))
}

#[cfg(test)]
mod tests;
