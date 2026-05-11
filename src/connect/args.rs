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

pub(super) fn parse_args(args: &[String]) -> ParsedArgs {
    let mut parsed = ParsedArgs::default();
    let mut index = 0usize;

    while index < args.len() {
        let token = normalize_flag(&args[index]);
        match token.as_str() {
            "--json" | "-json" => {
                parsed.json_output = true;
                index += 1;
            }
            "--root" | "-root" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("connect requires a path after --root".to_string());
                    return parsed;
                };
                parsed.root_override = Some(PathBuf::from(value));
                index += 2;
            }
            "--client" | "-client" | "--client-id" | "-client-id" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("connect requires a value after --client".to_string());
                    return parsed;
                };
                parsed.client_id = Some(value.to_string());
                index += 2;
            }
            "--server" | "-server" | "--name" | "-name" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("connect requires a value after --server".to_string());
                    return parsed;
                };
                parsed.server_name = Some(value.to_string());
                index += 2;
            }
            "-h" | "--help" | "-?" => {
                parsed.help = true;
                return parsed;
            }
            _ => {
                if parsed.client_id.is_none() {
                    parsed.client_id = Some(args[index].to_string());
                    index += 1;
                    continue;
                }
                if parsed.server_name.is_none() {
                    parsed.server_name = Some(args[index].to_string());
                    index += 1;
                    continue;
                }
                parsed.error = Some(format!(
                    "unsupported connect argument in the Rust-only repo: {}",
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

fn normalize_flag(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
