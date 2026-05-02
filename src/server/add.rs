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
        let _ = writeln!(stderr, "server add requires a server name, for example: mcpace server add filesystem --command npx --arg @modelcontextprotocol/server-filesystem");
        return 2;
    };
    let result = match mcp_sources::write_mcp_server_entry(
        &root_path,
        mcp_sources::McpServerWriteOptions {
            name,
            server_type: parsed.server_type.clone(),
            command: parsed.command.clone(),
            args: parsed.args.clone(),
            url: parsed.url.clone(),
            env: parsed.env.clone(),
            headers: parsed.headers.clone(),
            settings_path: parsed.settings_path.clone(),
            enabled: !parsed.disabled,
            dry_run: parsed.dry_run,
            force: parsed.force,
        },
    ) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };
    render::render_add_result(&result, parsed.json_output, stdout)
}
