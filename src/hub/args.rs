use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub(super) struct ParsedArgs {
    pub(super) action: Option<String>,
    pub(super) json_output: bool,
    pub(super) help: bool,
    pub(super) root_override: Option<PathBuf>,
    pub(super) tail: usize,
    pub(super) foreground: bool,
    pub(super) error: Option<String>,
}

pub(super) fn parse_args(args: &[String]) -> ParsedArgs {
    let mut parsed = ParsedArgs {
        tail: 20,
        ..ParsedArgs::default()
    };
    let mut index = 0usize;

    while index < args.len() {
        let token = normalize_flag(&args[index]);
        match token.as_str() {
            "up" | "down" | "repair" | "status" | "logs" | "run" => {
                if parsed.action.is_some() {
                    parsed.error = Some("hub accepts only one action".to_string());
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
                    parsed.error = Some("hub requires a path after --root".to_string());
                    return parsed;
                };
                parsed.root_override = Some(PathBuf::from(value));
                index += 2;
            }
            "--tail" | "-tail" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("hub logs requires a value after --tail".to_string());
                    return parsed;
                };
                let parsed_tail = match value.parse::<usize>() {
                    Ok(number) if number > 0 => number,
                    _ => {
                        parsed.error =
                            Some("hub logs --tail must be a positive integer".to_string());
                        return parsed;
                    }
                };
                parsed.tail = parsed_tail;
                index += 2;
            }
            "--foreground" | "-foreground" => {
                parsed.foreground = true;
                index += 1;
            }
            "-h" | "--help" | "-?" => {
                parsed.help = true;
                return parsed;
            }
            _ => {
                parsed.error = Some(format!(
                    "unsupported hub arguments in the Rust-only repo: {}",
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
        "Usage: mcpace hub <up|down|repair|status|logs> [--json] [--root <path>] [--tail <n>] [--foreground]"
    );
    let _ = writeln!(stdout, "");
    let _ = writeln!(stdout, "Implemented now:");
    let _ = writeln!(stdout, "  mcpace hub up [--json] [--root <path>] [--foreground]");
    let _ = writeln!(stdout, "  mcpace hub down [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace hub repair [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace hub status [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace hub logs [--json] [--root <path>] [--tail <n>]");
}

fn normalize_flag(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
