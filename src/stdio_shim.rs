use crate::mcp_server;
use std::io::Write;
use std::path::PathBuf;

pub fn run(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
    {
        write_help(stdout);
        return 0;
    }

    let forwarded = normalize_compat_args(args);
    mcp_server::run(&forwarded, default_root, stdout, stderr)
}

fn normalize_compat_args(args: &[String]) -> Vec<String> {
    args.iter()
        .filter(|arg| arg.as_str() != "--json")
        .cloned()
        .collect()
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace stdio [--root <path>] [--client-id <id>] [--session-id <id>] [--project-root <path>] [--transport <stdio|streamable-http>]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "mcpace stdio starts the live MCP stdio server for local clients; stdio-shim remains a compatibility alias."
    );
    let _ = writeln!(
        stdout,
        "It speaks newline-delimited JSON-RPC over stdin/stdout and keeps diagnostics on stderr so MCP stdout stays protocol-clean."
    );
    let _ = writeln!(
        stdout,
        "The old preview-only --json flag is accepted as a no-op compatibility flag."
    );
}

#[cfg(test)]
mod tests;
