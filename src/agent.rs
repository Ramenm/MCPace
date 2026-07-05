use crate::serve;
use clap::{error::ErrorKind, Args, Parser, Subcommand};
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "mcpace agent",
    disable_version_flag = true,
    about = "Run or inspect the visible MCPace user-login agent"
)]
struct AgentCli {
    #[command(subcommand)]
    command: Option<AgentCommand>,
}

#[derive(Debug, Subcommand)]
enum AgentCommand {
    /// Run the foreground managed MCPace runtime from a login item.
    Run(AgentRuntimeArgs),
    /// Print the managed runtime status.
    Status(AgentRuntimeArgs),
}

#[derive(Clone, Debug, Default, Args)]
struct AgentRuntimeArgs {
    /// Mark this invocation as OS autostart initiated. This is metadata for reports and is not forwarded to serve.
    #[arg(long)]
    autostart: bool,

    /// MCPace project/root directory.
    #[arg(long, value_name = "PATH")]
    root: Option<PathBuf>,

    /// Bind host forwarded to `mcpace serve`.
    #[arg(long, value_name = "ADDR")]
    host: Option<String>,

    /// Bind port forwarded to `mcpace serve`.
    #[arg(long, value_name = "N")]
    port: Option<String>,

    /// Maximum concurrent HTTP connections forwarded to `mcpace serve`.
    #[arg(long = "max-connections", value_name = "N")]
    max_connections: Option<String>,

    /// HTTP IO timeout forwarded to `mcpace serve`.
    #[arg(long = "io-timeout-ms", value_name = "MS")]
    io_timeout_ms: Option<String>,

    /// Maximum accepted HTTP body size forwarded to `mcpace serve`.
    #[arg(long = "max-body-bytes", value_name = "BYTES")]
    max_body_bytes: Option<String>,

    /// Dashboard overview cache TTL forwarded to `mcpace serve`.
    #[arg(long = "overview-cache-ms", value_name = "MS")]
    overview_cache_ms: Option<String>,

    /// Forward JSON output mode to the managed runtime/status command.
    #[arg(long, short = 'j')]
    json: bool,
}

pub fn run(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let cli = match parse_cli(args) {
        Ok(value) => value,
        Err(error) => {
            let informational = matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            );
            let target: &mut dyn Write = if informational { stdout } else { stderr };
            let _ = write!(target, "{}", error);
            return if informational { 0 } else { 2 };
        }
    };

    match cli
        .command
        .unwrap_or_else(|| AgentCommand::Run(AgentRuntimeArgs::default()))
    {
        AgentCommand::Run(runtime) => run_managed_agent(runtime, default_root, stdout, stderr),
        AgentCommand::Status(runtime) => run_serve_status(runtime, default_root, stdout, stderr),
    }
}

fn parse_cli(args: &[String]) -> Result<AgentCli, clap::Error> {
    let mut argv = Vec::with_capacity(args.len() + 2);
    argv.push(OsString::from("mcpace agent"));
    if should_default_to_run(args) {
        argv.push(OsString::from("run"));
    }
    argv.extend(args.iter().map(|arg| {
        let normalized = if arg == "-?" || arg == "help" {
            "--help"
        } else {
            arg.as_str()
        };
        OsString::from(normalized)
    }));
    AgentCli::try_parse_from(argv)
}

fn should_default_to_run(args: &[String]) -> bool {
    match args.first().map(String::as_str) {
        None => true,
        Some("run" | "status" | "help") => false,
        Some("-h" | "--help" | "-?") => false,
        Some(value) if value.starts_with('-') => true,
        Some(_) => false,
    }
}

fn run_managed_agent(
    runtime: AgentRuntimeArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let hydration = crate::persistent_env::hydrate_login_environment();
    let _ = hydration.hydrated_keys.len();
    let mut serve_args = Vec::with_capacity(runtime.forwarded_len_hint() + 1);
    serve_args.push("--managed-service".to_string());
    runtime.append_forwarded_args(&mut serve_args);
    serve::run(&serve_args, runtime.root.or(default_root), stdout, stderr)
}

fn run_serve_status(
    runtime: AgentRuntimeArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let hydration = crate::persistent_env::hydrate_login_environment();
    let _ = hydration.hydrated_keys.len();
    let mut serve_args = Vec::with_capacity(runtime.forwarded_len_hint() + 1);
    serve_args.push("status".to_string());
    runtime.append_forwarded_args(&mut serve_args);
    serve::run(&serve_args, runtime.root.or(default_root), stdout, stderr)
}

impl AgentRuntimeArgs {
    fn forwarded_len_hint(&self) -> usize {
        let mut count = if self.json { 1 } else { 0 };
        for present in [
            self.host.is_some(),
            self.port.is_some(),
            self.max_connections.is_some(),
            self.io_timeout_ms.is_some(),
            self.max_body_bytes.is_some(),
            self.overview_cache_ms.is_some(),
        ] {
            if present {
                count += 2;
            }
        }
        count
    }

    fn append_forwarded_args(&self, out: &mut Vec<String>) {
        push_optional_arg(out, "--host", &self.host);
        push_optional_arg(out, "--port", &self.port);
        push_optional_arg(out, "--max-connections", &self.max_connections);
        push_optional_arg(out, "--io-timeout-ms", &self.io_timeout_ms);
        push_optional_arg(out, "--max-body-bytes", &self.max_body_bytes);
        push_optional_arg(out, "--overview-cache-ms", &self.overview_cache_ms);
        if self.json {
            out.push("--json".to_string());
        }
    }
}

fn push_optional_arg(out: &mut Vec<String>, flag: &str, value: &Option<String>) {
    if let Some(value) = value {
        out.push(flag.to_string());
        out.push(value.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_run_action_and_forwards_runtime_flags() {
        let args = vec![
            "--autostart".to_string(),
            "--root".to_string(),
            "/tmp/mcpace".to_string(),
            "--port".to_string(),
            "39022".to_string(),
        ];
        let cli = parse_cli(&args).expect("parse default run args");
        match cli.command.expect("default command") {
            AgentCommand::Run(runtime) => {
                assert!(runtime.autostart);
                assert_eq!(runtime.root, Some(PathBuf::from("/tmp/mcpace")));
                let mut forwarded = Vec::new();
                runtime.append_forwarded_args(&mut forwarded);
                assert_eq!(forwarded, vec!["--port".to_string(), "39022".to_string()]);
            }
            AgentCommand::Status(_) => panic!("expected run command"),
        }
    }

    #[test]
    fn rejects_unknown_arguments_instead_of_passing_shell_text() {
        let args = vec!["--shell".to_string()];
        let error = parse_cli(&args).expect_err("unknown arguments must be rejected");
        assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn status_command_uses_same_forwarding_contract() {
        let args = vec![
            "status".to_string(),
            "--json".to_string(),
            "--host".to_string(),
            "127.0.0.1".to_string(),
        ];
        let cli = parse_cli(&args).expect("parse status args");
        match cli.command.expect("status command") {
            AgentCommand::Status(runtime) => {
                let mut forwarded = Vec::new();
                runtime.append_forwarded_args(&mut forwarded);
                assert_eq!(
                    forwarded,
                    vec![
                        "--host".to_string(),
                        "127.0.0.1".to_string(),
                        "--json".to_string()
                    ]
                );
            }
            AgentCommand::Run(_) => panic!("expected status command"),
        }
    }
}
