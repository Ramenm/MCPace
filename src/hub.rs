mod args;
mod launcher;
pub(crate) mod leases;
mod lifecycle;
mod model;
pub(crate) mod runtime;
mod status;
use crate::diagnostics;

use self::args::{parse_cli, write_help};
use std::io::Write;
use std::path::PathBuf;

pub fn run(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let parsed = parse_cli(args);

    if let Some(error) = parsed.error.as_ref() {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 2;
    }
    if parsed.help || parsed.action.is_none() {
        write_help(stdout);
        return 0;
    }

    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        diagnostics::stderr_line(
            stderr,
            format_args!("mcpace root not found; expected mcpace.config.json"),
        );
        return 1;
    };

    match parsed.action.as_deref().unwrap_or_default() {
        "up" => lifecycle::run_up(
            &root_path,
            parsed.foreground,
            parsed.json_output,
            stdout,
            stderr,
        ),
        "down" => lifecycle::run_down(&root_path, parsed.json_output, stdout, stderr),
        "repair" => lifecycle::run_repair(&root_path, parsed.json_output, stdout, stderr),
        "status" => lifecycle::run_status(&root_path, parsed.json_output, stdout, stderr),
        "logs" => lifecycle::run_logs(&root_path, parsed.tail, parsed.json_output, stdout, stderr),
        "lease" => leases::run(&root_path, &parsed, stdout, stderr),
        "run" => lifecycle::run_loop_command(&root_path, stderr),
        other => {
            diagnostics::stderr_line(
                stderr,
                format_args!("unsupported hub action in the Rust-only repo: {}", other),
            );
            2
        }
    }
}
