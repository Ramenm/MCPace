use clap::{error::ErrorKind, Parser};
use std::ffi::OsString;
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

#[derive(Debug, Parser)]
#[command(
    name = "mcpace stdio",
    disable_version_flag = true,
    about = "Run the live MCPace MCP stdio server"
)]
struct McpStdioCli {
    /// MCPace project/root directory.
    #[arg(long = "root", value_name = "PATH")]
    root_override: Option<PathBuf>,

    /// Client catalog id used for routing/session context.
    #[arg(long = "client-id", value_name = "ID")]
    client_id: Option<String>,

    /// Optional chat/session id used to derive sticky leases.
    #[arg(long = "session-id", value_name = "ID")]
    session_id: Option<String>,

    /// Optional project root hint from the host client.
    #[arg(long = "project-root", value_name = "PATH")]
    project_root: Option<String>,

    /// Ingress transport label exposed to routing plans.
    #[arg(long = "transport", value_name = "stdio|streamable-http")]
    transport: Option<String>,

    /// Accepted for compatibility with the former preview shim. Live stdio is already JSON-RPC.
    #[arg(long = "json", hide = true)]
    _json_compat: bool,
}

pub(super) fn parse_cli(args: &[String]) -> ParsedArgs {
    match McpStdioCli::try_parse_from(argv(args)) {
        Ok(cli) => ParsedArgs {
            help: false,
            root_override: cli.root_override,
            client_id: cli.client_id,
            session_id: cli.session_id,
            project_root: cli.project_root,
            transport: cli.transport,
            error: None,
        },
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            ParsedArgs {
                help: true,
                ..ParsedArgs::default()
            }
        }
        Err(error) => ParsedArgs {
            error: Some(error.to_string()),
            ..ParsedArgs::default()
        },
    }
}

fn argv(args: &[String]) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(OsString::from("mcpace stdio"));
    argv.extend(args.iter().map(OsString::from));
    argv
}

pub(super) fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace stdio [--root <path>] [--client-id <id>] \
         [--session-id <id>] [--project-root <path>] \
         [--transport <stdio|streamable-http>]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "mcpace stdio starts a live MCP stdio server for local clients; mcp-server remains an internal compatibility command."
    );
    let _ = writeln!(
        stdout,
        "It speaks newline-delimited JSON-RPC over stdin/stdout and exposes a \
         focused MCPace management tool catalog."
    );
}

#[cfg(test)]
mod tests;
