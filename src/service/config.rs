use super::{ServiceConfig, ServiceConfigError, ServiceConfigResult};
use crate::resources;
use auto_launch::AutoLaunch;
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
        "auto-launch/linux-xdg-autostart"
    } else {
        "auto-launch/unsupported"
    };
    let mut warnings = vec![
        "Autostart is user-level by default; it does not install privileged system services.".to_string(),
        "MCPace autostart launches MCPace Agent natively; the previous Windows wscript/VBS wrapper is not used for new installs.".to_string(),
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

#[cfg(not(windows))]
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
    quote_shellish_token(value)
}

#[cfg(all(not(windows), not(target_os = "linux")))]
fn autolaunch_token_impl(value: &str) -> String {
    value.to_string()
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
