use super::args::ParsedArgs;
use super::render;
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
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };
    let Some(name) = parsed.name_filter.clone() else {
        let _ = writeln!(
            stderr,
            "server remove requires a server name, for example: mcpace server remove filesystem"
        );
        return 2;
    };
    let result = match mcp_sources::remove_mcp_server_entry(
        &root_path,
        mcp_sources::McpServerRemoveOptions {
            name,
            settings_path: parsed.settings_path.clone(),
            dry_run: parsed.dry_run,
        },
    ) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };
    render::render_remove_result(&result, parsed.json_output, stdout)
}
