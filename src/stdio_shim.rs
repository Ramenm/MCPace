use crate::client;
use crate::hub;
use crate::json::JsonValue;
use crate::json_helpers;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Default)]
struct ParsedArgs {
    json_output: bool,
    help: bool,
    root_override: Option<PathBuf>,
    passthrough: Vec<String>,
    error: Option<String>,
}

pub fn run(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let parsed = parse_args(args);
    if let Some(error) = parsed.error {
        let _ = writeln!(stderr, "{}", error);
        return 2;
    }
    if parsed.help {
        write_help(stdout);
        return 0;
    }
    if !parsed.json_output {
        let _ = writeln!(
            stderr,
            "stdio-shim is currently a bootstrap-only proof surface. Run 'mcpace stdio-shim --json ...' to normalize metadata, derive a sticky session lease, and ensure the hub is up. Live MCP stdio forwarding is not implemented yet."
        );
        return 2;
    }

    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };

    let client_plan = match run_subcommand(
        |stdout_buffer, stderr_buffer| {
            let mut forwarded = vec![
                "plan".to_string(),
                "--json".to_string(),
                "--root".to_string(),
                root_path.display().to_string(),
            ];
            forwarded.extend(parsed.passthrough.iter().cloned());
            client::run(
                &forwarded,
                Some(root_path.clone()),
                stdout_buffer,
                stderr_buffer,
            )
        },
        "client plan",
    ) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    let client_export = match run_subcommand(
        |stdout_buffer, stderr_buffer| {
            let client_id = json_helpers::string_at_path(&client_plan, &["context", "clientId"])
                .unwrap_or("unknown-client");
            let mut forwarded = vec![
                "export".to_string(),
                client_id.to_string(),
                "--json".to_string(),
                "--root".to_string(),
                root_path.display().to_string(),
            ];
            forwarded.extend(parsed.passthrough.iter().cloned());
            client::run(
                &forwarded,
                Some(root_path.clone()),
                stdout_buffer,
                stderr_buffer,
            )
        },
        "client export",
    ) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    let hub_bootstrap_status = match run_subcommand(
        |stdout_buffer, stderr_buffer| {
            let forwarded = vec![
                "up".to_string(),
                "--json".to_string(),
                "--root".to_string(),
                root_path.display().to_string(),
            ];
            hub::run(
                &forwarded,
                Some(root_path.clone()),
                stdout_buffer,
                stderr_buffer,
            )
        },
        "hub up",
    ) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    let hub_status = match run_subcommand(
        |stdout_buffer, stderr_buffer| {
            let forwarded = vec![
                "status".to_string(),
                "--json".to_string(),
                "--root".to_string(),
                root_path.display().to_string(),
            ];
            hub::run(
                &forwarded,
                Some(root_path.clone()),
                stdout_buffer,
                stderr_buffer,
            )
        },
        "hub status",
    ) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    let mut blockers = vec![
        "Live MCP stdio message forwarding is not implemented yet; this shim only normalizes routing context, derives a sticky lease, and ensures the persistent hub is up.".to_string(),
    ];
    blockers.extend(string_array_at_path(&client_export, &["blockers"]));
    blockers.sort();
    blockers.dedup();

    let report = JsonValue::object([
        ("mode", JsonValue::string("bootstrap-only")),
        ("hubBootstrapSucceeded", JsonValue::bool(true)),
        ("canForwardMcpToday", JsonValue::bool(false)),
        (
            "sessionLeaseId",
            match json_helpers::string_at_path(&client_plan, &["context", "sessionLeaseId"]) {
                Some(value) => JsonValue::string(value.to_string()),
                None => JsonValue::Null,
            },
        ),
        (
            "projectRoot",
            match json_helpers::string_at_path(&client_plan, &["context", "projectRoot"]) {
                Some(value) => JsonValue::string(value.to_string()),
                None => JsonValue::Null,
            },
        ),
        ("clientPlan", client_plan),
        ("adapterPreview", client_export),
        ("hubBootstrapStatus", hub_bootstrap_status),
        ("hubStatus", hub_status),
        (
            "blockers",
            JsonValue::array(blockers.into_iter().map(JsonValue::string)),
        ),
    ]);
    let _ = writeln!(stdout, "{}", report.to_pretty_string());
    0
}

fn parse_args(args: &[String]) -> ParsedArgs {
    let mut parsed = ParsedArgs::default();
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                parsed.json_output = true;
                index += 1;
            }
            "--root" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("stdio-shim requires a path after --root".to_string());
                    return parsed;
                };
                parsed.root_override = Some(PathBuf::from(value));
                index += 2;
            }
            "-h" | "--help" | "-?" => {
                parsed.help = true;
                return parsed;
            }
            other => {
                parsed.passthrough.push(other.to_string());
                index += 1;
            }
        }
    }

    parsed
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace stdio-shim --json [--root <path>] [--client-id <id>] [--session-id <id>] [--project-root <path>] [--transport <stdio|streamable-http>] [--metadata-json <json>]"
    );
    let _ = writeln!(stdout, "");
    let _ = writeln!(
        stdout,
        "stdio-shim is currently a bootstrap-only proof surface."
    );
    let _ = writeln!(
        stdout,
        "It reuses client planning/export logic, derives a sticky session lease, and ensures the local hub runtime is up."
    );
    let _ = writeln!(stdout, "It does not yet forward live MCP stdio traffic.");
}

fn run_subcommand<F>(mut runner: F, label: &str) -> Result<JsonValue, String>
where
    F: FnMut(&mut Vec<u8>, &mut Vec<u8>) -> i32,
{
    let mut stdout_buffer = Vec::new();
    let mut stderr_buffer = Vec::new();
    let exit_code = runner(&mut stdout_buffer, &mut stderr_buffer);
    if exit_code != 0 {
        let stderr_text = String::from_utf8(stderr_buffer).unwrap_or_default();
        let stdout_text = String::from_utf8(stdout_buffer).unwrap_or_default();
        let details = if !stderr_text.trim().is_empty() {
            stderr_text.trim().to_string()
        } else if !stdout_text.trim().is_empty() {
            stdout_text.trim().to_string()
        } else {
            format!("{} failed with exit code {}", label, exit_code)
        };
        return Err(format!("{} failed: {}", label, details));
    }

    let stdout_text = String::from_utf8(stdout_buffer)
        .map_err(|error| format!("{} produced non-UTF8 output: {}", label, error))?;
    crate::json::parse_str(stdout_text.trim())
        .map_err(|error| format!("{} produced invalid JSON: {}", label, error))
}

fn string_array_at_path(json: &JsonValue, path: &[&str]) -> Vec<String> {
    json_helpers::array_at_path(json, path)
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .map(|value| value.to_string())
                .collect()
        })
        .unwrap_or_default()
}
