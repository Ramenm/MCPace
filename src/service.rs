use crate::json::JsonValue;
use crate::resources;
use crate::runtimepaths;
use auto_launch::{
    AutoLaunch, AutoLaunchBuilder, LinuxLaunchMode, MacOSLaunchMode, WindowsEnableMode,
};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const APP_NAME: &str = "MCPace";

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
    let root_path = canonicalize_or_original(&root_path);
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
                match resources::parse_positive_usize(value, "service --max-connections") {
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
                match resources::parse_positive_u64(value, "service --io-timeout-ms") {
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
                match resources::parse_positive_usize(value, "service --max-body-bytes") {
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
    let _ = writeln!(stdout, "Usage: mcpace service <install|status|uninstall|print> [--json] [--root <path>] [--host <addr>] [--port <n>] [--max-connections <n>] [--io-timeout-ms <n>] [--max-body-bytes <n>] [--overview-cache-ms <n>] [--dry-run] [--no-enable]");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Uses the auto-launch crate to install user-level autostart without requiring mcpace in PATH.");
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
        "start".to_string(),
        "--root".to_string(),
        command_path_string(root_path),
        "--host".to_string(),
        host.to_string(),
        "--port".to_string(),
        port.to_string(),
    ];
    append_serve_resource_args(
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
            format!(
                "\"{}\"",
                autostart_script_path
                    .map(command_path_string)
                    .unwrap_or_else(|| "mcpace-autostart.vbs".to_string())
            ),
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
    let command_line = windows_command_line(
        std::iter::once(&config.target_app_path).chain(config.target_args.iter()),
    );
    let script = format!(
        "Option Explicit\r\nDim shell\r\nSet shell = CreateObject(\"WScript.Shell\")\r\nshell.Run \"{}\", 0, False\r\n",
        escape_vbscript_string(&command_line)
    );
    fs::write(&script_path, script)
        .map_err(|error| format!("failed to write {}: {}", script_path.display(), error))
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
fn remove_autostart_script(_config: &ServiceConfig) {}

#[cfg(windows)]
fn windows_command_line<'a, I>(args: I) -> String
where
    I: IntoIterator<Item = &'a String>,
{
    args.into_iter()
        .map(|arg| quote_windows_arg(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(windows)]
fn quote_windows_arg(arg: &str) -> String {
    if !arg.is_empty()
        && !arg
            .chars()
            .any(|ch| ch.is_whitespace() || ch == '"' || ch == '\\')
    {
        return arg.to_string();
    }

    let mut quoted = String::from("\"");
    let mut backslashes = 0usize;
    for ch in arg.chars() {
        match ch {
            '\\' => backslashes += 1,
            '"' => {
                quoted.push_str(&"\\".repeat(backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            }
            _ => {
                quoted.push_str(&"\\".repeat(backslashes));
                quoted.push(ch);
                backslashes = 0;
            }
        }
    }
    quoted.push_str(&"\\".repeat(backslashes * 2));
    quoted.push('"');
    quoted
}

#[cfg(windows)]
fn escape_vbscript_string(value: &str) -> String {
    value.replace('"', "\"\"")
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
    let mut builder = AutoLaunchBuilder::new();
    builder
        .set_app_name(APP_NAME)
        .set_app_path(&config.app_path)
        .set_args(&config.args)
        .set_macos_launch_mode(MacOSLaunchMode::LaunchAgent)
        .set_agent_extra_config("<key>KeepAlive</key><false/>")
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
        Ok(()) => report(
            "install",
            true,
            enabled_or_false(launcher),
            config,
            config.warnings.clone(),
            None,
        ),
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

fn canonicalize_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
