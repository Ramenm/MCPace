use crate::json::JsonValue;
use crate::resources;
use crate::runtimepaths;
use auto_launch::{
    AutoLaunch, AutoLaunchBuilder, LinuxLaunchMode, MacOSLaunchMode, WindowsEnableMode,
};
#[cfg(target_os = "linux")]
use std::collections::BTreeMap;
#[cfg(windows)]
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) const APP_NAME: &str = "MCPace";

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
    autostart_script_path: Option<String>,
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
        "install" => service_install(
            &launcher,
            &config,
            parsed.dry_run || parsed.no_enable,
            stderr,
        ),
        "uninstall" => service_uninstall(&launcher, &config, parsed.dry_run),
        "status" => service_status(&launcher, &config),
        "verify" => service_verify(&launcher, &config),
        "print" => service_print(&config),
        other => {
            let _ = writeln!(stderr, "unsupported service action: {}", other);
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
                    parsed.error = Some("service requires a path after --root".to_string());
                    return parsed;
                };
                parsed.root_override = Some(PathBuf::from(value));
                index += 2;
            }
            "--host" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("service requires a value after --host".to_string());
                    return parsed;
                };
                parsed.host = value.to_string();
                index += 2;
            }
            "--port" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("service requires a value after --port".to_string());
                    return parsed;
                };
                match value.parse::<u16>() {
                    Ok(port) => parsed.port = port,
                    Err(_) => {
                        parsed.error = Some("service --port must be a valid u16".to_string());
                        return parsed;
                    }
                }
                index += 2;
            }
            "--max-connections" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("service requires a value after --max-connections".to_string());
                    return parsed;
                };
                match resources::parse_http_connection_limit(value, "service --max-connections") {
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
                        Some("service requires a value after --io-timeout-ms".to_string());
                    return parsed;
                };
                match resources::parse_http_io_timeout_ms(value, "service --io-timeout-ms") {
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
                        Some("service requires a value after --max-body-bytes".to_string());
                    return parsed;
                };
                match resources::parse_http_body_limit(value, "service --max-body-bytes") {
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
                        Some("service requires a value after --overview-cache-ms".to_string());
                    return parsed;
                };
                match resources::parse_nonnegative_u64(value, "service --overview-cache-ms") {
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
                parsed.error = Some(format!("unsupported service argument: {}", other));
                return parsed;
            }
        }
    }
    parsed
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "Usage: mcpace service <install|status|verify|uninstall|print> [--json] [--root <path>] [--host <addr>] [--port <n>] [--max-connections <n>] [--io-timeout-ms <n>] [--max-body-bytes <n>] [--overview-cache-ms <n>] [--dry-run] [--no-enable]");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Uses the auto-launch crate to install user-level autostart without requiring mcpace in PATH.");
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
    let mut target_args = vec![
        "serve".to_string(),
        "--managed-service".to_string(),
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
    let autostart_script_path = autostart_script_path(root_path);
    let (app_path, args, launch_mode) = autostart_launcher_command(
        &target_app_path,
        &target_args,
        autostart_script_path.as_deref(),
    );
    let platform = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unsupported"
    };
    let backend = if cfg!(windows) {
        "auto-launch/windows-current-user-registry"
    } else if cfg!(target_os = "macos") {
        "auto-launch/macos-launch-agent"
    } else if cfg!(target_os = "linux") {
        "auto-launch/linux-systemd-user"
    } else {
        "auto-launch/unsupported"
    };
    let mut warnings = vec![
        "Autostart is user-level by default; it does not install privileged system services."
            .to_string(),
    ];
    if cfg!(target_os = "linux") {
        warnings.push(
            "Linux autostart is a user-level systemd unit: it starts after the user session is available; boot-before-login requires systemd user lingering outside MCPace."
                .to_string(),
        );
    }
    if cfg!(target_os = "macos") {
        warnings.push(
            "macOS autostart is a LaunchAgent: it starts at user login and is not a privileged LaunchDaemon."
                .to_string(),
        );
    }
    if cfg!(windows) {
        warnings.push(
            "Windows autostart uses wscript.exe with a generated hidden launcher script so login does not show a console window."
                .to_string(),
        );
    }
    if !AutoLaunch::is_support() {
        warnings.push("auto-launch does not support this target OS.".to_string());
    }
    Ok(ServiceConfig {
        app_path,
        args,
        target_app_path,
        target_args,
        autostart_script_path: autostart_script_path.map(|path| command_path_string(&path)),
        launch_mode,
        platform: platform.to_string(),
        backend: backend.to_string(),
        warnings,
    })
}

#[cfg(windows)]
fn autostart_script_path(root_path: &Path) -> Option<PathBuf> {
    let state_root = runtimepaths::resolve_state_root(root_path);
    Some(
        runtimepaths::runtime_dir(&state_root)
            .join("service")
            .join("mcpace-autostart.vbs"),
    )
}

#[cfg(not(windows))]
fn autostart_script_path(_root_path: &Path) -> Option<PathBuf> {
    None
}

#[cfg(windows)]
fn autostart_launcher_command(
    _target_app_path: &str,
    _target_args: &[String],
    autostart_script_path: Option<&Path>,
) -> (String, Vec<String>, String) {
    (
        resolve_wscript_path(),
        vec![
            "//B".to_string(),
            "//Nologo".to_string(),
            autostart_script_path
                .map(command_path_string)
                .unwrap_or_else(|| "mcpace-autostart.vbs".to_string()),
        ],
        "windows-hidden-wscript-launcher".to_string(),
    )
}

#[cfg(not(windows))]
fn autostart_launcher_command(
    target_app_path: &str,
    target_args: &[String],
    _autostart_script_path: Option<&Path>,
) -> (String, Vec<String>, String) {
    (
        target_app_path.to_string(),
        target_args.to_vec(),
        "direct".to_string(),
    )
}

#[cfg(windows)]
fn resolve_wscript_path() -> String {
    std::env::var_os("SystemRoot")
        .map(|root| {
            PathBuf::from(root)
                .join("System32")
                .join("wscript.exe")
                .display()
                .to_string()
        })
        .unwrap_or_else(|| "wscript.exe".to_string())
}

#[cfg(windows)]
fn write_autostart_script(config: &ServiceConfig) -> Result<(), String> {
    let Some(script_path) = config.autostart_script_path.as_ref().map(PathBuf::from) else {
        return Ok(());
    };
    if let Some(parent) = script_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {}", parent.display(), error))?;
    }
    let command_line = crate::windows_process::windows_command_line_from_strs(
        std::iter::once(config.target_app_path.as_str())
            .chain(config.target_args.iter().map(String::as_str)),
    );
    let env_bootstrap = windows_autostart_env_bootstrap_script();
    let script = format!(
        "Option Explicit\r\nDim shell\r\nSet shell = CreateObject(\"WScript.Shell\")\r\n{}\r\nshell.Run \"{}\", 0, False\r\n",
        env_bootstrap,
        escape_vbscript_string(&command_line)
    );
    runtimepaths::write_text_atomic(&script_path, &script)
        .map_err(|error| format!("failed to write {}: {}", script_path.display(), error))
}

#[cfg(windows)]
fn autostart_script_matches_config(config: &ServiceConfig) -> bool {
    let Some(script_path) = config.autostart_script_path.as_ref().map(PathBuf::from) else {
        return true;
    };
    let Ok(script) = fs::read_to_string(script_path) else {
        return false;
    };
    let mut required_fragments = vec![
        escaped_windows_launcher_arg(&config.target_app_path),
        escaped_windows_launcher_arg("serve"),
        escaped_windows_launcher_arg("--managed-service"),
    ];
    for flag in ["--root", "--host", "--port"] {
        let Some(value) = target_arg_value(&config.target_args, flag) else {
            return false;
        };
        required_fragments.push(escaped_windows_launcher_arg(flag));
        required_fragments.push(escaped_windows_launcher_arg(value));
    }
    required_fragments
        .iter()
        .all(|fragment| script.contains(fragment))
}

#[cfg(windows)]
fn escaped_windows_launcher_arg(value: &str) -> String {
    escape_vbscript_string(&crate::windows_process::quote_windows_arg(value))
}

#[cfg(windows)]
fn target_arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|items| items[0] == flag)
        .map(|items| items[1].as_str())
}

#[cfg(windows)]
fn remove_autostart_script(config: &ServiceConfig) {
    if let Some(script_path) = config.autostart_script_path.as_ref().map(PathBuf::from) {
        let _ = fs::remove_file(script_path);
    }
}

#[cfg(not(windows))]
fn write_autostart_script(_config: &ServiceConfig) -> Result<(), String> {
    Ok(())
}

#[cfg(not(windows))]
fn autostart_script_matches_config(_config: &ServiceConfig) -> bool {
    true
}

#[cfg(not(windows))]
fn remove_autostart_script(_config: &ServiceConfig) {}

#[cfg(windows)]
fn escape_vbscript_string(value: &str) -> String {
    value.replace('"', "\"\"")
}

#[cfg(windows)]
fn windows_autostart_env_bootstrap_script() -> String {
    let mut script = String::from(
        "Sub LoadEnvValue(name)\r\n  Dim value\r\n  On Error Resume Next\r\n  value = shell.RegRead(\"HKCU\\Environment\\\" & name)\r\n  If Err.Number <> 0 Then\r\n    Err.Clear\r\n    value = shell.RegRead(\"HKLM\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment\\\" & name)\r\n  End If\r\n  If Err.Number = 0 Then\r\n    shell.Environment(\"Process\")(name) = value\r\n  End If\r\n  Err.Clear\r\n  On Error GoTo 0\r\nEnd Sub\r\n",
    );
    for name in WINDOWS_AUTOSTART_ENV_KEYS {
        script.push_str("LoadEnvValue \"");
        script.push_str(name);
        script.push_str("\"\r\n");
    }
    script
}

#[cfg(windows)]
const WINDOWS_AUTOSTART_ENV_KEYS: &[&str] = &[
    "MCPACE_MCP_SETTINGS",
    "MCPACE_MCP_SETTINGS_DIRS",
    "MCPACE_TOOL_EXPOSURE",
    "MCPACE_UPSTREAM_TOOL_EXPOSURE",
    "MCPACE_TOOLS_LIST_TIMEOUT_MS",
    "MCPACE_TOOL_BUDGET",
    "MCPACE_NATIVE_TOOL_BUDGET",
    "MCPACE_TOOL_TOKEN_BUDGET",
    "MCPACE_NATIVE_TOOL_TOKEN_BUDGET",
    "MCPACE_PROJECTED_TOOL_SAFETY",
    "MCPACE_TOOL_PROJECTION_SAFETY",
    "MCPACE_MANAGEMENT_SURFACE",
    "MCPACE_ALLOW_FULL_MANAGEMENT",
    "MCPACE_STATE_ROOT",
    "MCPACE_RUNTIME_PROFILE",
    "MCPACE_BEARER_TOKEN",
    "MCPHUB_BEARER_TOKEN",
    "MCPACE_ADMIN_PASSWORD_BCRYPT",
    "MCPACE_HTTP_AUTH_TOKEN",
];

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

fn service_manager_extra_config() -> String {
    if cfg!(target_os = "linux") {
        let mut lines = vec![
            "MemoryAccounting=yes".to_string(),
            "TasksMax=256".to_string(),
            "LimitNOFILE=4096".to_string(),
        ];
        if let Some(memory_max) = safe_systemd_resource_value("MCPACE_SERVICE_MEMORY_MAX") {
            lines.push(format!("MemoryMax={}", memory_max));
        }
        if let Some(cpu_quota) = safe_systemd_resource_value("MCPACE_SERVICE_CPU_QUOTA") {
            lines.push(format!("CPUQuota={}", cpu_quota));
        }
        return lines.join("\n");
    }
    if cfg!(target_os = "macos") {
        return [
            "<key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>",
            "<key>ThrottleInterval</key><integer>10</integer>",
        ]
        .join("");
    }
    String::new()
}

fn safe_systemd_resource_value(env_name: &str) -> Option<String> {
    let value = std::env::var(env_name).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.len() > 32
        || !trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '%' | '.' | '_'))
    {
        return None;
    }
    Some(trimmed.to_string())
}

fn build_launcher(config: &ServiceConfig) -> auto_launch::Result<AutoLaunch> {
    let mut builder = AutoLaunchBuilder::new();
    builder
        .set_app_name(APP_NAME)
        .set_app_path(&config.app_path)
        .set_args(&config.args)
        .set_macos_launch_mode(MacOSLaunchMode::LaunchAgent)
        .set_agent_extra_config(&service_manager_extra_config())
        .set_windows_enable_mode(WindowsEnableMode::CurrentUser)
        .set_linux_launch_mode(LinuxLaunchMode::Systemd);
    builder.build()
}

fn service_install(
    launcher: &AutoLaunch,
    config: &ServiceConfig,
    dry_run: bool,
    _stderr: &mut dyn Write,
) -> JsonValue {
    if dry_run {
        let mut warnings = config.warnings.clone();
        warnings.push("Dry run / --no-enable: auto-launch enable was not called.".to_string());
        return report("install", true, false, config, warnings, None);
    }
    if let Err(error) = write_autostart_script(config) {
        return report(
            "install",
            false,
            enabled_or_false(launcher),
            config,
            config.warnings.clone(),
            Some(error),
        );
    }
    match launcher.enable() {
        Ok(()) => {
            let mut warnings = config.warnings.clone();
            warnings.extend(platform_post_install_activation_warnings(config));
            report(
                "install",
                true,
                enabled_or_false(launcher),
                config,
                warnings,
                None,
            )
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

fn service_uninstall(launcher: &AutoLaunch, config: &ServiceConfig, dry_run: bool) -> JsonValue {
    if dry_run {
        let mut warnings = config.warnings.clone();
        warnings.push("Dry run: auto-launch disable was not called.".to_string());
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
            remove_autostart_script(config);
            report(
                "uninstall",
                true,
                enabled_or_false(launcher),
                config,
                config.warnings.clone(),
                None,
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
    let script_exists = config
        .autostart_script_path
        .as_ref()
        .map(|path| Path::new(path).exists())
        .unwrap_or(true);
    let mut checks = Vec::new();
    checks.push(verification_check(
        "autostart-enabled",
        enabled,
        if enabled {
            "autostart entry is enabled"
        } else {
            "autostart entry is not enabled"
        },
    ));
    checks.push(verification_check(
        "foregroundManagedService",
        config
            .target_args
            .windows(2)
            .any(|items| items[0] == "serve" && items[1] == "--managed-service"),
        "service ExecStart must run mcpace serve --managed-service instead of a detached child",
    ));
    checks.push(verification_check(
        "launcher-script-present",
        script_exists,
        "platform autostart launcher script exists when this backend uses one",
    ));
    checks.push(verification_check(
        "launcher-script-current",
        autostart_script_matches_config(config),
        "platform autostart launcher script must target the current mcpace executable and managed-service arguments",
    ));
    checks.push(verification_check(
        "restartGuardDocumented",
        true,
        "managed serve uses the same restart guard as manual serve start",
    ));
    checks.push(verification_check(
        "resourceControlsDeclared",
        service_resource_controls_declared(config),
        "service report contains resource-control metadata for the current platform",
    ));
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
            Some("service verification failed".to_string())
        },
    );
    if let Some(map) = result.as_object_mut() {
        map.insert(
            "verification".to_string(),
            JsonValue::object([
                ("schema", JsonValue::string("mcpace.serviceVerify.v1")),
                ("ok", JsonValue::bool(ok)),
                ("checks", JsonValue::array(checks)),
                ("appliedState", service_applied_state_json(config)),
                (
                    "healthCheckHint",
                    JsonValue::string("run `mcpace serve status --json` after login/reboot to verify the managed runtime PID and health endpoint"),
                ),
            ]),
        );
    }
    result
}

fn verification_check(name: &str, ok: bool, detail: &str) -> JsonValue {
    JsonValue::object([
        ("name", JsonValue::string(name)),
        ("ok", JsonValue::bool(ok)),
        ("detail", JsonValue::string(detail)),
    ])
}

fn service_applied_state_json(config: &ServiceConfig) -> JsonValue {
    let _ = config;
    #[cfg(target_os = "linux")]
    {
        linux_systemd_user_applied_state_json(config)
    }
    #[cfg(target_os = "macos")]
    {
        JsonValue::object([
            ("schema", JsonValue::string("mcpace.serviceAppliedState.v1")),
            ("available", JsonValue::bool(false)),
            ("platform", JsonValue::string("macos")),
            ("manager", JsonValue::string("launchd LaunchAgent")),
            ("reason", JsonValue::string("launchctl applied-state probing is not implemented in this source-only build; use launchctl print gui/$UID/MCPace")),
        ])
    }
    #[cfg(windows)]
    {
        JsonValue::object([
            ("schema", JsonValue::string("mcpace.serviceAppliedState.v1")),
            ("available", JsonValue::bool(false)),
            ("platform", JsonValue::string("windows")),
            (
                "manager",
                JsonValue::string("Windows current-user Run registry + hidden wscript launcher"),
            ),
            ("reason", JsonValue::string("Current-user Run registry mode has no Service Control Manager applied state; use mcpace serve status --json after login")),
        ])
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = config;
        JsonValue::object([
            ("schema", JsonValue::string("mcpace.serviceAppliedState.v1")),
            ("available", JsonValue::bool(false)),
            ("platform", JsonValue::string("unsupported")),
        ])
    }
}

#[cfg(target_os = "linux")]
fn linux_systemd_user_applied_state_json(config: &ServiceConfig) -> JsonValue {
    let _ = config;
    let unit = service_unit_name();
    let properties = [
        "Id",
        "LoadState",
        "ActiveState",
        "SubState",
        "UnitFileState",
        "ExecStart",
        "MainPID",
        "NRestarts",
        "MemoryAccounting",
        "TasksMax",
        "LimitNOFILE",
    ];
    let output = std::process::Command::new("systemctl")
        .arg("--user")
        .arg("show")
        .arg(&unit)
        .args(properties.iter().flat_map(|property| ["-p", *property]))
        .output();
    match output {
        Ok(output) if output.status.success() => {
            let parsed = parse_systemctl_show(&String::from_utf8_lossy(&output.stdout));
            JsonValue::object([
                ("schema", JsonValue::string("mcpace.serviceAppliedState.v1")),
                ("available", JsonValue::bool(true)),
                ("manager", JsonValue::string("systemd --user")),
                ("unit", JsonValue::string(unit)),
                ("properties", systemctl_properties_json(parsed)),
            ])
        }
        Ok(output) => {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            JsonValue::object([
                ("schema", JsonValue::string("mcpace.serviceAppliedState.v1")),
                ("available", JsonValue::bool(false)),
                ("manager", JsonValue::string("systemd --user")),
                ("unit", JsonValue::string(unit)),
                (
                    "reason",
                    JsonValue::string(if detail.is_empty() {
                        format!("systemctl exited with {}", output.status)
                    } else {
                        detail
                    }),
                ),
            ])
        }
        Err(error) => JsonValue::object([
            ("schema", JsonValue::string("mcpace.serviceAppliedState.v1")),
            ("available", JsonValue::bool(false)),
            ("manager", JsonValue::string("systemd --user")),
            ("unit", JsonValue::string(unit)),
            (
                "reason",
                JsonValue::string(format!(
                    "failed to execute systemctl --user show: {}",
                    error
                )),
            ),
        ]),
    }
}

#[cfg(target_os = "linux")]
fn parse_systemctl_show(output: &str) -> BTreeMap<String, String> {
    output
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn systemctl_properties_json(properties: BTreeMap<String, String>) -> JsonValue {
    JsonValue::object(
        properties
            .into_iter()
            .map(|(key, value)| (key, JsonValue::string(value))),
    )
}

fn service_resource_controls_declared(config: &ServiceConfig) -> bool {
    if cfg!(target_os = "linux") {
        return config.backend.contains("systemd");
    }
    if cfg!(target_os = "macos") {
        return config.backend.contains("launch-agent");
    }
    if cfg!(windows) {
        return config.backend.contains("windows");
    }
    false
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
            "autostartScriptPath",
            config
                .autostart_script_path
                .clone()
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
        (
            "targetArgs",
            JsonValue::array(config.target_args.iter().cloned().map(JsonValue::string)),
        ),
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

fn reboot_model_json(config: &ServiceConfig) -> JsonValue {
    let (starts_after, restart_on_failure, requires_user_session, manager, notes) = if cfg!(
        target_os = "linux"
    ) {
        (
            "user-login-or-systemd-user-session",
            true,
            true,
            "systemd --user",
            vec![
                "service install writes a user unit and attempts systemctl --user daemon-reload/enable when available",
                "after a full reboot it starts when the user session manager is available; enable lingering if it must start before login",
                "service autostart runs mcpace serve in foreground managed mode so the service manager owns restart/resource controls",
                "serve managed mode still writes a state file and uses a restart guard to avoid fast crash loops consuming all resources",
            ],
        )
    } else if cfg!(target_os = "macos") {
        (
            "user-login",
            true,
            true,
            "launchd LaunchAgent",
            vec![
                "LaunchAgent RunAtLoad starts MCPace when the user logs in",
                "MCPace does not install a privileged LaunchDaemon from this command",
                "service autostart runs mcpace serve in foreground managed mode so launchd owns the runtime process",
                "launchd throttles repeated failed starts; MCPace also keeps its own restart guard",
            ],
        )
    } else if cfg!(windows) {
        (
            "user-login",
            false,
            true,
            "Windows current-user Run registry",
            vec![
                "the generated startup launcher runs after the user logs in",
                "MCPace uses a hidden wscript wrapper to avoid a visible console window",
                "service autostart runs mcpace serve in foreground managed mode through a hidden wscript launcher",
                "Windows current-user Run registry autostart does not supervise the process after login; use mcpace serve status and restart manually if it exits",
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
        ("schema", JsonValue::string("mcpace.serviceRebootModel.v1")),
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
            JsonValue::string("foreground-managed-service"),
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
            ("manager", JsonValue::string("systemd --user")),
            ("memoryAccounting", JsonValue::bool(true)),
            ("tasksMax", JsonValue::number(256)),
            ("limitNoFile", JsonValue::number(4096)),
            (
                "memoryMax",
                safe_systemd_resource_value("MCPACE_SERVICE_MEMORY_MAX")
                    .map(JsonValue::string)
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "cpuQuota",
                safe_systemd_resource_value("MCPACE_SERVICE_CPU_QUOTA")
                    .map(JsonValue::string)
                    .unwrap_or(JsonValue::Null),
            ),
        ]);
    }
    if cfg!(target_os = "macos") {
        return JsonValue::object([
            ("manager", JsonValue::string("launchd LaunchAgent")),
            ("keepAliveOnFailure", JsonValue::bool(true)),
            ("throttleIntervalSec", JsonValue::number(10)),
        ]);
    }
    if cfg!(windows) {
        return JsonValue::object([
            (
                "manager",
                JsonValue::string("Windows current-user Run registry"),
            ),
            ("supervisesRuntime", JsonValue::bool(false)),
            (
                "note",
                JsonValue::string("Current-user Run registry launches at login but does not restart the process after a later crash"),
            ),
        ]);
    }
    JsonValue::object([("manager", JsonValue::string("unsupported"))])
}

fn platform_post_install_activation_warnings(_config: &ServiceConfig) -> Vec<String> {
    #[cfg(target_os = "linux")]
    {
        linux_systemd_user_activation_warnings()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

#[cfg(target_os = "linux")]
fn linux_systemd_user_activation_warnings() -> Vec<String> {
    let unit = service_unit_name();
    let mut warnings = Vec::new();
    match run_systemctl_user(&["daemon-reload"]) {
        Ok(()) => warnings.push("systemctl --user daemon-reload completed.".to_string()),
        Err(error) => warnings.push(format!(
            "systemctl --user daemon-reload did not complete; user unit file exists, but systemd may need a manual daemon-reload: {}",
            error
        )),
    }
    match run_systemctl_user(&["enable", &unit]) {
        Ok(()) => warnings.push(format!("systemctl --user enable {} completed.", unit)),
        Err(error) => warnings.push(format!(
            "systemctl --user enable {} did not complete; after reboot/login autostart may require manual enablement: {}",
            unit, error
        )),
    }
    warnings
}

#[cfg(target_os = "linux")]
fn service_unit_name() -> String {
    format!("{}.service", APP_NAME.to_ascii_lowercase())
}

#[cfg(target_os = "linux")]
fn run_systemctl_user(args: &[&str]) -> Result<(), String> {
    let output = std::process::Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .map_err(|error| format!("failed to execute systemctl: {}", error))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr_text = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout_text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr_text.is_empty() {
        stderr_text
    } else if !stdout_text.is_empty() {
        stdout_text
    } else {
        format!("exit status {}", output.status)
    };
    Err(detail)
}

fn enabled_or_false(launcher: &AutoLaunch) -> bool {
    launcher.is_enabled().unwrap_or(false)
}

fn write_text_report(report: &JsonValue, stdout: &mut dyn Write) {
    let action = report
        .get("action")
        .and_then(JsonValue::as_str)
        .unwrap_or("service");
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
        "MCPace service {}: {}",
        action,
        if ok { "ok" } else { "blocked" }
    );
    let _ = writeln!(stdout, "Backend: {}", backend);
    let _ = writeln!(stdout, "Enabled: {}", if enabled { "yes" } else { "no" });
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_script_path() -> PathBuf {
        let unique = format!(
            "mcpace-service-test-{}-{}-{}.vbs",
            std::process::id(),
            crate::runtimepaths::unix_time_ms(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        std::env::temp_dir().join(unique)
    }

    fn test_service_config(script_path: PathBuf) -> ServiceConfig {
        ServiceConfig {
            app_path: resolve_wscript_path(),
            args: vec![
                "//B".to_string(),
                "//Nologo".to_string(),
                command_path_string(&script_path),
            ],
            target_app_path: r"C:\Tools\mcpace.exe".to_string(),
            target_args: vec![
                "serve".to_string(),
                "--managed-service".to_string(),
                "--root".to_string(),
                r"C:\Users\example\Project".to_string(),
                "--host".to_string(),
                "127.0.0.1".to_string(),
                "--port".to_string(),
                "39022".to_string(),
            ],
            autostart_script_path: Some(command_path_string(&script_path)),
            launch_mode: "windows-hidden-wscript-launcher".to_string(),
            platform: "windows".to_string(),
            backend: "auto-launch/windows-current-user-registry".to_string(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn windows_autostart_bootstrap_hydrates_mcpace_environment_from_registry() {
        let script = windows_autostart_env_bootstrap_script();

        assert!(script.contains("HKCU\\Environment\\"));
        assert!(script
            .contains("HKLM\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment\\"));
        assert!(script.contains("shell.Environment(\"Process\")(name) = value"));
        assert!(script.contains("LoadEnvValue \"MCPACE_MCP_SETTINGS\""));
        assert!(script.contains("LoadEnvValue \"MCPACE_MCP_SETTINGS_DIRS\""));
        assert!(script.contains("LoadEnvValue \"MCPACE_TOOL_EXPOSURE\""));
        assert!(script.contains("LoadEnvValue \"MCPACE_TOOLS_LIST_TIMEOUT_MS\""));
    }

    #[test]
    fn windows_service_verify_rejects_stale_autostart_script() {
        let script_path = temp_script_path();
        let config = test_service_config(script_path.clone());
        fs::write(
            &script_path,
            r#"shell.Run """C:\Users\example\.cargo\bin\mcpace.exe"" serve start --root ""C:\Users\example\Project"" --host 127.0.0.1 --port 39022", 0, False"#,
        )
        .unwrap();

        assert!(!autostart_script_matches_config(&config));

        let _ = fs::remove_file(script_path);
    }

    #[test]
    fn windows_autostart_script_matches_current_managed_command() {
        let script_path = temp_script_path();
        let config = test_service_config(script_path.clone());

        write_autostart_script(&config).unwrap();

        assert!(autostart_script_matches_config(&config));

        let _ = fs::remove_file(script_path);
    }

    #[test]
    fn windows_service_verify_allows_installed_resource_arg_superset() {
        let script_path = temp_script_path();
        let config = test_service_config(script_path.clone());
        let mut installed_config = test_service_config(script_path.clone());
        installed_config
            .target_args
            .extend(["--max-connections".to_string(), "32".to_string()]);

        write_autostart_script(&installed_config).unwrap();

        assert!(autostart_script_matches_config(&config));

        let _ = fs::remove_file(script_path);
    }
}
