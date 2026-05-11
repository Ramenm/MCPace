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

pub(super) fn parse_args(args: &[String]) -> ParsedArgs {
    let mut parsed = ParsedArgs::default();
    let mut index = 0usize;

    while index < args.len() {
        let token = normalize_flag(&args[index]);
        match token.as_str() {
            "doctor" | "readiness" => {
                if parsed.action.is_some() {
                    parsed.error = Some("verify accepts only one action".to_string());
                    return parsed;
                }
                parsed.action = Some(token);
                index += 1;
            }
            "--json" | "-json" => {
                parsed.json_output = true;
                index += 1;
            }
            "--root" | "-root" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("verify requires a path after --root".to_string());
                    return parsed;
                };
                parsed.root_override = Some(PathBuf::from(value));
                index += 2;
            }
            "-h" | "--help" | "-?" => {
                parsed.help = true;
                return parsed;
            }
            _ => {
                parsed.error = Some(format!(
                    "unsupported verify arguments in the Rust-only repo: {}",
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
        "Usage: mcpace verify <doctor|readiness> [--json] [--root <path>]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Implemented now:");
    let _ = writeln!(stdout, "  mcpace verify doctor [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace verify readiness [--json] [--root <path>]");
}

fn normalize_flag(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
