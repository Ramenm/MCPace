use crate::json::JsonValue;
use crate::resources;
use crate::runtimepaths;
use auto_launch::{
    AutoLaunch, AutoLaunchBuilder, LinuxLaunchMode, MacOSLaunchMode, WindowsEnableMode,
};
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) const APP_NAME: &str = "MCPace Agent";
pub(crate) const LEGACY_APP_NAME: &str = "MCPace";

#[derive(Debug)]
struct ParsedArgs {
    action: String,
    json_output: bool,
    root_override: Option<PathBuf>,
    host: String,
    port: u16,
    max_connections: Option<usize>,
    io_timeout_ms: Option<u64>,
    max_body_bytes: Option<usize>,
    overview_cache_ms: Option<u64>,
    dry_run: bool,
    no_enable: bool,
    help: bool,
    error: Option<String>,
}

impl Default for ParsedArgs {
    fn default() -> Self {
        Self {
            action: "status".to_string(),
            json_output: false,
            root_override: None,
            host: runtimepaths::DEFAULT_LOCAL_HOST.to_string(),
            port: runtimepaths::DEFAULT_LOCAL_MCP_PORT,
            max_connections: None,
            io_timeout_ms: None,
            max_body_bytes: None,
            overview_cache_ms: None,
            dry_run: false,
            no_enable: false,
            help: false,
            error: None,
        }
    }
}

struct ServiceConfig {
    app_path: String,
    args: Vec<String>,
    target_app_path: String,
    target_args: Vec<String>,
    launch_mode: String,
    platform: String,
    backend: String,
    warnings: Vec<String>,
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

    let Some(root_path) = parsed.root_override.clone().or(default_root) else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };
    let root_path = runtimepaths::canonicalize_or_original(&root_path);
    let config = match service_config(
        &root_path,
        &parsed.host,
        parsed.port,
        parsed.max_connections,
        parsed.io_timeout_ms,
        parsed.max_body_bytes,
        parsed.overview_cache_ms,
    ) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };
    let launcher = match build_launcher(&config) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "failed to build autostart launcher: {}", error);
            return 1;
        }
    };

    let report = match parsed.action.as_str() {
        "install" | "enable" => {
            service_install(&launcher, &config, parsed.dry_run || parsed.no_enable)
        }
        "repair" => service_install(&launcher, &config, parsed.dry_run || parsed.no_enable),
        "uninstall" | "disable" => service_uninstall(&launcher, &config, parsed.dry_run),
        "status" => service_status(&launcher, &config),
        "verify" | "doctor" => service_verify(&launcher, &config),
        "print" | "plan" => service_print(&config),
        other => {
            let _ = writeln!(stderr, "unsupported autostart action: {}", other);
            return 2;
        }
    };

    if parsed.json_output {
        let _ = writeln!(stdout, "{}", report.to_pretty_string());
    } else {
        write_text_report(&report, stdout);
    }
    if report
        .get("ok")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false)
    {
        0
    } else {
        1
    }
}

fn parse_args(args: &[String]) -> ParsedArgs {
    let mut parsed = ParsedArgs::default();
    let mut index = 0usize;
    if let Some(first) = args.first() {
        if !first.starts_with('-') {
            parsed.action = first.to_ascii_lowercase();
            index = 1;
        }
    }
    while index < args.len() {
        match args[index].as_str() {
            "--json" | "-json" => {
                parsed.json_output = true;
                index += 1;
            }
            "--root" | "-root" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("autostart requires a path after --root".to_string());
                    return parsed;
                };
                parsed.root_override = Some(PathBuf::from(value));
                index += 2;
            }
            "--host" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("autostart requires a value after --host".to_string());
                    return parsed;
                };
                parsed.host = value.to_string();
                index += 2;
            }
            "--port" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("autostart requires a value after --port".to_string());
                    return parsed;
                };
                match value.parse::<u16>() {
                    Ok(port) => parsed.port = port,
                    Err(_) => {
                        parsed.error = Some("autostart --port must be a valid u16".to_string());
                        return parsed;
                    }
                }
                index += 2;
            }
            "--max-connections" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("autostart requires a value after --max-connections".to_string());
                    return parsed;
                };
                match resources::parse_http_connection_limit(value, "autostart --max-connections") {
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
                    parsed.error =
                        Some("autostart requires a value after --io-timeout-ms".to_string());
                    return parsed;
                };
                match resources::parse_http_io_timeout_ms(value, "autostart --io-timeout-ms") {
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
                        Some("autostart requires a value after --max-body-bytes".to_string());
                    return parsed;
                };
                match resources::parse_http_body_limit(value, "autostart --max-body-bytes") {
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
                        Some("autostart requires a value after --overview-cache-ms".to_string());
                    return parsed;
                };
                match resources::parse_nonnegative_u64(value, "autostart --overview-cache-ms") {
                    Ok(ttl_ms) => parsed.overview_cache_ms = Some(ttl_ms),
                    Err(error) => {
                        parsed.error = Some(error);
                        return parsed;
                    }
                }
                index += 2;
            }
            "--dry-run" => {
                parsed.dry_run = true;
                index += 1;
            }
            "--no-enable" => {
                parsed.no_enable = true;
                index += 1;
            }
            "-h" | "--help" | "-?" => {
                parsed.help = true;
                return parsed;
            }
            other => {
                parsed.error = Some(format!("unsupported autostart argument: {}", other));
                return parsed;
            }
        }
    }
    parsed
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "Usage: mcpace autostart <enable|repair|status|verify|disable|print> [--json] [--root <path>] [--host <addr>] [--port <n>] [--max-connections <n>] [--io-timeout-ms <n>] [--max-body-bytes <n>] [--overview-cache-ms <n>] [--dry-run] [--no-enable]");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Installs a visible user-level login item named MCPace Agent through the upstream auto-launch crate.");
    let _ = writeln!(stdout, "On Windows the login item launches `mcpace agent start --autostart`, which starts a hidden background serve runtime and exits so no persistent console window remains.");
    let _ = writeln!(stdout, "On launchd/XDG platforms the login item launches `mcpace agent run --autostart`; the agent then runs the foreground managed MCPace runtime.");
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

fn service_config(
    root_path: &Path,
    host: &str,
    port: u16,
    max_connections: Option<usize>,
    io_timeout_ms: Option<u64>,
    max_body_bytes: Option<usize>,
    overview_cache_ms: Option<u64>,
) -> Result<ServiceConfig, String> {
    let target_app_path = std::env::current_exe()
        .map_err(|error| format!("failed to resolve current executable: {}", error))?
        .display()
        .to_string();
    let agent_action = if cfg!(windows) { "start" } else { "run" };
    let mut target_args = vec![
        "agent".to_string(),
        agent_action.to_string(),
        "--autostart".to_string(),
        "--root".to_string(),
        command_path_string(root_path),
        "--host".to_string(),
        host.to_string(),
        "--port".to_string(),
        port.to_string(),
    ];
    resources::append_serve_resource_args(
        &mut target_args,
        max_connections,
        io_timeout_ms,
        max_body_bytes,
        overview_cache_ms,
    );
    let platform = current_platform();
    let backend = if cfg!(windows) {
        "auto-launch/windows-current-user-run"
    } else if cfg!(target_os = "macos") {
        "auto-launch/macos-launch-agent"
    } else if cfg!(target_os = "linux") {
        "auto-launch/linux-xdg-autostart"
    } else {
        "auto-launch/unsupported"
    };
    let mut warnings = vec![
        "Autostart is user-level by default; it does not install privileged system services.".to_string(),
        "MCPace autostart launches MCPace Agent directly; the previous Windows wscript/VBS wrapper is not used for new installs.".to_string(),
    ];
    if cfg!(target_os = "linux") {
        warnings.push(
            "Linux default autostart uses XDG Autostart for desktop login visibility; use a future system-service mode for boot-before-login supervision.".to_string(),
        );
    }
    if cfg!(target_os = "macos") {
        warnings.push(
            "macOS autostart is a LaunchAgent: it starts at user login and is not a privileged LaunchDaemon.".to_string(),
        );
    }
    if cfg!(windows) {
        warnings.push(
            "Windows autostart uses the current-user Run login item with the visible name MCPace Agent; the entry starts a hidden background serve runtime and exits instead of holding a console window open.".to_string(),
        );
    }
    if !AutoLaunch::is_support() {
        warnings.push("auto-launch does not support this target OS.".to_string());
    }
    let app_path = autolaunch_token(&target_app_path);
    let args = target_args
        .iter()
        .map(|arg| autolaunch_token(arg))
        .collect::<Vec<_>>();
    Ok(ServiceConfig {
        app_path,
        args,
        target_app_path,
        target_args,
        launch_mode: if cfg!(windows) {
            "direct-mcpace-agent-background-start".to_string()
        } else {
            "direct-mcpace-agent-foreground-run".to_string()
        },
        platform: platform.to_string(),
        backend: backend.to_string(),
        warnings,
    })
}

fn autolaunch_token(value: &str) -> String {
    autolaunch_token_impl(value)
}

#[cfg(windows)]
fn autolaunch_token_impl(value: &str) -> String {
    crate::windows_process::quote_windows_arg(value)
}

#[cfg(target_os = "linux")]
fn autolaunch_token_impl(value: &str) -> String {
    quote_shellish_token(value)
}

#[cfg(all(not(windows), not(target_os = "linux")))]
fn autolaunch_token_impl(value: &str) -> String {
    value.to_string()
}

#[cfg(any(test, not(windows)))]
fn quote_shellish_token(value: &str) -> String {
    if !needs_shellish_quote(value) {
        return value.to_string();
    }
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`");
    format!("\"{}\"", escaped)
}

#[cfg(any(test, not(windows)))]
fn needs_shellish_quote(value: &str) -> bool {
    value.is_empty()
        || value.chars().any(|ch| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    '"' | '\''
                        | '&'
                        | '|'
                        | ';'
                        | '<'
                        | '>'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '$'
                        | '`'
                )
        })
}

fn current_platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unsupported"
    }
}

fn command_path_string(path: &Path) -> String {
    let text = path.display().to_string();
    strip_extended_windows_prefix(&text)
}

#[cfg(windows)]
fn strip_extended_windows_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{}", rest);
    }
    path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
}

#[cfg(not(windows))]
fn strip_extended_windows_prefix(path: &str) -> String {
    path.to_string()
}

fn build_launcher(config: &ServiceConfig) -> auto_launch::Result<AutoLaunch> {
    build_named_launcher(APP_NAME, config)
}

fn build_legacy_launcher(config: &ServiceConfig) -> auto_launch::Result<AutoLaunch> {
    build_named_launcher(LEGACY_APP_NAME, config)
}

fn build_named_launcher(app_name: &str, config: &ServiceConfig) -> auto_launch::Result<AutoLaunch> {
    let mut builder = AutoLaunchBuilder::new();
    builder
        .set_app_name(app_name)
        .set_app_path(&config.app_path)
        .set_args(&config.args)
        .set_macos_launch_mode(MacOSLaunchMode::LaunchAgent)
        .set_windows_enable_mode(WindowsEnableMode::CurrentUser)
        .set_linux_launch_mode(LinuxLaunchMode::XdgAutostart);
    builder.build()
}

fn service_install(launcher: &AutoLaunch, config: &ServiceConfig, dry_run: bool) -> JsonValue {
    if dry_run {
        let mut warnings = config.warnings.clone();
        warnings.push("Dry run / --no-enable: auto-launch enable was not called.".to_string());
        warnings.extend(cleanup_legacy_autostart(config, true).warnings);
        let mut result = report("install", true, false, config, warnings, None);
        attach_current_session_start(&mut result, CurrentSessionStart::skipped("dry-run"));
        return result;
    }
    match launcher.enable() {
        Ok(()) => {
            let enabled = enabled_or_false(launcher);
            let cleanup = cleanup_legacy_autostart(config, false);
            let current_start = start_current_session_runtime(config);
            let current_start_ok = current_start.ok_or_not_attempted();
            let mut warnings = config.warnings.clone();
            warnings.extend(cleanup.warnings);
            warnings.extend(current_start.warnings.clone());
            let ok = enabled && cleanup.ok && current_start_ok;
            let mut result = report(
                "install",
                ok,
                enabled,
                config,
                warnings,
                if ok {
                    None
                } else if !cleanup.ok {
                    Some(
                        "auto-launch enable completed but legacy autostart cleanup failed"
                            .to_string(),
                    )
                } else if !current_start_ok {
                    Some(
                        "auto-launch enable completed but current-session MCPace endpoint did not start"
                            .to_string(),
                    )
                } else {
                    Some("auto-launch enable completed but is_enabled returned false".to_string())
                },
            );
            attach_current_session_start(&mut result, current_start);
            result
        }
        Err(error) => report(
            "install",
            false,
            enabled_or_false(launcher),
            config,
            config.warnings.clone(),
            Some(error.to_string()),
        ),
    }
}

#[derive(Clone, Debug)]
struct CurrentSessionStart {
    attempted: bool,
    ok: bool,
    exit_code: Option<i32>,
    stdout_tail: String,
    stderr_tail: String,
    reason: String,
    warnings: Vec<String>,
}

impl CurrentSessionStart {
    fn skipped(reason: &str) -> Self {
        Self {
            attempted: false,
            ok: true,
            exit_code: None,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            reason: reason.to_string(),
            warnings: Vec::new(),
        }
    }

    fn ok_or_not_attempted(&self) -> bool {
        !self.attempted || self.ok
    }
}

fn start_current_session_runtime(config: &ServiceConfig) -> CurrentSessionStart {
    if config
        .target_args
        .get(1)
        .is_none_or(|value| value != "start")
    {
        return CurrentSessionStart::skipped(
            "current-session start is only needed for background-start login entries",
        );
    }
    let Some(agent_args) = config.target_args.get(1..) else {
        return CurrentSessionStart {
            attempted: true,
            ok: false,
            exit_code: None,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            reason: "autostart target arguments did not include an agent command".to_string(),
            warnings: vec!["Failed to start current-session MCPace runtime: malformed autostart target arguments".to_string()],
        };
    };
    let mut args = agent_args.to_vec();
    if !args.iter().any(|value| value == "--json" || value == "-j") {
        args.push("--json".to_string());
    }
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = crate::agent::run(&args, None, &mut stdout, &mut stderr);
    let stdout_tail = text_tail(&String::from_utf8_lossy(&stdout), 4096);
    let stderr_tail = text_tail(&String::from_utf8_lossy(&stderr), 4096);
    let ok = exit_code == 0 && current_session_endpoint_listening(config);
    let mut warnings = Vec::new();
    let reason = if ok {
        "current-session MCPace endpoint is running".to_string()
    } else {
        let reason = format!(
            "current-session MCPace endpoint did not become reachable after agent start (exit code {})",
            exit_code
        );
        warnings.push(reason.clone());
        reason
    };
    CurrentSessionStart {
        attempted: true,
        ok,
        exit_code: Some(exit_code),
        stdout_tail,
        stderr_tail,
        reason,
        warnings,
    }
}

fn attach_current_session_start(report: &mut JsonValue, start: CurrentSessionStart) {
    if let Some(map) = report.as_object_mut() {
        map.insert(
            "currentSessionStart".to_string(),
            JsonValue::object([
                ("attempted", JsonValue::bool(start.attempted)),
                ("ok", JsonValue::bool(start.ok)),
                (
                    "exitCode",
                    start
                        .exit_code
                        .map(|code| JsonValue::number(code as i64))
                        .unwrap_or(JsonValue::Null),
                ),
                ("reason", JsonValue::string(start.reason)),
                ("stdoutTail", JsonValue::string(start.stdout_tail)),
                ("stderrTail", JsonValue::string(start.stderr_tail)),
            ]),
        );
    }
}

fn text_tail(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.to_string();
    }
    value.chars().skip(char_count - max_chars).collect()
}

fn current_session_endpoint_listening(config: &ServiceConfig) -> bool {
    let Some(root) = target_arg_value(&config.target_args, "--root") else {
        return false;
    };
    let host =
        target_arg_value(&config.target_args, "--host").unwrap_or(runtimepaths::DEFAULT_LOCAL_HOST);
    let port = target_arg_value(&config.target_args, "--port")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(runtimepaths::DEFAULT_LOCAL_MCP_PORT);
    let args = vec![
        "status".to_string(),
        "--json".to_string(),
        "--root".to_string(),
        root.to_string(),
        "--host".to_string(),
        host.to_string(),
        "--port".to_string(),
        port.to_string(),
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    crate::serve::run(&args, None, &mut stdout, &mut stderr) == 0
        && String::from_utf8_lossy(&stdout).contains(r#""status": "running""#)
}

fn service_uninstall(launcher: &AutoLaunch, config: &ServiceConfig, dry_run: bool) -> JsonValue {
    if dry_run {
        let mut warnings = config.warnings.clone();
        warnings.push("Dry run: auto-launch disable was not called.".to_string());
        warnings.extend(cleanup_legacy_autostart(config, true).warnings);
        return report(
            "uninstall",
            true,
            enabled_or_false(launcher),
            config,
            warnings,
            None,
        );
    }
    match launcher.disable() {
        Ok(()) => {
            let cleanup = cleanup_legacy_autostart(config, false);
            let enabled = enabled_or_false(launcher);
            let mut warnings = config.warnings.clone();
            warnings.extend(cleanup.warnings);
            report(
                "uninstall",
                !enabled && cleanup.ok,
                enabled,
                config,
                warnings,
                if cleanup.ok {
                    None
                } else {
                    Some(
                        "auto-launch disable completed but legacy autostart cleanup failed"
                            .to_string(),
                    )
                },
            )
        }
        Err(error) => report(
            "uninstall",
            false,
            enabled_or_false(launcher),
            config,
            config.warnings.clone(),
            Some(error.to_string()),
        ),
    }
}

struct LegacyCleanup {
    ok: bool,
    warnings: Vec<String>,
}

fn cleanup_legacy_autostart(config: &ServiceConfig, dry_run: bool) -> LegacyCleanup {
    match legacy_autostart_present(config) {
        Ok(false) => LegacyCleanup {
            ok: true,
            warnings: Vec::new(),
        },
        Ok(true) if dry_run => LegacyCleanup {
            ok: true,
            warnings: vec![format!(
                "Legacy '{}' autostart entry is present; dry run did not remove it.",
                LEGACY_APP_NAME
            )],
        },
        Ok(true) => match build_legacy_launcher(config) {
            Ok(legacy_launcher) => match legacy_launcher.disable() {
                Ok(()) => LegacyCleanup {
                    ok: true,
                    warnings: vec![format!(
                        "Removed legacy '{}' autostart entry so only '{}' launches at login.",
                        LEGACY_APP_NAME, APP_NAME
                    )],
                },
                Err(error) => LegacyCleanup {
                    ok: false,
                    warnings: vec![format!(
                        "Failed to remove legacy '{}' autostart entry: {}",
                        LEGACY_APP_NAME, error
                    )],
                },
            },
            Err(error) => LegacyCleanup {
                ok: false,
                warnings: vec![format!(
                    "Failed to build legacy '{}' autostart cleanup handle: {}",
                    LEGACY_APP_NAME, error
                )],
            },
        },
        Err(error) => LegacyCleanup {
            ok: false,
            warnings: vec![format!(
                "Failed to inspect legacy '{}' autostart entry: {}",
                LEGACY_APP_NAME, error
            )],
        },
    }
}

fn service_status(launcher: &AutoLaunch, config: &ServiceConfig) -> JsonValue {
    match launcher.is_enabled() {
        Ok(enabled) => report(
            "status",
            true,
            enabled,
            config,
            config.warnings.clone(),
            None,
        ),
        Err(error) => report(
            "status",
            false,
            false,
            config,
            config.warnings.clone(),
            Some(error.to_string()),
        ),
    }
}

fn service_print(config: &ServiceConfig) -> JsonValue {
    report("print", true, false, config, config.warnings.clone(), None)
}

fn service_verify(launcher: &AutoLaunch, config: &ServiceConfig) -> JsonValue {
    let (enabled, enabled_query_error) = match launcher.is_enabled() {
        Ok(value) => (value, None),
        Err(error) => (false, Some(error.to_string())),
    };
    let checks = service_verification_checks(config, enabled);
    let ok = checks.iter().all(|check| {
        check
            .get("ok")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
    });
    let mut warnings = config.warnings.clone();
    if let Some(error) = enabled_query_error {
        warnings.push(format!("failed to query autostart state: {}", error));
    }
    let mut result = report(
        "verify",
        ok,
        enabled,
        config,
        warnings,
        if ok {
            None
        } else {
            Some("autostart verification failed".to_string())
        },
    );
    if let Some(map) = result.as_object_mut() {
        map.insert(
            "verification".to_string(),
            JsonValue::object([
                ("schema", JsonValue::string("mcpace.autostartVerify.v1")),
                ("ok", JsonValue::bool(ok)),
                ("checks", JsonValue::array(checks)),
                ("appliedState", service_applied_state_json(config)),
                (
                    "healthCheckHint",
                    JsonValue::string("run `mcpace agent status --json` or `mcpace serve status --json` after login/reboot to verify the runtime PID and health endpoint"),
                ),
            ]),
        );
    }
    result
}

fn service_verification_checks(config: &ServiceConfig, enabled: bool) -> Vec<JsonValue> {
    let persistent_env_mismatches = crate::persistent_env::current_env_registry_mismatches();
    let (legacy_absent, legacy_detail) = legacy_autostart_absent_detail(config);
    let current_endpoint_live = current_session_endpoint_listening(config);
    vec![
        verification_check(
            "autostart-enabled",
            enabled,
            if enabled {
                "autostart entry is enabled"
            } else {
                "autostart entry is not enabled"
            },
        ),
        verification_check(
            "target-executable-exists",
            Path::new(&config.target_app_path).is_file(),
            "autostart target executable exists at targetAppPath",
        ),
        verification_check(
            "launches-mcpace-agent",
            launches_mcpace_agent(config),
            "autostart must launch `mcpace agent start/run --autostart`, not serve directly",
        ),
        verification_check(
            "direct-mcpace-entry",
            !config.app_path.to_ascii_lowercase().contains("wscript"),
            "autostart entry should point at MCPace, not Windows Script Host",
        ),
        verification_check(
            "legacy-autostart-entry-removed",
            legacy_absent,
            &legacy_detail,
        ),
        verification_check(
            "root-forwarded",
            target_arg_value(&config.target_args, "--root").is_some(),
            "MCPace Agent receives an explicit root path for login startup",
        ),
        verification_check(
            "root-path-valid",
            target_root_path_valid(config),
            "forwarded --root points at a directory containing mcpace.config.json",
        ),
        verification_check(
            "resource-args-forwarded",
            resources_forwarded(config),
            "resource-control flags are forwarded to MCPace Agent when configured",
        ),
        verification_check(
            "current-session-endpoint-listening",
            current_endpoint_live,
            if current_endpoint_live {
                "current MCPace HTTP endpoint is listening for MCP clients"
            } else {
                "current MCPace HTTP endpoint is not listening; run `mcpace autostart repair --root <project>` or `mcpace serve start --root <project>`"
            },
        ),
        verification_check(
            "windows-persistent-env-aligned",
            persistent_env_mismatches.is_empty(),
            &persistent_env_alignment_detail(&persistent_env_mismatches),
        ),
    ]
}

fn legacy_autostart_absent_detail(config: &ServiceConfig) -> (bool, String) {
    match legacy_autostart_present(config) {
        Ok(false) => (
            true,
            format!(
                "legacy '{}' autostart entry is absent; '{}' is the only MCPace login entry",
                LEGACY_APP_NAME, APP_NAME
            ),
        ),
        Ok(true) => (
            false,
            format!(
                "legacy '{}' autostart entry is still present and should be removed",
                LEGACY_APP_NAME
            ),
        ),
        Err(error) => (
            false,
            format!(
                "failed to inspect legacy '{}' autostart entry: {}",
                LEGACY_APP_NAME, error
            ),
        ),
    }
}

#[cfg(windows)]
fn legacy_autostart_present(_config: &ServiceConfig) -> Result<bool, String> {
    windows_run_value(LEGACY_APP_NAME).map(|value| value.is_some())
}

#[cfg(not(windows))]
fn legacy_autostart_present(config: &ServiceConfig) -> Result<bool, String> {
    build_legacy_launcher(config)
        .map_err(|error| error.to_string())?
        .is_enabled()
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
fn windows_run_value(value_name: &str) -> Result<Option<String>, String> {
    let mut command = std::process::Command::new("reg");
    command.args([
        "query",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
        "/v",
        value_name,
    ]);
    crate::windows_process::configure_no_window(&mut command);
    let output = command
        .output()
        .map_err(|error| format!("failed to query Windows Run registry: {}", error))?;
    if !output.status.success() {
        return Ok(None);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix(value_name) else {
            continue;
        };
        let rest = rest.trim_start();
        let mut parts = rest.splitn(2, char::is_whitespace);
        let value_type = parts.next().unwrap_or_default();
        if !value_type.starts_with("REG_") {
            continue;
        }
        return Ok(Some(
            parts.next().unwrap_or_default().trim_start().to_string(),
        ));
    }
    Ok(None)
}

fn launches_mcpace_agent(config: &ServiceConfig) -> bool {
    config
        .target_args
        .first()
        .is_some_and(|value| value == "agent")
        && config
            .target_args
            .get(1)
            .is_some_and(|value| value == "run" || value == "start")
        && config
            .target_args
            .iter()
            .any(|value| value == "--autostart")
}

fn resources_forwarded(config: &ServiceConfig) -> bool {
    [
        "--max-connections",
        "--io-timeout-ms",
        "--max-body-bytes",
        "--overview-cache-ms",
    ]
    .iter()
    .all(|flag| {
        !config.target_args.iter().any(|value| value == *flag)
            || target_arg_value(&config.target_args, flag).is_some()
    })
}

fn target_arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|items| items[0] == flag)
        .map(|items| items[1].as_str())
}

fn target_root_path_valid(config: &ServiceConfig) -> bool {
    let Some(root) = target_arg_value(&config.target_args, "--root") else {
        return false;
    };
    crate::reporoot::has_root_markers(Path::new(root))
}

fn persistent_env_alignment_detail(mismatches: &[String]) -> String {
    if mismatches.is_empty() {
        if cfg!(windows) {
            return format!(
                "Windows login agent hydrates persistent path environment keys from registry when present: {}",
                crate::persistent_env::LOGIN_ENV_KEYS.join(", ")
            );
        }
        return "persistent Windows registry environment hydration is not required on this platform".to_string();
    }
    format!(
        "current process has MCPace path environment values that are not available to Windows login startup: {}",
        mismatches.join("; ")
    )
}

fn verification_check(name: &str, ok: bool, detail: &str) -> JsonValue {
    JsonValue::object([
        ("name", JsonValue::string(name)),
        ("ok", JsonValue::bool(ok)),
        ("detail", JsonValue::string(detail)),
    ])
}

fn service_applied_state_json(config: &ServiceConfig) -> JsonValue {
    let (manager, visible_in, supervised_by_os) = if cfg!(target_os = "linux") {
        ("XDG Autostart", "desktop session autostart settings", false)
    } else if cfg!(target_os = "macos") {
        ("launchd LaunchAgent", "Login Items / LaunchAgents", true)
    } else if cfg!(windows) {
        (
            "Windows current-user Run registry",
            "Settings > Apps > Startup / Task Manager Startup apps",
            false,
        )
    } else {
        ("unsupported", "unsupported", false)
    };
    JsonValue::object([
        (
            "schema",
            JsonValue::string("mcpace.autostartAppliedState.v1"),
        ),
        ("platform", JsonValue::string(config.platform.clone())),
        ("manager", JsonValue::string(manager)),
        ("visibleAs", JsonValue::string(APP_NAME)),
        ("visibleIn", JsonValue::string(visible_in)),
        ("supervisedByOs", JsonValue::bool(supervised_by_os)),
        ("supervisedByMcpaceAgent", JsonValue::bool(!cfg!(windows))),
        ("environment", service_environment_json()),
        ("command", command_json(config)),
    ])
}

fn service_environment_json() -> JsonValue {
    JsonValue::object([
        (
            "schema",
            JsonValue::string("mcpace.autostartEnvironment.v1"),
        ),
        ("windowsRegistryHydration", JsonValue::bool(cfg!(windows))),
        (
            "persistentPathKeys",
            JsonValue::array(
                crate::persistent_env::LOGIN_ENV_KEYS
                    .iter()
                    .map(|key| JsonValue::string(*key)),
            ),
        ),
        (
            "detail",
            JsonValue::string(if cfg!(windows) {
                "MCPace Agent reads persistent user/machine registry environment for path-like MCPace settings before starting serve"
            } else {
                "This platform inherits login-manager environment directly; Windows registry hydration is not used"
            }),
        ),
    ])
}

fn report(
    action: &str,
    ok: bool,
    enabled: bool,
    config: &ServiceConfig,
    warnings: Vec<String>,
    error: Option<String>,
) -> JsonValue {
    JsonValue::object([
        ("ok", JsonValue::bool(ok)),
        ("action", JsonValue::string(action)),
        ("enabled", JsonValue::bool(enabled)),
        ("platform", JsonValue::string(config.platform.clone())),
        ("backend", JsonValue::string(config.backend.clone())),
        ("appName", JsonValue::string(APP_NAME)),
        ("legacyAppName", JsonValue::string(LEGACY_APP_NAME)),
        ("appPath", JsonValue::string(config.app_path.clone())),
        ("launchMode", JsonValue::string(config.launch_mode.clone())),
        ("rebootModel", reboot_model_json(config)),
        (
            "args",
            JsonValue::array(config.args.iter().cloned().map(JsonValue::string)),
        ),
        (
            "targetAppPath",
            JsonValue::string(config.target_app_path.clone()),
        ),
        (
            "targetArgs",
            JsonValue::array(config.target_args.iter().cloned().map(JsonValue::string)),
        ),
        (
            "renderedCommand",
            JsonValue::string(rendered_target_command(config)),
        ),
        ("command", command_json(config)),
        (
            "error",
            error.map(JsonValue::string).unwrap_or(JsonValue::Null),
        ),
        (
            "warnings",
            JsonValue::array(warnings.into_iter().map(JsonValue::string)),
        ),
    ])
}

fn command_json(config: &ServiceConfig) -> JsonValue {
    JsonValue::object([
        ("program", JsonValue::string(config.app_path.clone())),
        (
            "args",
            JsonValue::array(config.args.iter().cloned().map(JsonValue::string)),
        ),
        (
            "targetProgram",
            JsonValue::string(config.target_app_path.clone()),
        ),
        (
            "targetArgs",
            JsonValue::array(config.target_args.iter().cloned().map(JsonValue::string)),
        ),
        (
            "rendered",
            JsonValue::string(rendered_target_command(config)),
        ),
        ("displayName", JsonValue::string(APP_NAME)),
        (
            "purpose",
            JsonValue::string("Launches the local MCPace runtime for MCP clients after user login"),
        ),
    ])
}

#[cfg(windows)]
fn rendered_target_command(config: &ServiceConfig) -> String {
    std::iter::once(config.target_app_path.as_str())
        .chain(config.target_args.iter().map(String::as_str))
        .map(crate::windows_process::quote_windows_arg)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(not(windows))]
fn rendered_target_command(config: &ServiceConfig) -> String {
    std::iter::once(config.target_app_path.as_str())
        .chain(config.target_args.iter().map(String::as_str))
        .map(quote_shellish_token)
        .collect::<Vec<_>>()
        .join(" ")
}

fn reboot_model_json(config: &ServiceConfig) -> JsonValue {
    let (starts_after, restart_on_failure, requires_user_session, manager, notes) = if cfg!(
        target_os = "linux"
    ) {
        (
            "desktop-user-login",
            false,
            true,
            "XDG Autostart via auto-launch",
            vec![
                "autostart install writes a user-visible XDG .desktop login item",
                "MCPace Agent owns the managed foreground serve runtime and restart guard",
                "XDG Autostart is intentionally a user-login launcher, not a system service supervisor",
            ],
        )
    } else if cfg!(target_os = "macos") {
        (
            "user-login",
            true,
            true,
            "launchd LaunchAgent via auto-launch",
            vec![
                "LaunchAgent RunAtLoad starts MCPace Agent when the user logs in",
                "MCPace does not install a privileged LaunchDaemon from this command",
                "MCPace Agent runs the managed foreground serve runtime",
            ],
        )
    } else if cfg!(windows) {
        (
            "user-login",
            false,
            true,
            "Windows current-user Run registry via auto-launch",
            vec![
                "the login item starts MCPace Agent after the current user logs in",
                "MCPace Agent starts a hidden background serve runtime and exits",
                "new installs do not use wscript.exe or generated VBS launchers",
                "Windows current-user Run registry launches at login but does not restart the process after a later crash",
            ],
        )
    } else {
        (
            "unsupported",
            false,
            true,
            "unsupported",
            vec!["this target OS is not supported by the autostart backend"],
        )
    };
    JsonValue::object([
        (
            "schema",
            JsonValue::string("mcpace.autostartRebootModel.v1"),
        ),
        ("platform", JsonValue::string(config.platform.clone())),
        ("manager", JsonValue::string(manager)),
        ("startsAfter", JsonValue::string(starts_after)),
        (
            "requiresUserSession",
            JsonValue::bool(requires_user_session),
        ),
        ("restartOnFailure", JsonValue::bool(restart_on_failure)),
        ("restartGuard", JsonValue::bool(true)),
        (
            "supervisionMode",
            JsonValue::string(if cfg!(windows) {
                "mcpace-agent-background-start"
            } else {
                "mcpace-agent-managed-foreground"
            }),
        ),
        ("resourceControls", service_resource_controls_json()),
        (
            "notes",
            JsonValue::array(notes.into_iter().map(JsonValue::string)),
        ),
    ])
}

fn service_resource_controls_json() -> JsonValue {
    if cfg!(target_os = "linux") {
        return JsonValue::object([
            ("manager", JsonValue::string("XDG Autostart")),
            ("osSupervisor", JsonValue::bool(false)),
            ("supervisesRuntime", JsonValue::bool(false)),
            ("note", JsonValue::string("XDG Autostart is visible after desktop login; MCPace Agent and serve restart guard handle MCPace-specific lifecycle checks")),
        ]);
    }
    if cfg!(target_os = "macos") {
        return JsonValue::object([
            ("manager", JsonValue::string("launchd LaunchAgent")),
            ("keepAliveOnFailure", JsonValue::bool(true)),
            ("supervisesRuntime", JsonValue::bool(true)),
        ]);
    }
    if cfg!(windows) {
        return JsonValue::object([
            ("manager", JsonValue::string("Windows current-user Run registry")),
            ("osSupervisor", JsonValue::bool(false)),
            ("supervisesRuntime", JsonValue::bool(false)),
            ("note", JsonValue::string("Current-user Run registry launches at login but does not supervise after a later crash")),
        ]);
    }
    JsonValue::object([("manager", JsonValue::string("unsupported"))])
}

fn enabled_or_false(launcher: &AutoLaunch) -> bool {
    launcher.is_enabled().unwrap_or(false)
}

fn write_text_report(report: &JsonValue, stdout: &mut dyn Write) {
    let action = report
        .get("action")
        .and_then(JsonValue::as_str)
        .unwrap_or("autostart");
    let ok = report
        .get("ok")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let enabled = report
        .get("enabled")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let backend = report
        .get("backend")
        .and_then(JsonValue::as_str)
        .unwrap_or("unknown");
    let _ = writeln!(
        stdout,
        "MCPace autostart {}: {}",
        action,
        if ok { "ok" } else { "blocked" }
    );
    let _ = writeln!(stdout, "Visible as: {}", APP_NAME);
    let _ = writeln!(stdout, "Backend: {}", backend);
    let _ = writeln!(stdout, "Enabled: {}", if enabled { "yes" } else { "no" });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_config_launches_agent_not_serve_or_wscript() {
        let root = PathBuf::from("/tmp/mcpace");
        let config = service_config(&root, "127.0.0.1", 39022, None, None, None, None).unwrap();
        assert_eq!(config.args[0], "agent");
        assert_eq!(config.args[1], if cfg!(windows) { "start" } else { "run" });
        if cfg!(windows) {
            assert_eq!(config.launch_mode, "direct-mcpace-agent-background-start");
        } else {
            assert_eq!(config.launch_mode, "direct-mcpace-agent-foreground-run");
        }
        assert!(config.args.iter().any(|value| value == "--autostart"));
        assert!(!config.app_path.to_ascii_lowercase().contains("wscript"));
        assert!(launches_mcpace_agent(&config));
    }

    #[test]
    fn parse_action_aliases_keep_existing_service_surface_compatible() {
        let args = vec![
            "enable".to_string(),
            "--dry-run".to_string(),
            "--json".to_string(),
        ];
        let parsed = parse_args(&args);
        assert_eq!(parsed.action, "enable");
        assert!(parsed.dry_run);
        assert!(parsed.json_output);
    }

    #[test]
    fn target_arg_value_finds_forwarded_root() {
        let root = PathBuf::from("/tmp/mcpace");
        let config = service_config(&root, "127.0.0.1", 39022, None, None, None, None).unwrap();
        assert_eq!(
            target_arg_value(&config.target_args, "--root"),
            Some("/tmp/mcpace")
        );
    }

    #[test]
    fn autolaunch_tokens_quote_space_and_shell_sensitive_paths() {
        assert_eq!(quote_shellish_token("/tmp/mcpace"), "/tmp/mcpace");
        assert_eq!(
            quote_shellish_token("/tmp/MCPace Demo"),
            "\"/tmp/MCPace Demo\""
        );
        assert_eq!(quote_shellish_token("/tmp/mc$pace"), "\"/tmp/mc\\$pace\"");
    }

    #[test]
    fn service_config_keeps_plain_target_args_but_escapes_autolaunch_args() {
        let root = PathBuf::from("/tmp/MCPace Demo");
        let config = service_config(&root, "127.0.0.1", 39022, None, None, None, None).unwrap();
        assert_eq!(
            target_arg_value(&config.target_args, "--root"),
            Some("/tmp/MCPace Demo")
        );
        if cfg!(any(windows, target_os = "linux")) {
            assert!(config
                .args
                .iter()
                .any(|value| value == "\"/tmp/MCPace Demo\""));
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_autolaunch_token_uses_windows_quoting_without_doubling_path_separators() {
        assert_eq!(
            autolaunch_token(r"C:\Program Files\MCPace\mcpace.exe"),
            r#""C:\Program Files\MCPace\mcpace.exe""#
        );
    }

    #[test]
    fn verification_checks_cover_command_viability_not_just_enabled_state() {
        let root =
            std::env::temp_dir().join(format!("mcpace-service-verify-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("mcpace.config.json"), "{}\n").unwrap();
        let config = service_config(&root, "127.0.0.1", 39022, None, None, None, None).unwrap();

        let checks =
            JsonValue::array(service_verification_checks(&config, false)).to_compact_string();

        assert!(checks.contains("target-executable-exists"), "{}", checks);
        assert!(checks.contains("root-path-valid"), "{}", checks);
        assert!(
            checks.contains("legacy-autostart-entry-removed"),
            "{}",
            checks
        );
        assert!(
            checks.contains("windows-persistent-env-aligned"),
            "{}",
            checks
        );
        assert!(
            checks.contains("current-session-endpoint-listening"),
            "{}",
            checks
        );
        assert!(
            checks.contains(r#""name":"root-path-valid","ok":true"#),
            "{}",
            checks
        );

        let _ = std::fs::remove_dir_all(root);
    }
}
