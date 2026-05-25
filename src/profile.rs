use crate::json::JsonValue;
use crate::json_helpers;
use crate::text_utils::normalize_flag;
use std::collections::BTreeMap;
use std::env;
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

    let root_path = parsed.root_override.or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };

    let catalog = match build_profile_catalog_from_config(&root_path) {
        Ok(config) => config,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };
    let resolved = match load_runtime_profile_selection(&root_path) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
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

pub fn load_runtime_profile_selection(root_path: &Path) -> Result<RuntimeProfileSelection, String> {
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
        "Native Rust path supports read-only profile inspection."
    );
    let _ = writeln!(
        stdout,
        "Profile mutation is not implemented yet in the Rust-only repo."
    );
}

fn parse_args(args: &[String]) -> ParsedArgs {
    let mut parsed = ParsedArgs::default();
    let mut index = 0usize;

    while index < args.len() {
        let token = normalize_flag(&args[index]);
        match token.as_str() {
            "show" => {
                index += 1;
            }
            "--json" | "-json" => {
                parsed.json_output = true;
                index += 1;
            }
            "--root" | "-root" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("profile requires a path after --root".to_string());
                    return parsed;
                };
                parsed.root_override = Some(PathBuf::from(value));
                index += 2;
            }
            "-h" | "--help" | "-?" => {
                parsed.help = true;
                return parsed;
            }
            "--name" | "-name" | "--apply" | "-apply" => {
                parsed.error = Some(
                    "profile mutation is not implemented yet in the Rust-only repo".to_string(),
                );
                return parsed;
            }
            _ => {
                parsed.error = Some(format!(
                    "unsupported profile arguments in the Rust-only repo: {}",
                    args[index]
                ));
                return parsed;
            }
        }
    }

    parsed
}

fn build_profile_catalog_from_config(root_path: &Path) -> Result<ProfileCatalog, String> {
    let config_path = root_path.join("mcpace.config.json");
    let json = json_helpers::read_json_file(&config_path)?;
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
) -> Result<BTreeMap<String, bool>, String> {
    let config_path = root_path.join("mcpace.config.json");
    let json = json_helpers::read_json_file(&config_path)?;
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
                    overrides.insert(server_name.trim().to_ascii_lowercase(), enabled);
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
