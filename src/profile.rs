use crate::diagnostics;
use crate::json::JsonValue;
use crate::json_helpers;
use crate::mcp_sources;
use clap::{error::ErrorKind, ArgAction, Parser};
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct ProfileReport {
    active_profile: String,
    default_profile: String,
    selection_source: String,
    profiles: Vec<ProfileSummary>,
}

#[derive(Debug, Clone)]
struct ProfileSummary {
    name: String,
    description: String,
    is_active: bool,
    server_override_count: usize,
}

#[derive(Debug, Default)]
struct ParsedArgs {
    json_output: bool,
    help: bool,
    root_override: Option<PathBuf>,
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct CatalogProfile {
    name: String,
    description: String,
    server_override_count: usize,
}

#[derive(Debug, Clone)]
struct ProfileCatalog {
    default_profile: String,
    profiles: BTreeMap<String, CatalogProfile>,
}

#[derive(Debug, Clone)]
pub struct RuntimeProfileSelection {
    pub active_profile: String,
    pub default_profile: String,
    pub selection_source: String,
    pub server_overrides: BTreeMap<String, bool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeProfileError {
    ConfigRead {
        path: PathBuf,
        source: json_helpers::JsonFileError,
    },
}

pub type RuntimeProfileResult<T> = Result<T, RuntimeProfileError>;

impl fmt::Display for RuntimeProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConfigRead { path, source } => {
                write!(
                    formatter,
                    "failed to load runtime profile config {}: {}",
                    path.display(),
                    source
                )
            }
        }
    }
}

impl std::error::Error for RuntimeProfileError {}

impl From<RuntimeProfileError> for String {
    fn from(error: RuntimeProfileError) -> Self {
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

    let catalog = match build_profile_catalog_from_config(&root_path) {
        Ok(config) => config,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    let resolved = match load_runtime_profile_selection(&root_path) {
        Ok(value) => value,
        Err(error) => {
            diagnostics::stderr_line(stderr, format_args!("{}", error));
            return 1;
        }
    };
    let report = build_report(catalog, resolved.active_profile, resolved.selection_source);

    if parsed.json_output {
        let _ = writeln!(stdout, "{}", report.to_json_value().to_pretty_string());
        0
    } else {
        let _ = writeln!(stdout, "Active runtime profile: {}", report.active_profile);
        let _ = writeln!(
            stdout,
            "Default runtime profile: {}",
            report.default_profile
        );
        let _ = writeln!(stdout, "Selection source: {}", report.selection_source);
        let _ = writeln!(stdout, "Available profiles:");
        for profile in &report.profiles {
            let marker = if profile.is_active { '*' } else { ' ' };
            let _ = writeln!(
                stdout,
                " {} {} - {} (server overrides: {})",
                marker, profile.name, profile.description, profile.server_override_count
            );
        }
        0
    }
}

pub fn load_runtime_profile_selection(
    root_path: &Path,
) -> RuntimeProfileResult<RuntimeProfileSelection> {
    let catalog = build_profile_catalog_from_config(root_path)?;
    let (active_profile, selection_source) = resolve_active_profile(&catalog);
    let server_overrides = read_server_overrides(root_path, &active_profile)?;

    Ok(RuntimeProfileSelection {
        active_profile,
        default_profile: catalog.default_profile,
        selection_source,
        server_overrides,
    })
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace profile [show] [--json] [--root <path>]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "Native Rust path supports read-only profile inspection; profile writes are reviewed through mcpace.config.json."
    );
}

#[derive(Debug, Parser)]
#[command(
    name = "mcpace profile",
    disable_version_flag = true,
    about = "Inspect MCPace runtime profiles"
)]
struct ProfileCli {
    /// Profile action. The Rust CLI currently exposes show.
    action: Option<String>,

    /// Emit machine-readable JSON.
    #[arg(long = "json", short = 'j')]
    json_output: bool,

    /// MCPace project/root directory.
    #[arg(long = "root", value_name = "PATH")]
    root_override: Option<PathBuf>,

    /// Reserved for future reviewed profile writes.
    #[arg(long = "name", value_name = "PROFILE", hide = true)]
    mutation_name: Option<String>,

    /// Reserved for future reviewed profile writes.
    #[arg(long = "apply", action = ArgAction::SetTrue, hide = true)]
    apply: bool,
}

fn parse_cli(args: &[String]) -> ParsedArgs {
    match ProfileCli::try_parse_from(profile_argv(args)) {
        Ok(cli) => compose_profile_args(cli),
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

fn compose_profile_args(cli: ProfileCli) -> ParsedArgs {
    if cli.apply || cli.mutation_name.is_some() {
        return ParsedArgs {
            error: Some(
                "profile writes are intentionally reviewed through mcpace.config.json; use profile show to inspect the active profile".to_string(),
            ),
            ..ParsedArgs::default()
        };
    }

    if let Some(action) = cli.action.as_deref() {
        if action != "show" {
            return ParsedArgs {
                error: Some(format!(
                    "unsupported profile arguments in the Rust-only repo: {}",
                    action
                )),
                ..ParsedArgs::default()
            };
        }
    }

    ParsedArgs {
        json_output: cli.json_output,
        help: false,
        root_override: cli.root_override,
        error: None,
    }
}

fn profile_argv(args: &[String]) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(OsString::from("mcpace profile"));
    argv.extend(
        args.iter()
            .map(|arg| OsString::from(normalize_profile_flag(arg))),
    );
    argv
}

fn normalize_profile_flag(arg: &str) -> &str {
    match arg {
        "-json" => "--json",
        "-root" => "--root",
        "-name" => "--name",
        "-apply" => "--apply",
        "-?" => "--help",
        other => other,
    }
}

fn build_profile_catalog_from_config(root_path: &Path) -> RuntimeProfileResult<ProfileCatalog> {
    let config_path = root_path.join("mcpace.config.json");
    let json = json_helpers::read_json_file(&config_path).map_err(|source| {
        RuntimeProfileError::ConfigRead {
            path: config_path.clone(),
            source,
        }
    })?;
    let runtime = json_helpers::object_at_path(&json, &["profiles", "runtime"]);
    let default = runtime
        .and_then(|runtime| runtime.get("default"))
        .and_then(JsonValue::as_str)
        .unwrap_or("safe")
        .trim()
        .to_ascii_lowercase();

    let mut profiles: BTreeMap<String, CatalogProfile> = BTreeMap::new();
    for (name, description, overrides) in builtin_profiles() {
        profiles.insert(
            name.to_string(),
            CatalogProfile {
                name: name.to_string(),
                description: description.to_string(),
                server_override_count: overrides,
            },
        );
    }

    if let Some(runtime_profiles) = runtime
        .and_then(|runtime| runtime.get("profiles"))
        .and_then(JsonValue::as_object)
    {
        for (name, raw_profile) in runtime_profiles {
            let normalized_name = name.trim().to_ascii_lowercase();
            if normalized_name.is_empty() {
                continue;
            }
            let description = raw_profile
                .get("description")
                .and_then(JsonValue::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .or_else(|| {
                    profiles
                        .get(&normalized_name)
                        .map(|profile| profile.description.clone())
                })
                .unwrap_or_default();
            let override_count = raw_profile
                .get("serverOverrides")
                .and_then(JsonValue::as_object)
                .map(|object| object.len())
                .unwrap_or(0);
            profiles.insert(
                normalized_name.clone(),
                CatalogProfile {
                    name: normalized_name,
                    description,
                    server_override_count: override_count,
                },
            );
        }
    }

    let resolved_default = if default.is_empty() || !profiles.contains_key(&default) {
        "safe".to_string()
    } else {
        default
    };

    Ok(ProfileCatalog {
        default_profile: resolved_default,
        profiles,
    })
}

fn builtin_profiles() -> [(&'static str, &'static str, usize); 3] {
    [
        (
            "safe",
            "Required path plus source-default safe optional integrations.",
            0,
        ),
        (
            "extended",
            "Reserved for validated local productivity integrations.",
            0,
        ),
        (
            "full",
            "Turns on all optional integrations and lets runtime gating decide what actually runs.",
            6,
        ),
    ]
}

fn resolve_active_profile(catalog: &ProfileCatalog) -> (String, String) {
    let env_profile = env::var("MCPACE_RUNTIME_PROFILE").ok();
    if let Some(value) = env_profile
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let normalized = value.to_ascii_lowercase();
        if catalog.profiles.contains_key(&normalized) {
            return (normalized, "environment".to_string());
        }
        return ("safe".to_string(), "fallback-safe".to_string());
    }

    if catalog.profiles.contains_key(&catalog.default_profile) {
        return (
            catalog.default_profile.clone(),
            "config-default".to_string(),
        );
    }

    ("safe".to_string(), "fallback-safe".to_string())
}

fn read_server_overrides(
    root_path: &Path,
    active_profile: &str,
) -> RuntimeProfileResult<BTreeMap<String, bool>> {
    let config_path = root_path.join("mcpace.config.json");
    let json = json_helpers::read_json_file(&config_path).map_err(|source| {
        RuntimeProfileError::ConfigRead {
            path: config_path.clone(),
            source,
        }
    })?;
    let runtime_profiles =
        json_helpers::object_at_path(&json, &["profiles", "runtime", "profiles"]);
    let mut overrides = BTreeMap::new();

    if let Some(runtime_profiles) = runtime_profiles {
        for (name, raw_profile) in runtime_profiles {
            if name.trim().to_ascii_lowercase() != active_profile {
                continue;
            }
            if let Some(entries) = raw_profile
                .get("serverOverrides")
                .and_then(JsonValue::as_object)
            {
                for (server_name, entry) in entries {
                    let enabled = match entry {
                        JsonValue::Bool(value) => *value,
                        JsonValue::Object(object) => object
                            .get("enabled")
                            .and_then(JsonValue::as_bool)
                            .unwrap_or(true),
                        _ => true,
                    };
                    let normalized_server_name = mcp_sources::normalize_server_name(server_name);
                    if !normalized_server_name.is_empty() {
                        overrides.insert(normalized_server_name, enabled);
                    }
                }
            }
            break;
        }
    }

    Ok(overrides)
}

fn build_report(
    catalog: ProfileCatalog,
    active_profile: String,
    selection_source: String,
) -> ProfileReport {
    let profiles = catalog
        .profiles
        .values()
        .map(|profile| ProfileSummary {
            name: profile.name.clone(),
            description: profile.description.clone(),
            is_active: profile.name == active_profile,
            server_override_count: profile.server_override_count,
        })
        .collect::<Vec<_>>();

    ProfileReport {
        active_profile,
        default_profile: catalog.default_profile,
        selection_source,
        profiles,
    }
}

impl ProfileReport {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            (
                "activeProfile",
                JsonValue::string(self.active_profile.clone()),
            ),
            (
                "defaultProfile",
                JsonValue::string(self.default_profile.clone()),
            ),
            (
                "selectionSource",
                JsonValue::string(self.selection_source.clone()),
            ),
            (
                "profiles",
                JsonValue::array(self.profiles.iter().map(ProfileSummary::to_json_value)),
            ),
        ])
    }
}

impl ProfileSummary {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("name", JsonValue::string(self.name.clone())),
            ("description", JsonValue::string(self.description.clone())),
            ("isActive", JsonValue::bool(self.is_active)),
            (
                "serverOverrideCount",
                JsonValue::number(self.server_override_count),
            ),
        ])
    }
}
