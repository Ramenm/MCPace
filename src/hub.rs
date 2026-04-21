mod args;
mod launcher;
mod lifecycle;
mod model;
mod runtime;
mod status;

use self::args::{parse_args, write_help, ParsedArgs};
use std::io::Write;
use std::path::PathBuf;

pub fn run(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let ParsedArgs {
        action,
        json_output,
        help,
        root_override,
        tail,
        foreground,
        error,
    } = parse_args(args);

    if let Some(error) = error {
        let _ = writeln!(stderr, "{}", error);
        return 2;
    }
    if help || action.is_none() {
        write_help(stdout);
        return 0;
    }

    let root_path = root_override.or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };

    match action.as_deref().unwrap_or_default() {
        "up" => lifecycle::run_up(&root_path, foreground, json_output, stdout, stderr),
        "down" => lifecycle::run_down(&root_path, json_output, stdout, stderr),
        "repair" => lifecycle::run_repair(&root_path, json_output, stdout, stderr),
        "status" => lifecycle::run_status(&root_path, json_output, stdout, stderr),
        "logs" => lifecycle::run_logs(&root_path, tail, json_output, stdout, stderr),
        "run" => lifecycle::run_loop_command(&root_path, stderr),
        other => {
            let _ = writeln!(
                stderr,
                "unsupported hub action in the Rust-only repo: {}",
                other
            );
            2
        }
    }
}
