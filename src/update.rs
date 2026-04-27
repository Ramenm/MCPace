use crate::json::JsonValue;
use std::cmp::Ordering;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const DEFAULT_PACKAGE_NAME: &str = "@mcpace/cli";
const UPDATE_TIMEOUT_ENV: &str = "MCPACE_UPDATE_CHECK_TIMEOUT_MS";
const DEFAULT_UPDATE_TIMEOUT_MS: u64 = 10_000;

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

#[derive(Debug)]
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
}

pub fn run(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    let parsed = parse_args(args);
    if let Some(error) = parsed.error {
        let _ = writeln!(stderr, "{}", error);
        return 2;
    }
    if parsed.help {
        write_help(stdout);
        return 0;
    }
    if parsed.action != "check" {
        let _ = writeln!(stderr, "unsupported update action: {}", parsed.action);
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

fn parse_args(args: &[String]) -> ParsedArgs {
    let mut parsed = ParsedArgs {
        action: "check".to_string(),
        json_output: false,
        latest_version: None,
        source: UpdateSource::None,
        package_name: DEFAULT_PACKAGE_NAME.to_string(),
        help: false,
        error: None,
    };
    let mut source_explicit = false;
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "check" => {
                parsed.action = "check".to_string();
                index += 1;
            }
            "--json" | "-json" => {
                parsed.json_output = true;
                index += 1;
            }
            "--latest-version" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("update check requires a value after --latest-version".to_string());
                    return parsed;
                };
                parsed.latest_version = Some(value.to_string());
                parsed.source = UpdateSource::Argument;
                source_explicit = true;
                index += 2;
            }
            "--source" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("update check requires a value after --source".to_string());
                    return parsed;
                };
                match value.as_str() {
                    "none" => parsed.source = UpdateSource::None,
                    "env" => parsed.source = UpdateSource::Env,
                    "npm" => parsed.source = UpdateSource::Npm,
                    other => {
                        parsed.error = Some(format!(
                            "unsupported update source '{}'; expected none, env, or npm",
                            other
                        ));
                        return parsed;
                    }
                }
                source_explicit = true;
                index += 2;
            }
            "--package" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("update check requires a value after --package".to_string());
                    return parsed;
                };
                parsed.package_name = value.to_string();
                index += 2;
            }
            "--root" => {
                if args.get(index + 1).is_none() {
                    parsed.error = Some("update check requires a path after --root".to_string());
                    return parsed;
                }
                index += 2;
            }
            "-h" | "--help" | "-?" => {
                parsed.help = true;
                return parsed;
            }
            other if other.starts_with('-') => {
                parsed.error = Some(format!("unsupported update argument: {}", other));
                return parsed;
            }
            other => {
                if parsed.action != "check" {
                    parsed.error = Some("update accepts only one action".to_string());
                    return parsed;
                }
                parsed.action = other.to_string();
                index += 1;
            }
        }
    }

    if !source_explicit {
        if parsed.latest_version.is_some() {
            parsed.source = UpdateSource::Argument;
        } else if std::env::var("MCPACE_LATEST_VERSION")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .is_some()
        {
            parsed.source = UpdateSource::Env;
        } else if std::env::var("MCPACE_UPDATE_SOURCE")
            .ok()
            .map(|value| value.eq_ignore_ascii_case("npm"))
            .unwrap_or(false)
        {
            parsed.source = UpdateSource::Npm;
        }
    }

    parsed
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "Usage: mcpace update check [--json] [--source none|env|npm] [--latest-version <semver>] [--package <name>]");
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "Checks whether the installed MCPace binary is behind the selected release source."
    );
    let _ = writeln!(stdout, "This command never rewrites the running binary; it only reports recommended package-manager commands.");
}

fn check_update(parsed: &ParsedArgs) -> UpdateReport {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let mut source = parsed.source;
    let latest = match parsed.latest_version.as_ref() {
        Some(value) => Ok(Some(value.to_string())),
        None => match parsed.source {
            UpdateSource::Argument => Ok(None),
            UpdateSource::Env => read_env_latest_version(),
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
            reason = Some(error);
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
                reason = Some("no latest version source configured; pass --latest-version, --source env, or --source npm".to_string());
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
    }
}

fn read_env_latest_version() -> Result<Option<String>, String> {
    Ok(std::env::var("MCPACE_LATEST_VERSION")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

fn read_npm_latest_version(package_name: &str) -> Result<Option<String>, String> {
    let timeout = update_timeout();
    let mut command = Command::new("npm");
    command
        .args(["view", package_name, "version", "--silent"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    crate::windows_process::configure_no_window(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to run npm update check: {}", error))?;

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "npm update check timed out after {}ms",
                    timeout.as_millis()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(error) => return Err(format!("failed to wait for npm update check: {}", error)),
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|error| format!("failed to read npm update check output: {}", error))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(if detail.is_empty() {
            "npm update check failed".to_string()
        } else {
            format!("npm update check failed: {}", detail)
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
    let _ = writeln!(stdout, "Self-update: disabled");
    let _ = writeln!(
        stdout,
        "Recommended update: {}",
        report.recommended_commands[0]
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
            ("source", JsonValue::string(self.source.label())),
            ("packageName", JsonValue::string(self.package_name.clone())),
            ("selfUpdateEnabled", JsonValue::bool(false)),
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
mod tests {
    use super::*;

    #[test]
    fn semver_compare_handles_current_and_outdated_cases() {
        assert_eq!(compare_semver("0.3.5", "0.3.5"), Some(Ordering::Equal));
        assert_eq!(compare_semver("0.3.5", "0.3.6"), Some(Ordering::Less));
        assert_eq!(compare_semver("0.4.0", "0.3.6"), Some(Ordering::Greater));
    }

    #[test]
    fn update_check_with_explicit_latest_reports_outdated() {
        let parsed = ParsedArgs {
            action: "check".to_string(),
            json_output: true,
            latest_version: Some("99.0.0".to_string()),
            source: UpdateSource::Argument,
            package_name: DEFAULT_PACKAGE_NAME.to_string(),
            help: false,
            error: None,
        };
        let report = check_update(&parsed);
        assert_eq!(report.status, "outdated");
        assert!(report.update_available);
        assert_eq!(report.latest_version.as_deref(), Some("99.0.0"));
    }
}
