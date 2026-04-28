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
        let _ = writeln!(stderr, "{}", error);
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
