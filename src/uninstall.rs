use crate::json::{parse_str, JsonValue};
use crate::{autostart, cleanup, client, diagnostics, runtimepaths, serve};
use clap::{error::ErrorKind, Parser};
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "mcpace uninstall",
    disable_version_flag = true,
    about = "Remove MCPace local integration without removing user configuration"
)]
struct UninstallCli {
    /// Preview every action without changing runtime, startup, or client files.
    #[arg(long = "dry-run")]
    dry_run: bool,

    /// Preserve MCPace entries in supported client configuration files.
    #[arg(long = "keep-clients")]
    keep_clients: bool,

    /// Emit machine-readable JSON.
    #[arg(long = "json", short = 'j')]
    json_output: bool,

    /// MCPace project/root directory.
    #[arg(long = "root", value_name = "PATH")]
    root_override: Option<PathBuf>,
}

pub fn run(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let cli = match UninstallCli::try_parse_from(argv(args)) {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            write_help(stdout);
            return 0;
        }
        Err(error) => {
            let _ = write!(stderr, "{}", error);
            return 2;
        }
    };

    let Some(root_path) = cli.root_override.or(default_root) else {
        diagnostics::stderr_line(
            stderr,
            format_args!("mcpace root not found; expected mcpace.config.json"),
        );
        return 1;
    };
    let root_path = runtimepaths::canonicalize_or_original(&root_path);
    let root_text = root_path.display().to_string();

    let stop = if cli.dry_run {
        Component::planned("stop configured runtime and supervisor")
    } else {
        capture_json(|component_stdout, component_stderr| {
            serve::run(
                &[
                    "stop".to_string(),
                    "--json".to_string(),
                    "--root".to_string(),
                    root_text.clone(),
                ],
                Some(root_path.clone()),
                component_stdout,
                component_stderr,
            )
        })
    };

    let mut startup_args = vec![
        "disable".to_string(),
        "--json".to_string(),
        "--root".to_string(),
        root_text.clone(),
    ];
    if cli.dry_run {
        startup_args.push("--dry-run".to_string());
    }
    let startup = capture_json(|component_stdout, component_stderr| {
        autostart::run(
            &startup_args,
            Some(root_path.clone()),
            component_stdout,
            component_stderr,
        )
    });

    let clients = if cli.keep_clients {
        JsonValue::object([
            ("schema", JsonValue::string("mcpace.clientRemoval.v1")),
            ("ok", JsonValue::bool(true)),
            ("dryRun", JsonValue::bool(cli.dry_run)),
            ("kept", JsonValue::bool(true)),
            ("removed", JsonValue::array(std::iter::empty())),
            ("skipped", JsonValue::array(std::iter::empty())),
            ("failed", JsonValue::array(std::iter::empty())),
        ])
    } else {
        match client::remove_owned_integrations(&root_path, cli.dry_run) {
            Ok(report) => report,
            Err(error) => JsonValue::object([
                ("schema", JsonValue::string("mcpace.clientRemoval.v1")),
                ("ok", JsonValue::bool(false)),
                ("error", JsonValue::string(error.to_string())),
            ]),
        }
    };

    let mut cleanup_args = vec![
        "runtime".to_string(),
        "--json".to_string(),
        "--root".to_string(),
        root_text.clone(),
    ];
    if cli.dry_run {
        cleanup_args.push("--dry-run".to_string());
    }
    let runtime_cleanup = capture_json(|component_stdout, component_stderr| {
        cleanup::run(
            &cleanup_args,
            Some(root_path.clone()),
            component_stdout,
            component_stderr,
        )
    });

    let clients_ok = clients
        .get("ok")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let ok = stop.ok() && startup.ok() && clients_ok && runtime_cleanup.ok();
    let report = JsonValue::object([
        ("schema", JsonValue::string("mcpace.uninstall.v1")),
        ("ok", JsonValue::bool(ok)),
        ("dryRun", JsonValue::bool(cli.dry_run)),
        ("rootPath", JsonValue::string(root_text)),
        ("packageRemoved", JsonValue::bool(false)),
        ("configurationPreserved", JsonValue::bool(true)),
        ("upstreamDefinitionsPreserved", JsonValue::bool(true)),
        ("backupsPreserved", JsonValue::bool(true)),
        ("stop", stop.into_json()),
        ("startup", startup.into_json()),
        ("clients", clients),
        ("runtimeCleanup", runtime_cleanup.into_json()),
        (
            "next",
            JsonValue::string("Remove the npm package with your package manager only after this command succeeds."),
        ),
    ]);

    if cli.json_output {
        let _ = writeln!(stdout, "{}", report.to_pretty_string());
    } else {
        write_text_report(&report, stdout);
    }

    if ok {
        0
    } else {
        1
    }
}

struct Component {
    status: i32,
    report: Option<JsonValue>,
    error: Option<String>,
    planned: bool,
}

impl Component {
    fn planned(action: &str) -> Self {
        Self {
            status: 0,
            report: Some(JsonValue::object([
                ("ok", JsonValue::bool(true)),
                ("planned", JsonValue::bool(true)),
                ("action", JsonValue::string(action)),
            ])),
            error: None,
            planned: true,
        }
    }

    fn ok(&self) -> bool {
        self.status == 0
            && self
                .report
                .as_ref()
                .and_then(|value| value.get("ok"))
                .and_then(JsonValue::as_bool)
                .unwrap_or(self.planned)
    }

    fn into_json(self) -> JsonValue {
        JsonValue::object([
            ("exitCode", JsonValue::number(self.status)),
            ("planned", JsonValue::bool(self.planned)),
            ("report", self.report.unwrap_or(JsonValue::Null)),
            (
                "error",
                self.error.map(JsonValue::string).unwrap_or(JsonValue::Null),
            ),
        ])
    }
}

fn capture_json(run_component: impl FnOnce(&mut dyn Write, &mut dyn Write) -> i32) -> Component {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let status = run_component(&mut stdout, &mut stderr);
    let stdout_text = String::from_utf8_lossy(&stdout).trim().to_string();
    let stderr_text = String::from_utf8_lossy(&stderr).trim().to_string();
    match parse_str(&stdout_text) {
        Ok(report) => Component {
            status,
            report: Some(report),
            error: if stderr_text.is_empty() {
                None
            } else {
                Some(stderr_text)
            },
            planned: false,
        },
        Err(error) => Component {
            status,
            report: None,
            error: Some(if stderr_text.is_empty() {
                format!("component returned invalid JSON: {}", error)
            } else {
                stderr_text
            }),
            planned: false,
        },
    }
}

fn write_text_report(report: &JsonValue, stdout: &mut dyn Write) {
    let ok = report
        .get("ok")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let dry_run = report
        .get("dryRun")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let removed_clients = report
        .get("clients")
        .and_then(|value| value.get("removed"))
        .and_then(JsonValue::as_array)
        .map_or(0, <[JsonValue]>::len);

    let _ = writeln!(
        stdout,
        "MCPace uninstall {}: {}",
        if dry_run { "preview" } else { "complete" },
        if ok { "ok" } else { "needs attention" }
    );
    let _ = writeln!(
        stdout,
        "Owned client integrations removed/planned: {}",
        removed_clients
    );
    let _ = writeln!(
        stdout,
        "Configuration, upstream definitions, and backups were preserved."
    );
    let _ = writeln!(
        stdout,
        "The npm package was not removed; use your package manager after this command succeeds."
    );
}

fn argv(args: &[String]) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(OsString::from("mcpace uninstall"));
    argv.extend(args.iter().map(OsString::from));
    argv
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace uninstall [--dry-run] [--keep-clients] [--json] [--root <path>]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Stops MCPace, removes current and historical MCPace login-startup entries, removes only verified MCPace-owned client entries, and clears ephemeral runtime state.");
    let _ = writeln!(stdout, "Durable MCPace configuration, upstream definitions, backups, and the installed package are preserved.");
}

#[cfg(test)]
mod tests;
