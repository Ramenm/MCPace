use super::actions::client_install_support_summary;
use crate::runtimepaths;
use crate::text_utils::normalize_flag;
use clap::{error::ErrorKind, Parser};
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Default, Clone)]
pub(super) struct ParsedArgs {
    pub(super) action: Option<String>,
    pub(super) json_output: bool,
    pub(super) help: bool,
    pub(super) root_override: Option<PathBuf>,
    pub(super) client_id: Option<String>,
    pub(super) session_id: Option<String>,
    pub(super) project_root: Option<String>,
    pub(super) transport: Option<String>,
    pub(super) metadata_json: Option<String>,
    pub(super) dry_run: bool,
    pub(super) diff: bool,
    pub(super) backup: Option<String>,
    pub(super) error: Option<String>,
}

#[derive(Debug, Parser)]
#[command(
    name = "mcpace advanced client",
    disable_version_flag = true,
    about = "Plan, export, install, restore, and list MCPace client integrations"
)]
struct ClientCli {
    #[arg(value_name = "ACTION")]
    action: Option<String>,

    #[arg(value_name = "CLIENT")]
    target: Option<String>,

    #[arg(value_name = "EXTRA")]
    extra: Vec<String>,

    #[arg(long = "json")]
    json_output: bool,

    #[arg(long = "root", value_name = "PATH")]
    root_override: Option<PathBuf>,

    #[arg(long = "client-id", value_name = "ID")]
    client_id: Option<String>,

    #[arg(long = "session-id", value_name = "ID")]
    session_id: Option<String>,

    #[arg(long = "project-root", value_name = "PATH")]
    project_root: Option<String>,

    #[arg(long = "transport", value_name = "stdio|streamable-http")]
    transport: Option<String>,

    #[arg(long = "metadata-json", value_name = "JSON")]
    metadata_json: Option<String>,

    #[arg(long = "dry-run")]
    dry_run: bool,

    #[arg(long = "diff")]
    diff: bool,

    #[arg(long = "backup", value_name = "ID|latest")]
    backup: Option<String>,

    #[arg(long = "latest")]
    latest: bool,
}

pub(super) fn parse_cli(args: &[String]) -> ParsedArgs {
    match ClientCli::try_parse_from(argv(args)) {
        Ok(cli) => parsed_from_cli(cli),
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

fn parsed_from_cli(cli: ClientCli) -> ParsedArgs {
    let action = cli.action.map(|value| normalize_flag(&value));
    let mut parsed = ParsedArgs {
        action,
        json_output: cli.json_output,
        help: false,
        root_override: cli.root_override,
        client_id: cli.client_id,
        session_id: cli.session_id,
        project_root: cli.project_root,
        transport: cli.transport,
        metadata_json: cli.metadata_json,
        dry_run: cli.dry_run,
        diff: cli.diff,
        backup: if cli.latest {
            Some("latest".to_string())
        } else {
            cli.backup
        },
        error: None,
    };

    if !cli.extra.is_empty() {
        parsed.error = Some(format!(
            "unsupported client arguments in the Rust-only repo: {}",
            cli.extra.join(" ")
        ));
        return parsed;
    }

    match parsed.action.as_deref() {
        Some("plan" | "list") | None => {
            if cli.target.is_some() {
                parsed.error = Some(format!(
                    "unsupported client arguments in the Rust-only repo: {}",
                    cli.target.unwrap_or_default()
                ));
            }
        }
        Some("install" | "export" | "restore") => {
            if parsed.client_id.is_none() {
                parsed.client_id = cli.target;
            } else if cli.target.is_some() {
                parsed.error = Some(
                    "client target was provided both positionally and with --client-id".to_string(),
                );
            }
            if parsed.client_id.is_none() {
                let action = parsed.action.clone().unwrap_or_default();
                parsed.error = Some(format!(
                    "client {action} requires a client target; use 'mcpace advanced client list' to inspect supported surfaces"
                ));
            }
        }
        Some(other) => {
            parsed.error = Some(format!(
                "unsupported client arguments in the Rust-only repo: {}",
                other
            ));
        }
    }

    parsed
}

fn argv(args: &[String]) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(OsString::from("mcpace advanced client"));
    argv.extend(args.iter().map(OsString::from));
    argv
}

pub(super) fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace advanced client <plan|list|install|restore|export> [options]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Implemented now:");
    let _ = writeln!(
        stdout,
        "  mcpace advanced client list [--json] [--root <path>]"
    );
    let _ = writeln!(stdout, "  mcpace advanced client plan [--json] [--root <path>] [--client-id <id>] [--session-id <id>] [--project-root <path>] [--transport <stdio|streamable-http>] [--metadata-json <json>]");
    let _ = writeln!(
        stdout,
        "  mcpace advanced client install <client|all> [--json] [--root <path>] [--dry-run] [--diff]"
    );
    let _ = writeln!(
        stdout,
        "  mcpace advanced client restore <client|all> [--json] [--root <path>] [--backup <id|latest>]"
    );
    let _ = writeln!(stdout, "  mcpace advanced client export <client> [--json] [--root <path>] [--transport <stdio|streamable-http>] [--session-id <id>] [--project-root <path>] [--metadata-json <json>]");
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "client list shows the currently verified/generic client target catalog."
    );
    let _ = writeln!(stdout, "client plan inspects routing context, derived session leases, and server arbitration without starting a hub runtime.");
    let _ = writeln!(
        stdout,
        "client install currently supports {}. Use client install all to patch every catalog-declared local client that has an install writer. It writes only the MCPace-owned config entry or block and defaults to the broadest documented shared scope for that client surface.",
        client_install_support_summary()
    );
    let _ = writeln!(
        stdout,
        "Use --dry-run to preview install patches without writing client config files; add --diff to inspect the exact candidate config change. Real writes create a rollback backup that can be applied with client restore."
    );
    let _ = writeln!(
        stdout,
        "client export is HTTP-first: for local clients that document Streamable HTTP, it emits the configured MCPace URL (default {}). Override it with mcpace.config.json serve.publicUrl or MCPACE_PUBLIC_MCP_URL when a cloud/public connector must reach MCPace.",
        runtimepaths::default_local_mcp_url()
    );
}
