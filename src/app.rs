use crate::catalog::{find, normalize};
use crate::client_catalog::client_install_support_summary;
use crate::runtimepaths;
use crate::{
    candidates, cleanup, client, connect, dashboard, doctor, hub, init, lab, mcp_server, profile,
    projects, release, repair, reporoot, serve, server, service, setup, stdio_shim, update, verify,
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
            let _ = writeln!(stderr, "Run 'mcpace help' to see the public commands.");
            2
        }
    }
}

fn run_version(stdout: &mut dyn Write) -> i32 {
    let _ = writeln!(stdout, "{}", env!("CARGO_PKG_VERSION"));
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
    let _ = writeln!(
        stdout,
        "Local MCP process scheduler for concurrent AI agents."
    );
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Usage:");
    let _ = writeln!(stdout, "  mcpace up");
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
    let _ = writeln!(stdout, "`up` creates/repairs MCPace home, imports existing MCP servers, starts the endpoint, and wires detected clients. It does not add a default upstream server.");
    let _ = writeln!(stdout, "Server type is inferred from command/url/path/package input. Endpoint: {}. Supported client patchers: {}.", runtimepaths::default_local_mcp_url(), client_install_support_summary());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::fs;

    struct EnvGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &std::path::Path) -> Self {
            let previous = std::env::var_os(key);
            std::env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    #[test]
    fn version_reports_binary_version_not_project_config_version() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "mcpace-version-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("mcpace.config.json"),
            r#"{ "version": "999.999.999" }"#,
        )
        .unwrap();
        let _root_env = EnvGuard::set("MCPACE_ROOT", &root);

        let mut stdout = Vec::new();
        let status = run(vec!["--version".to_string()], &mut stdout, &mut Vec::new());

        assert_eq!(status, 0);
        assert_eq!(
            String::from_utf8(stdout).unwrap().trim(),
            env!("CARGO_PKG_VERSION")
        );

        let _ = fs::remove_dir_all(root);
    }
}
