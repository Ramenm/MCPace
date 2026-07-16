use crate::catalog::{find, normalize};
use crate::client_catalog::client_install_support_summary;
use crate::runtimepaths;
use crate::{
    agent, autostart, candidates, cleanup, client, connect, dashboard, diagnostics, doctor, hub,
    init, lab, mcp_server, profile, projects, release, repair, reporoot, serve, server, service,
    setup, stdio_shim, update, verify,
};
use clap::{error::ErrorKind, Parser};
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

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
        "auto" | "autodiscover" | "server-auto" => {
            return run_server_with_action("auto", &args[1..], root_path, stdout, stderr)
        }
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
        "version" => run_version(stdout),
        "doctor" => run_doctor(&args[1..], root_path, stdout, stderr),
        "setup" => setup::run(&args[1..], root_path, stdout, stderr),
        "service" => service::run(&args[1..], root_path, stdout, stderr),
        "autostart" => autostart::run(&args[1..], root_path, stdout, stderr),
        "agent" => agent::run(&args[1..], root_path, stdout, stderr),
        "dashboard" => dashboard::run(&args[1..], root_path, stdout, stderr),
        "serve" => serve::run(&args[1..], root_path, stdout, stderr),
        "init" => init::run(&args[1..], root_path, stdout, stderr),
        "hub" => hub::run(&args[1..], root_path, stdout, stderr),
        "stdio" | "stdio-shim" => stdio_shim::run(&args[1..], root_path, stdout, stderr),
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
            diagnostics::stderr_line(stderr, format_args!("unknown command: {}", args[0]));
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

#[derive(Debug, Parser)]
#[command(
    name = "mcpace doctor",
    disable_version_flag = true,
    about = "Run MCPace doctor checks"
)]
struct DoctorCli {
    /// Emit machine-readable JSON.
    #[arg(long = "json", short = 'j')]
    json_output: bool,

    /// MCPace project/root directory.
    #[arg(long = "root", value_name = "PATH")]
    root_override: Option<PathBuf>,
}

fn run_doctor(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let cli = match DoctorCli::try_parse_from(doctor_argv(args)) {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            let _ = writeln!(stdout, "Usage: mcpace doctor [--json] [--root <path>]");
            return 0;
        }
        Err(error) => {
            let _ = write!(stderr, "{}", error);
            return 2;
        }
    };

    let report = doctor::run(cli.root_override.or(default_root));
    if cli.json_output {
        let _ = writeln!(stdout, "{}", report.to_pretty_json());
        return 0;
    }

    doctor::write_text_report(&report, stdout);
    0
}

fn doctor_argv(args: &[String]) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(OsString::from("mcpace doctor"));
    argv.extend(
        args.iter()
            .map(|arg| OsString::from(normalize_doctor_flag(arg))),
    );
    argv
}

fn normalize_doctor_flag(arg: &str) -> &str {
    match arg {
        "-json" => "--json",
        "-root" => "--root",
        "-?" => "--help",
        other => other,
    }
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
    diagnostics::stderr_line(
        stderr,
        format_args!(
            "command '{}' is not available in the public CLI surface; run 'mcpace help' for supported commands.",
            name
        ),
    );
    2
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "MCPace");
    let _ = writeln!(
        stdout,
        "Local MCP process scheduler for concurrent AI agents."
    );
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Usage:");
    let _ = writeln!(stdout, "  mcpace up [--no-autostart]");
    let _ = writeln!(
        stdout,
        "  mcpace install <path|package|url|command...> [--as <name>] [--dry-run]"
    );
    let _ = writeln!(
        stdout,
        "  mcpace auto [query] [--dry-run]        # one-command server discovery/setup/probe"
    );
    let _ = writeln!(stdout, "  mcpace serve [start|stop|status]");
    let _ = writeln!(
        stdout,
        "  mcpace stdio [--root <path>]              # MCP stdio launch surface"
    );
    let _ = writeln!(
        stdout,
        "  mcpace autostart <enable|disable|status>  # user-level launch at login"
    );
    let _ = writeln!(
        stdout,
        "  mcpace server <auto|list|test|remove|enable|disable|sources>"
    );
    let _ = writeln!(stdout, "  mcpace client <install|export|list|restore>");
    let _ = writeln!(stdout, "  mcpace connect [client]");
    let _ = writeln!(
        stdout,
        "  mcpace lab [--json]               # evidence corpus for auto-classifier decisions"
    );
    let _ = writeln!(stdout, "  mcpace doctor [--json]");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Quickstart:");
    let _ = writeln!(stdout, "  mcpace up");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "`up` creates/repairs MCPace home, imports existing MCP servers, starts the endpoint, wires detected clients, and installs or repairs user-level autostart. Use --no-autostart for session-only use. It does not add a default upstream server.");
    let _ = writeln!(stdout, "Server type is inferred from command/url/path/package input. Endpoint: {}. Supported client patchers: {}.", runtimepaths::default_local_mcp_url(), client_install_support_summary());
}

#[cfg(test)]
mod tests;
