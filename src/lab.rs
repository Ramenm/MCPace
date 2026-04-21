mod analysis;
mod args;
mod loader;
mod model;
mod render;

use self::analysis::assess_scenarios;
use self::args::{parse_args, write_help};
use self::loader::{load_runtime_capabilities, load_runtime_scenarios};
use self::render::{
    render_coverage, render_gaps, render_list, render_matrix, render_report, render_show,
};
use std::io::Write;
use std::path::PathBuf;

pub fn run(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let parsed = parse_args(args);
    if let Some(error) = parsed.error {
        let _ = writeln!(stderr, "{}", error);
        return 2;
    }
    if parsed.help || parsed.action.is_none() {
        write_help(stdout);
        return 0;
    }

    let root_path = parsed.root_override.or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };

    let scenarios = match load_runtime_scenarios(&root_path) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };
    let capabilities = match load_runtime_capabilities(&root_path) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };
    let assessments = assess_scenarios(&scenarios, &capabilities);

    match parsed.action.as_deref().unwrap_or_default() {
        "list" => render_list(&assessments, parsed.json_output, stdout),
        "matrix" => render_matrix(&assessments, &capabilities, parsed.json_output, stdout),
        "coverage" => render_coverage(&assessments, parsed.json_output, stdout),
        "gaps" => render_gaps(&assessments, &capabilities, parsed.json_output, stdout),
        "report" | "run" => render_report(&assessments, &capabilities, parsed.json_output, stdout),
        "show" => render_show(
            &assessments,
            &capabilities,
            parsed.id_filter.as_deref(),
            parsed.json_output,
            stdout,
            stderr,
        ),
        other => {
            let _ = writeln!(
                stderr,
                "unsupported lab action in the Rust-only repo: {}",
                other
            );
            2
        }
    }
}
