use crate::catalog::{find, normalize, public_commands, CommandRoute};
use crate::client_catalog::client_install_support_summary;
use crate::runtimepaths;
use crate::{
    agent, autostart, candidates, cleanup, client, diagnostics, hub, init, lab, mcp_server,
    profile, projects, release, reporoot, serve, server, setup, status, stdio_shim, uninstall,
    update, verify,
};
use std::io::Write;
use std::path::PathBuf;

pub fn run(args: Vec<String>, stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    let root_path = reporoot::find_from_current_or_executable();
    if args.is_empty() {
        write_help(stdout);
        return 0;
    }

    let normalized = normalize(&args[0]);
    let Some(spec) = find(&normalized) else {
        diagnostics::stderr_line(stderr, format_args!("unknown command: {}", args[0]));
        diagnostics::stderr_line(
            stderr,
            format_args!("Run 'mcpace help' to see the public commands."),
        );
        return 2;
    };

    match spec.route {
        CommandRoute::Help => run_help(&args[1..], stdout, stderr),
        CommandRoute::Version => run_version(stdout),
        CommandRoute::Up => setup::run(&args[1..], root_path, stdout, stderr),
        CommandRoute::Start => run_lifecycle("start", &args[1..], root_path, stdout, stderr),
        CommandRoute::Stop => run_lifecycle("stop", &args[1..], root_path, stdout, stderr),
        CommandRoute::Restart => run_lifecycle("restart", &args[1..], root_path, stdout, stderr),
        CommandRoute::Status => status::run(&args[1..], root_path, stdout, stderr),
        CommandRoute::Install => {
            run_server_with_action("install", &args[1..], root_path, stdout, stderr)
        }
        CommandRoute::Uninstall => uninstall::run(&args[1..], root_path, stdout, stderr),
        CommandRoute::Advanced => run_advanced(&args[1..], root_path, stdout, stderr),
        CommandRoute::Stdio | CommandRoute::StdioShim => {
            stdio_shim::run(&args[1..], root_path, stdout, stderr)
        }
        CommandRoute::Agent => agent::run(&args[1..], root_path, stdout, stderr),
        CommandRoute::Serve => serve::run(&args[1..], root_path, stdout, stderr),
        CommandRoute::Hub => hub::run(&args[1..], root_path, stdout, stderr),
        CommandRoute::McpServer => mcp_server::run(&args[1..], root_path, stdout, stderr),
    }
}

pub(crate) fn run_internal(
    args: Vec<String>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    run(canonical_internal_args(args), stdout, stderr)
}

fn canonical_internal_args(args: Vec<String>) -> Vec<String> {
    let Some(first) = args.first().map(|value| normalize(value)) else {
        return args;
    };
    let prefix: &[&str] = match first.as_str() {
        "doctor" => &["advanced", "doctor"],
        "server" => &["advanced", "server"],
        "client" => &["advanced", "client"],
        "autostart" | "service" => &["advanced", "autostart"],
        "cleanup" => &["advanced", "runtime", "cleanup"],
        "repair" => &["advanced", "runtime", "repair"],
        "dashboard" => &["advanced", "runtime", "foreground"],
        "lab" => &["advanced", "dev", "lab"],
        "candidates" => &["advanced", "dev", "candidates"],
        "profile" => &["advanced", "dev", "profile"],
        "projects" => &["advanced", "dev", "projects"],
        "release" => &["advanced", "dev", "release"],
        "init" => &["advanced", "dev", "init"],
        "update" => &["advanced", "update"],
        "verify" => {
            let action = args.get(1).map(|value| normalize(value));
            return match action.as_deref() {
                Some("readiness") => {
                    let mut canonical = vec![
                        "advanced".to_string(),
                        "doctor".to_string(),
                        "readiness".to_string(),
                    ];
                    canonical.extend(args.into_iter().skip(2));
                    canonical
                }
                Some("doctor") => {
                    let mut canonical = vec!["advanced".to_string(), "doctor".to_string()];
                    canonical.extend(args.into_iter().skip(2));
                    canonical
                }
                _ => args,
            };
        }
        _ => return args,
    };

    let mut canonical = prefix
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    canonical.extend(args.into_iter().skip(1));
    canonical
}

fn run_help(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    match args.first().map(|value| normalize(value)) {
        None => {
            write_help(stdout);
            0
        }
        Some(topic) if topic == "advanced" => {
            write_advanced_help(stdout);
            0
        }
        Some(topic) if topic == "start" || topic == "stop" || topic == "restart" => {
            write_lifecycle_help(&topic, stdout);
            0
        }
        Some(topic) if topic == "status" => {
            status::run(&["--help".to_string()], None, stdout, stderr)
        }
        Some(topic) if topic == "uninstall" => {
            uninstall::run(&["--help".to_string()], None, stdout, stderr)
        }
        Some(topic) if topic == "up" => setup::run(&["--help".to_string()], None, stdout, stderr),
        Some(topic) if topic == "install" => {
            run_server_with_action("install", &["--help".to_string()], None, stdout, stderr)
        }
        Some(topic) => {
            diagnostics::stderr_line(stderr, format_args!("unknown help topic: {}", topic));
            diagnostics::stderr_line(
                stderr,
                format_args!("Run 'mcpace help' to see the public commands."),
            );
            2
        }
    }
}

fn run_version(stdout: &mut dyn Write) -> i32 {
    let _ = writeln!(stdout, "{}", env!("CARGO_PKG_VERSION"));
    0
}

fn run_lifecycle(
    action: &str,
    args: &[String],
    root_path: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    if args
        .first()
        .is_some_and(|value| matches!(value.as_str(), "-h" | "--help"))
    {
        write_lifecycle_help(action, stdout);
        return 0;
    }
    let mut forwarded = vec![action.to_string()];
    forwarded.extend(args.iter().cloned());
    serve::run(&forwarded, root_path, stdout, stderr)
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

fn run_advanced(
    args: &[String],
    root_path: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let Some(group) = args.first().map(|value| normalize(value)) else {
        write_advanced_help(stdout);
        return 0;
    };
    if matches!(group.as_str(), "-h" | "--help" | "help") {
        write_advanced_help(stdout);
        return 0;
    }

    match group.as_str() {
        "doctor" => {
            if args
                .get(1)
                .is_some_and(|value| normalize(value) == "readiness")
            {
                verify::run(&args[1..], root_path, stdout, stderr)
            } else {
                run_verify_with_action("doctor", &args[1..], root_path, stdout, stderr)
            }
        }
        "server" => server::run(&args[1..], root_path, stdout, stderr),
        "client" => client::run(&args[1..], root_path, stdout, stderr),
        "autostart" => autostart::run(&args[1..], root_path, stdout, stderr),
        "runtime" => run_advanced_runtime(&args[1..], root_path, stdout, stderr),
        "lease" => run_hub_with_action("lease", &args[1..], root_path, stdout, stderr),
        "update" => update::run(&args[1..], stdout, stderr),
        "dev" => run_advanced_dev(&args[1..], root_path, stdout, stderr),
        _ => {
            diagnostics::stderr_line(
                stderr,
                format_args!("unknown advanced command: {}", args[0]),
            );
            diagnostics::stderr_line(
                stderr,
                format_args!("Run 'mcpace advanced --help' to see advanced commands."),
            );
            2
        }
    }
}

fn run_advanced_runtime(
    args: &[String],
    root_path: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let Some(action) = args.first().map(|value| normalize(value)) else {
        write_advanced_runtime_help(stdout);
        return 0;
    };
    if matches!(action.as_str(), "-h" | "--help" | "help") {
        write_advanced_runtime_help(stdout);
        return 0;
    }
    match action.as_str() {
        "foreground" => serve::run(&args[1..], root_path, stdout, stderr),
        "logs" => run_hub_with_action("logs", &args[1..], root_path, stdout, stderr),
        "repair" => run_hub_with_action("repair", &args[1..], root_path, stdout, stderr),
        "cleanup" => cleanup::run(&args[1..], root_path, stdout, stderr),
        _ => {
            diagnostics::stderr_line(
                stderr,
                format_args!("unknown advanced runtime action: {}", args[0]),
            );
            diagnostics::stderr_line(
                stderr,
                format_args!("Run 'mcpace advanced runtime --help' for usage."),
            );
            2
        }
    }
}

fn run_advanced_dev(
    args: &[String],
    root_path: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let Some(action) = args.first().map(|value| normalize(value)) else {
        write_advanced_dev_help(stdout);
        return 0;
    };
    if matches!(action.as_str(), "-h" | "--help" | "help") {
        write_advanced_dev_help(stdout);
        return 0;
    }
    match action.as_str() {
        "lab" => lab::run(&args[1..], root_path, stdout, stderr),
        "candidates" => candidates::run(&args[1..], root_path, stdout, stderr),
        "profile" => profile::run(&args[1..], root_path, stdout, stderr),
        "projects" => projects::run(&args[1..], root_path, stdout, stderr),
        "release" => release::run(&args[1..], root_path, stdout, stderr),
        "init" => init::run(&args[1..], root_path, stdout, stderr),
        _ => {
            diagnostics::stderr_line(
                stderr,
                format_args!("unknown advanced dev action: {}", args[0]),
            );
            diagnostics::stderr_line(
                stderr,
                format_args!("Run 'mcpace advanced dev --help' for usage."),
            );
            2
        }
    }
}

fn run_hub_with_action(
    action: &str,
    args: &[String],
    root_path: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let mut forwarded = vec![action.to_string()];
    forwarded.extend(args.iter().cloned());
    hub::run(&forwarded, root_path, stdout, stderr)
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

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "MCPace");
    let _ = writeln!(
        stdout,
        "Local MCP process scheduler for concurrent AI agents."
    );
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Usage: mcpace <command> [options]");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Commands:");
    for command in public_commands()
        .filter(|command| !matches!(command.route, CommandRoute::Help | CommandRoute::Version))
    {
        let _ = writeln!(stdout, "  {:10} {}", command.name, command.description);
    }
    let _ = writeln!(
        stdout,
        "  {:10} Show this help or help for one command.",
        "help"
    );
    let _ = writeln!(stdout, "  {:10} Print the MCPace version.", "version");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Quickstart:");
    let _ = writeln!(stdout, "  mcpace up");
    let _ = writeln!(stdout, "  mcpace status");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "`up` is convergent: it creates or repairs MCPace home, imports existing MCP servers, starts the endpoint, wires detected clients, and installs or repairs user-level login startup. Use --no-autostart for session-only use.");
    let _ = writeln!(
        stdout,
        "Endpoint: {}. Supported client patchers: {}.",
        runtimepaths::default_local_mcp_url(),
        client_install_support_summary()
    );
}

fn write_lifecycle_help(action: &str, stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "Usage: mcpace {} [--json] [--root <path>]", action);
    let _ = writeln!(stdout);
    let detail = match action {
        "start" => "Starts the already-configured runtime without changing clients or login startup.",
        "stop" => "Stops the current runtime while leaving login startup enabled for the next login.",
        "restart" => "Restarts the current runtime without changing configuration, clients, or login startup.",
        _ => "Controls the configured runtime.",
    };
    let _ = writeln!(stdout, "{}", detail);
}

fn write_advanced_help(stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "Usage: mcpace advanced <command> [options]");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Commands:");
    let _ = writeln!(stdout, "  doctor       Run detailed readiness diagnostics");
    let _ = writeln!(
        stdout,
        "  server       Discover, inspect, test, and configure upstream servers"
    );
    let _ = writeln!(
        stdout,
        "  client       Inspect, install, restore, and export client integration"
    );
    let _ = writeln!(
        stdout,
        "  autostart    Inspect, repair, disable, or prove login startup"
    );
    let _ = writeln!(
        stdout,
        "  runtime      Foreground serving, logs, repair, and safe cleanup"
    );
    let _ = writeln!(stdout, "  lease        Inspect and control runtime leases");
    let _ = writeln!(
        stdout,
        "  update       Show package-manager update guidance"
    );
    let _ = writeln!(
        stdout,
        "  dev          Maintainer-only evidence and release tooling"
    );
}

fn write_advanced_runtime_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace advanced runtime <foreground|logs|repair|cleanup> [options]"
    );
}

fn write_advanced_dev_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace advanced dev <lab|candidates|profile|projects|release|init> [options]"
    );
    let _ = writeln!(
        stdout,
        "Maintainer-only commands are intentionally excluded from the public surface."
    );
}

#[cfg(test)]
mod tests;
