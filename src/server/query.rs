use super::args::ParsedArgs;
use super::loader::load_server_records;
use super::render::{render_capabilities, render_list};
use crate::diagnostics;
use std::io::Write;
use std::path::PathBuf;

pub(super) fn run(
    action: &str,
    parsed: &ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        diagnostics::stderr_line(
            stderr,
            format_args!("mcpace root not found; expected mcpace.config.json"),
        );
        return 1;
    };
    let records = match load_server_records(&root_path) {
        Ok(records) => records,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };

    match action {
        "list" => render_list(&records, parsed.json_output, stdout),
        "capabilities" => render_capabilities(
            &records,
            parsed.name_filter.as_deref(),
            parsed.json_output,
            stdout,
            stderr,
        ),
        other => {
            diagnostics::stderr_line(
                stderr,
                format_args!("unsupported server action in the Rust-only repo: {}", other),
            );
            2
        }
    }
}
