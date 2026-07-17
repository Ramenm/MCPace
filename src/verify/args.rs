use clap::{error::ErrorKind, Parser, ValueEnum};
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub(super) struct ParsedArgs {
    pub(super) action: Option<String>,
    pub(super) json_output: bool,
    pub(super) help: bool,
    pub(super) root_override: Option<PathBuf>,
    pub(super) error: Option<String>,
}

#[derive(Clone, Debug, ValueEnum)]
enum VerifyAction {
    Doctor,
    Readiness,
}

#[derive(Debug, Parser)]
#[command(
    name = "mcpace advanced doctor",
    disable_version_flag = true,
    about = "Run MCPace verification and readiness checks"
)]
struct VerifyCli {
    /// Verification action to run.
    #[arg(value_enum)]
    action: Option<VerifyAction>,

    /// Emit machine-readable JSON.
    #[arg(long = "json", short = 'j')]
    json_output: bool,

    /// MCPace project/root directory.
    #[arg(long = "root", value_name = "PATH")]
    root_override: Option<PathBuf>,
}

pub(super) fn parse_cli(args: &[String]) -> ParsedArgs {
    match VerifyCli::try_parse_from(argv(args)) {
        Ok(cli) => ParsedArgs {
            action: cli.action.map(|action| match action {
                VerifyAction::Doctor => "doctor".to_string(),
                VerifyAction::Readiness => "readiness".to_string(),
            }),
            json_output: cli.json_output,
            help: false,
            root_override: cli.root_override,
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
    argv.push(OsString::from("mcpace advanced doctor"));
    argv.extend(args.iter().map(OsString::from));
    argv
}

pub(super) fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace advanced doctor [--json] [--root <path>]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Implemented now:");
    let _ = writeln!(stdout, "  mcpace advanced doctor [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace status [--json] [--root <path>]");
}

#[cfg(test)]
mod tests;
