use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub(crate) struct ParsedArgs {
    pub(crate) action: Option<String>,
    pub(crate) json_output: bool,
    pub(crate) help: bool,
    pub(crate) name_filter: Option<String>,
    pub(crate) root_override: Option<PathBuf>,
    pub(crate) server_type: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<String>,
    pub(crate) headers: Vec<String>,
    pub(crate) settings_path: Option<PathBuf>,
    pub(crate) import_path: Option<PathBuf>,
    pub(crate) install_name_override: Option<String>,
    pub(crate) paths: Vec<String>,
    pub(crate) timeout_ms: Option<u64>,
    pub(crate) dry_run: bool,
    pub(crate) force: bool,
    pub(crate) disabled: bool,
    pub(crate) refresh: bool,
    pub(crate) error: Option<String>,
}

pub(super) fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "Usage: mcpace server <list|capabilities|sources|candidates|add|install|import|remove|enable|disable|test> [--json] [--root <path>] [--name <server>]");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Implemented now:");
    let _ = writeln!(stdout, "  mcpace server list [--json] [--root <path>]");
    let _ = writeln!(
        stdout,
        "  mcpace server capabilities [--json] [--root <path>] [--name <server>]"
    );
    let _ = writeln!(stdout, "  mcpace server sources [--json] [--root <path>]");
    let _ = writeln!(
        stdout,
        "  mcpace server candidates [--json] [--root <path>]"
    );
    let _ = writeln!(stdout, "  mcpace server install <npm-package|npm:package|pypi:package|oci:image|url> [--as <server-name>] [--type npm|pypi|oci|streamable-http] [--path <path>...] [--arg <arg>...] [--env KEY=VALUE...] [--settings <path>] [--dry-run] [--force] [--disabled] [--json]");
    let _ = writeln!(stdout, "  mcpace server install --url <url> [--as <server-name>] [--header KEY=VALUE...] [--settings <path>] [--dry-run] [--force] [--disabled] [--json]");
    let _ = writeln!(stdout, "  mcpace server install <name> --command <cmd> [--arg <arg>...] [--path <path>...] [--env KEY=VALUE...] [--settings <path>] [--dry-run] [--force] [--disabled] [--json]");
    let _ = writeln!(stdout, "  mcpace server add <name> --command <cmd> [--arg <arg>...] [--env KEY=VALUE...] [--settings <path>] [--dry-run] [--force] [--disabled] [--json]");
    let _ = writeln!(stdout, "  mcpace server add <name> --url <url> [--type http|streamable-http] [--header KEY=VALUE...] [--settings <path>] [--dry-run] [--force] [--disabled] [--json]");
    let _ = writeln!(
        stdout,
        "  mcpace server import --from <mcp-settings.json> [--settings <target.json>] [--dry-run] [--force] [--disabled] [--json]"
    );
    let _ = writeln!(
        stdout,
        "  mcpace server remove <name> [--settings <path>] [--dry-run] [--json]"
    );
    let _ = writeln!(
        stdout,
        "  mcpace server enable <name> [--settings <path>] [--dry-run] [--json]"
    );
    let _ = writeln!(
        stdout,
        "  mcpace server disable <name> [--settings <path>] [--dry-run] [--json]"
    );
    let _ = writeln!(
        stdout,
        "  mcpace server test [<name>|--name <server>] [--timeout-ms <ms>] [--refresh] [--json] [--root <path>]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "server install derives a reviewable MCP settings fragment from a package spec, URL, or command; server add writes fully custom fragments under mcp_settings.d/ by default; server import copies existing mcpServers blocks into MCPace fragments; server enable/disable flips the entry without deleting it; server remove deletes the entry from the source where it was found; and server test performs live stdio initialize/tools-list smoke probes without editing JSON by hand.");
}

pub(super) fn parse_args(args: &[String]) -> ParsedArgs {
    let mut parsed = ParsedArgs::default();
    let mut index = 0usize;

    while index < args.len() {
        let token = normalize_flag(&args[index]);
        match token.as_str() {
            "list" | "capabilities" | "sources" | "candidates" | "add" | "install" | "import"
            | "remove" | "enable" | "disable" | "test" => {
                if parsed.action.is_some() {
                    parsed.error = Some("server accepts only one action".to_string());
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
                    parsed.error = Some("server requires a path after --root".to_string());
                    return parsed;
                };
                parsed.root_override = Some(PathBuf::from(value));
                index += 2;
            }
            "--name" | "-name" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("server requires a value after --name".to_string());
                    return parsed;
                };
                parsed.name_filter = Some(value.to_string());
                index += 2;
            }
            "--as" | "-as" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("server install requires a server name after --as".to_string());
                    return parsed;
                };
                parsed.install_name_override = Some(value.to_string());
                index += 2;
            }
            "--path" | "-path" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("server install requires a path after --path".to_string());
                    return parsed;
                };
                parsed.paths.push(value.to_string());
                index += 2;
            }
            "--type" | "-type" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("server add requires a value after --type".to_string());
                    return parsed;
                };
                parsed.server_type = Some(value.to_string());
                index += 2;
            }
            "--command" | "-command" | "--cmd" | "-cmd" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("server add requires a value after --command".to_string());
                    return parsed;
                };
                parsed.command = Some(value.to_string());
                index += 2;
            }
            "--url" | "-url" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("server add requires a value after --url".to_string());
                    return parsed;
                };
                parsed.url = Some(value.to_string());
                index += 2;
            }
            "--arg" | "-arg" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("server add requires a value after --arg".to_string());
                    return parsed;
                };
                parsed.args.push(value.to_string());
                index += 2;
            }
            "--env" | "-env" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("server add requires KEY=VALUE after --env".to_string());
                    return parsed;
                };
                parsed.env.push(value.to_string());
                index += 2;
            }
            "--header" | "-header" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("server add requires KEY=VALUE after --header".to_string());
                    return parsed;
                };
                parsed.headers.push(value.to_string());
                index += 2;
            }
            "--settings" | "-settings" | "--source" | "-source" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some(
                        "server add/import/remove requires a path after --settings".to_string(),
                    );
                    return parsed;
                };
                parsed.settings_path = Some(PathBuf::from(value));
                index += 2;
            }
            "--from" | "-from" | "--file" | "-file" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("server import requires a path after --from".to_string());
                    return parsed;
                };
                parsed.import_path = Some(PathBuf::from(value));
                index += 2;
            }
            "--timeout-ms" | "-timeout-ms" | "--timeout" | "-timeout" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("server test requires a value after --timeout-ms".to_string());
                    return parsed;
                };
                match value.trim().parse::<u64>() {
                    Ok(timeout) if timeout > 0 => parsed.timeout_ms = Some(timeout),
                    _ => {
                        parsed.error =
                            Some("server test --timeout-ms must be a positive integer".to_string());
                        return parsed;
                    }
                }
                index += 2;
            }
            "--refresh" => {
                parsed.refresh = true;
                index += 1;
            }
            "--dry-run" => {
                parsed.dry_run = true;
                index += 1;
            }
            "--force" => {
                parsed.force = true;
                index += 1;
            }
            "--disabled" => {
                parsed.disabled = true;
                index += 1;
            }
            "-h" | "--help" | "-?" => {
                parsed.help = true;
                return parsed;
            }
            _ => {
                if parsed.action.as_deref() == Some("import") && parsed.import_path.is_none() {
                    parsed.import_path = Some(PathBuf::from(&args[index]));
                    index += 1;
                    continue;
                }
                if matches!(
                    parsed.action.as_deref(),
                    Some("add")
                        | Some("install")
                        | Some("remove")
                        | Some("enable")
                        | Some("disable")
                        | Some("test")
                ) && parsed.name_filter.is_none()
                {
                    parsed.name_filter = Some(args[index].to_string());
                    index += 1;
                    continue;
                }
                parsed.error = Some(format!(
                    "unsupported server arguments in the Rust-only repo: {}",
                    args[index]
                ));
                return parsed;
            }
        }
    }

    parsed
}

fn normalize_flag(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
