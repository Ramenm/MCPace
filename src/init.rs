use crate::diagnostics;
use crate::doctor;
use crate::json::JsonValue;
use crate::profile;
use crate::runtimepaths;
use crate::setup;
use crate::text_utils::yes_no;
use crate::verify;
use clap::{error::ErrorKind, Parser};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
struct ParsedArgs {
    json_output: bool,
    help: bool,
    root_override: Option<PathBuf>,
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct InitReport {
    root_path: String,
    state_root: String,
    runtime_dir: String,
    hub_dir: String,
    project_registry_path: String,
    lease_store_path: String,
    config_version: Option<String>,
    active_profile: String,
    profile_selection_source: String,
    ready_for_read_only_ops: bool,
    ready_for_runtime_ops: bool,
    created_paths: Vec<String>,
    existing_paths: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum InitError {
    RuntimePath(runtimepaths::RuntimePathError),
    Io {
        operation: &'static str,
        path: PathBuf,
        reason: String,
    },
    Upstream(String),
}

type InitResult<T> = Result<T, InitError>;

impl InitError {
    fn io(operation: &'static str, path: &Path, error: impl fmt::Display) -> Self {
        Self::Io {
            operation,
            path: path.to_path_buf(),
            reason: error.to_string(),
        }
    }
}

impl fmt::Display for InitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RuntimePath(error) => write!(formatter, "{}", error),
            Self::Io {
                operation,
                path,
                reason,
            } => {
                write!(
                    formatter,
                    "failed to {} {}: {}",
                    operation,
                    path.display(),
                    reason
                )
            }
            Self::Upstream(error) => write!(formatter, "{}", error),
        }
    }
}

impl std::error::Error for InitError {}

impl From<runtimepaths::RuntimePathError> for InitError {
    fn from(error: runtimepaths::RuntimePathError) -> Self {
        Self::RuntimePath(error)
    }
}

impl From<String> for InitError {
    fn from(error: String) -> Self {
        Self::Upstream(error)
    }
}

impl From<verify::ReadinessError> for InitError {
    fn from(error: verify::ReadinessError) -> Self {
        Self::Upstream(error.to_string())
    }
}

impl From<InitError> for String {
    fn from(error: InitError) -> Self {
        error.to_string()
    }
}

pub fn run(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let parsed = parse_cli(args);
    if let Some(error) = parsed.error {
        diagnostics::stderr_line(stderr, format_args!("{}", error));
        return 2;
    }
    if parsed.help {
        write_help(stdout);
        return 0;
    }

    let root_path = parsed.root_override.or(default_root);
    let Some(root_path) = root_path else {
        diagnostics::stderr_line(
            stderr,
            format_args!("mcpace root not found; expected mcpace.config.json"),
        );
        return 1;
    };

    let report = match initialize_layout(&root_path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };

    if parsed.json_output {
        let _ = writeln!(stdout, "{}", report.to_json_value().to_pretty_string());
        return 0;
    }

    let _ = writeln!(stdout, "Initialized MCPace state layout");
    let _ = writeln!(stdout, "Root path: {}", report.root_path);
    let _ = writeln!(stdout, "State root: {}", report.state_root);
    let _ = writeln!(stdout, "Runtime dir: {}", report.runtime_dir);
    let _ = writeln!(stdout, "Hub dir: {}", report.hub_dir);
    let _ = writeln!(stdout, "Project registry: {}", report.project_registry_path);
    let _ = writeln!(stdout, "Lease store: {}", report.lease_store_path);
    let _ = writeln!(stdout, "Active profile: {}", report.active_profile);
    let _ = writeln!(
        stdout,
        "Profile selection source: {}",
        report.profile_selection_source
    );
    let _ = writeln!(
        stdout,
        "Read-only readiness: {}",
        yes_no(report.ready_for_read_only_ops)
    );
    let _ = writeln!(
        stdout,
        "Runtime readiness: {}",
        yes_no(report.ready_for_runtime_ops)
    );
    if !report.created_paths.is_empty() {
        let _ = writeln!(stdout, "Created: {}", report.created_paths.join(", "));
    }
    if !report.existing_paths.is_empty() {
        let _ = writeln!(
            stdout,
            "Already existed: {}",
            report.existing_paths.join(", ")
        );
    }
    0
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace advanced dev init [--json] [--root <path>]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Implemented now:");
    let _ = writeln!(
        stdout,
        "  mcpace advanced dev init [--json] [--root <path>]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "init creates the runtime state layout, seeds empty state stores, and reports current readiness.");
}

#[derive(Debug, Parser)]
#[command(
    name = "mcpace advanced dev init",
    disable_version_flag = true,
    about = "Initialize the MCPace state layout"
)]
struct InitCli {
    #[arg(long = "json")]
    json_output: bool,

    #[arg(long = "root", value_name = "PATH")]
    root_override: Option<PathBuf>,
}

fn parse_cli(args: &[String]) -> ParsedArgs {
    match InitCli::try_parse_from(argv(args)) {
        Ok(cli) => ParsedArgs {
            json_output: cli.json_output,
            help: false,
            root_override: cli.root_override,
            error: None,
        },
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            ParsedArgs {
                help: true,
                ..ParsedArgs::default()
            }
        }
        Err(error) => ParsedArgs {
            error: Some(error.to_string()),
            ..ParsedArgs::default()
        },
    }
}

fn argv(args: &[String]) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(OsString::from("mcpace advanced dev init"));
    argv.extend(args.iter().map(OsString::from));
    argv
}

fn initialize_layout(root_path: &Path) -> InitResult<InitReport> {
    setup::bootstrap_root_layout(root_path).map_err(InitError::Upstream)?;
    let state_root = runtimepaths::resolve_state_root(root_path);
    let runtime_dir = runtimepaths::runtime_dir(&state_root);
    let runtime_dir_existed = runtime_dir.exists();
    runtimepaths::ensure_runtime_dir(&state_root)?;
    let hub_dir = runtimepaths::hub_dir(&state_root);
    let hub_dir_existed = hub_dir.exists();
    runtimepaths::ensure_hub_dir(&state_root)?;
    let project_registry_path = runtimepaths::project_registry_path(&state_root);
    let lease_store_path = runtimepaths::hub_leases_path(&state_root);

    let mut created_paths = Vec::new();
    let mut existing_paths = Vec::new();
    register_dir_state(
        &runtime_dir,
        runtime_dir_existed,
        &mut created_paths,
        &mut existing_paths,
    );
    register_dir_state(
        &hub_dir,
        hub_dir_existed,
        &mut created_paths,
        &mut existing_paths,
    );
    seed_json_if_missing(
        &project_registry_path,
        JsonValue::object([
            ("version", JsonValue::number(1)),
            ("projects", JsonValue::Object(BTreeMap::new())),
        ]),
        &mut created_paths,
        &mut existing_paths,
    )?;
    seed_json_if_missing(
        &lease_store_path,
        JsonValue::object([
            ("version", JsonValue::number(2)),
            ("leases", JsonValue::Object(BTreeMap::new())),
            ("sessions", JsonValue::Object(BTreeMap::new())),
            (
                "updatedAtMs",
                JsonValue::number(runtimepaths::unix_time_ms()),
            ),
        ]),
        &mut created_paths,
        &mut existing_paths,
    )?;

    let runtime_profile = profile::load_runtime_profile_selection(root_path)
        .map_err(|error| InitError::Upstream(error.to_string()))?;
    let readiness = verify::collect_readiness(root_path)?;

    Ok(InitReport {
        root_path: root_path.display().to_string(),
        state_root: state_root.display().to_string(),
        runtime_dir: runtime_dir.display().to_string(),
        hub_dir: hub_dir.display().to_string(),
        project_registry_path: project_registry_path.display().to_string(),
        lease_store_path: lease_store_path.display().to_string(),
        config_version: doctor::read_config_version(root_path),
        active_profile: runtime_profile.active_profile,
        profile_selection_source: runtime_profile.selection_source,
        ready_for_read_only_ops: readiness.ready_for_read_only_ops,
        ready_for_runtime_ops: readiness.ready_for_runtime_ops,
        created_paths,
        existing_paths,
    })
}

fn register_dir_state(
    path: &Path,
    existed_before_init: bool,
    created_paths: &mut Vec<String>,
    existing_paths: &mut Vec<String>,
) {
    if existed_before_init {
        existing_paths.push(path.display().to_string());
    } else {
        created_paths.push(path.display().to_string());
    }
}

fn seed_json_if_missing(
    path: &Path,
    value: JsonValue,
    created_paths: &mut Vec<String>,
    existing_paths: &mut Vec<String>,
) -> InitResult<()> {
    if path.is_file() {
        existing_paths.push(path.display().to_string());
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| InitError::io("create", parent, error))?;
    }
    runtimepaths::write_text_atomic(path, &value.to_pretty_string())?;
    created_paths.push(path.display().to_string());
    Ok(())
}

#[cfg(test)]
mod tests;

impl InitReport {
    fn to_json_value(&self) -> JsonValue {
        let mut map = BTreeMap::new();
        map.insert(
            "rootPath".to_string(),
            JsonValue::string(self.root_path.clone()),
        );
        map.insert(
            "stateRoot".to_string(),
            JsonValue::string(self.state_root.clone()),
        );
        map.insert(
            "runtimeDir".to_string(),
            JsonValue::string(self.runtime_dir.clone()),
        );
        map.insert(
            "hubDir".to_string(),
            JsonValue::string(self.hub_dir.clone()),
        );
        map.insert(
            "projectRegistryPath".to_string(),
            JsonValue::string(self.project_registry_path.clone()),
        );
        map.insert(
            "leaseStorePath".to_string(),
            JsonValue::string(self.lease_store_path.clone()),
        );
        match &self.config_version {
            Some(value) => {
                map.insert(
                    "configVersion".to_string(),
                    JsonValue::string(value.clone()),
                );
            }
            None => {
                map.insert("configVersion".to_string(), JsonValue::Null);
            }
        }
        map.insert(
            "activeProfile".to_string(),
            JsonValue::string(self.active_profile.clone()),
        );
        map.insert(
            "profileSelectionSource".to_string(),
            JsonValue::string(self.profile_selection_source.clone()),
        );
        map.insert(
            "readyForReadOnlyOps".to_string(),
            JsonValue::bool(self.ready_for_read_only_ops),
        );
        map.insert(
            "readyForRuntimeOps".to_string(),
            JsonValue::bool(self.ready_for_runtime_ops),
        );
        map.insert(
            "createdPaths".to_string(),
            JsonValue::array(self.created_paths.iter().cloned().map(JsonValue::string)),
        );
        map.insert(
            "existingPaths".to_string(),
            JsonValue::array(self.existing_paths.iter().cloned().map(JsonValue::string)),
        );
        JsonValue::Object(map)
    }
}
