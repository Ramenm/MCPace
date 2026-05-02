use super::args::ParsedArgs;
use super::render;
use crate::upstream;
use std::io::Write;
use std::path::PathBuf;

pub(super) fn run(
    parsed: &ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };
    let result = match upstream::probe_servers(
        &root_path,
        parsed.name_filter.as_deref(),
        parsed.timeout_ms,
        parsed.refresh,
    ) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };
    render::render_test_result(&result, parsed.json_output, stdout)
}
