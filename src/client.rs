mod actions;
mod args;
mod context;
mod metadata;
mod model;
mod pathing;
mod plan;
mod render;
use crate::diagnostics;

use self::actions::{run_export, run_install, run_list, run_plan, run_restore};
use self::args::{parse_cli, write_help, ParsedArgs};
use crate::json::JsonValue;
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClientRuntimePlanError {
    message: String,
}

impl fmt::Display for ClientRuntimePlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ClientRuntimePlanError {}

impl From<String> for ClientRuntimePlanError {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl From<ClientRuntimePlanError> for String {
    fn from(error: ClientRuntimePlanError) -> Self {
        error.to_string()
    }
}

type ClientRuntimePlanResult<T> = Result<T, ClientRuntimePlanError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClientIntegrationError {
    message: String,
}

impl fmt::Display for ClientIntegrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ClientIntegrationError {}

impl From<String> for ClientIntegrationError {
    fn from(message: String) -> Self {
        Self { message }
    }
}

pub(crate) type ClientIntegrationResult<T> = Result<T, ClientIntegrationError>;

#[derive(Debug, Default, Clone)]
pub(crate) struct RuntimePlanRequest {
    pub(crate) client_id: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) project_root: Option<String>,
    pub(crate) transport: Option<String>,
    pub(crate) metadata_json: Option<String>,
}

pub(crate) fn runtime_plan_json(
    root_path: &Path,
    request: RuntimePlanRequest,
) -> ClientRuntimePlanResult<JsonValue> {
    let parsed = ParsedArgs {
        action: Some("plan".to_string()),
        json_output: true,
        help: false,
        root_override: Some(root_path.to_path_buf()),
        client_id: request.client_id,
        session_id: request.session_id,
        project_root: request.project_root,
        transport: request.transport,
        metadata_json: request.metadata_json,
        dry_run: false,
        diff: false,
        backup: None,
        error: None,
    };
    actions::build_plan_json(parsed, root_path).map_err(ClientRuntimePlanError::from)
}

pub(crate) fn remove_owned_integrations(
    root_path: &Path,
    dry_run: bool,
) -> ClientIntegrationResult<JsonValue> {
    actions::remove_owned_client_integrations(root_path, dry_run)
}

pub fn run(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let parsed = parse_cli(args);
    if let Some(error) = parsed.error.clone() {
        write_command_error(&parsed, stdout, stderr, &error);
        return 2;
    }
    if parsed.help || parsed.action.is_none() {
        write_help(stdout);
        return 0;
    }

    let action = parsed.action.clone().unwrap_or_default();
    if action != "install" && (parsed.dry_run || parsed.diff) {
        diagnostics::stderr_line(
            stderr,
            format_args!(
                "--dry-run and --diff are currently supported only for 'mcpace advanced client install'"
            ),
        );
        return 2;
    }
    if action != "restore" && parsed.backup.is_some() {
        diagnostics::stderr_line(
            stderr,
            format_args!(
                "--backup and --latest are currently supported only for 'mcpace advanced client restore'"
            ),
        );
        return 2;
    }
    match action.as_str() {
        "plan" => run_plan(parsed, default_root, stdout, stderr),
        "list" => run_list(parsed, default_root, stdout, stderr),
        "export" => run_export(parsed, default_root, stdout, stderr),
        "install" => run_install(parsed, default_root, stdout, stderr),
        "restore" => run_restore(parsed, default_root, stdout, stderr),
        other => {
            diagnostics::stderr_line(
                stderr,
                format_args!("unsupported client action in the Rust-only repo: {}", other),
            );
            2
        }
    }
}

fn write_command_error(
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    error: &str,
) {
    if parsed.json_output {
        let action = parsed.action.as_deref().unwrap_or("unknown");
        let payload = JsonValue::object([
            ("schema", JsonValue::string("mcpace.clientError.v1")),
            ("ok", JsonValue::bool(false)),
            ("action", JsonValue::string(action)),
            ("error", JsonValue::string(error)),
        ]);
        let _ = writeln!(stdout, "{}", payload.to_pretty_string());
        return;
    }

    diagnostics::stderr_line(stderr, format_args!("{}", error));
}

#[cfg(test)]
mod tests;
