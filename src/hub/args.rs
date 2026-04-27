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
                    parsed.error = Some("hub accepts only one top-level action".to_string());
                    return parsed;
                }
                parsed.action = Some(token);
                index += 1;
            }
            "lease" | "leases" => {
                if parsed.action.is_some() {
                    parsed.error = Some("hub accepts only one top-level action".to_string());
                    return parsed;
                }
                parsed.action = Some("lease".to_string());
                if token == "leases" {
                    parsed.lease_action = Some("list".to_string());
                }
                index += 1;
            }
            "acquire" | "renew" | "release" | "list" => {
                if parsed.action.is_none() {
                    parsed.action = Some("lease".to_string());
                }
                if parsed.action.as_deref() != Some("lease") {
                    parsed.error = Some(format!(
                        "hub sub-action '{}' is only valid under hub lease",
                        token
                    ));
                    return parsed;
                }
                if parsed.lease_action.is_some() {
                    parsed.error = Some("hub lease accepts only one action".to_string());
                    return parsed;
                }
                parsed.lease_action = Some(token);
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
            "--server" | "-server" | "--name" | "-name" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("hub lease requires a value after --server".to_string());
                    return parsed;
                };
                parsed.server_name = Some(value.to_string());
                index += 2;
            }
            "--lease-id" | "-lease-id" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some(
                        "hub lease release/renew requires a value after --lease-id".to_string(),
                    );
                    return parsed;
                };
                parsed.lease_id = Some(value.to_string());
                index += 2;
            }
            "--client-id" | "-client-id" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("hub lease requires a value after --client-id".to_string());
                    return parsed;
                };
                parsed.client_id = Some(value.to_string());
                index += 2;
            }
            "--session-id" | "-session-id" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("hub lease requires a value after --session-id".to_string());
                    return parsed;
                };
                parsed.session_id = Some(value.to_string());
                index += 2;
            }
            "--project-root" | "-project-root" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("hub lease requires a value after --project-root".to_string());
                    return parsed;
                };
                parsed.project_root = Some(value.to_string());
                index += 2;
            }
            "--transport" | "-transport" | "--ingress" | "-ingress" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("hub lease requires a value after --transport".to_string());
                    return parsed;
                };
                parsed.transport = Some(value.to_string());
                index += 2;
            }
            "--metadata-json" | "-metadata-json" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("hub lease requires a JSON string after --metadata-json".to_string());
                    return parsed;
                };
                parsed.metadata_json = Some(value.to_string());
                index += 2;
            }
            "--ttl-ms" | "-ttl-ms" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("hub lease acquire/renew requires a value after --ttl-ms".to_string());
                    return parsed;
                };
                match value.parse::<u128>() {
                    Ok(number) if number > 0 => parsed.ttl_ms = Some(number),
                    _ => {
                        parsed.error =
                            Some("hub lease --ttl-ms must be a positive integer".to_string());
                        return parsed;
                    }
                }
                index += 2;
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
        "Usage: mcpace hub <up|down|repair|status|logs|lease> [--json] [--root <path>] [options]"
    );
    let _ = writeln!(stdout, "");
    let _ = writeln!(stdout, "Implemented now:");
    let _ = writeln!(
        stdout,
        "  mcpace hub up [--json] [--root <path>] [--foreground]"
    );
    let _ = writeln!(stdout, "  mcpace hub down [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace hub repair [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace hub status [--json] [--root <path>]");
    let _ = writeln!(
        stdout,
        "  mcpace hub logs [--json] [--root <path>] [--tail <n>]"
    );
    let _ = writeln!(stdout, "  mcpace hub lease list [--json] [--root <path>]");
    let _ = writeln!(
        stdout,
        "  mcpace hub lease acquire --server <name> [--json] [--root <path>] [--client-id <id>] [--session-id <id>] [--project-root <path>] [--transport <stdio|streamable-http>] [--ttl-ms <n>] [--metadata-json <json>]"
    );
    let _ = writeln!(
        stdout,
        "  mcpace hub lease renew --lease-id <id> [--json] [--root <path>] [--ttl-ms <n>]"
    );
    let _ = writeln!(
        stdout,
        "  mcpace hub lease release --lease-id <id> [--json] [--root <path>]"
    );
}

fn normalize_flag(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
