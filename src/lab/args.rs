use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub(super) struct ParsedArgs {
    pub(super) action: Option<String>,
    pub(super) json_output: bool,
    pub(super) help: bool,
    pub(super) id_filter: Option<String>,
    pub(super) root_override: Option<PathBuf>,
    pub(super) error: Option<String>,
}

pub(super) fn parse_args(args: &[String]) -> ParsedArgs {
    let mut parsed = ParsedArgs::default();
    let mut index = 0usize;

    while index < args.len() {
        match args[index].trim().to_ascii_lowercase().as_str() {
            "list" | "matrix" | "show" | "coverage" | "gaps" | "report" | "run" => {
                if parsed.action.is_some() {
                    parsed.error = Some("lab accepts only one action".to_string());
                    return parsed;
                }
                parsed.action = Some(args[index].trim().to_ascii_lowercase());
                index += 1;
            }
            "--json" | "-json" => {
                parsed.json_output = true;
                index += 1;
            }
            "--root" | "-root" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("lab requires a path after --root".to_string());
                    return parsed;
                };
                parsed.root_override = Some(PathBuf::from(value));
                index += 2;
            }
            "--id" | "-id" | "--name" | "-name" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("lab show requires a value after --id".to_string());
                    return parsed;
                };
                parsed.id_filter = Some(value.to_string());
                index += 2;
            }
            "-h" | "--help" | "-?" => {
                parsed.help = true;
                return parsed;
            }
            _ => {
                parsed.error = Some(format!(
                    "unsupported lab arguments in the Rust-only repo: {}",
                    args[index]
                ));
                return parsed;
            }
        }
    }

    parsed
}

pub(super) fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace lab <list|matrix|coverage|gaps|report|show> [options]"
    );
    let _ = writeln!(stdout, "");
    let _ = writeln!(stdout, "Implemented now:");
    let _ = writeln!(stdout, "  mcpace lab list [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace lab matrix [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace lab coverage [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace lab gaps [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace lab report [--json] [--root <path>]");
    let _ = writeln!(
        stdout,
        "  mcpace lab show --id <scenario> [--json] [--root <path>]"
    );
    let _ = writeln!(stdout, "");
    let _ = writeln!(stdout, "lab reads runtime fixtures plus a capability inventory and turns them into a concrete backlog: what is covered now, what is only partially covered, and what is still blocked.");
}
