use crate::json::JsonValue;
use crate::json_helpers;
use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct Report {
    pub project: ProjectStatus,
    pub tools: Vec<ToolStatus>,
}

#[derive(Debug, Clone)]
pub struct ProjectStatus {
    pub root_path: Option<String>,
    pub config_found: bool,
    pub cargo_manifest_found: bool,
    pub npm_workspace_found: bool,
    pub release_manifest_found: bool,
    pub config_version: Option<String>,
    pub rust_source_ready: bool,
    pub npm_surface_ready: bool,
    pub runtime_prerequisites_ready: bool,
    pub container_tooling_ready: bool,
}

#[derive(Debug, Clone)]
pub struct ToolStatus {
    pub name: String,
    pub required: bool,
    pub found: bool,
    pub version: Option<String>,
}

struct ToolProbeSpec {
    name: &'static str,
    required: bool,
    version_args: &'static [&'static str],
}

const TOOL_PROBE_SPECS: &[ToolProbeSpec] = &[
    ToolProbeSpec {
        name: "cargo",
        required: true,
        version_args: &["--version"],
    },
    ToolProbeSpec {
        name: "rustc",
        required: true,
        version_args: &["--version"],
    },
    ToolProbeSpec {
        name: "node",
        required: true,
        version_args: &["--version"],
    },
    ToolProbeSpec {
        name: "npm",
        required: true,
        version_args: &["--version"],
    },
    ToolProbeSpec {
        name: "docker",
        required: false,
        version_args: &["--version"],
    },
];

impl Report {
    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("project", self.project.to_json_value()),
            (
                "tools",
                JsonValue::array(self.tools.iter().map(ToolStatus::to_json_value)),
            ),
        ])
    }

    pub fn to_pretty_json(&self) -> String {
        self.to_json_value().to_pretty_string()
    }
}

impl ProjectStatus {
    fn to_json_value(&self) -> JsonValue {
        let mut map = BTreeMap::new();
        match &self.root_path {
            Some(value) => {
                map.insert("rootPath".to_string(), JsonValue::string(value.clone()));
            }
            None => {
                map.insert("rootPath".to_string(), JsonValue::Null);
            }
        }
        map.insert(
            "configFound".to_string(),
            JsonValue::bool(self.config_found),
        );
        map.insert(
            "cargoManifestFound".to_string(),
            JsonValue::bool(self.cargo_manifest_found),
        );
        map.insert(
            "npmWorkspaceFound".to_string(),
            JsonValue::bool(self.npm_workspace_found),
        );
        map.insert(
            "releaseManifestFound".to_string(),
            JsonValue::bool(self.release_manifest_found),
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
            "rustSourceReady".to_string(),
            JsonValue::bool(self.rust_source_ready),
        );
        map.insert(
            "npmSurfaceReady".to_string(),
            JsonValue::bool(self.npm_surface_ready),
        );
        map.insert(
            "runtimePrerequisitesReady".to_string(),
            JsonValue::bool(self.runtime_prerequisites_ready),
        );
        map.insert(
            "containerToolingReady".to_string(),
            JsonValue::bool(self.container_tooling_ready),
        );
        JsonValue::Object(map)
    }
}

impl ToolStatus {
    fn to_json_value(&self) -> JsonValue {
        let mut map = BTreeMap::new();
        map.insert("name".to_string(), JsonValue::string(self.name.clone()));
        map.insert("required".to_string(), JsonValue::bool(self.required));
        map.insert("found".to_string(), JsonValue::bool(self.found));
        match &self.version {
            Some(value) => {
                map.insert("version".to_string(), JsonValue::string(value.clone()));
            }
            None => {
                map.insert("version".to_string(), JsonValue::Null);
            }
        }
        JsonValue::Object(map)
    }
}

pub fn run(root_path: Option<PathBuf>) -> Report {
    let tools = TOOL_PROBE_SPECS
        .iter()
        .map(|spec| tool_status(spec.name, spec.required, spec.version_args))
        .collect::<Vec<_>>();
    let project = load_project_status(root_path.as_deref(), &tools);
    Report { project, tools }
}

pub fn write_text_report(report: &Report, stdout: &mut dyn std::io::Write) {
    let _ = writeln!(
        stdout,
        "Project root: {}",
        report.project.root_path.as_deref().unwrap_or("not found")
    );
    let _ = writeln!(
        stdout,
        "Config version: {}",
        report
            .project
            .config_version
            .as_deref()
            .unwrap_or("unknown")
    );
    let _ = writeln!(
        stdout,
        "Rust source readiness: {}",
        yes_no(report.project.rust_source_ready)
    );
    let _ = writeln!(
        stdout,
        "npm surface readiness: {}",
        yes_no(report.project.npm_surface_ready)
    );
    let _ = writeln!(
        stdout,
        "Runtime prerequisites readiness: {}",
        yes_no(report.project.runtime_prerequisites_ready)
    );
    let _ = writeln!(
        stdout,
        "Optional container tooling readiness: {}",
        yes_no(report.project.container_tooling_ready)
    );
    let _ = writeln!(stdout, "Tools:");
    for tool in &report.tools {
        let state = if tool.found { "found" } else { "missing" };
        let required = if tool.required {
            "required"
        } else {
            "optional"
        };
        let version = tool
            .version
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let _ = writeln!(
            stdout,
            "- {}: {} ({}, {})",
            tool.name, state, required, version
        );
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

pub fn read_config_version(root_path: &Path) -> Option<String> {
    let config_path = root_path.join("mcpace.config.json");
    let json = json_helpers::read_json_file(&config_path).ok()?;
    json_helpers::string_at_path(&json, &["version"]).map(str::to_string)
}

pub fn command_available(name: &str) -> bool {
    find_command_path(name).is_some()
}

fn load_project_status(root_path: Option<&Path>, tools: &[ToolStatus]) -> ProjectStatus {
    let root_path_string = root_path.map(|value| value.display().to_string());
    let config_found = root_path
        .map(|value| value.join("mcpace.config.json").is_file())
        .unwrap_or(false);
    let cargo_manifest_found = root_path
        .map(|value| value.join("Cargo.toml").is_file())
        .unwrap_or(false);
    let npm_workspace_found = root_path
        .map(|value| value.join("package.json").is_file() && value.join("packages").is_dir())
        .unwrap_or(false);
    let release_manifest_found = root_path
        .map(|value| value.join("release-manifest.json").is_file())
        .unwrap_or(false);
    let config_version = root_path.and_then(read_config_version);

    let cargo_ready = tool_found(tools, "cargo");
    let rustc_ready = tool_found(tools, "rustc");
    let node_ready = tool_found(tools, "node");
    let npm_ready = tool_found(tools, "npm");
    let docker_ready = tool_found(tools, "docker");
    let container_tooling_required = runtime_requires_container_tooling(root_path);

    ProjectStatus {
        root_path: root_path_string,
        config_found,
        cargo_manifest_found,
        npm_workspace_found,
        release_manifest_found,
        config_version,
        rust_source_ready: cargo_manifest_found && cargo_ready && rustc_ready,
        npm_surface_ready: npm_workspace_found && node_ready && npm_ready,
        runtime_prerequisites_ready: config_found
            && (!container_tooling_required || docker_ready),
        container_tooling_ready: docker_ready,
    }
}

fn runtime_requires_container_tooling(root_path: Option<&Path>) -> bool {
    let Some(root_path) = root_path else {
        return false;
    };
    let config_path = root_path.join("mcpace.config.json");
    let Ok(config) = json_helpers::read_json_file(&config_path) else {
        return false;
    };
    let Some(servers) = json_helpers::object_at_path(&config, &["servers"]) else {
        return false;
    };

    servers.values().any(|server| {
        let Some(server) = server.as_object() else {
            return false;
        };
        let kind = server
            .get("kind")
            .and_then(JsonValue::as_str)
            .map(|value| value.trim().to_ascii_lowercase())
            .unwrap_or_default();
        let runtime_enabled = ["required", "defaultEnabled", "autoStart"]
            .iter()
            .any(|key| {
                server
                    .get(*key)
                    .and_then(JsonValue::as_bool)
                    .unwrap_or(false)
            });
        kind.starts_with("container-") && runtime_enabled
    })
}

fn tool_found(tools: &[ToolStatus], name: &str) -> bool {
    tools.iter()
        .find(|tool| tool.name == name)
        .map(|tool| tool.found)
        .unwrap_or(false)
}

fn tool_status(name: &str, required: bool, args: &[&str]) -> ToolStatus {
    let found = command_available(name);
    let version = if found { command_version(name, args) } else { None };

    ToolStatus {
        name: name.to_string(),
        required,
        found,
        version,
    }
}

fn command_version(name: &str, args: &[&str]) -> Option<String> {
    let path = find_command_path(name)?;
    let output = Command::new(path).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_first_line(&output.stdout)
}

fn find_command_path(name: &str) -> Option<PathBuf> {
    let candidate = Path::new(name);
    if candidate.is_absolute() || name.contains(std::path::MAIN_SEPARATOR) {
        return is_runnable_path(candidate).then(|| candidate.to_path_buf());
    }

    let path_value = env::var_os("PATH")?;
    for directory in env::split_paths(&path_value) {
        let direct = directory.join(name);
        if is_runnable_path(&direct) {
            return Some(direct);
        }

        if cfg!(windows) {
            let pathext = env::var_os("PATHEXT")
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_string());
            for extension in pathext.split(';').filter(|entry| !entry.trim().is_empty()) {
                let trimmed = extension.trim();
                let suffix = if trimmed.starts_with('.') {
                    trimmed.to_string()
                } else {
                    format!(".{}", trimmed)
                };
                let with_extension = directory.join(format!("{}{}", name, suffix));
                if is_runnable_path(&with_extension) {
                    return Some(with_extension);
                }
            }
        }
    }

    None
}

fn is_runnable_path(path: &Path) -> bool {
    path.is_file()
}

fn parse_first_line(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    text.lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
}
