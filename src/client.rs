mod actions;
mod args;
mod context;
mod metadata;
mod model;
mod pathing;
mod plan;
mod render;

use self::actions::{run_export, run_install, run_list, run_plan, run_restore};
use self::args::{parse_args, write_help, ParsedArgs};
use crate::json::JsonValue;
use std::io::Write;
use std::path::{Path, PathBuf};

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
) -> Result<JsonValue, String> {
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
    actions::build_plan_json(parsed, root_path)
}

pub fn run(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let parsed = parse_args(args);
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
        let _ = writeln!(
            stderr,
            "--dry-run and --diff are currently supported only for 'mcpace client install'"
        );
        return 2;
    }
    if action != "restore" && parsed.backup.is_some() {
        let _ = writeln!(
            stderr,
            "--backup and --latest are currently supported only for 'mcpace client restore'"
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
            let _ = writeln!(
                stderr,
                "unsupported client action in the Rust-only repo: {}",
                other
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

    let _ = writeln!(stderr, "{}", error);
}

#[cfg(test)]
mod tests {
    use super::run;
    use crate::json::{parse_str, JsonValue};

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn client_export_json_missing_target_is_machine_readable() {
        let args = strings(&["export", "--json"]);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let status = run(&args, None, &mut stdout, &mut stderr);

        assert_eq!(status, 2);
        assert!(stderr.is_empty());
        let output = String::from_utf8(stdout).expect("json output is utf-8");
        let payload = parse_str(&output).expect("missing target error must be valid JSON");
        assert_eq!(
            payload.get("schema").and_then(JsonValue::as_str),
            Some("mcpace.clientError.v1")
        );
        assert_eq!(payload.get("ok").and_then(JsonValue::as_bool), Some(false));
        assert_eq!(
            payload.get("action").and_then(JsonValue::as_str),
            Some("export")
        );
        assert!(payload
            .get("error")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .contains("requires a client target"));
    }

    #[test]
    fn client_export_text_missing_target_points_to_catalog() {
        let args = strings(&["export"]);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let status = run(&args, None, &mut stdout, &mut stderr);

        assert_eq!(status, 2);
        assert!(stdout.is_empty());
        let error = String::from_utf8(stderr).expect("stderr is utf-8");
        assert!(error.contains("requires a client target"));
        assert!(error.contains("mcpace client list"));
    }
}
