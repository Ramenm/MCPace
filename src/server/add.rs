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
    let Some(name) = parsed.name_filter.clone() else {
        diagnostics::stderr_line(stderr, format_args!("server add requires a server name, for example: mcpace server add filesystem --command npx --arg @modelcontextprotocol/server-filesystem"));
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
            profile_hints: Vec::new(),
        },
    ) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    render::render_add_result(&result, parsed.json_output, stdout)
}
