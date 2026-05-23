use crate::client_catalog::{self, ClientTargetRecord};
use crate::json::JsonValue;
use crate::json_helpers;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const MCPACE_ENTRY_NAME: &str = "MCPace";

#[derive(Debug, Clone)]
struct ConfigFinding {
    path: PathBuf,
    source: String,
    client_targets: BTreeSet<String>,
    exists: bool,
    parse_error: Option<String>,
    managed_entries: Vec<String>,
    direct_entries: Vec<String>,
    server_shapes: Vec<String>,
    warnings: Vec<String>,
}

impl ConfigFinding {
    fn empty(path: PathBuf, source: String) -> Self {
        Self {
            path,
            source,
            client_targets: BTreeSet::new(),
            exists: false,
            parse_error: None,
            managed_entries: Vec::new(),
            direct_entries: Vec::new(),
            server_shapes: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn status(&self) -> &'static str {
        if !self.exists {
            return "missing";
        }
        if self.parse_error.is_some() {
            return "unreadable";
        }
        if !self.direct_entries.is_empty() && !self.managed_entries.is_empty() {
            return "mixed-direct-and-mcpace";
        }
        if !self.direct_entries.is_empty() {
            return "direct-upstream-only";
        }
        if !self.managed_entries.is_empty() {
            return "managed-single-entry";
        }
        "no-mcp-servers"
    }

    fn repair_recommended(&self) -> bool {
        !self.direct_entries.is_empty() || self.parse_error.is_some()
    }

    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("path", JsonValue::string(self.path.display().to_string())),
            ("source", JsonValue::string(self.source.clone())),
            (
                "clientTargets",
                JsonValue::array(self.client_targets.iter().cloned().map(JsonValue::string)),
            ),
            ("exists", JsonValue::bool(self.exists)),
            ("status", JsonValue::string(self.status())),
            (
                "parseError",
                self.parse_error
                    .clone()
                    .map(JsonValue::string)
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "managedEntries",
                JsonValue::array(self.managed_entries.iter().cloned().map(JsonValue::string)),
            ),
            (
                "directEntries",
                JsonValue::array(self.direct_entries.iter().cloned().map(JsonValue::string)),
            ),
            (
                "serverShapes",
                JsonValue::array(self.server_shapes.iter().cloned().map(JsonValue::string)),
            ),
            (
                "repairRecommended",
                JsonValue::bool(self.repair_recommended()),
            ),
            (
                "warnings",
                JsonValue::array(self.warnings.iter().cloned().map(JsonValue::string)),
            ),
        ])
    }
}

#[derive(Debug, Clone)]
pub struct ClientConfigDoctorReport {
    root_path: Option<String>,
    current_dir: String,
    status: String,
    scanned_count: usize,
    existing_count: usize,
    direct_config_count: usize,
    managed_config_count: usize,
    repair_recommended: bool,
    restart_required: bool,
    findings: Vec<ConfigFinding>,
    warnings: Vec<String>,
}

impl ClientConfigDoctorReport {
    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("schema", JsonValue::string("mcpace.clientConfigDoctor.v1")),
            (
                "rootPath",
                self.root_path
                    .clone()
                    .map(JsonValue::string)
                    .unwrap_or(JsonValue::Null),
            ),
            ("currentDir", JsonValue::string(self.current_dir.clone())),
            ("status", JsonValue::string(self.status.clone())),
            ("scannedCount", JsonValue::number(self.scanned_count)),
            ("existingCount", JsonValue::number(self.existing_count)),
            (
                "directConfigCount",
                JsonValue::number(self.direct_config_count),
            ),
            (
                "managedConfigCount",
                JsonValue::number(self.managed_config_count),
            ),
            (
                "repairRecommended",
                JsonValue::bool(self.repair_recommended),
            ),
            ("restartRequired", JsonValue::bool(self.restart_required)),
            (
                "findings",
                JsonValue::array(self.findings.iter().map(ConfigFinding::to_json_value)),
            ),
            (
                "warnings",
                JsonValue::array(self.warnings.iter().cloned().map(JsonValue::string)),
            ),
            ("repairCommand", JsonValue::string("mcpace repair clients")),
            (
                "previewCommand",
                JsonValue::string("mcpace repair clients --dry-run --diff"),
            ),
        ])
    }
}

pub fn collect(root_path: Option<&Path>) -> ClientConfigDoctorReport {
    let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let registry = client_catalog::load_registry(root_path).unwrap_or_else(|error| {
        client_catalog::ClientRegistry {
            targets: Vec::new(),
            sources: Vec::new(),
            warnings: vec![format!("failed to load client catalog: {error}")],
        }
    });
    let mut warnings = registry.warnings.clone();
    let mut candidates = BTreeMap::<PathBuf, ConfigFinding>::new();

    for target in &registry.targets {
        if client_catalog::normalize(&target.surface_class) == "cloud" {
            continue;
        }
        register_target_paths(
            &mut candidates,
            target,
            root_path,
            &current_dir,
            &mut warnings,
        );
    }
    register_common_project_paths(&mut candidates, root_path, &current_dir);

    for finding in candidates.values_mut() {
        analyze_config_file(finding);
    }

    let mut findings = candidates.into_values().collect::<Vec<_>>();
    findings.sort_by(|left, right| left.path.cmp(&right.path));

    let scanned_count = findings.len();
    let existing_count = findings.iter().filter(|finding| finding.exists).count();
    let direct_config_count = findings
        .iter()
        .filter(|finding| !finding.direct_entries.is_empty())
        .count();
    let managed_config_count = findings
        .iter()
        .filter(|finding| !finding.managed_entries.is_empty())
        .count();
    let repair_recommended = findings.iter().any(ConfigFinding::repair_recommended);
    let restart_required = findings.iter().any(|finding| {
        finding.exists
            && (!finding.direct_entries.is_empty() || !finding.managed_entries.is_empty())
    });
    let status = if repair_recommended {
        "repair-recommended"
    } else if managed_config_count > 0 {
        "managed"
    } else if existing_count > 0 {
        "no-direct-upstreams-found"
    } else {
        "no-client-configs-found"
    }
    .to_string();

    if direct_config_count > 0 {
        warnings.push(format!(
            "{} client config(s) still contain direct upstream MCP server entries; these can launch subprocesses outside MCPace until repaired and the client is fully restarted",
            direct_config_count
        ));
    }
    if managed_config_count > 0 || direct_config_count > 0 {
        warnings.push(
            "After changing MCP client config, fully quit and reopen the client so stale MCP subprocesses are not reused.".to_string(),
        );
    }
    warnings.sort();
    warnings.dedup();

    ClientConfigDoctorReport {
        root_path: root_path.map(|path| path.display().to_string()),
        current_dir: current_dir.display().to_string(),
        status,
        scanned_count,
        existing_count,
        direct_config_count,
        managed_config_count,
        repair_recommended,
        restart_required,
        findings,
        warnings,
    }
}

pub fn write_text(report: &ClientConfigDoctorReport, stdout: &mut dyn Write) {
    let _ = writeln!(stdout, "Client config doctor: {}", report.status);
    let _ = writeln!(
        stdout,
        "Scanned {} candidate config(s); {} exist; {} contain direct upstream entries; {} contain MCPace.",
        report.scanned_count, report.existing_count, report.direct_config_count, report.managed_config_count
    );
    for finding in report.findings.iter().filter(|finding| finding.exists) {
        let _ = writeln!(
            stdout,
            "- {} [{}] direct={} managed={} source={}",
            finding.path.display(),
            finding.status(),
            finding.direct_entries.len(),
            finding.managed_entries.len(),
            finding.source
        );
    }
    if report.repair_recommended {
        let _ = writeln!(stdout, "Repair: run `mcpace repair clients` (preview: `mcpace repair clients --dry-run --diff`).");
    }
    if report.restart_required {
        let _ = writeln!(
            stdout,
            "Restart: fully quit and reopen the MCP client after repair."
        );
    }
    for warning in &report.warnings {
        let _ = writeln!(stdout, "warning: {}", warning);
    }
}

fn register_target_paths(
    candidates: &mut BTreeMap<PathBuf, ConfigFinding>,
    target: &ClientTargetRecord,
    root_path: Option<&Path>,
    current_dir: &Path,
    warnings: &mut Vec<String>,
) {
    let mut expressions = target.config_paths.clone();
    if let Some(install) = target.install_support.as_ref() {
        expressions.push(install.preferred_config_path.clone());
    }
    expressions.sort();
    expressions.dedup();

    for expression in expressions {
        for path in expand_config_expression(&expression, root_path, current_dir, warnings) {
            let normalized = normalize_path_key(path);
            let entry = candidates
                .entry(normalized.clone())
                .or_insert_with(|| ConfigFinding::empty(normalized, expression.clone()));
            entry.client_targets.insert(target.id.clone());
            if !entry.source.contains(&expression) {
                entry.source = format!("{}, {}", entry.source, expression);
            }
        }
    }
}

fn register_common_project_paths(
    candidates: &mut BTreeMap<PathBuf, ConfigFinding>,
    root_path: Option<&Path>,
    current_dir: &Path,
) {
    let anchors = project_anchors(root_path, current_dir);
    for anchor in anchors {
        for relative in [
            ".vscode/mcp.json",
            ".cursor/mcp.json",
            ".mcp.json",
            ".claude.json",
            ".gemini/settings.json",
            ".kiro/settings/mcp.json",
        ] {
            let path = normalize_path_key(anchor.join(relative));
            candidates.entry(path.clone()).or_insert_with(|| {
                ConfigFinding::empty(path, format!("project-common:{relative}"))
            });
        }
        if let Ok(entries) = fs::read_dir(&anchor) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) == Some("code-workspace") {
                    let path = normalize_path_key(path);
                    candidates.entry(path.clone()).or_insert_with(|| {
                        ConfigFinding::empty(path, "project-common:*.code-workspace".to_string())
                    });
                }
            }
        }
    }
}

fn expand_config_expression(
    expression: &str,
    root_path: Option<&Path>,
    current_dir: &Path,
    warnings: &mut Vec<String>,
) -> Vec<PathBuf> {
    let expression = expression.trim();
    if expression.is_empty()
        || expression.contains('<')
        || expression.contains("repository-level")
        || expression.contains("dashboard")
    {
        return Vec::new();
    }

    let mut raw_paths = Vec::new();
    if let Some(rest) = expression
        .strip_prefix("~/")
        .or_else(|| expression.strip_prefix("~\\"))
    {
        if let Some(home) = user_home_dir() {
            raw_paths.push(home.join(split_path(rest)));
        }
    } else if expression.starts_with('.') {
        for anchor in project_anchors(root_path, current_dir) {
            raw_paths.push(anchor.join(split_path(expression)));
        }
    } else if Path::new(expression).is_absolute() {
        raw_paths.push(PathBuf::from(expression));
    }

    let mut expanded = Vec::new();
    for path in raw_paths {
        if path.to_string_lossy().contains('*') {
            expanded.extend(expand_simple_wildcard(&path, warnings));
        } else {
            expanded.push(path);
        }
    }
    expanded
}

fn split_path(value: &str) -> PathBuf {
    let mut path = PathBuf::new();
    for segment in value.split(['/', '\\']) {
        if !segment.is_empty() && segment != "." {
            path.push(segment);
        }
    }
    path
}

fn expand_simple_wildcard(path: &Path, warnings: &mut Vec<String>) -> Vec<PathBuf> {
    let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
        return Vec::new();
    };
    let Some(parent) = path.parent() else {
        return Vec::new();
    };
    let Some((prefix, suffix)) = file_name.split_once('*') else {
        return Vec::new();
    };
    let entries = match fs::read_dir(parent) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            warnings.push(format!(
                "failed to scan wildcard config path '{}': {}",
                path.display(),
                error
            ));
            return Vec::new();
        }
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|candidate| {
            candidate
                .file_name()
                .and_then(|value| value.to_str())
                .map(|name| name.starts_with(prefix) && name.ends_with(suffix))
                .unwrap_or(false)
        })
        .collect()
}

fn project_anchors(root_path: Option<&Path>, current_dir: &Path) -> Vec<PathBuf> {
    let mut anchors = Vec::new();
    anchors.push(current_dir.to_path_buf());
    if let Some(root_path) = root_path {
        anchors.push(root_path.to_path_buf());
    }
    anchors.sort();
    anchors.dedup();
    anchors
}

fn normalize_path_key(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

fn analyze_config_file(finding: &mut ConfigFinding) {
    finding.exists = finding.path.is_file();
    if !finding.exists {
        return;
    }
    let raw = match fs::read_to_string(&finding.path) {
        Ok(value) => value,
        Err(error) => {
            finding.parse_error = Some(format!("failed to read config: {error}"));
            return;
        }
    };
    let extension = finding
        .path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if extension == "toml" || extension == "yaml" || extension == "yml" {
        analyze_text_config(finding, &raw);
        return;
    }
    match crate::json::parse_str(&raw) {
        Ok(value) => analyze_json_config(finding, &value),
        Err(error) => finding.parse_error = Some(error),
    }
}

fn analyze_json_config(finding: &mut ConfigFinding, value: &JsonValue) {
    for (shape_name, servers) in json_server_maps(value) {
        finding.server_shapes.push(shape_name);
        for name in servers.keys() {
            if name.eq_ignore_ascii_case(MCPACE_ENTRY_NAME) {
                finding.managed_entries.push(name.clone());
            } else {
                finding.direct_entries.push(name.clone());
            }
        }
    }
    finding.managed_entries.sort();
    finding.managed_entries.dedup();
    finding.direct_entries.sort();
    finding.direct_entries.dedup();
    finding.server_shapes.sort();
    finding.server_shapes.dedup();
}

fn json_server_maps(value: &JsonValue) -> Vec<(String, &BTreeMap<String, JsonValue>)> {
    let mut maps = Vec::new();
    for (name, path) in [
        ("mcpServers", &["mcpServers"][..]),
        ("servers", &["servers"][..]),
        ("mcp.servers", &["mcp", "servers"][..]),
        ("settings.mcp.servers", &["settings", "mcp", "servers"][..]),
        ("settings.mcpServers", &["settings", "mcpServers"][..]),
    ] {
        if let Some(object) = json_helpers::object_at_path(value, path) {
            maps.push((name.to_string(), object));
        }
    }
    maps
}

fn analyze_text_config(finding: &mut ConfigFinding, raw: &str) {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("mcpace") {
        finding.managed_entries.push(MCPACE_ENTRY_NAME.to_string());
    }
    if (lower.contains("mcp_servers") || lower.contains("mcpservers") || lower.contains("mcp:"))
        && !lower.contains("mcpace")
    {
        finding
            .warnings
            .push("text config appears to contain MCP settings but cannot be safely normalized without a schema-aware parser".to_string());
    }
}

fn user_home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

pub fn run(root_path: Option<&Path>, json_output: bool, stdout: &mut dyn Write) -> i32 {
    let report = collect(root_path);
    if json_output {
        let _ = writeln!(stdout, "{}", report.to_json_value().to_pretty_string());
    } else {
        write_text(&report, stdout);
    }
    if report.status == "repair-recommended" {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        env::temp_dir().join(format!("mcpace-client-doctor-{name}-{nonce}"))
    }

    #[test]
    fn detects_workspace_json_direct_and_managed_entries() {
        let root = unique_root("workspace");
        fs::create_dir_all(root.join(".vscode")).expect("mkdir");
        fs::write(
            root.join(".vscode/mcp.json"),
            r#"{"servers":{"filesystem":{"command":"npx"},"MCPace":{"url":"http://127.0.0.1:39022/mcp"}}}"#,
        )
        .expect("write config");

        let report = collect(Some(&root));
        assert_eq!(report.direct_config_count, 1);
        assert!(report.repair_recommended);
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.status() == "mixed-direct-and-mcpace"));

        let _ = fs::remove_dir_all(root);
    }
}
