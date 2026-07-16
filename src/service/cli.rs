use crate::resources;
use clap::{error::ErrorKind, Parser};
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug)]
pub(crate) struct ParsedArgs {
    pub(crate) action: String,
    pub(crate) json_output: bool,
    pub(crate) root_override: Option<PathBuf>,
    pub(crate) host: Option<String>,
    pub(crate) port: Option<u16>,
    pub(crate) max_connections: Option<usize>,
    pub(crate) io_timeout_ms: Option<u64>,
    pub(crate) max_body_bytes: Option<usize>,
    pub(crate) overview_cache_ms: Option<u64>,
    pub(crate) dry_run: bool,
    pub(crate) no_enable: bool,
    pub(crate) help: bool,
    pub(crate) error: Option<String>,
}

impl Default for ParsedArgs {
    fn default() -> Self {
        Self {
            action: "status".to_string(),
            json_output: false,
            root_override: None,
            host: None,
            port: None,
            max_connections: None,
            io_timeout_ms: None,
            max_body_bytes: None,
            overview_cache_ms: None,
            dry_run: false,
            no_enable: false,
            help: false,
            error: None,
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "mcpace autostart",
    disable_version_flag = true,
    about = "Manage the visible MCPace Agent user-level autostart entry"
)]
struct ServiceCli {
    #[arg(value_name = "enable|repair|status|verify|disable|print")]
    action: Option<String>,

    #[arg(value_name = "EXTRA")]
    extra: Vec<String>,

    #[arg(long = "json")]
    json_output: bool,

    #[arg(long = "root", value_name = "PATH")]
    root_override: Option<PathBuf>,

    #[arg(long = "host", value_name = "ADDR")]
    host: Option<String>,

    #[arg(long = "port", value_name = "N")]
    port: Option<u16>,

    #[arg(long = "max-connections", value_name = "N")]
    max_connections: Option<usize>,

    #[arg(long = "io-timeout-ms", value_name = "MS")]
    io_timeout_ms: Option<u64>,

    #[arg(long = "max-body-bytes", value_name = "N")]
    max_body_bytes: Option<usize>,

    #[arg(long = "overview-cache-ms", value_name = "MS")]
    overview_cache_ms: Option<u64>,

    #[arg(long = "dry-run")]
    dry_run: bool,

    #[arg(long = "no-enable")]
    no_enable: bool,
}

pub(crate) fn parse_cli(args: &[String]) -> ParsedArgs {
    match ServiceCli::try_parse_from(argv(args)) {
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

fn parsed_from_cli(cli: ServiceCli) -> ParsedArgs {
    let mut parsed = ParsedArgs {
        action: cli
            .action
            .as_deref()
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| "status".to_string()),
        json_output: cli.json_output,
        root_override: cli.root_override,
        host: cli.host,
        port: cli.port,
        max_connections: cli.max_connections,
        io_timeout_ms: cli.io_timeout_ms,
        max_body_bytes: cli.max_body_bytes,
        overview_cache_ms: cli.overview_cache_ms,
        dry_run: cli.dry_run,
        no_enable: cli.no_enable,
        help: false,
        error: None,
    };

    if !cli.extra.is_empty() {
        parsed.error = Some(format!(
            "unsupported autostart argument: {}",
            cli.extra.join(" ")
        ));
        return parsed;
    }
    if parsed.max_connections == Some(0) {
        parsed.error = Some("autostart --max-connections must be a positive integer".to_string());
        return parsed;
    }
    if parsed.io_timeout_ms == Some(0) {
        parsed.error = Some("autostart --io-timeout-ms must be a positive integer".to_string());
        return parsed;
    }
    if parsed.max_body_bytes == Some(0) {
        parsed.error = Some("autostart --max-body-bytes must be a positive integer".to_string());
        return parsed;
    }

    parsed
}

fn argv(args: &[String]) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(OsString::from("mcpace autostart"));
    argv.extend(
        args.iter()
            .map(|arg| OsString::from(normalize_compat_arg(arg))),
    );
    argv
}

fn normalize_compat_arg(arg: &str) -> String {
    match arg {
        "-json" => "--json".to_string(),
        "-root" => "--root".to_string(),
        "-host" => "--host".to_string(),
        "-port" => "--port".to_string(),
        "-max-connections" => "--max-connections".to_string(),
        "-io-timeout-ms" => "--io-timeout-ms".to_string(),
        "-max-body-bytes" => "--max-body-bytes".to_string(),
        "-overview-cache-ms" => "--overview-cache-ms".to_string(),
        "-?" => "--help".to_string(),
        _ => arg.to_string(),
    }
}

pub(crate) fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "Usage: mcpace autostart <enable|repair|status|verify|disable|print> [--json] [--root <path>] [--host <addr>] [--port <n>] [--max-connections <n>] [--io-timeout-ms <n>] [--max-body-bytes <n>] [--overview-cache-ms <n>] [--dry-run] [--no-enable]");
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "Installs or repairs the user-level MCPace Agent started by `mcpace up`."
    );
    let _ = writeln!(stdout, "On Windows the login item launches mcpace-agent-launcher.exe, which supervises `mcpace agent run --autostart` without opening a terminal window.");
    let _ = writeln!(stdout, "On Linux systemd --user restarts non-zero exits; on macOS a LaunchAgent keeps the managed runtime available after login.");
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "Serve resource defaults: max connections={}, IO timeout={}ms, max body={} bytes, overview cache={}ms.",
        resources::default_http_connection_limit(),
        resources::default_http_io_timeout_ms(),
        resources::default_max_http_body_bytes(),
        resources::default_dashboard_overview_cache_ms()
    );
}
