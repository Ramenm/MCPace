use crate::codex_config;
use crate::json::JsonValue;
use crate::json_helpers;
use crate::mcp_sources;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const VERSION_COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

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
    pub runtime_prerequisites: Vec<RuntimePrerequisiteStatus>,
    pub missing_runtime_prerequisites: Vec<String>,
    pub client_config_warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ToolStatus {
    pub name: String,
    pub required: bool,
    pub found: bool,
    pub version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RuntimePrerequisiteStatus {
    pub name: String,
    pub found: bool,
    pub reasons: Vec<String>,
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
        map.insert(
            "runtimePrerequisites".to_string(),
            JsonValue::array(
                self.runtime_prerequisites
                    .iter()
                    .map(RuntimePrerequisiteStatus::to_json_value),
            ),
        );
        map.insert(
            "missingRuntimePrerequisites".to_string(),
            JsonValue::array(
                self.missing_runtime_prerequisites
                    .iter()
                    .cloned()
                    .map(JsonValue::string),
            ),
        );
        map.insert(
            "clientConfigWarnings".to_string(),
            JsonValue::array(
                self.client_config_warnings
                    .iter()
                    .cloned()
                    .map(JsonValue::string),
            ),
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

impl RuntimePrerequisiteStatus {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("name", JsonValue::string(self.name.clone())),
            ("found", JsonValue::bool(self.found)),
            (
                "reasons",
                JsonValue::array(self.reasons.iter().cloned().map(JsonValue::string)),
            ),
        ])
    }
}

pub fn run(root_path: Option<PathBuf>) -> Report {
    run_with_version_probe_policy(root_path, true)
}

pub fn run_without_version_probes(root_path: Option<PathBuf>) -> Report {
    run_with_version_probe_policy(root_path, false)
}

fn run_with_version_probe_policy(root_path: Option<PathBuf>, probe_versions: bool) -> Report {
    let tools = TOOL_PROBE_SPECS
        .iter()
        .map(|spec| tool_status(spec.name, spec.required, spec.version_args, probe_versions))
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
    let _ = writeln!(
        stdout,
        "Missing runtime prerequisites: {}",
        join_or_none(&report.project.missing_runtime_prerequisites)
    );
    let _ = writeln!(
        stdout,
        "Client config warnings: {}",
        join_or_none(&report.project.client_config_warnings)
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

fn join_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
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
    let runtime_prerequisites = collect_runtime_prerequisites(root_path);
    let missing_runtime_prerequisites = runtime_prerequisites
        .iter()
        .filter(|prerequisite| !prerequisite.found)
        .map(|prerequisite| prerequisite.name.clone())
        .collect::<Vec<_>>();
    let client_config_warnings = collect_client_config_warnings();

    ProjectStatus {
        root_path: root_path_string,
        config_found,
        cargo_manifest_found,
        npm_workspace_found,
        release_manifest_found,
        config_version,
        rust_source_ready: cargo_manifest_found && cargo_ready && rustc_ready,
        npm_surface_ready: npm_workspace_found && node_ready && npm_ready,
        runtime_prerequisites_ready: config_found && missing_runtime_prerequisites.is_empty(),
        container_tooling_ready: docker_ready,
        runtime_prerequisites,
        missing_runtime_prerequisites,
        client_config_warnings,
    }
}

fn collect_client_config_warnings() -> Vec<String> {
    let Some(home) = user_home_dir() else {
        return Vec::new();
    };
    let mut warnings = Vec::new();
    warnings.extend(collect_codex_toml_command_warnings(
        &home.join(".codex").join("config.toml"),
    ));
    warnings.sort();
    warnings.dedup();
    warnings
}

fn collect_codex_toml_command_warnings(config_path: &Path) -> Vec<String> {
    let Ok(contents) = std::fs::read_to_string(config_path) else {
        return Vec::new();
    };
    codex_config::missing_mcp_server_commands(&contents, None)
        .into_iter()
        .map(|entry| {
            format!(
                "Codex MCP server '{}' in '{}' uses command '{}', but that program was not found on PATH; this can fail MCP startup before MCPace runs.",
                entry.server_name,
                config_path.display(),
                entry.command
            )
        })
        .collect()
}

fn user_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn collect_runtime_prerequisites(root_path: Option<&Path>) -> Vec<RuntimePrerequisiteStatus> {
    let Some(root_path) = root_path else {
        return Vec::new();
    };
    let config_path = root_path.join("mcpace.config.json");
    let config = json_helpers::read_json_file(&config_path).ok();
    let servers = config
        .as_ref()
        .and_then(|value| json_helpers::object_at_path(value, &["servers"]));
    let source_settings = load_source_runtime_commands(root_path);

    let mut required_commands: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (server_name, command) in &source_settings {
        add_runtime_prerequisite(
            &mut required_commands,
            command,
            format!("enabled stdio source command for server '{}'", server_name),
        );
    }

    if let Some(servers) = servers {
        for (server_name, server) in servers {
            let Some(server) = server.as_object() else {
                continue;
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
            if !runtime_enabled {
                continue;
            }

            if kind.starts_with("container-") {
                add_runtime_prerequisite(
                    &mut required_commands,
                    "docker",
                    format!("container runtime required by server '{}'", server_name),
                );
            }

            for command in json_helpers::strings_from_array(
                server.get("requiredCommands").and_then(JsonValue::as_array),
            ) {
                add_runtime_prerequisite(
                    &mut required_commands,
                    &command,
                    format!("requiredCommands entry for server '{}'", server_name),
                );
            }
        }
    }

    required_commands
        .into_iter()
        .map(|(name, reasons)| RuntimePrerequisiteStatus {
            found: command_available(&name),
            name,
            reasons,
        })
        .collect()
}

fn load_source_runtime_commands(root_path: &Path) -> BTreeMap<String, String> {
    let Ok(registry) = mcp_sources::load_mcp_server_registry(root_path) else {
        return BTreeMap::new();
    };

    let mut commands = BTreeMap::new();
    for entry in registry.servers.values() {
        let Some(server) = entry.value.as_object() else {
            continue;
        };
        let enabled = server
            .get("enabled")
            .and_then(JsonValue::as_bool)
            .unwrap_or(true);
        let command = server
            .get("command")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("");
        let url = server
            .get("url")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .unwrap_or("");
        let source_type = infer_runtime_source_type(
            server.get("type").and_then(JsonValue::as_str).unwrap_or(""),
            command,
            url,
        );
        if enabled && source_type == "stdio" && !command.is_empty() {
            commands.insert(entry.normalized_name.clone(), command.to_string());
        }
    }
    commands
}

fn infer_runtime_source_type(raw_source_type: &str, command: &str, url: &str) -> String {
    let normalized = normalize_runtime_source_type(raw_source_type);
    if !normalized.is_empty() {
        return normalized;
    }
    if !command.trim().is_empty() {
        "stdio".to_string()
    } else if !url.trim().is_empty() {
        "http".to_string()
    } else {
        "stdio".to_string()
    }
}

fn normalize_runtime_source_type(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "" => String::new(),
        "streamablehttp" | "streamable-http" | "http-stream" | "remote-http" | "remote-sse"
        | "remote" | "http" | "sse" => "http".to_string(),
        "stdio" | "local" | "local-stdio" | "local-command" | "command" => "stdio".to_string(),
        other => other.to_string(),
    }
}

fn add_runtime_prerequisite(
    required_commands: &mut BTreeMap<String, Vec<String>>,
    command: &str,
    reason: String,
) {
    let normalized = command.trim();
    if normalized.is_empty() {
        return;
    }
    required_commands
        .entry(normalized.to_string())
        .or_default()
        .push(reason);
}

fn tool_found(tools: &[ToolStatus], name: &str) -> bool {
    tools
        .iter()
        .find(|tool| tool.name == name)
        .map(|tool| tool.found)
        .unwrap_or(false)
}

fn tool_status(name: &str, required: bool, args: &[&str], probe_versions: bool) -> ToolStatus {
    let found = command_available(name);
    let version = if found && probe_versions {
        command_version(name, args)
    } else {
        None
    };

    ToolStatus {
        name: name.to_string(),
        required,
        found,
        version,
    }
}

fn command_version(name: &str, args: &[&str]) -> Option<String> {
    let path = find_command_path(name)?;
    let mut command = Command::new(path);
    #[cfg(windows)]
    crate::windows_process::configure_no_window(&mut command);
    command.args(args);
    let output = command_output_with_timeout(&mut command, VERSION_COMMAND_TIMEOUT)?;
    if !output.status.success() {
        return None;
    }
    parse_first_line(&output.stdout)
}

fn command_output_with_timeout(command: &mut Command, timeout: Duration) -> Option<Output> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => return child.wait_with_output().ok(),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                thread::sleep(Duration::from_millis(25));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

fn find_command_path(name: &str) -> Option<PathBuf> {
    which::which(name).ok()
}

fn parse_first_line(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    text.lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
}
