use crate::text_utils::normalize_flag;
use clap::{error::ErrorKind, Parser};
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

#[cfg(test)]
mod tests;

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
    let _ = writeln!(stdout, "  mcpace server auto --dry-run              # use embedded/local approved catalog, install trusted candidates, probe");
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

#[derive(Debug, Parser)]
#[command(
    name = "mcpace server",
    disable_version_flag = true,
    about = "Manage installed and discovered MCP servers"
)]
struct ServerCli {
    #[arg(value_name = "ACTION")]
    action: Option<String>,

    #[arg(value_name = "VALUE")]
    values: Vec<String>,

    #[arg(long = "json")]
    json_output: bool,

    #[arg(long = "root", value_name = "PATH")]
    root_override: Option<PathBuf>,

    #[arg(long = "name", value_name = "NAME")]
    name_filter: Option<String>,

    #[arg(long = "as", value_name = "NAME")]
    install_name_override: Option<String>,

    #[arg(long = "path", value_name = "PATH")]
    paths: Vec<String>,

    #[arg(long = "type", value_name = "TYPE")]
    server_type: Option<String>,

    #[arg(long = "command", alias = "cmd", value_name = "COMMAND")]
    command: Option<String>,

    #[arg(long = "url", value_name = "URL")]
    url: Option<String>,

    #[arg(long = "arg", value_name = "ARG")]
    args: Vec<String>,

    #[arg(long = "env", value_name = "KEY=VALUE")]
    env: Vec<String>,

    #[arg(long = "header", value_name = "KEY=VALUE")]
    headers: Vec<String>,

    #[arg(long = "settings", alias = "source", value_name = "PATH")]
    settings_path: Option<PathBuf>,

    #[arg(long = "from", alias = "file", value_name = "PATH")]
    import_path: Option<PathBuf>,

    #[arg(long = "timeout-ms", alias = "timeout", value_name = "MS")]
    timeout_ms: Option<u64>,

    #[arg(long = "mode", value_name = "MODE")]
    execution_mode: Option<String>,

    #[arg(long = "affinity", value_name = "client,project,chat")]
    affinity: Vec<String>,

    #[arg(long = "queue-timeout-ms", alias = "queue-timeout", value_name = "MS")]
    queue_timeout_ms: Option<u64>,

    #[arg(long = "reuse-policy", value_name = "POLICY")]
    reuse_policy: Option<String>,

    #[arg(long = "max-workers", alias = "workers", value_name = "N")]
    max_workers: Option<usize>,

    #[arg(
        long = "max-in-flight-per-worker",
        alias = "in-flight",
        value_name = "N"
    )]
    max_in_flight_per_worker: Option<usize>,

    #[arg(long = "client-id", alias = "client", value_name = "ID")]
    client_id: Option<String>,

    #[arg(long = "session-id", alias = "chat", value_name = "ID")]
    session_id: Option<String>,

    #[arg(long = "project-root", alias = "project", value_name = "PATH")]
    project_root: Option<String>,

    #[arg(long = "transport", value_name = "stdio|streamable-http")]
    transport: Option<String>,

    #[arg(long = "metadata-json", value_name = "JSON")]
    metadata_json: Option<String>,

    #[arg(long = "refresh", alias = "refresh-registry")]
    refresh: bool,

    #[arg(long = "auto", alias = "auto-mode")]
    auto_mode: bool,

    #[arg(long = "auto-install", alias = "apply")]
    auto_install: bool,

    #[arg(long = "allow-review", alias = "review-ok")]
    allow_review_install: bool,

    #[arg(long = "dry-run")]
    dry_run: bool,

    #[arg(long = "force")]
    force: bool,

    #[arg(long = "disabled")]
    disabled: bool,
}

pub(super) fn parse_cli(args: &[String]) -> ParsedArgs {
    match ServerCli::try_parse_from(argv(args)) {
        Ok(cli) => parsed_from_cli(cli),
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

fn parsed_from_cli(cli: ServerCli) -> ParsedArgs {
    let action = cli.action.as_deref().map(normalize_flag);
    let auto_mode = cli.auto_mode;
    let mut parsed = ParsedArgs {
        action: action.clone(),
        json_output: cli.json_output,
        help: false,
        name_filter: cli.name_filter,
        root_override: cli.root_override,
        server_type: cli.server_type,
        command: cli.command,
        url: cli.url,
        args: cli.args,
        env: cli.env,
        headers: cli.headers,
        settings_path: cli.settings_path,
        import_path: cli.import_path,
        install_name_override: cli.install_name_override,
        paths: cli.paths,
        timeout_ms: cli.timeout_ms,
        dry_run: cli.dry_run,
        force: cli.force,
        auto_install: cli.auto_install || auto_mode,
        auto_mode,
        allow_review_install: cli.allow_review_install,
        disabled: cli.disabled,
        refresh: cli.refresh,
        execution_mode: cli.execution_mode,
        affinity: split_affinity(cli.affinity),
        queue_timeout_ms: cli.queue_timeout_ms,
        reuse_policy: cli.reuse_policy,
        max_workers: cli.max_workers,
        max_in_flight_per_worker: cli.max_in_flight_per_worker,
        client_id: cli.client_id,
        session_id: cli.session_id,
        project_root: cli.project_root,
        transport: cli.transport,
        metadata_json: cli.metadata_json,
        error: None,
    };

    if !matches!(
        action.as_deref(),
        None | Some(
            "list"
                | "capabilities"
                | "sources"
                | "candidates"
                | "discover"
                | "auto"
                | "add"
                | "install"
                | "import"
                | "remove"
                | "enable"
                | "disable"
                | "test"
                | "set-policy"
                | "instances"
                | "leases"
        )
    ) {
        parsed.error = Some(format!(
            "unsupported server arguments in the Rust-only repo: {}",
            cli.action.unwrap_or_default()
        ));
        return parsed;
    }

    if parsed.timeout_ms == Some(0) {
        parsed.error = Some("server test --timeout-ms must be a positive integer".to_string());
        return parsed;
    }
    if parsed.queue_timeout_ms == Some(0) {
        parsed.error =
            Some("server set-policy --queue-timeout-ms must be a positive integer".to_string());
        return parsed;
    }
    if parsed.max_workers == Some(0) {
        parsed.error =
            Some("server set-policy --max-workers must be a positive integer".to_string());
        return parsed;
    }
    if parsed.max_in_flight_per_worker == Some(0) {
        parsed.error = Some(
            "server set-policy --max-in-flight-per-worker must be a positive integer".to_string(),
        );
        return parsed;
    }

    apply_positionals(&mut parsed, cli.values);
    parsed
}

fn split_affinity(values: Vec<String>) -> Vec<String> {
    values
        .iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn apply_positionals(parsed: &mut ParsedArgs, values: Vec<String>) {
    let values = values
        .into_iter()
        .map(|value| {
            value
                .strip_prefix(ESCAPED_SINGLE_DASH_PREFIX)
                .unwrap_or(&value)
                .to_string()
        })
        .collect::<Vec<_>>();
    if values.is_empty() || parsed.error.is_some() {
        return;
    }

    match parsed.action.as_deref() {
        Some("import") if parsed.import_path.is_none() => {
            parsed.import_path = values.first().map(PathBuf::from);
            if values.len() > 1 {
                parsed.error = Some(format!(
                    "unsupported server arguments in the Rust-only repo: {}",
                    values[1..].join(" ")
                ));
            }
        }
        Some("install" | "discover" | "auto") => {
            let mut value = parsed.name_filter.take().unwrap_or_default();
            if !value.trim().is_empty() && !values.is_empty() {
                value.push(' ');
            }
            value.push_str(&values.join(" "));
            parsed.name_filter = Some(value);
        }
        Some("add" | "remove" | "enable" | "disable" | "test" | "set-policy")
            if parsed.name_filter.is_none() =>
        {
            parsed.name_filter = values.first().cloned();
            if values.len() > 1 {
                parsed.error = Some(format!(
                    "unsupported server arguments in the Rust-only repo: {}",
                    values[1..].join(" ")
                ));
            }
        }
        _ => {
            parsed.error = Some(format!(
                "unsupported server arguments in the Rust-only repo: {}",
                values.join(" ")
            ));
        }
    }
}

const ESCAPED_SINGLE_DASH_PREFIX: &str = "\u{e000}mcpace-single-dash:";

fn argv(args: &[String]) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(OsString::from("mcpace server"));
    argv.extend(args.iter().map(|arg| {
        let normalized = normalize_compat_arg(arg);
        if normalized.starts_with('-') && !normalized.starts_with("--") && normalized != "-" {
            OsString::from(format!("{}{}", ESCAPED_SINGLE_DASH_PREFIX, normalized))
        } else {
            OsString::from(normalized)
        }
    }));
    argv
}

fn normalize_compat_arg(arg: &str) -> String {
    match normalize_flag(arg).as_str() {
        "-json" => "--json".to_string(),
        "-root" => "--root".to_string(),
        "-name" => "--name".to_string(),
        "-as" => "--as".to_string(),
        "-path" => "--path".to_string(),
        "-type" => "--type".to_string(),
        "-command" => "--command".to_string(),
        "-cmd" => "--cmd".to_string(),
        "-url" => "--url".to_string(),
        "-arg" => "--arg".to_string(),
        "-env" => "--env".to_string(),
        "-header" => "--header".to_string(),
        "-settings" => "--settings".to_string(),
        "-source" => "--source".to_string(),
        "-from" => "--from".to_string(),
        "-file" => "--file".to_string(),
        "-timeout-ms" => "--timeout-ms".to_string(),
        "-timeout" => "--timeout".to_string(),
        "-mode" => "--mode".to_string(),
        "-affinity" => "--affinity".to_string(),
        "-queue-timeout-ms" => "--queue-timeout-ms".to_string(),
        "-queue-timeout" => "--queue-timeout".to_string(),
        "-reuse-policy" => "--reuse-policy".to_string(),
        "-max-workers" => "--max-workers".to_string(),
        "-workers" => "--workers".to_string(),
        "-max-in-flight-per-worker" => "--max-in-flight-per-worker".to_string(),
        "-in-flight" => "--in-flight".to_string(),
        "-client-id" => "--client-id".to_string(),
        "-client" => "--client".to_string(),
        "-session-id" => "--session-id".to_string(),
        "-chat" => "--chat".to_string(),
        "-project-root" => "--project-root".to_string(),
        "-project" => "--project".to_string(),
        "-transport" => "--transport".to_string(),
        "-metadata-json" => "--metadata-json".to_string(),
        "-refresh-registry" => "--refresh-registry".to_string(),
        "-?" => "--help".to_string(),
        _ => arg.to_string(),
    }
}
