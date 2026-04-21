mod actions;
mod args;
mod context;
mod metadata;
mod model;
mod pathing;
mod plan;
mod render;

use self::actions::{run_export, run_install, run_list, run_plan};
use self::args::{parse_args, write_help};
use std::io::Write;
use std::path::PathBuf;

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
    match action.as_str() {
        "plan" => run_plan(parsed, default_root, stdout, stderr),
        "list" => run_list(parsed, default_root, stdout, stderr),
        "export" => run_export(parsed, default_root, stdout, stderr),
        "install" => run_install(parsed, default_root, stdout, stderr),
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
