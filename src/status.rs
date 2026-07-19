use crate::json::{parse_str, JsonValue};
use crate::{autostart, diagnostics, runtimepaths, serve};
use clap::{error::ErrorKind, Parser};
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "mcpace status",
    disable_version_flag = true,
    about = "Show aggregate MCPace runtime and login-startup status"
)]
struct StatusCli {
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
    let cli = match StatusCli::try_parse_from(argv(args)) {
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
    let initialized = root_path.join("mcpace.config.json").is_file();

    let runtime = capture_json(|component_stdout, component_stderr| {
        serve::run(
            &[
                "status".to_string(),
                "--json".to_string(),
                "--root".to_string(),
                root_text.clone(),
            ],
            Some(root_path.clone()),
            component_stdout,
            component_stderr,
        )
    });
    let startup = capture_json(|component_stdout, component_stderr| {
        autostart::run(
            &[
                "verify".to_string(),
                "--json".to_string(),
                "--root".to_string(),
                root_text.clone(),
            ],
            Some(root_path.clone()),
            component_stdout,
            component_stderr,
        )
    });

    let runtime_running = runtime
        .value
        .as_ref()
        .and_then(|value| value.get("status"))
        .and_then(JsonValue::as_str)
        == Some("running");
    let startup_enabled = startup
        .value
        .as_ref()
        .and_then(|value| value.get("enabled"))
        .and_then(JsonValue::as_bool);
    let startup_valid = startup
        .value
        .as_ref()
        .and_then(|value| value.get("ok"))
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let ok = initialized && runtime_running;

    let report = JsonValue::object([
        ("schema", JsonValue::string("mcpace.status.v1")),
        ("ok", JsonValue::bool(ok)),
        ("initialized", JsonValue::bool(initialized)),
        ("rootPath", JsonValue::string(root_text)),
        ("runtimeRunning", JsonValue::bool(runtime_running)),
        (
            "startupEnabled",
            startup_enabled
                .map(JsonValue::bool)
                .unwrap_or(JsonValue::Null),
        ),
        ("startupValid", JsonValue::bool(startup_valid)),
        ("runtime", component_value(runtime)),
        ("startup", component_value(startup)),
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

struct CapturedComponent {
    status: i32,
    value: Option<JsonValue>,
    error: Option<String>,
}

fn capture_json(
    run_component: impl FnOnce(&mut dyn Write, &mut dyn Write) -> i32,
) -> CapturedComponent {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let status = run_component(&mut stdout, &mut stderr);
    let stdout_text = String::from_utf8_lossy(&stdout).trim().to_string();
    let stderr_text = String::from_utf8_lossy(&stderr).trim().to_string();
    match parse_str(&stdout_text) {
        Ok(value) => CapturedComponent {
            status,
            value: Some(value),
            error: if stderr_text.is_empty() {
                None
            } else {
                Some(stderr_text)
            },
        },
        Err(error) => CapturedComponent {
            status,
            value: None,
            error: Some(if stderr_text.is_empty() {
                format!("component returned invalid JSON: {}", error)
            } else {
                stderr_text
            }),
        },
    }
}

fn component_value(component: CapturedComponent) -> JsonValue {
    JsonValue::object([
        ("exitCode", JsonValue::number(component.status)),
        ("report", component.value.unwrap_or(JsonValue::Null)),
        (
            "error",
            component
                .error
                .map(JsonValue::string)
                .unwrap_or(JsonValue::Null),
        ),
    ])
}

fn write_text_report(report: &JsonValue, stdout: &mut dyn Write) {
    let running = report
        .get("runtimeRunning")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let initialized = report
        .get("initialized")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let startup_enabled = report.get("startupEnabled").and_then(JsonValue::as_bool);
    let startup_valid = report
        .get("startupValid")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let root = report
        .get("rootPath")
        .and_then(JsonValue::as_str)
        .unwrap_or("unknown");

    let _ = writeln!(
        stdout,
        "MCPace: {}",
        if running {
            "running"
        } else {
            "stopped or unhealthy"
        }
    );
    let _ = writeln!(
        stdout,
        "Initialized: {}",
        if initialized { "yes" } else { "no" }
    );
    let _ = writeln!(stdout, "Root: {}", root);
    let _ = writeln!(
        stdout,
        "Login startup: {} ({})",
        match startup_enabled {
            Some(true) => "enabled",
            Some(false) => "disabled",
            None => "unknown",
        },
        if startup_valid {
            "valid"
        } else {
            "needs attention"
        }
    );
    let _ = writeln!(
        stdout,
        "Run `mcpace advanced doctor` for detailed diagnostics."
    );
}

fn argv(args: &[String]) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(OsString::from("mcpace status"));
    argv.extend(args.iter().map(OsString::from));
    argv
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "Usage: mcpace status [--json] [--root <path>]");
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "Read-only aggregate of runtime health and login-startup registration."
    );
}

#[cfg(test)]
mod tests;
