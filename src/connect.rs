mod args;
mod model;
mod render;

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
    if parsed.help {
        write_help(stdout);
        return 0;
    }
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };
    let report = match model::build_report(&root_path, &parsed) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };
    render::render(&report, parsed.json_output, stdout)
}
