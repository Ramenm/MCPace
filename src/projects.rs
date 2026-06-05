use crate::json::JsonValue;
use crate::json_helpers;
use crate::runtimepaths;
use crate::text_utils::normalize_flag;
use std::collections::{hash_map::DefaultHasher, BTreeMap};
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
struct ParsedArgs {
    json_output: bool,
    help: bool,
    scan: bool,
    scan_path: Option<PathBuf>,
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

    if parsed.scan {
        let project_path = resolve_scan_project_path(&root_path, parsed.scan_path.as_deref());
        let summary = match scan_project(&project_path) {
            Ok(value) => value,
            Err(error) => {
                let _ = writeln!(stderr, "{}", error);
                return 1;
            }
        };
        if let Err(error) = upsert_project(&registry_path, &summary) {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
        if parsed.json_output {
            let _ = writeln!(stdout, "{}", summary.to_json_value().to_pretty_string());
        } else {
            let _ = writeln!(stdout, "Registered project: {}", summary.name);
            let _ = writeln!(stdout, "    id: {}", summary.project_id);
            let _ = writeln!(stdout, "    host: {}", summary.host_path);
            let _ = writeln!(stdout, "    type: {}", summary.detected_type);
            if !summary.markers.is_empty() {
                let _ = writeln!(stdout, "    markers: {}", summary.markers.join(", "));
            }
        }
        return 0;
    }

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
            let _ = writeln!(stdout, "No project registry entries yet. Run `mcpace project scan --root <mcpace-root> <project-path>` to register one.");
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
        "Usage: mcpace projects [list|scan [project-path]] [--json] [--root <path>]"
    );
    let _ = writeln!(stdout);
    let _ = writeln!(
        stdout,
        "Native Rust path supports project registry inspection and one-project scan/upsert."
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
            "scan" | "--scan" | "-scan" => {
                parsed.scan = true;
                index += 1;
                if let Some(value) = args.get(index) {
                    if !value.starts_with('-') {
                        parsed.scan_path = Some(PathBuf::from(value));
                        index += 1;
                    }
                }
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

fn resolve_scan_project_path(root_path: &Path, scan_path: Option<&Path>) -> PathBuf {
    let Some(path) = scan_path else {
        return root_path.to_path_buf();
    };
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| root_path.to_path_buf())
            .join(path)
    }
}

fn scan_project(path: &Path) -> Result<ProjectSummary, String> {
    let canonical = runtimepaths::canonicalize_or_original(path);
    if !canonical.is_dir() {
        return Err(format!(
            "project scan path is not a directory: {}",
            canonical.display()
        ));
    }
    let markers = detect_project_markers(&canonical);
    let detected_type = detected_type_for_markers(&markers).to_string();
    let name = canonical
        .file_name()
        .map(|value| value.to_string_lossy().trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| canonical.display().to_string());
    let host_path = canonical.display().to_string();
    Ok(ProjectSummary {
        project_id: project_id_for_path(&host_path),
        name,
        host_path,
        detected_type,
        markers,
        last_used_at: runtimepaths::unix_time_ms().to_string(),
        state: "active".to_string(),
    })
}

fn detect_project_markers(path: &Path) -> Vec<String> {
    let candidates = [
        "mcpace.config.json",
        "mcp_settings.json",
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "requirements.txt",
        "go.mod",
        ".git",
    ];
    candidates
        .iter()
        .filter(|marker| path.join(marker).exists())
        .map(|marker| (*marker).to_string())
        .collect()
}

fn detected_type_for_markers(markers: &[String]) -> &'static str {
    let has = |name: &str| markers.iter().any(|marker| marker == name);
    if has("mcpace.config.json") || has("mcp_settings.json") {
        "mcpace-workspace"
    } else if has("Cargo.toml") && has("package.json") {
        "rust-node"
    } else if has("Cargo.toml") {
        "rust"
    } else if has("package.json") {
        "node"
    } else if has("pyproject.toml") || has("requirements.txt") {
        "python"
    } else if has("go.mod") {
        "go"
    } else if has(".git") {
        "git-worktree"
    } else {
        "generic"
    }
}

fn project_id_for_path(path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    let key = if cfg!(windows) {
        path.to_ascii_lowercase()
    } else {
        path.to_string()
    };
    key.hash(&mut hasher);
    format!("proj-{:016x}", hasher.finish())
}

fn upsert_project(path: &Path, summary: &ProjectSummary) -> Result<(), String> {
    let mut root = if path.is_file() {
        json_helpers::read_json_file(path)?
    } else {
        JsonValue::object([
            ("version", JsonValue::number(1)),
            ("projects", JsonValue::Object(BTreeMap::new())),
        ])
    };

    let Some(root_map) = root.as_object_mut() else {
        return Err(format!(
            "project registry {} must be a JSON object",
            path.display()
        ));
    };
    root_map
        .entry("version".to_string())
        .or_insert_with(|| JsonValue::number(1));
    if !matches!(root_map.get("projects"), Some(JsonValue::Object(_))) {
        root_map.insert("projects".to_string(), JsonValue::Object(BTreeMap::new()));
    }
    let Some(projects) = root_map
        .get_mut("projects")
        .and_then(JsonValue::as_object_mut)
    else {
        return Err(format!(
            "project registry {} has invalid projects object",
            path.display()
        ));
    };
    projects.insert(summary.project_id.clone(), summary.to_json_value());
    runtimepaths::write_text_atomic(path, &root.to_pretty_string())
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
            ("projectId", JsonValue::string(self.project_id.clone())),
            ("name", JsonValue::string(self.name.clone())),
            ("hostPath", JsonValue::string(self.host_path.clone())),
            (
                "detectedType",
                JsonValue::string(self.detected_type.clone()),
            ),
            (
                "markers",
                JsonValue::array(self.markers.iter().cloned().map(JsonValue::string)),
            ),
            ("lastUsedAt", JsonValue::string(self.last_used_at.clone())),
            ("state", JsonValue::string(self.state.clone())),
        ])
    }
}
