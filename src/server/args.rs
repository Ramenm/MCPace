use crate::text_utils::normalize_flag;
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
    pub(crate) auto_install: bool,
    pub(crate) auto_mode: bool,
    pub(crate) allow_review_install: bool,
    pub(crate) disabled: bool,
    pub(crate) refresh: bool,
    pub(crate) execution_mode: Option<String>,
    pub(crate) affinity: Vec<String>,
    pub(crate) queue_timeout_ms: Option<u64>,
    pub(crate) reuse_policy: Option<String>,
    pub(crate) max_workers: Option<usize>,
    pub(crate) max_in_flight_per_worker: Option<usize>,
    pub(crate) client_id: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) project_root: Option<String>,
    pub(crate) transport: Option<String>,
    pub(crate) metadata_json: Option<String>,
    pub(crate) error: Option<String>,
}

pub(super) fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace server <auto|install|discover|import|list|test|remove|enable|disable|sources|set-policy|instances|leases> [options]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Common commands:");
    let _ = writeln!(stdout, "  mcpace server install <path|package|url|command...> [--as <name>] [--path <path>...] [--dry-run]");
    let _ = writeln!(
        stdout,
        "  mcpace server import <mcp.json> [--dry-run] [--force] [--disabled]"
    );
    let _ = writeln!(
        stdout,
        "  mcpace server test [<name>|--name <server>] [--refresh]"
    );
    let _ = writeln!(stdout, "  mcpace server auto [query] [--json] [--dry-run]");
    let _ = writeln!(
        stdout,
        "  mcpace server discover [query] [--json] [--auto] [advanced flags]"
    );
    let _ = writeln!(stdout, "  mcpace server list [--json]");
    let _ = writeln!(stdout, "  mcpace server instances [--client-id <id>] [--session-id <chat>] [--project-root <path>] [--json]");
    let _ = writeln!(stdout, "  mcpace server leases [--json]");
    let _ = writeln!(stdout, "  mcpace server sources [--json]");
    let _ = writeln!(
        stdout,
        "  mcpace server remove|enable|disable <name> [--dry-run]"
    );
    let _ = writeln!(
        stdout,
        "  mcpace server set-policy <name> --mode <shared|serialized|session-isolated|project-isolated|pool> [--affinity client,project,chat] [--queue-timeout-ms <ms>] [--reuse-policy <sticky|ttl|never>] [--max-workers <n>] [--dry-run]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "Install auto-detects the server type and never adds a default server. Examples:"
    );
    let _ = writeln!(
        stdout,
        "  mcpace server import ./mcp.json                 # reuse an existing mcpServers config"
    );
    let _ = writeln!(stdout, "  mcpace server install . --as filesystem         # explicit filesystem server for this directory");
    let _ = writeln!(
        stdout,
        "  mcpace server install @modelcontextprotocol/server-filesystem --as filesystem --path ."
    );
    let _ = writeln!(
        stdout,
        "  mcpace server install pypi:mcp-server-time --as time"
    );
    let _ = writeln!(
        stdout,
        "  mcpace server install http://127.0.0.1:8010/mcp --as local-gateway"
    );
    let _ = writeln!(
        stdout,
        "  mcpace server install npx -y @modelcontextprotocol/server-filesystem . --as filesystem"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Import accepts either top-level mcpServers (Claude/Cursor style) or servers (VS Code style), skips MCPace's own client entry, preserves unrelated fields, auto-fills enabled/type when possible, and can park imported sources with --disabled.");
    let _ = writeln!(stdout, "Advanced still available: capabilities, candidates, add, --settings, --force, --disabled, --env, --header, --type.");
    let _ = writeln!(stdout, "Dynamic discovery examples:");
    let _ = writeln!(stdout, "  mcpace server auto --dry-run              # one-command auto mode: refresh when needed, install approved/trusted, probe");
    let _ = writeln!(stdout, "  mcpace server auto filesystem --json      # auto-select one server without choosing its type");
    let _ = writeln!(
        stdout,
        "  mcpace server discover filesystem --json  # advanced plan-only search"
    );
    let _ = writeln!(stdout, "Concurrency policy examples:");
    let _ = writeln!(stdout, "  mcpace server set-policy filesystem --mode session-isolated --affinity client,project,chat");
    let _ = writeln!(
        stdout,
        "  mcpace server set-policy fetch --mode pool --max-workers 4 --queue-timeout-ms 5000"
    );
    let _ = writeln!(
        stdout,
        "  mcpace server instances --client-id cursor --session-id chat-a --project-root ."
    );
    let _ = writeln!(stdout, "  mcpace server leases --json");
    let _ = writeln!(stdout, "Local path input such as . or /repo auto-installs the filesystem server only when you explicitly run install/up with that path.");
}

pub(super) fn parse_args(args: &[String]) -> ParsedArgs {
    let mut parsed = ParsedArgs::default();
    let mut index = 0usize;

    while index < args.len() {
        let token = normalize_flag(&args[index]);
        match token.as_str() {
            "--" => {
                if parsed.action.as_deref() == Some("install") {
                    if index + 1 >= args.len() {
                        parsed.error =
                            Some("server install -- requires a command after --".to_string());
                        return parsed;
                    }
                    let mut value = parsed.name_filter.take().unwrap_or_default();
                    if !value.trim().is_empty() {
                        value.push(' ');
                    }
                    value.push_str(&args[index + 1..].join(" "));
                    parsed.name_filter = Some(value);
                    break;
                }
                parsed.error =
                    Some("-- is only supported for server install command specs".to_string());
                return parsed;
            }
            "list" | "capabilities" | "sources" | "candidates" | "discover" | "auto" | "add"
            | "install" | "import" | "remove" | "enable" | "disable" | "test" | "set-policy"
            | "instances" | "leases" => {
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
            "--mode" | "-mode" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("server set-policy requires a value after --mode".to_string());
                    return parsed;
                };
                parsed.execution_mode = Some(value.to_string());
                index += 2;
            }
            "--affinity" | "-affinity" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some(
                        "server set-policy requires a comma-separated list after --affinity"
                            .to_string(),
                    );
                    return parsed;
                };
                parsed.affinity.extend(
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|item| !item.is_empty())
                        .map(str::to_string),
                );
                index += 2;
            }
            "--queue-timeout-ms" | "-queue-timeout-ms" | "--queue-timeout" | "-queue-timeout" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some(
                        "server set-policy requires a positive integer after --queue-timeout-ms"
                            .to_string(),
                    );
                    return parsed;
                };
                match value.trim().parse::<u64>() {
                    Ok(timeout) if timeout > 0 => parsed.queue_timeout_ms = Some(timeout),
                    _ => {
                        parsed.error = Some(
                            "server set-policy --queue-timeout-ms must be a positive integer"
                                .to_string(),
                        );
                        return parsed;
                    }
                }
                index += 2;
            }
            "--reuse-policy" | "-reuse-policy" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("server set-policy requires a value after --reuse-policy".to_string());
                    return parsed;
                };
                parsed.reuse_policy = Some(value.to_string());
                index += 2;
            }
            "--max-workers" | "-max-workers" | "--workers" | "-workers" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some(
                        "server set-policy requires a positive integer after --max-workers"
                            .to_string(),
                    );
                    return parsed;
                };
                match value.trim().parse::<usize>() {
                    Ok(count) if count > 0 => parsed.max_workers = Some(count),
                    _ => {
                        parsed.error = Some(
                            "server set-policy --max-workers must be a positive integer"
                                .to_string(),
                        );
                        return parsed;
                    }
                }
                index += 2;
            }
            "--max-in-flight-per-worker"
            | "-max-in-flight-per-worker"
            | "--in-flight"
            | "-in-flight" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("server set-policy requires a positive integer after --max-in-flight-per-worker".to_string());
                    return parsed;
                };
                match value.trim().parse::<usize>() {
                    Ok(count) if count > 0 => parsed.max_in_flight_per_worker = Some(count),
                    _ => {
                        parsed.error = Some("server set-policy --max-in-flight-per-worker must be a positive integer".to_string());
                        return parsed;
                    }
                }
                index += 2;
            }
            "--client-id" | "-client-id" | "--client" | "-client" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("server instances requires a value after --client-id".to_string());
                    return parsed;
                };
                parsed.client_id = Some(value.to_string());
                index += 2;
            }
            "--session-id" | "-session-id" | "--chat" | "-chat" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("server instances requires a value after --session-id".to_string());
                    return parsed;
                };
                parsed.session_id = Some(value.to_string());
                index += 2;
            }
            "--project-root" | "-project-root" | "--project" | "-project" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("server instances requires a value after --project-root".to_string());
                    return parsed;
                };
                parsed.project_root = Some(value.to_string());
                index += 2;
            }
            "--transport" | "-transport" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("server instances requires a value after --transport".to_string());
                    return parsed;
                };
                parsed.transport = Some(value.to_string());
                index += 2;
            }
            "--metadata-json" | "-metadata-json" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some(
                        "server instances requires a JSON value after --metadata-json".to_string(),
                    );
                    return parsed;
                };
                parsed.metadata_json = Some(value.to_string());
                index += 2;
            }
            "--refresh" | "--refresh-registry" | "-refresh-registry" => {
                parsed.refresh = true;
                index += 1;
            }
            "--auto" | "--auto-mode" => {
                parsed.auto_mode = true;
                parsed.auto_install = true;
                index += 1;
            }
            "--auto-install" | "--apply" => {
                parsed.auto_install = true;
                index += 1;
            }
            "--allow-review" | "--review-ok" => {
                parsed.allow_review_install = true;
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
                    Some("install") | Some("discover") | Some("auto")
                ) {
                    let value = match parsed.name_filter.take() {
                        Some(existing) if !existing.trim().is_empty() => {
                            format!("{} {}", existing, args[index])
                        }
                        _ => args[index].to_string(),
                    };
                    parsed.name_filter = Some(value);
                    index += 1;
                    continue;
                }
                if matches!(
                    parsed.action.as_deref(),
                    Some("add")
                        | Some("remove")
                        | Some("enable")
                        | Some("disable")
                        | Some("test")
                        | Some("set-policy")
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
