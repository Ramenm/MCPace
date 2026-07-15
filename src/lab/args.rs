use clap::{error::ErrorKind, Parser, ValueEnum};
use std::ffi::OsString;
use std::fmt;
use std::io::Write;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
enum LabArgsError {
    InvalidTimeout,
}

impl fmt::Display for LabArgsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTimeout => {
                formatter.write_str("lab probe --timeout-ms must be a positive integer")
            }
        }
    }
}

impl std::error::Error for LabArgsError {}

#[derive(Debug, Default)]
pub(super) struct ParsedArgs {
    pub(super) action: Option<String>,
    pub(super) json_output: bool,
    pub(super) help: bool,
    pub(super) id_filter: Option<String>,
    pub(super) root_override: Option<PathBuf>,
    pub(super) timeout_ms: Option<u64>,
    pub(super) refresh: bool,
    pub(super) error: Option<String>,
}

#[derive(Clone, Debug, ValueEnum)]
enum LabAction {
    List,
    Matrix,
    Show,
    Coverage,
    Gaps,
    Report,
    Run,
    Probe,
}

#[derive(Debug, Parser)]
#[command(
    name = "mcpace lab",
    disable_version_flag = true,
    about = "Inspect and probe MCP runtime classification evidence"
)]
struct LabCli {
    /// Lab action. Defaults to report.
    #[arg(value_enum)]
    action: Option<LabAction>,

    /// Emit machine-readable JSON.
    #[arg(long = "json", short = 'j')]
    json_output: bool,

    /// MCPace project/root directory.
    #[arg(long = "root", value_name = "PATH")]
    root_override: Option<PathBuf>,

    /// Scenario/server id filter for show and probe.
    #[arg(long = "id", alias = "name", value_name = "ID")]
    id_filter: Option<String>,

    /// Live probe timeout in milliseconds.
    #[arg(long = "timeout-ms", value_name = "MS")]
    timeout_ms: Option<u64>,

    /// Refresh cached live probe evidence.
    #[arg(long = "refresh")]
    refresh: bool,
}

pub(super) fn parse_cli(args: &[String]) -> ParsedArgs {
    match LabCli::try_parse_from(argv(args)) {
        Ok(cli) => match validate_timeout(cli.timeout_ms) {
            Ok(timeout_ms) => ParsedArgs {
                action: cli.action.map(lab_action_name),
                json_output: cli.json_output,
                help: false,
                id_filter: cli.id_filter,
                root_override: cli.root_override,
                timeout_ms,
                refresh: cli.refresh,
                error: None,
            },
            Err(error) => ParsedArgs {
                error: Some(error.to_string()),
                ..ParsedArgs::default()
            },
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

fn lab_action_name(action: LabAction) -> String {
    match action {
        LabAction::List => "list",
        LabAction::Matrix => "matrix",
        LabAction::Show => "show",
        LabAction::Coverage => "coverage",
        LabAction::Gaps => "gaps",
        LabAction::Report => "report",
        LabAction::Run => "run",
        LabAction::Probe => "probe",
    }
    .to_string()
}

fn validate_timeout(value: Option<u64>) -> Result<Option<u64>, LabArgsError> {
    match value {
        Some(0) => Err(LabArgsError::InvalidTimeout),
        other => Ok(other),
    }
}

fn argv(args: &[String]) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(OsString::from("mcpace lab"));
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
        "-timeout-ms" => "--timeout-ms",
        "-refresh" => "--refresh",
        "-id" => "--id",
        "-name" => "--name",
        "-?" => "--help",
        other => other,
    }
}

pub(super) fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace lab [report|list|matrix|coverage|gaps|show|probe] [options]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Default action: report. Implemented now:");
    let _ = writeln!(stdout, "  mcpace lab [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace lab list [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace lab matrix [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace lab coverage [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace lab gaps [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace lab report [--json] [--root <path>]");
    let _ = writeln!(
        stdout,
        "  mcpace lab show --id <scenario> [--json] [--root <path>]"
    );
    let _ = writeln!(
        stdout,
        "  mcpace lab probe [--id <server>] [--timeout-ms <ms>] [--refresh] [--json] [--root <path>]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "lab reads runtime fixtures plus a capability inventory and turns them into an evidence report: server -> evidence -> runtimeType/stateClass/effectClass -> concurrencyPolicy. The probe action performs a safe live MCP handshake (initialize + notifications/initialized + tools/list only) and never calls tools/call.");
}

#[cfg(test)]
mod tests;
