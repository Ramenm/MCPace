use crate::catalog::{find, normalize, COMMANDS};
use crate::client_catalog::client_install_support_summary;
use crate::resources;
use crate::runtimepaths;
use crate::{
    candidates, client, dashboard, doctor, hub, init, lab, mcp_server, profile, projects, release,
    repair, reporoot, serve, server, service, setup, stdio_shim, update, verify,
};
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn run(args: Vec<String>, stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    let root_path = reporoot::find_from_current_or_executable();

    if args.is_empty() {
        write_help(stdout);
        return 0;
    }

    let normalized = normalize(&args[0]);
    match normalized.as_str() {
        "project" => return projects::run(&args[1..], root_path, stdout, stderr),
        "servers" => return run_server_with_action("list", &args[1..], root_path, stdout, stderr),
        "capabilities" | "server-capabilities" => {
            return run_server_with_action("capabilities", &args[1..], root_path, stdout, stderr)
        }
        "readiness" | "status" => {
            return run_verify_with_action("readiness", &args[1..], root_path, stdout, stderr)
        }
        "check" | "probe" => {
            return run_verify_with_action("doctor", &args[1..], root_path, stdout, stderr)
        }
        "smoke" | "stress-status" | "stress-startup-status" => {
            return run_planned(&normalized, stderr)
        }
        _ => {}
    }

    let resolved = find(&normalized)
        .map(|spec| spec.name)
        .unwrap_or(normalized.as_str());
    match resolved {
        "help" => {
            write_help(stdout);
            0
        }
        "version" => run_version(root_path.as_deref(), stdout, stderr),
        "doctor" => run_doctor(&args[1..], root_path, stdout, stderr),
        "setup" => setup::run(&args[1..], root_path, stdout, stderr),
        "service" => service::run(&args[1..], root_path, stdout, stderr),
        "dashboard" => dashboard::run(&args[1..], root_path, stdout, stderr),
        "serve" => serve::run(&args[1..], root_path, stdout, stderr),
        "init" => init::run(&args[1..], root_path, stdout, stderr),
        "hub" => hub::run(&args[1..], root_path, stdout, stderr),
        "stdio-shim" => stdio_shim::run(&args[1..], root_path, stdout, stderr),
        "mcp-server" => mcp_server::run(&args[1..], root_path, stdout, stderr),
        "client" => client::run(&args[1..], root_path, stdout, stderr),
        "profile" => profile::run(&args[1..], root_path, stdout, stderr),
        "projects" => projects::run(&args[1..], root_path, stdout, stderr),
        "candidates" => candidates::run(&args[1..], root_path, stdout, stderr),
        "lab" => lab::run(&args[1..], root_path, stdout, stderr),
        "server" => server::run(&args[1..], root_path, stdout, stderr),
        "verify" => verify::run(&args[1..], root_path, stdout, stderr),
        "repair" => repair::run(&args[1..], root_path, stdout, stderr),
        "update" => update::run(&args[1..], stdout, stderr),
        "release" => release::run(&args[1..], root_path, stdout, stderr),
        _ => {
            let _ = writeln!(stderr, "unknown command: {}", args[0]);
            let _ = writeln!(
                stderr,
                "Run 'mcpace help' to see the current Rust-only surface."
            );
            2
        }
    }
}

fn run_version(root_path: Option<&Path>, stdout: &mut dyn Write, _stderr: &mut dyn Write) -> i32 {
    let version = root_path
        .and_then(doctor::read_config_version)
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    let _ = writeln!(stdout, "{}", version);
    0
}

fn run_doctor(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let mut json_output = false;
    let mut root_override = default_root;
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                json_output = true;
                index += 1;
            }
            "--root" => {
                let Some(value) = args.get(index + 1) else {
                    let _ = writeln!(stderr, "doctor requires a path after --root");
                    return 2;
                };
                root_override = Some(PathBuf::from(value));
                index += 2;
            }
            "-h" | "--help" => {
                let _ = writeln!(stdout, "Usage: mcpace doctor [--json] [--root <path>]");
                return 0;
            }
            other => {
                let _ = writeln!(stderr, "unsupported doctor argument: {}", other);
                return 2;
            }
        }
    }

    let report = doctor::run(root_override);
    if json_output {
        let _ = writeln!(stdout, "{}", report.to_pretty_json());
        return 0;
    }

    doctor::write_text_report(&report, stdout);
    0
}

fn run_server_with_action(
    action: &str,
    args: &[String],
    root_path: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let mut forwarded = vec![action.to_string()];
    forwarded.extend(args.iter().cloned());
    server::run(&forwarded, root_path, stdout, stderr)
}

fn run_verify_with_action(
    action: &str,
    args: &[String],
    root_path: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let mut forwarded = vec![action.to_string()];
    forwarded.extend(args.iter().cloned());
    verify::run(&forwarded, root_path, stdout, stderr)
}

fn run_planned(name: &str, stderr: &mut dyn Write) -> i32 {
    let _ = writeln!(
        stderr,
        "command '{}' is not implemented yet in the Rust-only repo. The legacy PowerShell entrypoints were removed; use the current native subset or continue the grouped Rust implementation.",
        name
    );
    2
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "MCPace Rust-only local MCP hub");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Implemented now:");
    let _ = writeln!(stdout, "  version");
    let _ = writeln!(stdout, "  doctor [--json] [--root <path>]");
    let _ = writeln!(
        stdout,
        "  setup [--json] [--root <path>] [--host <addr>] [--port <n>] [--max-connections <n>] [--io-timeout-ms <n>] [--max-body-bytes <n>] [--overview-cache-ms <n>] [--skip-client-install] [--install-service|--install-autostart] [--no-enable]"
    );
    let _ = writeln!(
        stdout,
        "  service install|status|uninstall|print [--json] [--root <path>] [--host <addr>] [--port <n>] [--max-connections <n>] [--io-timeout-ms <n>] [--max-body-bytes <n>] [--overview-cache-ms <n>] [--dry-run] [--no-enable]"
    );
    let _ = writeln!(
        stdout,
        "  dashboard [--root <path>] [--host <addr>] [--port <n>] [--max-connections <n>] [--io-timeout-ms <n>] [--max-body-bytes <n>] [--overview-cache-ms <n>]"
    );
    let _ = writeln!(
        stdout,
        "  serve [--root <path>] [--host <addr>] [--port <n>] [--max-connections <n>] [--io-timeout-ms <n>] [--max-body-bytes <n>] [--overview-cache-ms <n>]"
    );
    let _ = writeln!(
        stdout,
        "  serve start|stop|status [--json] [--root <path>] [--host <addr>] [--port <n>] [--max-connections <n>] [--io-timeout-ms <n>] [--max-body-bytes <n>] [--overview-cache-ms <n>]"
    );
    let _ = writeln!(
        stdout,
        "  local HTTP defaults: max connections={}, IO timeout={}ms, max body={} bytes, overview cache={}ms, health cache={}ms",
        resources::default_http_connection_limit(),
        resources::DEFAULT_HTTP_IO_TIMEOUT_MS,
        resources::DEFAULT_MAX_HTTP_BODY_BYTES,
        resources::DEFAULT_DASHBOARD_OVERVIEW_CACHE_MS,
        resources::DEFAULT_DASHBOARD_HEALTH_CACHE_MS
    );
    let _ = writeln!(stdout, "  init [--json] [--root <path>]");
    let _ = writeln!(stdout, "  hub up [--json] [--root <path>] [--foreground]");
    let _ = writeln!(stdout, "  hub down [--json] [--root <path>]");
    let _ = writeln!(stdout, "  hub repair [--json] [--root <path>]");
    let _ = writeln!(stdout, "  hub status [--json] [--root <path>]");
    let _ = writeln!(stdout, "  hub logs [--json] [--root <path>] [--tail <n>]");
    let _ = writeln!(stdout, "  hub lease list [--json] [--root <path>]");
    let _ = writeln!(stdout, "  hub lease acquire --server <name> [--json] [--root <path>] [--client-id <id>] [--session-id <id>] [--project-root <path>] [--ttl-ms <n>]");
    let _ = writeln!(
        stdout,
        "  hub lease renew --lease-id <id> [--json] [--root <path>] [--ttl-ms <n>]"
    );
    let _ = writeln!(
        stdout,
        "  hub lease release --lease-id <id> [--json] [--root <path>]"
    );
    let _ = writeln!(stdout, "  profile [show] [--json] [--root <path>]");
    let _ = writeln!(stdout, "  projects [list] [--json] [--root <path>]");
    let _ = writeln!(stdout, "  candidates [--json] [--root <path>]");
    let _ = writeln!(stdout, "  client list [--json] [--root <path>]");
    let _ = writeln!(stdout, "  client plan [--json] [--root <path>] [--client-id <id>] [--session-id <id>] [--project-root <path>] [--transport <stdio|streamable-http>]");
    let _ = writeln!(
        stdout,
        "  client install <client|all> [--json] [--root <path>] [--dry-run] [--diff]"
    );
    let _ = writeln!(
        stdout,
        "  client restore <client|all> [--json] [--root <path>] [--backup <id|latest>]"
    );
    let _ = writeln!(stdout, "  client export <client> [--json] [--root <path>] [--transport <stdio|streamable-http>] [--session-id <id>] [--project-root <path>]");
    let _ = writeln!(stdout, "  mcp-server [--root <path>] [--client-id <id>] [--session-id <id>] [--project-root <path>] [--transport <stdio|streamable-http>]  # internal compatibility");
    let _ = writeln!(stdout, "  stdio-shim --json [--root <path>] [--client-id <id>] [--session-id <id>] [--project-root <path>] [--transport <stdio|streamable-http>] [--metadata-json <json>]  # internal bootstrap proof");
    let _ = writeln!(stdout, "  lab list [--json] [--root <path>]");
    let _ = writeln!(stdout, "  lab matrix [--json] [--root <path>]");
    let _ = writeln!(stdout, "  lab coverage [--json] [--root <path>]");
    let _ = writeln!(stdout, "  lab gaps [--json] [--root <path>]");
    let _ = writeln!(stdout, "  lab report [--json] [--root <path>]");
    let _ = writeln!(
        stdout,
        "  lab show --id <scenario> [--json] [--root <path>]"
    );
    let _ = writeln!(stdout, "  server list [--json] [--root <path>]");
    let _ = writeln!(
        stdout,
        "  server capabilities [--json] [--root <path>] [--name <server>]"
    );
    let _ = writeln!(stdout, "  server candidates [--json] [--root <path>]");
    let _ = writeln!(stdout, "  verify doctor [--json] [--root <path>]");
    let _ = writeln!(stdout, "  verify readiness [--json] [--root <path>]");
    let _ = writeln!(stdout, "  repair [--json] [--root <path>]");
    let _ = writeln!(stdout, "  release [build] [--json] [--root <path>]");
    let _ = writeln!(stdout, "  update check [--json] [--source none|env|npm] [--latest-version <semver>] [--package <name>]");
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "doctor/profile/projects/candidates/client-plan/lab/server/verify have native Rust read paths; setup starts the one-port MCPace endpoint, installs supported local clients, and smokes /healthz plus /mcp in one command; service installs user-level autostart entries without requiring mcpace in PATH; serve is the public one-port MCPace surface on {} and now has start/stop/status lifecycle commands, dashboard provides the same local web control surface, init seeds the runtime layout, hub owns a local lifecycle/state/log/repair/lease surface, client install patches MCPace entries for catalog-declared local patchers ({}) and client install all can patch every supported local target in one pass with dry-run/diff previews plus restoreable backups, client export emits connectable MCPace URL contracts for HTTP-capable clients plus preview-only blocked surfaces for unsupported lanes, stdio-shim remains a bootstrap proof surface, mcp-server remains an internal compatibility lane, update check reports safe package-manager update guidance without self-updating, and release build now wraps the local artifact/proof bundle without publishing.",
        runtimepaths::default_local_mcp_url(),
        client_install_support_summary()
    );
    let _ = writeln!(
        stdout,
        "Compatibility aliases: project, servers, capabilities, check, status, readiness, probe."
    );
    let _ = writeln!(
        stdout,
        "Unsupported commands are reported as not implemented yet in the Rust-only repo."
    );
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Still planned grouped surfaces:");
    for command in COMMANDS.iter().filter(|command| !command.implemented) {
        let _ = writeln!(stdout, "  {:<8} {}", command.name, command.description);
    }
}
