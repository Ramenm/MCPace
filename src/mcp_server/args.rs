use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub(super) struct ParsedArgs {
    pub(super) help: bool,
    pub(super) root_override: Option<PathBuf>,
    pub(super) client_id: Option<String>,
    pub(super) session_id: Option<String>,
    pub(super) project_root: Option<String>,
    pub(super) transport: Option<String>,
    pub(super) error: Option<String>,
}

pub(super) fn parse_args(args: &[String]) -> ParsedArgs {
    let mut parsed = ParsedArgs::default();
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("mcp-server requires a path after --root".to_string());
                    return parsed;
                };
                parsed.root_override = Some(PathBuf::from(value));
                index += 2;
            }
            "--client-id" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("mcp-server requires a value after --client-id".to_string());
                    return parsed;
                };
                parsed.client_id = Some(value.to_string());
                index += 2;
            }
            "--session-id" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("mcp-server requires a value after --session-id".to_string());
                    return parsed;
                };
                parsed.session_id = Some(value.to_string());
                index += 2;
            }
            "--project-root" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("mcp-server requires a value after --project-root".to_string());
                    return parsed;
                };
                parsed.project_root = Some(value.to_string());
                index += 2;
            }
            "--transport" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("mcp-server requires a value after --transport".to_string());
                    return parsed;
                };
                parsed.transport = Some(value.to_string());
                index += 2;
            }
            "-h" | "--help" | "-?" => {
                parsed.help = true;
                return parsed;
            }
            other => {
                parsed.error = Some(format!("unsupported mcp-server argument: {}", other));
                return parsed;
            }
        }
    }

    parsed
}

pub(super) fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace mcp-server [--root <path>] [--client-id <id>] \
         [--session-id <id>] [--project-root <path>] \
         [--transport <stdio|streamable-http>]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "mcp-server starts a live MCP stdio server for local clients."
    );
    let _ = writeln!(
        stdout,
        "It speaks newline-delimited JSON-RPC over stdin/stdout and exposes a \
         focused MCPace management tool catalog."
    );
}
