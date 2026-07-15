mod analysis;
mod args;
mod loader;
mod model;
mod render;
use crate::diagnostics;

use self::analysis::assess_scenarios;
use self::args::{parse_cli, write_help};
use self::loader::{load_runtime_capabilities, load_runtime_scenarios};
use self::render::{
    render_coverage, render_gaps, render_list, render_matrix, render_probe, render_report,
    render_show,
};
use crate::upstream;
use std::io::Write;
use std::path::PathBuf;

pub fn run(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let parsed = parse_cli(args);
    if let Some(error) = parsed.error {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 2;
    }
    if parsed.help {
        write_help(stdout);
        return 0;
    }

    let root_path = parsed.root_override.or(default_root);
    let Some(root_path) = root_path else {
        diagnostics::stderr_line(
            stderr,
            format_args!("mcpace root not found; expected mcpace.config.json"),
        );
        return 1;
    };

    let scenarios = match load_runtime_scenarios(&root_path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    let capabilities = match load_runtime_capabilities(&root_path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    let assessments = assess_scenarios(&scenarios, &capabilities);

    match parsed.action.as_deref().unwrap_or("report") {
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
        "probe" => match upstream::probe_servers(
            &root_path,
            parsed.id_filter.as_deref(),
            parsed.timeout_ms,
            parsed.refresh,
        ) {
            Ok(value) => render_probe(&value, parsed.json_output, stdout),
            Err(error) => {
                diagnostics::stderr_line(stderr, format_args!("{}", error));
                1
            }
        },
        other => {
            diagnostics::stderr_line(
                stderr,
                format_args!("unsupported lab action in the Rust-only repo: {}", other),
            );
            2
        }
    }
}
