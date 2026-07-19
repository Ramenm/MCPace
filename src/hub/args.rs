use crate::text_utils::normalize_flag;
use clap::{error::ErrorKind, Parser};
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub(super) struct ParsedArgs {
    pub(super) action: Option<String>,
    pub(super) lease_action: Option<String>,
    pub(super) json_output: bool,
    pub(super) help: bool,
    pub(super) root_override: Option<PathBuf>,
    pub(super) tail: usize,
    pub(super) foreground: bool,
    pub(super) server_name: Option<String>,
    pub(super) lease_id: Option<String>,
    pub(super) client_id: Option<String>,
    pub(super) session_id: Option<String>,
    pub(super) project_root: Option<String>,
    pub(super) transport: Option<String>,
    pub(super) metadata_json: Option<String>,
    pub(super) ttl_ms: Option<u128>,
    pub(super) takeover: bool,
    pub(super) error: Option<String>,
}

#[derive(Debug, Parser)]
#[command(
    name = "mcpace hub",
    disable_version_flag = true,
    about = "Manage the MCPace local hub runtime and lease store"
)]
struct HubCli {
    #[arg(value_name = "ACTION")]
    action: Option<String>,

    #[arg(value_name = "LEASE_ACTION")]
    lease_action: Option<String>,

    #[arg(value_name = "EXTRA")]
    extra: Vec<String>,

    #[arg(long = "json")]
    json_output: bool,

    #[arg(long = "root", value_name = "PATH")]
    root_override: Option<PathBuf>,

    #[arg(long = "tail", value_name = "N")]
    tail: Option<usize>,

    #[arg(long = "foreground")]
    foreground: bool,

    #[arg(long = "takeover")]
    takeover: bool,

    #[arg(long = "server", value_name = "NAME")]
    server_name: Option<String>,

    #[arg(long = "lease-id", value_name = "ID")]
    lease_id: Option<String>,

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

    #[arg(long = "ttl-ms", value_name = "MS")]
    ttl_ms: Option<u128>,
}

pub(super) fn parse_cli(args: &[String]) -> ParsedArgs {
    match HubCli::try_parse_from(argv(args)) {
        Ok(cli) => parsed_from_cli(cli),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            ParsedArgs {
                help: true,
                tail: 20,
                ..ParsedArgs::default()
            }
        }
        Err(error) => ParsedArgs {
            error: Some(error.to_string()),
            tail: 20,
            ..ParsedArgs::default()
        },
    }
}

fn parsed_from_cli(cli: HubCli) -> ParsedArgs {
    let mut action = cli.action.map(|value| normalize_flag(&value));
    let mut lease_action = cli.lease_action.map(|value| normalize_flag(&value));

    if action.as_deref() == Some("leases") {
        action = Some("lease".to_string());
        lease_action.get_or_insert_with(|| "list".to_string());
    }
    if matches!(
        action.as_deref(),
        Some("acquire" | "renew" | "release" | "list")
    ) {
        lease_action = action.take();
        action = Some("lease".to_string());
    }

    let mut parsed = ParsedArgs {
        action,
        lease_action,
        json_output: cli.json_output,
        help: false,
        root_override: cli.root_override,
        tail: cli.tail.unwrap_or(20),
        foreground: cli.foreground,
        server_name: cli.server_name,
        lease_id: cli.lease_id,
        client_id: cli.client_id,
        session_id: cli.session_id,
        project_root: cli.project_root,
        transport: cli.transport,
        metadata_json: cli.metadata_json,
        ttl_ms: cli.ttl_ms,
        takeover: cli.takeover,
        error: None,
    };

    if parsed.tail == 0 {
        parsed.error = Some("hub logs --tail must be a positive integer".to_string());
        return parsed;
    }
    if parsed.ttl_ms == Some(0) {
        parsed.error = Some("hub lease --ttl-ms must be a positive integer".to_string());
        return parsed;
    }
    if !cli.extra.is_empty() {
        parsed.error = Some(format!(
            "unsupported hub arguments in the Rust-only repo: {}",
            cli.extra.join(" ")
        ));
        return parsed;
    }

    match parsed.action.as_deref() {
        None | Some("up" | "down" | "repair" | "status" | "logs" | "run" | "lease") => {}
        Some(other) => {
            parsed.error = Some(format!(
                "unsupported hub arguments in the Rust-only repo: {}",
                other
            ));
        }
    }
    if parsed.action.as_deref() != Some("lease") && parsed.lease_action.is_some() {
        parsed.error = Some(format!(
            "hub sub-action '{}' is only valid under hub lease",
            parsed.lease_action.clone().unwrap_or_default()
        ));
    }
    if let Some(lease_action) = parsed.lease_action.as_deref() {
        if !matches!(lease_action, "acquire" | "renew" | "release" | "list") {
            parsed.error = Some(format!(
                "hub sub-action '{}' is only valid under hub lease",
                lease_action
            ));
        }
    }

    parsed
}

fn argv(args: &[String]) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(OsString::from("mcpace hub"));
    argv.extend(args.iter().map(OsString::from));
    argv
}

pub(super) fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "`mcpace hub` is a hidden internal entrypoint.");
    let _ = writeln!(stdout, "Use grouped advanced commands instead:");
    let _ = writeln!(
        stdout,
        "  mcpace advanced runtime logs [--json] [--root <path>] [--tail <n>]"
    );
    let _ = writeln!(
        stdout,
        "  mcpace advanced runtime repair [--json] [--root <path>]"
    );
    let _ = writeln!(
        stdout,
        "  mcpace advanced lease list [--json] [--root <path>]"
    );
    let _ = writeln!(
        stdout,
        "  mcpace advanced lease acquire --server <name> [lease options]"
    );
    let _ = writeln!(
        stdout,
        "  mcpace advanced lease renew --lease-id <id> [--ttl-ms <n>]"
    );
    let _ = writeln!(stdout, "  mcpace advanced lease release --lease-id <id>");
}
