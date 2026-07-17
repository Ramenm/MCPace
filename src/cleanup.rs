use crate::diagnostics;
use crate::json::JsonValue;
use crate::runtimepaths;
use clap::{error::ErrorKind, Parser, ValueEnum};
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct ParsedArgs {
    scope: String,
    json_output: bool,
    root_override: Option<PathBuf>,
    dry_run: bool,
    help: bool,
    error: Option<String>,
}

impl Default for ParsedArgs {
    fn default() -> Self {
        Self {
            scope: "status".to_string(),
            json_output: false,
            root_override: None,
            dry_run: false,
            help: false,
            error: None,
        }
    }
}

#[derive(Debug)]
struct CleanupAction {
    id: &'static str,
    class: &'static str,
    path: PathBuf,
    destructive: bool,
    existed: bool,
    removed: bool,
    error: Option<String>,
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
    let report = cleanup_report(&root_path, &parsed.scope, parsed.dry_run);

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

#[derive(Clone, Debug, ValueEnum)]
enum CleanupScope {
    Status,
    Cache,
    Runtime,
    Logs,
    #[value(name = "all-safe")]
    AllSafe,
}

#[derive(Debug, Parser)]
#[command(
    name = "mcpace advanced runtime cleanup",
    disable_version_flag = true,
    about = "Inspect or remove MCPace runtime artifacts"
)]
struct CleanupCli {
    /// Cleanup scope. Defaults to status.
    #[arg(value_enum)]
    scope: Option<CleanupScope>,

    /// Emit machine-readable JSON.
    #[arg(long = "json", short = 'j')]
    json_output: bool,

    /// MCPace project/root directory.
    #[arg(long = "root", value_name = "PATH")]
    root_override: Option<PathBuf>,

    /// Preview cleanup without deleting files.
    #[arg(long = "dry-run")]
    dry_run: bool,
}

fn parse_cli(args: &[String]) -> ParsedArgs {
    match CleanupCli::try_parse_from(cleanup_argv(args)) {
        Ok(cli) => ParsedArgs {
            scope: cli
                .scope
                .map(cleanup_scope_name)
                .unwrap_or_else(|| "status".to_string()),
            json_output: cli.json_output,
            root_override: cli.root_override,
            dry_run: cli.dry_run,
            help: false,
            error: None,
        },
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

fn cleanup_scope_name(scope: CleanupScope) -> String {
    match scope {
        CleanupScope::Status => "status",
        CleanupScope::Cache => "cache",
        CleanupScope::Runtime => "runtime",
        CleanupScope::Logs => "logs",
        CleanupScope::AllSafe => "all-safe",
    }
    .to_string()
}

fn cleanup_argv(args: &[String]) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(OsString::from("mcpace advanced runtime cleanup"));
    argv.extend(args.iter().map(OsString::from));
    argv
}

fn cleanup_report(root_path: &Path, scope: &str, dry_run: bool) -> JsonValue {
    let state_root = runtimepaths::resolve_state_root(root_path);
    let mut coordination_error = None;
    let _lifecycle_coordination = if !dry_run && matches!(scope, "runtime" | "all-safe") {
        match crate::serve::acquire_lifecycle_coordination(&state_root) {
            Ok(guard) => Some(guard),
            Err(error) => {
                coordination_error = Some(format!(
                    "runtime cleanup is blocked because lifecycle coordination could not be acquired: {}",
                    error
                ));
                None
            }
        }
    } else {
        None
    };
    let mut actions = planned_actions(&state_root, scope);
    let mut warnings = Vec::new();

    if actions.is_empty() && !matches!(scope, "status") {
        warnings.push(format!(
            "unknown cleanup scope '{}'; use status, cache, runtime, logs, or all-safe",
            scope
        ));
    }

    let runtime_blocker =
        coordination_error.or_else(|| runtime_cleanup_blocker(root_path, &state_root, scope));
    if let Some(blocker) = runtime_blocker {
        warnings.push(blocker.clone());
        for action in &mut actions {
            if matches!(
                action.class,
                "ephemeral-runtime" | "recoverable-runtime-marker"
            ) {
                action.error = Some(blocker.clone());
            }
        }
    }

    if !dry_run && !matches!(scope, "status") {
        for action in &mut actions {
            if !action.existed || action.error.is_some() {
                continue;
            }
            let result = if action.path.is_dir() {
                fs::remove_dir_all(&action.path)
            } else {
                fs::remove_file(&action.path)
            };
            match result {
                Ok(()) => action.removed = true,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    action.removed = false;
                    action.existed = false;
                }
                Err(error) => action.error = Some(error.to_string()),
            }
        }
    }

    let blockers = actions
        .iter()
        .filter(|action| action.error.is_some())
        .map(|action| {
            JsonValue::string(format!(
                "{}: {}",
                action.id,
                action
                    .error
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string())
            ))
        })
        .collect::<Vec<_>>();
    let ok = blockers.is_empty();

    JsonValue::object([
        ("ok", JsonValue::bool(ok)),
        ("scope", JsonValue::string(scope.to_string())),
        ("dryRun", JsonValue::bool(dry_run)),
        ("rootPath", JsonValue::string(root_path.display().to_string())),
        ("stateRoot", JsonValue::string(state_root.display().to_string())),
        (
            "policy",
            JsonValue::string(
                "cleanup only removes disposable cache, logs, and ephemeral runtime markers; it never removes durable config, backups, source fragments, or external client config".to_string(),
            ),
        ),
        (
            "actions",
            JsonValue::array(actions.iter().map(CleanupAction::to_json_value)),
        ),
        (
            "warnings",
            JsonValue::array(warnings.into_iter().map(JsonValue::string)),
        ),
        ("blockers", JsonValue::array(blockers)),
    ])
}

fn runtime_cleanup_blocker(root_path: &Path, state_root: &Path, scope: &str) -> Option<String> {
    if !matches!(scope, "runtime" | "all-safe") {
        return None;
    }

    match crate::serve::managed_runtime_is_live(root_path) {
        Ok(true) => {
            return Some(
                "runtime cleanup is blocked while the managed MCPace HTTP service is live; stop it before removing ownership markers"
                    .to_string(),
            );
        }
        Ok(false) => {}
        Err(error) => {
            return Some(format!(
                "runtime cleanup is blocked because managed serve liveness could not be verified: {}",
                error
            ));
        }
    }

    let hub_metadata_exists = [
        runtimepaths::hub_state_path(state_root),
        runtimepaths::hub_health_path(state_root),
        runtimepaths::hub_lock_path(state_root),
    ]
    .iter()
    .any(|path| path.exists());
    if !hub_metadata_exists {
        return None;
    }

    match crate::hub::runtime_is_live(root_path) {
        Ok(true) => Some(
            "runtime cleanup is blocked while the MCPace hub is live; stop it before removing ownership markers"
                .to_string(),
        ),
        Ok(false) => None,
        Err(error) => Some(format!(
            "runtime cleanup is blocked because hub liveness could not be verified: {}",
            error
        )),
    }
}

fn planned_actions(state_root: &Path, scope: &str) -> Vec<CleanupAction> {
    let mut actions = Vec::new();
    if matches!(scope, "status" | "cache" | "all-safe") {
        actions.push(action(
            "tool-list-cache",
            "disposable-cache",
            runtimepaths::tool_list_cache_dir(state_root),
            false,
        ));
    }
    if matches!(scope, "status" | "runtime" | "all-safe") {
        actions.push(action(
            "hub-stop-signal",
            "ephemeral-runtime",
            runtimepaths::hub_stop_path(state_root),
            false,
        ));
        actions.push(action(
            "hub-lock",
            "ephemeral-runtime",
            runtimepaths::hub_lock_path(state_root),
            false,
        ));
        actions.push(action(
            "serve-state",
            "recoverable-runtime-marker",
            runtimepaths::serve_state_path(state_root),
            false,
        ));
    }
    if matches!(scope, "status" | "logs" | "all-safe") {
        actions.push(action(
            "hub-events-log",
            "diagnostic-log",
            runtimepaths::hub_log_path(state_root),
            false,
        ));
        actions.push(action(
            "serve-stdout-log",
            "diagnostic-log",
            runtimepaths::serve_stdout_log_path(state_root),
            false,
        ));
        actions.push(action(
            "serve-stderr-log",
            "diagnostic-log",
            runtimepaths::serve_stderr_log_path(state_root),
            false,
        ));
    }
    actions
}

fn action(
    id: &'static str,
    class: &'static str,
    path: PathBuf,
    destructive: bool,
) -> CleanupAction {
    let existed = path.exists();
    CleanupAction {
        id,
        class,
        path,
        destructive,
        existed,
        removed: false,
        error: None,
    }
}

impl CleanupAction {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("id", JsonValue::string(self.id)),
            ("class", JsonValue::string(self.class)),
            ("path", JsonValue::string(self.path.display().to_string())),
            ("destructive", JsonValue::bool(self.destructive)),
            ("existed", JsonValue::bool(self.existed)),
            ("removed", JsonValue::bool(self.removed)),
            (
                "error",
                self.error
                    .clone()
                    .map(JsonValue::string)
                    .unwrap_or(JsonValue::Null),
            ),
        ])
    }
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "Usage: mcpace advanced runtime cleanup [status|cache|runtime|logs|all-safe] [--json] [--root <path>] [--dry-run]");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Safe cleanup preserves durable user config, MCP source fragments, external client configs, and client-install backups.");
}

fn write_text_report(report: &JsonValue, stdout: &mut dyn Write) {
    let scope = report
        .get("scope")
        .and_then(JsonValue::as_str)
        .unwrap_or("cleanup");
    let ok = report
        .get("ok")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let dry_run = report
        .get("dryRun")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let _ = writeln!(
        stdout,
        "MCPace cleanup {}: {}",
        scope,
        if ok { "ok" } else { "blocked" }
    );
    let _ = writeln!(stdout, "Dry run: {}", if dry_run { "yes" } else { "no" });
    let _ = writeln!(
        stdout,
        "Policy: durable config, source fragments, client config, and backups are preserved."
    );
}

#[cfg(test)]
mod tests;
