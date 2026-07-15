use clap::{error::ErrorKind, Parser};
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Default, Clone)]
pub(super) struct ParsedArgs {
    pub(super) json_output: bool,
    pub(super) help: bool,
    pub(super) root_override: Option<PathBuf>,
    pub(super) client_id: Option<String>,
    pub(super) server_name: Option<String>,
    pub(super) error: Option<String>,
}

#[derive(Debug, Parser)]
#[command(
    name = "mcpace connect",
    disable_version_flag = true,
    about = "Show client-first MCPace connection guidance"
)]
struct ConnectCli {
    /// Optional positional client and server name.
    #[arg(value_name = "client/server", num_args = 0..=2)]
    positionals: Vec<String>,

    /// Emit machine-readable JSON.
    #[arg(long = "json", short = 'j')]
    json_output: bool,

    /// MCPace project/root directory.
    #[arg(long = "root", value_name = "PATH")]
    root_override: Option<PathBuf>,

    /// Client catalog id.
    #[arg(long = "client", alias = "client-id", value_name = "ID")]
    client_id: Option<String>,

    /// Upstream server name.
    #[arg(long = "server", alias = "name", value_name = "NAME")]
    server_name: Option<String>,
}

pub(super) fn parse_cli(args: &[String]) -> ParsedArgs {
    match ConnectCli::try_parse_from(argv(args)) {
        Ok(cli) => compose(cli),
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

fn compose(cli: ConnectCli) -> ParsedArgs {
    let mut positionals = cli.positionals.into_iter();
    let first = positionals.next();
    let second = positionals.next();

    let (client_id, server_name, extra_error) =
        match (cli.client_id, cli.server_name, first, second) {
            (client_flag, server_flag, None, None) => (client_flag, server_flag, None),
            (None, None, first, second) => (first, second, None),
            (Some(client), None, Some(server), None) => (Some(client), Some(server), None),
            (None, Some(server), Some(client), None) => (Some(client), Some(server), None),
            (Some(_), Some(_), Some(extra), _)
            | (Some(_), _, Some(_), Some(extra))
            | (_, Some(_), Some(_), Some(extra)) => (
                None,
                None,
                Some(format!(
                    "unsupported connect argument in the Rust-only repo: {}",
                    extra
                )),
            ),
            _ => (
                None,
                None,
                Some("unsupported connect argument combination in the Rust-only repo".to_string()),
            ),
        };

    ParsedArgs {
        json_output: cli.json_output,
        help: false,
        root_override: cli.root_override,
        client_id,
        server_name,
        error: extra_error,
    }
}

fn argv(args: &[String]) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(OsString::from("mcpace connect"));
    argv.extend(
        args.iter()
            .map(|arg| OsString::from(normalize_compat_flag(arg))),
    );
    argv
}

fn normalize_compat_flag(arg: &str) -> &str {
    match arg {
        "-json" => "--json",
        "-root" => "--root",
        "-client" => "--client",
        "-client-id" => "--client-id",
        "-server" => "--server",
        "-name" => "--name",
        "-?" => "--help",
        other => other,
    }
}

pub(super) fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace connect [<client>] [--server <name>] [--json] [--root <path>]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Shows the client-first next steps for wiring MCPace into a local MCP client without editing JSON by hand.");
    let _ = writeln!(stdout, "It is read-only: it resolves the MCPace endpoint, upstream server sources, recommended client target, blockers, and exact follow-up commands.");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Examples:");
    let _ = writeln!(stdout, "  mcpace connect");
    let _ = writeln!(stdout, "  mcpace connect codex");
    let _ = writeln!(
        stdout,
        "  mcpace connect --client cursor-local --server filesystem --json"
    );
}

#[cfg(test)]
mod tests;
