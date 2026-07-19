use crate::diagnostics;
use crate::json::JsonValue;
use crate::runtimepaths;
use auto_launch::{
    AutoLaunch, AutoLaunchBuilder, LinuxLaunchMode, MacOSLaunchMode, WindowsEnableMode,
};
use std::fmt;
#[cfg(windows)]
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

mod cli;
mod config;
mod legacy;
mod proof;
mod verify;

use cli::{parse_cli, write_help};
#[cfg(all(test, windows))]
use config::autolaunch_token;
#[cfg(any(test, not(windows)))]
use config::quote_shellish_token;
#[cfg(all(test, target_os = "linux"))]
use config::{normalized_linux_path, quote_systemd_token};
use config::{
    service_config, start_user_supervisor_after_enable, stop_user_supervisor_before_disable,
    write_autostart_plan,
};
use legacy::{
    cleanup_legacy_autostart, cleanup_machine_wide_autostart, legacy_autostart_absent_detail,
    LegacyCleanup,
};
use proof::service_prove;
#[cfg(test)]
use proof::{report_with_supervisor_proof, SupervisorProof};
use verify::{
    launches_mcpace_agent, persistent_env_alignment_detail, resources_forwarded,
    service_applied_state_json, target_arg_value, target_root_path_valid, verification_check,
};

pub(crate) const APP_NAME: &str = "MCPace Agent";
pub(crate) const LEGACY_APP_NAME: &str = "MCPace";
const LINUX_AUTOSTART_ID: &str = "mcpace-agent";
const MACOS_AGENT_EXTRA_CONFIG: &str = "<key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict><key>ThrottleInterval</key><integer>10</integer>";
const AUTOSTART_APPLIED_STATE_SCHEMA: &str = "mcpace.autostartAppliedState.v1";
#[cfg(windows)]
const WINDOWS_AUTOSTART_PLAN_SCHEMA: &str = "mcpace.windowsAutostartPlan.v1";
#[cfg(windows)]
const WINDOWS_RUN_COMMAND_MAX_CHARS: usize = 260;

struct ServiceConfig {
    app_path: String,
    args: Vec<String>,
    launch_program_path: String,
    launch_args: Vec<String>,
    target_app_path: String,
    target_args: Vec<String>,
    launch_mode: String,
    platform: String,
    backend: String,
    warnings: Vec<String>,
    install_blocker: Option<String>,
    native_background: bool,
    autostart_plan_path: Option<String>,
}

#[cfg(windows)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowsAutostartPlan {
    schema: String,
    target_app_path: String,
    target_args: Vec<String>,
    root_path: Option<String>,
    launch_mode: String,
}

#[derive(Debug)]
enum ServiceConfigError {
    CurrentExecutable(std::io::Error),
    Autostart(String),
    #[cfg(windows)]
    WindowsRunRegistry(String),
}

type ServiceConfigResult<T> = Result<T, ServiceConfigError>;

impl fmt::Display for ServiceConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CurrentExecutable(error) => {
                write!(formatter, "failed to resolve current executable: {}", error)
            }
            Self::Autostart(error) => write!(formatter, "{}", error),
            #[cfg(windows)]
            Self::WindowsRunRegistry(error) => write!(formatter, "{}", error),
        }
    }
}

impl std::error::Error for ServiceConfigError {}

impl From<ServiceConfigError> for String {
    fn from(error: ServiceConfigError) -> Self {
        error.to_string()
    }
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

    let Some(root_path) = parsed.root_override.clone().or(default_root) else {
        diagnostics::stderr_line(
            stderr,
            format_args!("mcpace root not found; expected mcpace.config.json"),
        );
        return 1;
    };
    let root_path = runtimepaths::canonicalize_or_original(&root_path);
    let configured_endpoint = runtimepaths::resolve_serve_endpoint(Some(&root_path));
    let host = parsed.host.as_deref().unwrap_or(&configured_endpoint.host);
    let port = parsed.port.unwrap_or(configured_endpoint.port);
    let config = match service_config(
        &root_path,
        host,
        port,
        parsed.max_connections,
        parsed.io_timeout_ms,
        parsed.max_body_bytes,
        parsed.overview_cache_ms,
    ) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    let launcher = match build_launcher(&config) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(
                stderr,
                format_args!("failed to build autostart launcher: {}", error),
            );
            return 1;
        }
    };

    let report = match parsed.action.as_str() {
        "install" | "enable" | "repair" => {
            service_install(&launcher, &config, parsed.dry_run || parsed.no_enable)
        }
        "uninstall" | "disable" => service_uninstall(&launcher, &config, parsed.dry_run),
        "status" => service_status(&launcher, &config),
        "verify" | "doctor" => service_verify(&launcher, &config),
        "prove" => service_prove(&launcher, &config, parsed.dry_run),
        "print" | "plan" => service_print(&config),
        other => {
            diagnostics::stderr_line(
                stderr,
                format_args!("unsupported autostart action: {}", other),
            );
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

fn build_launcher(config: &ServiceConfig) -> auto_launch::Result<AutoLaunch> {
    let app_id = if cfg!(target_os = "linux") {
        LINUX_AUTOSTART_ID
    } else {
        APP_NAME
    };
    build_named_launcher(app_id, config)
}

fn build_named_launcher(app_name: &str, config: &ServiceConfig) -> auto_launch::Result<AutoLaunch> {
    let mut builder = AutoLaunchBuilder::new();
    builder
        .set_app_name(app_name)
        .set_app_path(&config.app_path)
        .set_args(&config.args)
        .set_macos_launch_mode(MacOSLaunchMode::LaunchAgent)
        .set_agent_extra_config(MACOS_AGENT_EXTRA_CONFIG)
        .set_windows_enable_mode(WindowsEnableMode::CurrentUser)
        .set_linux_launch_mode(LinuxLaunchMode::Systemd);
    builder.build()
}

fn service_install(launcher: &AutoLaunch, config: &ServiceConfig, dry_run: bool) -> JsonValue {
    if dry_run {
        let mut warnings = config.warnings.clone();
        warnings.push("Dry run / --no-enable: auto-launch enable was not called.".to_string());
        warnings.extend(
            write_autostart_plan(config, true).unwrap_or_else(|error| vec![error.to_string()]),
        );
        warnings.extend(cleanup_legacy_autostart(config, true).warnings);
        warnings.extend(cleanup_machine_wide_autostart(config, true).warnings);
        return report(
            "install",
            config.install_blocker.is_none(),
            false,
            config,
            warnings,
            config.install_blocker.clone(),
        );
    }
    if let Some(blocker) = config.install_blocker.clone() {
        return report(
            "install",
            false,
            enabled_or_false(launcher),
            config,
            config.warnings.clone(),
            Some(blocker),
        );
    }

    let mut warnings = config.warnings.clone();
    match write_autostart_plan(config, false) {
        Ok(plan_warnings) => warnings.extend(plan_warnings),
        Err(error) => {
            return report(
                "install",
                false,
                enabled_or_false(launcher),
                config,
                warnings,
                Some(error.to_string()),
            );
        }
    }

    match launcher.enable() {
        Ok(()) => {
            let enabled = enabled_or_false(launcher);
            let cleanup = cleanup_legacy_autostart(config, false);
            let machine_cleanup = cleanup_machine_wide_autostart(config, false);
            warnings.extend(cleanup.warnings);
            warnings.extend(machine_cleanup.warnings);
            let activation_error = if enabled && cleanup.ok && machine_cleanup.ok {
                start_user_supervisor_after_enable(config)
                    .err()
                    .map(|error| error.to_string())
            } else {
                None
            };
            let ok = enabled && cleanup.ok && machine_cleanup.ok && activation_error.is_none();
            let error = if !cleanup.ok {
                Some("auto-launch enable completed but legacy cleanup failed".to_string())
            } else if !machine_cleanup.ok {
                Some("auto-launch enable completed but machine-wide cleanup failed".to_string())
            } else if !enabled {
                Some("auto-launch enable completed but is_enabled returned false".to_string())
            } else {
                activation_error
            };
            report("install", ok, enabled, config, warnings, error)
        }
        Err(error) => report(
            "install",
            false,
            enabled_or_false(launcher),
            config,
            warnings,
            Some(error.to_string()),
        ),
    }
}

fn service_uninstall(launcher: &AutoLaunch, config: &ServiceConfig, dry_run: bool) -> JsonValue {
    if dry_run {
        let mut warnings = config.warnings.clone();
        warnings.push("Dry run: auto-launch disable was not called.".to_string());
        warnings.extend(remove_autostart_plan(config, true).warnings);
        warnings.extend(cleanup_legacy_autostart(config, true).warnings);
        warnings.extend(cleanup_machine_wide_autostart(config, true).warnings);
        return report(
            "uninstall",
            true,
            enabled_or_false(launcher),
            config,
            warnings,
            None,
        );
    }
    if let Err(error) = stop_user_supervisor_before_disable(config) {
        return report(
            "uninstall",
            false,
            enabled_or_false(launcher),
            config,
            config.warnings.clone(),
            Some(error.to_string()),
        );
    }
    match launcher.disable() {
        Ok(()) => {
            let plan_cleanup = remove_autostart_plan(config, false);
            let cleanup = cleanup_legacy_autostart(config, false);
            let machine_cleanup = cleanup_machine_wide_autostart(config, false);
            let enabled = enabled_or_false(launcher);
            let mut warnings = config.warnings.clone();
            warnings.extend(plan_cleanup.warnings);
            warnings.extend(cleanup.warnings);
            warnings.extend(machine_cleanup.warnings);
            report(
                "uninstall",
                !enabled && plan_cleanup.ok && cleanup.ok && machine_cleanup.ok,
                enabled,
                config,
                warnings,
                if plan_cleanup.ok && cleanup.ok && machine_cleanup.ok {
                    None
                } else if !plan_cleanup.ok {
                    Some(
                        "auto-launch disable completed but autostart plan cleanup failed"
                            .to_string(),
                    )
                } else if !cleanup.ok {
                    Some(
                        "auto-launch disable completed but legacy autostart cleanup failed"
                            .to_string(),
                    )
                } else {
                    Some(
                        "auto-launch disable completed but machine-wide autostart cleanup failed"
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

fn remove_autostart_plan(config: &ServiceConfig, dry_run: bool) -> LegacyCleanup {
    remove_autostart_plan_impl(config, dry_run)
}

#[cfg(windows)]
fn remove_autostart_plan_impl(config: &ServiceConfig, dry_run: bool) -> LegacyCleanup {
    let Some(plan_path) = config.autostart_plan_path.as_deref() else {
        return LegacyCleanup {
            ok: true,
            warnings: Vec::new(),
        };
    };
    let path = Path::new(plan_path);
    if !path.exists() {
        return LegacyCleanup {
            ok: true,
            warnings: Vec::new(),
        };
    }
    if dry_run {
        return LegacyCleanup {
            ok: true,
            warnings: vec![format!(
                "Dry run: would remove Windows autostart plan '{}'.",
                plan_path
            )],
        };
    }
    match fs::remove_file(path) {
        Ok(()) => LegacyCleanup {
            ok: true,
            warnings: vec![format!("Removed Windows autostart plan '{}'.", plan_path)],
        },
        Err(error) => LegacyCleanup {
            ok: false,
            warnings: vec![format!(
                "Failed to remove Windows autostart plan '{}': {}",
                plan_path, error
            )],
        },
    }
}

#[cfg(not(windows))]
fn remove_autostart_plan_impl(_config: &ServiceConfig, _dry_run: bool) -> LegacyCleanup {
    LegacyCleanup {
        ok: true,
        warnings: Vec::new(),
    }
}

#[cfg(windows)]
fn expected_windows_autostart_plan(config: &ServiceConfig) -> WindowsAutostartPlan {
    WindowsAutostartPlan {
        schema: WINDOWS_AUTOSTART_PLAN_SCHEMA.to_string(),
        target_app_path: config.target_app_path.clone(),
        target_args: config.target_args.clone(),
        root_path: target_arg_value(&config.target_args, "--root").map(str::to_string),
        launch_mode: config.launch_mode.clone(),
    }
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
                    JsonValue::string("run `mcpace status --json` after login/reboot to verify the runtime PID, health endpoint, and login-startup registration"),
                ),
            ]),
        );
    }
    result
}

fn service_verification_checks(config: &ServiceConfig, enabled: bool) -> Vec<JsonValue> {
    let persistent_env_mismatches = crate::persistent_env::current_env_registry_mismatches();
    let (legacy_absent, legacy_detail) = legacy_autostart_absent_detail(config);
    let (autostart_command_matches, autostart_command_detail) =
        autostart_command_matches_plan(config);
    let (autostart_plan_ok, autostart_plan_detail) = autostart_plan_matches_config(config);
    let (run_command_short, run_command_length_detail) = windows_run_command_length_detail(config);
    let (machine_entry_absent, machine_entry_detail) = machine_wide_autostart_absent_detail();
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
            "launch-program-exists",
            Path::new(&config.launch_program_path).is_file(),
            "autostart launch program exists at launchProgramPath",
        ),
        verification_check(
            "native-background-launcher",
            config.native_background,
            native_background_detail(config),
        ),
        verification_check(
            "launches-mcpace-agent",
            launches_mcpace_agent(config),
            "autostart must launch `mcpace agent run --autostart`, not serve directly",
        ),
        verification_check(
            "direct-mcpace-entry",
            !config.app_path.to_ascii_lowercase().contains("wscript"),
            "autostart entry must not point at Windows Script Host legacy wrappers",
        ),
        verification_check(
            "autostart-command-matches-plan",
            autostart_command_matches,
            &autostart_command_detail,
        ),
        verification_check(
            "windows-autostart-plan-written",
            autostart_plan_ok,
            &autostart_plan_detail,
        ),
        verification_check(
            "windows-run-command-length",
            run_command_short,
            &run_command_length_detail,
        ),
        verification_check(
            "machine-wide-autostart-entry-absent",
            machine_entry_absent,
            &machine_entry_detail,
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
            "windows-persistent-env-aligned",
            persistent_env_mismatches.is_empty(),
            &persistent_env_alignment_detail(&persistent_env_mismatches),
        ),
    ]
}

fn autostart_plan_matches_config(config: &ServiceConfig) -> (bool, String) {
    autostart_plan_matches_config_impl(config)
}

#[cfg(windows)]
fn autostart_plan_matches_config_impl(config: &ServiceConfig) -> (bool, String) {
    let Some(plan_path) = config.autostart_plan_path.as_deref() else {
        return (
            false,
            "Windows autostart plan path could not be resolved".to_string(),
        );
    };
    let content = match fs::read_to_string(plan_path) {
        Ok(value) => value,
        Err(error) => {
            return (
                false,
                format!(
                    "failed to read Windows autostart plan '{}': {}",
                    plan_path, error
                ),
            );
        }
    };
    let actual: WindowsAutostartPlan = match serde_json::from_str(&content) {
        Ok(value) => value,
        Err(error) => {
            return (
                false,
                format!(
                    "failed to parse Windows autostart plan '{}': {}",
                    plan_path, error
                ),
            );
        }
    };
    let expected = expected_windows_autostart_plan(config);
    if actual == expected {
        (
            true,
            format!(
                "Windows autostart plan matches targetAppPath/targetArgs at '{}'",
                plan_path
            ),
        )
    } else {
        (
            false,
            format!(
                "Windows autostart plan '{}' does not match current targetAppPath/targetArgs; run `mcpace advanced autostart repair --json`",
                plan_path
            ),
        )
    }
}

#[cfg(not(windows))]
fn autostart_plan_matches_config_impl(_config: &ServiceConfig) -> (bool, String) {
    (
        true,
        "separate Windows autostart plan file is not required on this platform".to_string(),
    )
}

fn windows_run_command_length_detail(config: &ServiceConfig) -> (bool, String) {
    windows_run_command_length_detail_impl(config)
}

#[cfg(windows)]
fn windows_run_command_length_detail_impl(config: &ServiceConfig) -> (bool, String) {
    let rendered = rendered_autostart_command(config);
    let length = rendered.chars().count();
    (
        length <= WINDOWS_RUN_COMMAND_MAX_CHARS,
        format!(
            "Windows Run command is {} characters; Microsoft documents Run value command lines as limited to {} characters",
            length, WINDOWS_RUN_COMMAND_MAX_CHARS
        ),
    )
}

#[cfg(not(windows))]
fn windows_run_command_length_detail_impl(_config: &ServiceConfig) -> (bool, String) {
    (
        true,
        "Windows Run command length limit is not relevant on this platform".to_string(),
    )
}

fn machine_wide_autostart_absent_detail() -> (bool, String) {
    machine_wide_autostart_absent_detail_impl()
}

#[cfg(windows)]
fn machine_wide_autostart_absent_detail_impl() -> (bool, String) {
    match windows_run_value_hklm(APP_NAME) {
        Ok(None) => (
            true,
            format!(
                "machine-wide '{}' Run entry is absent; current-user '{}' entry is authoritative",
                APP_NAME, APP_NAME
            ),
        ),
        Ok(Some(_)) => (
            false,
            format!(
                "machine-wide '{}' Run entry is still present and can launch a stale or duplicate agent; remove it from an elevated shell",
                APP_NAME
            ),
        ),
        Err(error) => (
            false,
            format!(
                "failed to inspect machine-wide '{}' Run entry: {}",
                APP_NAME, error
            ),
        ),
    }
}

#[cfg(not(windows))]
fn machine_wide_autostart_absent_detail_impl() -> (bool, String) {
    (
        true,
        "machine-wide Windows Run entries are not relevant on this platform".to_string(),
    )
}

fn native_background_detail(config: &ServiceConfig) -> &str {
    if cfg!(windows) {
        if config.native_background {
            "Windows autostart uses the GUI-subsystem mcpace-agent-launcher.exe sidecar so no terminal window is opened at login"
        } else {
            "Windows autostart would have to start mcpace.exe directly because mcpace-agent-launcher.exe is missing; that would open a terminal window"
        }
    } else {
        "this platform's login manager starts MCPace Agent without a Windows console window"
    }
}

fn autostart_command_matches_plan(config: &ServiceConfig) -> (bool, String) {
    autostart_command_matches_plan_impl(config)
}

#[cfg(windows)]
fn autostart_command_matches_plan_impl(config: &ServiceConfig) -> (bool, String) {
    match windows_run_value(APP_NAME) {
        Ok(Some(actual)) => {
            let expected = rendered_autostart_command(config);
            if actual == expected {
                (
                    true,
                    "Windows Run registry command matches the native hidden autostart plan"
                        .to_string(),
                )
            } else {
                (
                    false,
                    format!(
                        "Windows Run registry command differs from the native hidden autostart plan; expected '{}', actual '{}'",
                        expected, actual
                    ),
                )
            }
        }
        Ok(None) => (
            false,
            format!("Windows Run registry value '{}' is missing", APP_NAME),
        ),
        Err(error) => (
            false,
            format!("failed to inspect Windows Run registry command: {}", error),
        ),
    }
}

#[cfg(not(windows))]
fn autostart_command_matches_plan_impl(_config: &ServiceConfig) -> (bool, String) {
    (
        true,
        "platform autostart command is managed by auto-launch for this OS".to_string(),
    )
}

#[cfg(windows)]
const WINDOWS_RUN_HKCU: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const WINDOWS_RUN_HKLM: &str = r"HKLM\Software\Microsoft\Windows\CurrentVersion\Run";

#[cfg(windows)]
fn windows_run_value(value_name: &str) -> Result<Option<String>, ServiceConfigError> {
    windows_run_value_hkcu(value_name)
}

#[cfg(windows)]
fn windows_run_value_hkcu(value_name: &str) -> Result<Option<String>, ServiceConfigError> {
    windows_run_value_in_key(WINDOWS_RUN_HKCU, value_name)
}

#[cfg(windows)]
fn windows_run_value_hklm(value_name: &str) -> Result<Option<String>, ServiceConfigError> {
    windows_run_value_in_key(WINDOWS_RUN_HKLM, value_name)
}

#[cfg(windows)]
fn windows_run_value_in_key(
    key_path: &str,
    value_name: &str,
) -> Result<Option<String>, ServiceConfigError> {
    let mut command = std::process::Command::new("reg");
    command.args(["query", key_path, "/v", value_name]);
    crate::windows_process::configure_no_window(&mut command);
    let output = command.output().map_err(|error| {
        ServiceConfigError::WindowsRunRegistry(format!(
            "failed to query Windows Run registry '{}': {}",
            key_path, error
        ))
    })?;
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

#[cfg(windows)]
fn windows_delete_hklm_run_value(value_name: &str) -> Result<(), ServiceConfigError> {
    let mut command = std::process::Command::new("reg");
    command.args(["delete", WINDOWS_RUN_HKLM, "/v", value_name, "/f"]);
    crate::windows_process::configure_no_window(&mut command);
    let output = command.output().map_err(|error| {
        ServiceConfigError::WindowsRunRegistry(format!(
            "failed to delete Windows Run registry value '{}': {}",
            value_name, error
        ))
    })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Err(ServiceConfigError::WindowsRunRegistry(format!(
        "reg delete failed with status {}{}{}",
        output.status,
        if stdout.is_empty() { "" } else { ": " },
        if stdout.is_empty() { stderr } else { stdout }
    )))
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
            "launchProgramPath",
            JsonValue::string(config.launch_program_path.clone()),
        ),
        (
            "launchArgs",
            JsonValue::array(config.launch_args.iter().cloned().map(JsonValue::string)),
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
            "nativeBackground",
            JsonValue::bool(config.native_background),
        ),
        (
            "autostartPlanPath",
            config
                .autostart_plan_path
                .clone()
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
        (
            "installBlocker",
            config
                .install_blocker
                .clone()
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
        (
            "renderedCommand",
            JsonValue::string(rendered_target_command(config)),
        ),
        (
            "renderedAutostartCommand",
            JsonValue::string(rendered_autostart_command(config)),
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
            "launchProgram",
            JsonValue::string(config.launch_program_path.clone()),
        ),
        (
            "launchArgs",
            JsonValue::array(config.launch_args.iter().cloned().map(JsonValue::string)),
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
        (
            "renderedAutostart",
            JsonValue::string(rendered_autostart_command(config)),
        ),
        (
            "autostartPlanPath",
            config
                .autostart_plan_path
                .clone()
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
        ("displayName", JsonValue::string(APP_NAME)),
        (
            "purpose",
            JsonValue::string("Launches the local MCPace runtime for MCP clients after user login"),
        ),
    ])
}

fn rendered_autostart_command(config: &ServiceConfig) -> String {
    std::iter::once(config.app_path.as_str())
        .chain(config.args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
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
            "user-systemd-manager",
            true,
            true,
            "systemd user service via auto-launch",
            vec![
                "autostart install writes and enables mcpace-agent.service for the current user",
                "systemd restarts MCPace Agent after non-zero runtime exits",
                "boot-before-login requires user lingering; otherwise startup follows the user session",
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
            true,
            true,
            "Windows current-user Run registry via auto-launch",
            vec![
                "the login item starts MCPace Agent after the current user logs in",
                "new installs do not use wscript.exe, generated VBS launchers, or direct console-window startup",
                "Windows current-user Run registry starts mcpace-agent-launcher.exe; the hidden launcher supervises MCPace Agent without opening a terminal",
                "the launcher restarts non-zero exits with bounded exponential backoff",
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
            JsonValue::string("mcpace-agent-managed-foreground"),
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
            ("manager", JsonValue::string("systemd user service")),
            ("restartOnFailure", JsonValue::bool(true)),
            ("supervisesRuntime", JsonValue::bool(true)),
            ("note", JsonValue::string("mcpace-agent.service starts with the user systemd manager; lingering is required only for boot-before-login")),
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
            ("manager", JsonValue::string("Windows current-user Run registry + mcpace-agent-launcher.exe")),
            ("restartOnFailure", JsonValue::bool(true)),
            ("supervisesRuntime", JsonValue::bool(true)),
            ("note", JsonValue::string("The current-user Run entry starts the hidden launcher, which supervises MCPace Agent with bounded backoff")),
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
mod tests;
