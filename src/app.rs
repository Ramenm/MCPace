use crate::catalog::{find, normalize, COMMANDS};
use crate::{
    candidates, client, doctor, hub, init, lab, profile, projects, repair, reporoot, server,
    stdio_shim, verify,
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
        "init" => init::run(&args[1..], root_path, stdout, stderr),
        "hub" => hub::run(&args[1..], root_path, stdout, stderr),
        "stdio-shim" => stdio_shim::run(&args[1..], root_path, stdout, stderr),
        "client" => client::run(&args[1..], root_path, stdout, stderr),
        "profile" => profile::run(&args[1..], root_path, stdout, stderr),
        "projects" => projects::run(&args[1..], root_path, stdout, stderr),
        "candidates" => candidates::run(&args[1..], root_path, stdout, stderr),
        "lab" => lab::run(&args[1..], root_path, stdout, stderr),
        "server" => server::run(&args[1..], root_path, stdout, stderr),
        "verify" => verify::run(&args[1..], root_path, stdout, stderr),
        "repair" => repair::run(&args[1..], root_path, stdout, stderr),
        "release" => run_planned(resolved, stderr),
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
    let _ = writeln!(stdout, "");
    let _ = writeln!(stdout, "Implemented now:");
    let _ = writeln!(stdout, "  version");
    let _ = writeln!(stdout, "  doctor [--json] [--root <path>]");
    let _ = writeln!(stdout, "  init [--json] [--root <path>]");
    let _ = writeln!(stdout, "  hub up [--json] [--root <path>] [--foreground]");
    let _ = writeln!(stdout, "  hub down [--json] [--root <path>]");
    let _ = writeln!(stdout, "  hub repair [--json] [--root <path>]");
    let _ = writeln!(stdout, "  hub status [--json] [--root <path>]");
    let _ = writeln!(stdout, "  hub logs [--json] [--root <path>] [--tail <n>]");
    let _ = writeln!(stdout, "  stdio-shim --json [--root <path>] [--client-id <id>] [--session-id <id>] [--project-root <path>] [--transport <stdio|streamable-http>] [--metadata-json <json>]");
    let _ = writeln!(stdout, "  profile [show] [--json] [--root <path>]");
    let _ = writeln!(stdout, "  projects [list] [--json] [--root <path>]");
    let _ = writeln!(stdout, "  candidates [--json] [--root <path>]");
    let _ = writeln!(stdout, "  client list [--json] [--root <path>]");
    let _ = writeln!(stdout, "  client plan [--json] [--root <path>] [--client-id <id>] [--session-id <id>] [--project-root <path>] [--transport <stdio|streamable-http>]");
    let _ = writeln!(stdout, "  client export <client> [--json] [--root <path>] [--transport <stdio|streamable-http>] [--session-id <id>] [--project-root <path>]");
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
    let _ = writeln!(stdout, "");
    let _ = writeln!(stdout, "doctor/profile/projects/candidates/client-plan/lab/server/verify have native Rust read paths; init seeds the runtime layout, hub owns a local lifecycle/state/log/repair surface, stdio-shim now bootstraps routing context into the persistent hub as a JSON proof surface, client export emits preview-only adapter contracts, and client install plus grouped top-level release remain planned.");
    let _ = writeln!(
        stdout,
        "Compatibility aliases: project, servers, capabilities, check, status, readiness, probe."
    );
    let _ = writeln!(
        stdout,
        "Unsupported commands are reported as not implemented yet in the Rust-only repo."
    );
    let _ = writeln!(stdout, "");
    let _ = writeln!(stdout, "Still planned grouped surfaces:");
    for command in COMMANDS.iter().filter(|command| !command.implemented) {
        let _ = writeln!(stdout, "  {:<8} {}", command.name, command.description);
    }
}
