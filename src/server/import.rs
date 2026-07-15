use super::args::ParsedArgs;
use super::render;
use crate::diagnostics;
use crate::mcp_sources;
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
        diagnostics::stderr_line(
            stderr,
            format_args!("mcpace root not found; expected mcpace.config.json"),
        );
        return 1;
    };
    let Some(source_path) = parsed.import_path.clone() else {
        diagnostics::stderr_line(
            stderr,
            format_args!(
                "server import requires --from <mcp-settings.json> or a positional source path"
            ),
        );
        return 2;
    };
    let result = match mcp_sources::import_mcp_server_entries(
        &root_path,
        mcp_sources::McpServerImportOptions {
            source_path,
            settings_path: parsed.settings_path.clone(),
            dry_run: parsed.dry_run,
            force: parsed.force,
            disabled: parsed.disabled,
        },
    ) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    render::render_import_result(&result, parsed.json_output, stdout)
}
