mod args;
mod model;
mod render;

use self::args::{parse_args, write_help};
pub use self::model::{collect_readiness, ReadinessReport};
use self::render::{run_grouped_doctor, run_readiness};
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
        "doctor" => run_grouped_doctor(
            parsed.root_override.or(default_root),
            parsed.json_output,
            stdout,
            stderr,
        ),
        "readiness" => run_readiness(
            parsed.root_override.or(default_root),
            parsed.json_output,
            stdout,
            stderr,
        ),
        other => {
            let _ = writeln!(
                stderr,
                "unsupported verify action in the Rust-only repo: {}",
                other
            );
            2
        }
    }
}
