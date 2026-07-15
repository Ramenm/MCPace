use crate::diagnostics;
use crate::json::JsonValue;
use clap::{error::ErrorKind, Parser, ValueEnum};
use std::cmp::Ordering;
use std::ffi::OsString;
use std::fmt;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Eq, PartialEq)]
enum UpdateSourceError {
    UnsupportedEnvValue { value: String },
}

impl fmt::Display for UpdateSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedEnvValue { value } => write!(
                formatter,
                "unsupported MCPACE_UPDATE_SOURCE '{}'; expected none, env, or npm",
                value
            ),
        }
    }
}

impl std::error::Error for UpdateSourceError {}

impl From<UpdateSourceError> for String {
    fn from(error: UpdateSourceError) -> Self {
        error.to_string()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum UpdateCheckError {
    SpawnFailed { reason: String },
    Timeout { timeout_ms: u128 },
    WaitFailed { reason: String },
    OutputReadFailed { reason: String },
    CommandFailed { detail: Option<String> },
}

impl fmt::Display for UpdateCheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SpawnFailed { reason } => {
                write!(formatter, "failed to run npm update check: {}", reason)
            }
            Self::Timeout { timeout_ms } => write!(
                formatter,
                "npm update check timed out after {}ms",
                timeout_ms
            ),
            Self::WaitFailed { reason } => {
                write!(formatter, "failed to wait for npm update check: {}", reason)
            }
            Self::OutputReadFailed { reason } => write!(
                formatter,
                "failed to read npm update check output: {}",
                reason
            ),
            Self::CommandFailed { detail } => {
                match detail.as_deref().filter(|value| !value.is_empty()) {
                    Some(detail) => write!(formatter, "npm update check failed: {}", detail),
                    None => write!(formatter, "npm update check failed"),
                }
            }
        }
    }
}

impl std::error::Error for UpdateCheckError {}

impl From<UpdateCheckError> for String {
    fn from(error: UpdateCheckError) -> Self {
        error.to_string()
    }
}

const DEFAULT_PACKAGE_NAME: &str = "@mcpace/cli";
const UPDATE_TIMEOUT_ENV: &str = "MCPACE_UPDATE_CHECK_TIMEOUT_MS";
const DEFAULT_UPDATE_TIMEOUT_MS: u64 = 10_000;
const DASHBOARD_UPDATE_CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);
const DASHBOARD_UPDATE_FAILURE_CACHE_TTL: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Debug)]
struct CachedDashboardUpdate {
    checked_at: Instant,
    ttl: Duration,
    report: UpdateReport,
}

static DASHBOARD_UPDATE_CACHE: OnceLock<Mutex<Option<CachedDashboardUpdate>>> = OnceLock::new();

#[derive(Debug)]
struct ParsedArgs {
    action: String,
    json_output: bool,
    latest_version: Option<String>,
    source: UpdateSource,
    package_name: String,
    help: bool,
    error: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UpdateSource {
    None,
    Env,
    Npm,
    Argument,
}

impl UpdateSource {
    fn label(self) -> &'static str {
        match self {
            UpdateSource::None => "none",
            UpdateSource::Env => "env",
            UpdateSource::Npm => "npm",
            UpdateSource::Argument => "argument",
        }
    }
}

#[derive(Clone, Debug)]
struct UpdateReport {
    current_version: String,
    latest_version: Option<String>,
    status: String,
    update_available: bool,
    checked: bool,
    source: UpdateSource,
    package_name: String,
    reason: Option<String>,
    recommended_commands: Vec<String>,
    checked_at_ms: u128,
    cached: bool,
}

pub fn run(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    let parsed = parse_cli(args);
    if let Some(error) = parsed.error {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 2;
    }
    if parsed.help {
        write_help(stdout);
        return 0;
    }
    if parsed.action != "check" {
        diagnostics::stderr_line(
            stderr,
            format_args!("unsupported update action: {}", parsed.action),
        );
        return 2;
    }

    let report = check_update(&parsed);
    if parsed.json_output {
        let _ = writeln!(stdout, "{}", report.to_json_value().to_pretty_string());
        return 0;
    }

    write_text_report(&report, stdout);
    0
}

#[derive(Clone, Debug, ValueEnum)]
enum UpdateSourceArg {
    #[value(name = "none")]
    Disabled,
    Env,
    Npm,
}

#[derive(Debug, Parser)]
#[command(
    name = "mcpace update",
    disable_version_flag = true,
    about = "Check whether a newer MCPace npm package is available"
)]
struct UpdateCli {
    /// Update action. Defaults to check.
    action: Option<String>,

    /// Emit machine-readable JSON.
    #[arg(long = "json", short = 'j')]
    json_output: bool,

    /// Explicit latest version used by offline checks and tests.
    #[arg(long = "latest-version", value_name = "SEMVER")]
    latest_version: Option<String>,

    /// Version source to use for the latest-version check.
    #[arg(long = "source", value_enum, value_name = "none|env|npm")]
    source: Option<UpdateSourceArg>,

    /// npm package name to query when --source npm is selected.
    #[arg(long = "package", value_name = "NAME")]
    package_name: Option<String>,

    /// Accepted for command-shape compatibility; update checks do not need a project root.
    #[arg(long = "root", value_name = "PATH", hide = true)]
    _root_compat: Option<PathBuf>,
}

fn parse_cli(args: &[String]) -> ParsedArgs {
    let cli = match UpdateCli::try_parse_from(update_argv(args)) {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            return ParsedArgs {
                action: "check".to_string(),
                json_output: false,
                latest_version: None,
                source: UpdateSource::None,
                package_name: DEFAULT_PACKAGE_NAME.to_string(),
                help: true,
                error: None,
            };
        }
        Err(error) => {
            return ParsedArgs {
                action: "check".to_string(),
                json_output: false,
                latest_version: None,
                source: UpdateSource::None,
                package_name: DEFAULT_PACKAGE_NAME.to_string(),
                help: false,
                error: Some(error.to_string()),
            };
        }
    };

    let source = match derive_update_source(cli.source, cli.latest_version.as_ref()) {
        Ok(source) => source,
        Err(error) => {
            return ParsedArgs {
                action: cli.action.unwrap_or_else(|| "check".to_string()),
                json_output: cli.json_output,
                latest_version: cli.latest_version,
                source: UpdateSource::None,
                package_name: cli
                    .package_name
                    .unwrap_or_else(|| DEFAULT_PACKAGE_NAME.to_string()),
                help: false,
                error: Some(error.to_string()),
            };
        }
    };

    ParsedArgs {
        action: cli.action.unwrap_or_else(|| "check".to_string()),
        json_output: cli.json_output,
        latest_version: cli.latest_version,
        source,
        package_name: cli
            .package_name
            .unwrap_or_else(|| DEFAULT_PACKAGE_NAME.to_string()),
        help: false,
        error: None,
    }
}

fn derive_update_source(
    explicit: Option<UpdateSourceArg>,
    latest_version: Option<&String>,
) -> Result<UpdateSource, UpdateSourceError> {
    if let Some(source) = explicit {
        return Ok(match source {
            UpdateSourceArg::Disabled => UpdateSource::None,
            UpdateSourceArg::Env => UpdateSource::Env,
            UpdateSourceArg::Npm => UpdateSource::Npm,
        });
    }

    if latest_version.is_some() {
        return Ok(UpdateSource::Argument);
    }

    if std::env::var("MCPACE_LATEST_VERSION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .is_some()
    {
        return Ok(UpdateSource::Env);
    }

    match std::env::var("MCPACE_UPDATE_SOURCE").ok() {
        Some(value) => match value.to_ascii_lowercase().as_str() {
            "none" => Ok(UpdateSource::None),
            "env" => Ok(UpdateSource::Env),
            "npm" => Ok(UpdateSource::Npm),
            other => Err(UpdateSourceError::UnsupportedEnvValue {
                value: other.to_string(),
            }),
        },
        None => Ok(UpdateSource::Npm),
    }
}

fn update_argv(args: &[String]) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(OsString::from("mcpace update"));
    argv.extend(
        args.iter()
            .map(|arg| OsString::from(normalize_update_flag(arg))),
    );
    argv
}

fn normalize_update_flag(arg: &str) -> &str {
    match arg {
        "-json" => "--json",
        "-latest-version" => "--latest-version",
        "-source" => "--source",
        "-package" => "--package",
        "-root" => "--root",
        "-?" => "--help",
        other => other,
    }
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "Usage: mcpace update check [--json] [--source none|env|npm] [--latest-version <semver>] [--package <name>]");
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "Checks whether the installed MCPace binary is behind the selected release source."
    );
    let _ = writeln!(
        stdout,
        "Default source is npm. Use --source none for an offline/no-network check."
    );
    let _ = writeln!(
        stdout,
        "This command never rewrites the running binary; updates are package-manager managed."
    );
}

fn check_update(parsed: &ParsedArgs) -> UpdateReport {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let mut source = parsed.source;
    let latest = match parsed.latest_version.as_ref() {
        Some(value) => Ok(Some(value.to_string())),
        None => match parsed.source {
            UpdateSource::Argument => Ok(None),
            UpdateSource::Env => Ok(read_env_latest_version()),
            UpdateSource::Npm => read_npm_latest_version(&parsed.package_name),
            UpdateSource::None => Ok(None),
        },
    };

    if parsed.latest_version.is_some() {
        source = UpdateSource::Argument;
    }

    let mut reason = None;
    let latest_version = match latest {
        Ok(value) => value.and_then(|value| normalize_semver_text(&value)),
        Err(error) => {
            reason = Some(error.to_string());
            None
        }
    };

    let (status, update_available, checked) = match latest_version.as_deref() {
        Some(latest) => match compare_semver(&current_version, latest) {
            Some(Ordering::Less) => ("outdated".to_string(), true, true),
            Some(Ordering::Equal) | Some(Ordering::Greater) => ("current".to_string(), false, true),
            None => {
                reason = Some(format!(
                    "unable to compare current version {} with latest version {}",
                    current_version, latest
                ));
                ("unknown".to_string(), false, true)
            }
        },
        None => {
            if reason.is_none() {
                reason = Some("latest version source disabled; pass --latest-version, --source env, or --source npm".to_string());
            }
            ("unknown".to_string(), false, false)
        }
    };

    UpdateReport {
        current_version,
        latest_version,
        status,
        update_available,
        checked,
        source,
        package_name: parsed.package_name.clone(),
        reason,
        recommended_commands: recommended_commands(&parsed.package_name),
        checked_at_ms: current_epoch_ms(),
        cached: false,
    }
}

/// Returns a bounded, cached update report for the local dashboard.
///
/// The dashboard only asks while it is open. This never installs or rewrites a
/// binary: it can only report the package-manager command the user may choose
/// to run in a terminal.
pub(crate) fn dashboard_update_check_json() -> JsonValue {
    let cache = DASHBOARD_UPDATE_CACHE.get_or_init(|| Mutex::new(None));
    let now = Instant::now();
    if let Some(cached) = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .filter(|entry| now.duration_since(entry.checked_at) < entry.ttl)
    {
        let mut report = cached.report.clone();
        report.cached = true;
        return report.to_json_value();
    }

    let parsed = parse_cli(&[]);
    let report = match parsed.error.as_ref() {
        Some(reason) => {
            unavailable_update_report(reason.clone(), parsed.source, parsed.package_name.clone())
        }
        None => check_update(&parsed),
    };
    let ttl = if report.checked {
        DASHBOARD_UPDATE_CACHE_TTL
    } else {
        DASHBOARD_UPDATE_FAILURE_CACHE_TTL
    };
    *cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(CachedDashboardUpdate {
        checked_at: now,
        ttl,
        report: report.clone(),
    });
    report.to_json_value()
}

fn unavailable_update_report(
    reason: String,
    source: UpdateSource,
    package_name: String,
) -> UpdateReport {
    UpdateReport {
        current_version: env!("CARGO_PKG_VERSION").to_string(),
        latest_version: None,
        status: "unknown".to_string(),
        update_available: false,
        checked: false,
        source,
        recommended_commands: recommended_commands(&package_name),
        package_name,
        reason: Some(reason),
        checked_at_ms: current_epoch_ms(),
        cached: false,
    }
}

fn current_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
}

fn read_env_latest_version() -> Option<String> {
    std::env::var("MCPACE_LATEST_VERSION")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn npm_command_name() -> &'static str {
    if cfg!(windows) {
        "npm.cmd"
    } else {
        "npm"
    }
}

fn read_npm_latest_version(package_name: &str) -> Result<Option<String>, UpdateCheckError> {
    let timeout = update_timeout();
    let mut command = Command::new(npm_command_name());
    command
        .args(["view", package_name, "version", "--silent"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    crate::windows_process::configure_no_window(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| UpdateCheckError::SpawnFailed {
            reason: error.to_string(),
        })?;

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(UpdateCheckError::Timeout {
                    timeout_ms: timeout.as_millis(),
                });
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                return Err(UpdateCheckError::WaitFailed {
                    reason: error.to_string(),
                });
            }
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|error| UpdateCheckError::OutputReadFailed {
            reason: error.to_string(),
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(UpdateCheckError::CommandFailed {
            detail: if detail.is_empty() {
                None
            } else {
                Some(detail)
            },
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string))
}

fn update_timeout() -> Duration {
    let millis = std::env::var(UPDATE_TIMEOUT_ENV)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_UPDATE_TIMEOUT_MS);
    Duration::from_millis(millis)
}

fn normalize_semver_text(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_start_matches('v');
    let mut out = String::new();
    for ch in trimmed.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            out.push(ch);
        } else {
            break;
        }
    }
    if out.split('.').count() >= 3 {
        Some(out)
    } else {
        None
    }
}

fn compare_semver(left: &str, right: &str) -> Option<Ordering> {
    let left_parts = parse_semver(left)?;
    let right_parts = parse_semver(right)?;
    Some(left_parts.cmp(&right_parts))
}

fn parse_semver(value: &str) -> Option<(u64, u64, u64)> {
    let normalized = normalize_semver_text(value)?;
    let mut parts = normalized
        .split('.')
        .take(3)
        .map(|part| part.parse::<u64>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}

fn recommended_commands(package_name: &str) -> Vec<String> {
    vec![
        "mcpace update check --source npm".to_string(),
        format!("npm install -g {}@latest", package_name),
        format!("npx {}@latest help", package_name),
    ]
}

fn write_text_report(report: &UpdateReport, stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "Update status: {}", report.status);
    let _ = writeln!(stdout, "Current version: {}", report.current_version);
    let _ = writeln!(
        stdout,
        "Latest version: {}",
        report.latest_version.as_deref().unwrap_or("unknown")
    );
    let _ = writeln!(stdout, "Source: {}", report.source.label());
    if let Some(reason) = report.reason.as_ref() {
        let _ = writeln!(stdout, "Reason: {}", reason);
    }
    let _ = writeln!(
        stdout,
        "Self-update: package-manager managed (no in-place binary rewrite)"
    );
    let _ = writeln!(
        stdout,
        "Recommended update: {}",
        report.recommended_commands[1]
    );
}

impl UpdateReport {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            (
                "currentVersion",
                JsonValue::string(self.current_version.clone()),
            ),
            (
                "latestVersion",
                self.latest_version
                    .as_ref()
                    .map(|value| JsonValue::string(value.clone()))
                    .unwrap_or(JsonValue::Null),
            ),
            ("status", JsonValue::string(self.status.clone())),
            ("updateAvailable", JsonValue::bool(self.update_available)),
            ("checked", JsonValue::bool(self.checked)),
            (
                "checkedAtMs",
                JsonValue::number(self.checked_at_ms.to_string()),
            ),
            ("cached", JsonValue::bool(self.cached)),
            ("source", JsonValue::string(self.source.label())),
            ("packageName", JsonValue::string(self.package_name.clone())),
            ("selfUpdateEnabled", JsonValue::bool(false)),
            (
                "autoUpdateMode",
                JsonValue::string("package-manager-managed"),
            ),
            (
                "reason",
                self.reason
                    .as_ref()
                    .map(|value| JsonValue::string(value.clone()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "recommendedCommands",
                JsonValue::array(
                    self.recommended_commands
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
        ])
    }
}

#[cfg(test)]
mod tests;
