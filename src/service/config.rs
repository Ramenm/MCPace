#[cfg(target_os = "linux")]
use super::LINUX_AUTOSTART_ID;
use super::{ServiceConfig, ServiceConfigError, ServiceConfigResult};
use crate::resources;
use auto_launch::AutoLaunch;
#[cfg(target_os = "linux")]
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub(super) fn service_config(
    root_path: &Path,
    host: &str,
    port: u16,
    max_connections: Option<usize>,
    io_timeout_ms: Option<u64>,
    max_body_bytes: Option<usize>,
    overview_cache_ms: Option<u64>,
) -> ServiceConfigResult<ServiceConfig> {
    let target_app_path_buf =
        std::env::current_exe().map_err(ServiceConfigError::CurrentExecutable)?;
    let target_app_path = command_path_string(&target_app_path_buf);
    let mut target_args = vec![
        "agent".to_string(),
        "run".to_string(),
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
        "auto-launch/linux-systemd-user"
    } else {
        "auto-launch/unsupported"
    };
    let mut warnings = vec![
        "Autostart is user-level by default; it does not install privileged system services.".to_string(),
        "MCPace autostart launches MCPace Agent natively; the previous Windows wscript/VBS wrapper is not used for new installs.".to_string(),
    ];
    if cfg!(target_os = "linux") {
        warnings.push(
            "Linux autostart uses a systemd user service with Restart=on-failure; it starts with the user's systemd manager and does not require a desktop session.".to_string(),
        );
        warnings.push(
            "Boot-before-login requires systemd user lingering; without lingering MCPace starts when the user session begins.".to_string(),
        );
    }
    if cfg!(target_os = "macos") {
        warnings.push(
            "macOS autostart is a LaunchAgent: it starts at user login and is not a privileged LaunchDaemon.".to_string(),
        );
    }
    if cfg!(windows) {
        warnings.push(
            "Windows autostart uses the current-user Run login item with the visible name MCPace Agent and a hidden launcher sidecar.".to_string(),
        );
    }
    if !AutoLaunch::is_support() {
        warnings.push("auto-launch does not support this target OS.".to_string());
    }

    let autostart_plan_path =
        autostart_plan_path_for_platform().map(|path| command_path_string(&path));
    let autostart_target = autostart_target(
        &target_app_path_buf,
        &target_app_path,
        &target_args,
        autostart_plan_path.as_deref(),
    );
    warnings.extend(autostart_target.warnings.clone());
    let app_path = autolaunch_token(&autostart_target.launch_program_path);
    let args = autostart_target
        .launch_args
        .iter()
        .map(|arg| autolaunch_token(arg))
        .collect::<Vec<_>>();
    Ok(ServiceConfig {
        app_path,
        args,
        launch_program_path: autostart_target.launch_program_path,
        launch_args: autostart_target.launch_args,
        target_app_path,
        target_args,
        launch_mode: autostart_target.launch_mode,
        platform: platform.to_string(),
        backend: backend.to_string(),
        warnings,
        install_blocker: autostart_target.install_blocker,
        native_background: autostart_target.native_background,
        autostart_plan_path,
    })
}

struct AutostartTarget {
    launch_program_path: String,
    launch_args: Vec<String>,
    launch_mode: String,
    warnings: Vec<String>,
    install_blocker: Option<String>,
    native_background: bool,
}

fn autostart_target(
    target_app_path_buf: &Path,
    target_app_path: &str,
    target_args: &[String],
    autostart_plan_path: Option<&str>,
) -> AutostartTarget {
    autostart_target_impl(
        target_app_path_buf,
        target_app_path,
        target_args,
        autostart_plan_path,
    )
}

#[cfg(windows)]
fn autostart_target_impl(
    target_app_path_buf: &Path,
    target_app_path: &str,
    target_args: &[String],
    autostart_plan_path: Option<&str>,
) -> AutostartTarget {
    let hidden_launcher_path = windows_hidden_launcher_path(target_app_path_buf);
    let hidden_launcher = command_path_string(&hidden_launcher_path);
    if hidden_launcher_path.is_file() {
        let mut warnings = vec![format!(
            "Windows login startup uses '{}' to start MCPace Agent without opening a terminal window.",
            hidden_launcher
        )];
        if let Some(path) = autostart_plan_path {
            warnings.push(format!(
                "Windows Run startup command is intentionally short; full MCPace Agent arguments are stored in the per-user autostart plan at '{}'.",
                path
            ));
        }
        return AutostartTarget {
            launch_program_path: hidden_launcher.clone(),
            launch_args: vec!["--from-login".to_string()],
            launch_mode: "windows-native-hidden-launcher-plan".to_string(),
            warnings,
            install_blocker: None,
            native_background: true,
        };
    }

    AutostartTarget {
        launch_program_path: target_app_path.to_string(),
        launch_args: target_args.to_vec(),
        launch_mode: "blocked-missing-windows-hidden-launcher".to_string(),
        warnings: vec![format!(
            "Windows hidden launcher is missing at '{}'; refusing new autostart installs because direct mcpace.exe login startup opens a terminal window.",
            hidden_launcher
        )],
        install_blocker: Some(format!(
            "Windows hidden autostart launcher not found: {}. Build/package mcpace-agent-launcher.exe next to mcpace.exe, then run `mcpace autostart repair`.",
            hidden_launcher
        )),
        native_background: false,
    }
}

#[cfg(windows)]
fn windows_hidden_launcher_path(target_app_path: &Path) -> PathBuf {
    target_app_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("mcpace-agent-launcher.exe")
}

#[cfg(windows)]
fn autostart_plan_path_for_platform() -> Option<PathBuf> {
    Some(windows_autostart_plan_path())
}

#[cfg(not(windows))]
fn autostart_plan_path_for_platform() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
fn windows_autostart_plan_path() -> PathBuf {
    windows_autostart_state_dir().join("autostart-plan.json")
}

#[cfg(windows)]
fn windows_autostart_state_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("TEMP").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("MCPace")
        .join("agent")
}

#[cfg(target_os = "linux")]
fn autostart_target_impl(
    _target_app_path_buf: &Path,
    target_app_path: &str,
    target_args: &[String],
    _autostart_plan_path: Option<&str>,
) -> AutostartTarget {
    let env_program = ["/usr/bin/env", "/bin/env"]
        .into_iter()
        .find(|candidate| Path::new(candidate).is_file());
    let Some(env_program) = env_program else {
        return AutostartTarget {
            launch_program_path: target_app_path.to_string(),
            launch_args: target_args.to_vec(),
            launch_mode: "linux-systemd-direct-agent".to_string(),
            warnings: vec![
                "The env launcher was not found; the systemd user service will use its manager PATH."
                    .to_string(),
            ],
            install_blocker: None,
            native_background: true,
        };
    };

    let mut launch_args = Vec::with_capacity(target_args.len() + 2);
    if let Some(path) = linux_login_path() {
        launch_args.push(format!("PATH={}", path));
    }
    launch_args.push(target_app_path.to_string());
    launch_args.extend_from_slice(target_args);
    AutostartTarget {
        launch_program_path: env_program.to_string(),
        launch_args,
        launch_mode: "linux-systemd-user-env".to_string(),
        warnings: vec![
            "The systemd user service captures the current executable PATH so configured upstream launchers remain available after login."
                .to_string(),
        ],
        install_blocker: None,
        native_background: true,
    }
}

#[cfg(all(not(windows), not(target_os = "linux")))]
fn autostart_target_impl(
    _target_app_path_buf: &Path,
    target_app_path: &str,
    target_args: &[String],
    _autostart_plan_path: Option<&str>,
) -> AutostartTarget {
    AutostartTarget {
        launch_program_path: target_app_path.to_string(),
        launch_args: target_args.to_vec(),
        launch_mode: "direct-mcpace-agent".to_string(),
        warnings: Vec::new(),
        install_blocker: None,
        native_background: true,
    }
}

pub(super) fn autolaunch_token(value: &str) -> String {
    autolaunch_token_impl(value)
}

#[cfg(windows)]
fn autolaunch_token_impl(value: &str) -> String {
    crate::windows_process::quote_windows_arg(value)
}

#[cfg(target_os = "linux")]
fn autolaunch_token_impl(value: &str) -> String {
    quote_systemd_token(value)
}

#[cfg(all(not(windows), not(target_os = "linux")))]
fn autolaunch_token_impl(value: &str) -> String {
    value.to_string()
}

#[cfg(target_os = "linux")]
fn linux_login_path() -> Option<String> {
    let raw = std::env::var_os("PATH")?;
    let drop_windows_mounts =
        std::env::var_os("WSL_DISTRO_NAME").is_some() || std::env::var_os("WSL_INTEROP").is_some();
    normalized_linux_path(&raw, drop_windows_mounts)
}

#[cfg(target_os = "linux")]
pub(super) fn normalized_linux_path(
    raw: &std::ffi::OsStr,
    drop_windows_mounts: bool,
) -> Option<String> {
    let mut seen = HashSet::new();
    let paths = std::env::split_paths(raw)
        .filter(|path| path.is_absolute())
        .filter(|path| !(drop_windows_mounts && path.starts_with("/mnt")))
        .filter(|path| seen.insert(path.clone()))
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return None;
    }
    std::env::join_paths(paths)
        .ok()
        .map(|value| value.to_string_lossy().into_owned())
}

#[cfg(target_os = "linux")]
pub(super) fn quote_systemd_token(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "$$")
        .replace('%', "%%")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("\"{}\"", escaped)
}

#[cfg(any(test, not(windows)))]
pub(super) fn quote_shellish_token(value: &str) -> String {
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

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
pub(super) fn start_user_supervisor_after_enable(
    config: &ServiceConfig,
) -> ServiceConfigResult<()> {
    let endpoint = supervisor_endpoint(config)?;
    if supervisor_runtime_ready(&endpoint) {
        return Ok(());
    }
    stop_runtime_before_supervisor_start(config)?;
    start_platform_user_supervisor(config)?;
    match wait_for_supervisor_runtime(&endpoint) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = stop_runtime_before_supervisor_start(config);
            Err(error)
        }
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub(super) fn start_user_supervisor_after_enable(
    _config: &ServiceConfig,
) -> ServiceConfigResult<()> {
    Ok(())
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn stop_runtime_before_supervisor_start(config: &ServiceConfig) -> ServiceConfigResult<()> {
    let root = service_target_arg(&config.target_args, "--root").ok_or_else(|| {
        ServiceConfigError::Autostart(
            "autostart target does not include the required --root argument".to_string(),
        )
    })?;
    let mut command = std::process::Command::new(&config.target_app_path);
    command.args(["serve", "stop", "--json", "--root", root]);
    #[cfg(windows)]
    crate::windows_process::configure_no_window(&mut command);
    let output = command.output().map_err(|error| {
        ServiceConfigError::Autostart(format!(
            "failed to stop the existing runtime before supervisor activation: {}",
            error
        ))
    })?;
    if output.status.success() {
        return Ok(());
    }
    Err(ServiceConfigError::Autostart(format!(
        "failed to stop the existing runtime before supervisor activation: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

#[cfg(target_os = "linux")]
fn start_platform_user_supervisor(_config: &ServiceConfig) -> ServiceConfigResult<()> {
    let unit = format!("{}.service", LINUX_AUTOSTART_ID);
    for args in [
        vec!["--user", "daemon-reload"],
        vec!["--user", "start", unit.as_str()],
    ] {
        let output = std::process::Command::new("systemctl")
            .args(args)
            .output()
            .map_err(|error| {
                ServiceConfigError::Autostart(format!(
                    "failed to activate systemd user service '{}': {}",
                    unit, error
                ))
            })?;
        if !output.status.success() {
            return Err(ServiceConfigError::Autostart(format!(
                "failed to activate systemd user service '{}': {}",
                unit,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn start_platform_user_supervisor(_config: &ServiceConfig) -> ServiceConfigResult<()> {
    crate::macos_launch_agent::start(super::APP_NAME)
        .map_err(|error| ServiceConfigError::Autostart(error.to_string()))
}

#[cfg(windows)]
fn start_platform_user_supervisor(config: &ServiceConfig) -> ServiceConfigResult<()> {
    let args = config
        .launch_args
        .iter()
        .map(std::ffi::OsString::from)
        .collect::<Vec<_>>();
    let root = service_target_arg(&config.target_args, "--root").map(Path::new);
    crate::windows_process::spawn_detached_no_window(
        Path::new(&config.launch_program_path),
        &args,
        root,
    )
    .map(|_| ())
    .map_err(|error| {
        ServiceConfigError::Autostart(format!(
            "failed to start the Windows MCPace Agent supervisor: {}",
            error
        ))
    })
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
struct SupervisorEndpoint {
    root: PathBuf,
    host: String,
    port: u16,
    health_path: String,
    state_path: PathBuf,
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn supervisor_endpoint(config: &ServiceConfig) -> ServiceConfigResult<SupervisorEndpoint> {
    let root = service_target_arg(&config.target_args, "--root")
        .map(PathBuf::from)
        .ok_or_else(|| {
            ServiceConfigError::Autostart(
                "autostart target does not include the required --root argument".to_string(),
            )
        })?;
    let host = service_target_arg(&config.target_args, "--host")
        .unwrap_or("127.0.0.1")
        .to_string();
    let port = service_target_arg(&config.target_args, "--port")
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| {
            ServiceConfigError::Autostart(
                "autostart target does not include a valid --port argument".to_string(),
            )
        })?;
    let resolved = crate::runtimepaths::resolve_serve_endpoint(Some(&root));
    let state_path =
        crate::runtimepaths::serve_state_path(&crate::runtimepaths::resolve_state_root(&root));
    Ok(SupervisorEndpoint {
        root,
        host,
        port,
        health_path: resolved.health_path,
        state_path,
    })
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn supervisor_runtime_ready(endpoint: &SupervisorEndpoint) -> bool {
    let healthy = endpoint.state_path.is_file()
        && crate::http_probe::json_get(
            &crate::http_probe::probe_host(&endpoint.host),
            endpoint.port,
            &endpoint.health_path,
            std::time::Duration::from_secs(2),
            64 * 1024,
        )
        .ok()
        .and_then(|value| crate::json_helpers::bool_at_path(&value, &["ok"]))
        .unwrap_or(false);
    healthy && platform_user_supervisor_is_active(&endpoint.root)
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn wait_for_supervisor_runtime(endpoint: &SupervisorEndpoint) -> ServiceConfigResult<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if supervisor_runtime_ready(endpoint) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Err(ServiceConfigError::Autostart(format!(
        "user supervisor did not make the MCPace endpoint healthy at {} within 10 seconds",
        crate::runtimepaths::http_url(&endpoint.host, endpoint.port, &endpoint.health_path)
    )))
}

#[cfg(target_os = "linux")]
fn platform_user_supervisor_is_active(_root: &Path) -> bool {
    let unit = format!("{}.service", LINUX_AUTOSTART_ID);
    std::process::Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", &unit])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn platform_user_supervisor_is_active(_root: &Path) -> bool {
    crate::macos_launch_agent::is_loaded(super::APP_NAME).unwrap_or(false)
}

#[cfg(windows)]
fn platform_user_supervisor_is_active(root: &Path) -> bool {
    std::fs::read_to_string(
        root.join("data")
            .join("runtime")
            .join("agent")
            .join("supervisor.pid"),
    )
    .ok()
    .and_then(|value| value.trim().parse::<u32>().ok())
    .is_some_and(|pid| crate::windows_process::process_image_is(pid, "mcpace-agent-launcher.exe"))
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn service_target_arg<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|items| items[0] == flag)
        .map(|items| items[1].as_str())
}

#[cfg(target_os = "linux")]
pub(super) fn stop_user_supervisor_before_disable(
    _config: &ServiceConfig,
) -> ServiceConfigResult<()> {
    let unit = format!("{}.service", LINUX_AUTOSTART_ID);
    let output = std::process::Command::new("systemctl")
        .env("LC_ALL", "C")
        .args(["--user", "stop", &unit])
        .output()
        .map_err(|error| {
            ServiceConfigError::Autostart(format!(
                "failed to stop systemd user service '{}': {}",
                unit, error
            ))
        })?;
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if detail.contains("not loaded") || detail.contains("not found") {
        return Ok(());
    }
    Err(ServiceConfigError::Autostart(format!(
        "failed to stop systemd user service '{}': {}",
        unit, detail
    )))
}

#[cfg(windows)]
pub(super) fn stop_user_supervisor_before_disable(
    config: &ServiceConfig,
) -> ServiceConfigResult<()> {
    stop_runtime_before_supervisor_start(config)
}

#[cfg(target_os = "macos")]
pub(super) fn stop_user_supervisor_before_disable(
    config: &ServiceConfig,
) -> ServiceConfigResult<()> {
    stop_runtime_before_supervisor_start(config)
}

#[cfg(all(not(windows), not(target_os = "linux"), not(target_os = "macos")))]
pub(super) fn stop_user_supervisor_before_disable(
    _config: &ServiceConfig,
) -> ServiceConfigResult<()> {
    Ok(())
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
