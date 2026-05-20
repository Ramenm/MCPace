use crate::catalog::{find, normalize};
use crate::client_catalog::client_install_support_summary;
use crate::runtimepaths;
use crate::{
    candidates, cleanup, client, connect, dashboard, doctor, hub, init, lab, mcp_server, profile,
    projects, release, repair, reporoot, serve, server, service, setup, stdio_shim, update, verify,
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
        "install" | "add" | "add-server" | "server-install" => {
            return run_server_with_action("install", &args[1..], root_path, stdout, stderr)
        }
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
        "cleanup" => cleanup::run(&args[1..], root_path, stdout, stderr),
        "connect" => connect::run(&args[1..], root_path, stdout, stderr),
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
                "Run 'mcpace help' to see the public commands."
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
        "command '{}' is not available in the public CLI surface; run 'mcpace help' for supported commands.",
        name
    );
    2
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "MCPace");
    let _ = writeln!(stdout, "One MCP endpoint for all your AI clients.");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Usage:");
    let _ = writeln!(stdout, "  mcpace up");
    let _ = writeln!(stdout, "  mcpace install <path|package|url|command...> [--as <name>] [--dry-run]");
    let _ = writeln!(stdout, "  mcpace serve [start|stop|status]");
    let _ = writeln!(stdout, "  mcpace server <list|test|remove|enable|disable|sources>");
    let _ = writeln!(stdout, "  mcpace client <install|export|list|restore>");
    let _ = writeln!(stdout, "  mcpace connect [client]");
    let _ = writeln!(stdout, "  mcpace doctor [--json]");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Quickstart:");
    let _ = writeln!(stdout, "  mcpace up");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "`up` creates/repairs MCPace home, imports existing MCP servers, starts the endpoint, and wires detected clients. It does not add a default upstream server.");
    let _ = writeln!(stdout, "Server type is inferred from command/url/path/package input. Endpoint: {}. Supported client patchers: {}.", runtimepaths::default_local_mcp_url(), client_install_support_summary());
}
