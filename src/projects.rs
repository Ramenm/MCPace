use crate::json::JsonValue;
use crate::json_helpers;
use crate::runtimepaths;
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
pub struct ProjectSummary {
    project_id: String,
    name: String,
    host_path: String,
    detected_type: String,
    markers: Vec<String>,
    last_used_at: String,
    state: String,
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
        let _ = writeln!(
            stderr,
            "mcpace root not found; expected mcpace.config.json for state discovery"
        );
        return 1;
    };

    let state_root = runtimepaths::resolve_state_root(&root_path);
    let registry_path = runtimepaths::project_registry_path(&state_root);
    let projects = match read_projects(&registry_path) {
        Ok(items) => items,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    if parsed.json_output {
        let json = JsonValue::array(projects.iter().map(ProjectSummary::to_json_value));
        let _ = writeln!(stdout, "{}", json.to_pretty_string());
        0
    } else {
        let _ = writeln!(stdout, "Known projects: {}", projects.len());
        if projects.is_empty() {
            let _ = writeln!(stdout, "No project registry entries yet.");
            return 0;
        }

        for project in &projects {
            let _ = writeln!(stdout, "- {} [{}]", project.name, project.detected_type);
            let _ = writeln!(stdout, "    host: {}", project.host_path);
            if !project.markers.is_empty() {
                let _ = writeln!(stdout, "    markers: {}", project.markers.join(", "));
            }
            if !project.last_used_at.trim().is_empty() {
                let _ = writeln!(stdout, "    last used: {}", project.last_used_at);
            }
        }
        0
    }
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace projects [list] [--json] [--root <path>]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "Native Rust path supports read-only project registry inspection."
    );
    let _ = writeln!(
        stdout,
        "Project scanning is not implemented yet in the Rust-only repo."
    );
}

fn parse_args(args: &[String]) -> ParsedArgs {
    let mut parsed = ParsedArgs::default();
    let mut index = 0usize;

    while index < args.len() {
        let token = normalize_flag(&args[index]);
        match token.as_str() {
            "list" => {
                index += 1;
            }
            "--json" | "-json" => {
                parsed.json_output = true;
                index += 1;
            }
            "--root" | "-root" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("projects requires a path after --root".to_string());
                    return parsed;
                };
                parsed.root_override = Some(PathBuf::from(value));
                index += 2;
            }
            "-h" | "--help" | "-?" => {
                parsed.help = true;
                return parsed;
            }
            "--scan" | "-scan" => {
                parsed.error = Some(
                    "project scanning is not implemented yet in the Rust-only repo".to_string(),
                );
                return parsed;
            }
            _ => {
                parsed.error = Some(format!(
                    "unsupported projects arguments in the Rust-only repo: {}",
                    args[index]
                ));
                return parsed;
            }
        }
    }

    parsed
}

fn normalize_flag(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn read_projects(path: &Path) -> Result<Vec<ProjectSummary>, String> {
    if !path.is_file() {
        return Ok(Vec::new());
    }

    let json = json_helpers::read_json_file(path)?;
    let Some(projects_object) = json_helpers::object_at_path(&json, &["projects"]) else {
        return Ok(Vec::new());
    };

    let mut projects = projects_object
        .iter()
        .filter_map(|(key, record)| normalize_project_record(key, record))
        .collect::<Vec<_>>();

    projects.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
            .then_with(|| {
                left.host_path
                    .to_ascii_lowercase()
                    .cmp(&right.host_path.to_ascii_lowercase())
            })
    });

    Ok(projects)
}

fn normalize_project_record(key: &str, record: &JsonValue) -> Option<ProjectSummary> {
    let host_path = record.get("hostPath")?.as_str()?.trim();
    if host_path.is_empty() || !Path::new(host_path).is_dir() {
        return None;
    }

    let project_id = record
        .get("projectId")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(key)
        .to_string();

    Some(ProjectSummary {
        project_id,
        name: record
            .get("name")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .unwrap_or(key)
            .to_string(),
        host_path: host_path.to_string(),
        detected_type: record
            .get("detectedType")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string(),
        markers: json_helpers::strings_from_array(
            record.get("markers").and_then(JsonValue::as_array),
        ),
        last_used_at: record
            .get("lastUsedAt")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string(),
        state: record
            .get("state")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string(),
    })
}

impl ProjectSummary {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("ProjectId", JsonValue::string(self.project_id.clone())),
            ("Name", JsonValue::string(self.name.clone())),
            ("HostPath", JsonValue::string(self.host_path.clone())),
            (
                "DetectedType",
                JsonValue::string(self.detected_type.clone()),
            ),
            (
                "Markers",
                JsonValue::array(self.markers.iter().cloned().map(JsonValue::string)),
            ),
            ("LastUsedAt", JsonValue::string(self.last_used_at.clone())),
            ("State", JsonValue::string(self.state.clone())),
        ])
    }
}
